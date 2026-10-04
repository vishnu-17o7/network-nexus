//! Task-specific workbench views. All gutters inherit the continuous body surface.
use super::{fit_text, label, surface, Theme};
use crate::{
    app::{App, Page},
    command::clean,
    model::{bytes, ms, rate},
};
use ratatui::{prelude::*, widgets::*};

pub fn subtab_hit(page: Page, column: u16, width: u16) -> Option<Page> {
    tabs(page, width)
        .into_iter()
        .find_map(|(p, x, w)| (column >= x && column < x + w).then_some(p))
}

fn tabs(page: Page, width: u16) -> Vec<(Page, u16, u16)> {
    let pages = page.section_pages();
    let active = pages.iter().position(|p| *p == page).unwrap_or(0);
    let mut start = 0;
    while start < active
        && pages[start..=active]
            .iter()
            .map(|p| p.short_title().len() as u16 + 3)
            .sum::<u16>()
            > width.saturating_sub(2)
    {
        start += 1;
    }
    let mut x = 1;
    let mut result = Vec::new();
    for p in &pages[start..] {
        let w = p.short_title().len() as u16 + 3;
        if x + w > width {
            break;
        }
        result.push((*p, x, w));
        x += w;
    }
    result
}

fn navigation(f: &mut Frame, a: &App, area: Rect, t: Theme) {
    for (p, x, w) in tabs(a.page, area.width) {
        let text = format!(" {} ", p.short_title());
        f.render_widget(
            Paragraph::new(text).style(if p == a.page {
                Style::default().fg(t.accent).bold().underlined()
            } else {
                Style::default().fg(t.muted)
            }),
            Rect::new(area.x + x, area.y, w, 1),
        );
    }
}

pub fn draw(f: &mut Frame, a: &App, area: Rect, t: Theme) {
    let tight = area.height < 14;
    let compact = area.height < 24;
    let parts = Layout::vertical([
        Constraint::Length(if compact { 1 } else { 2 }),
        Constraint::Length(if tight {
            1
        } else if compact {
            2
        } else {
            3
        }),
        Constraint::Length(if compact { 1 } else { 2 }),
        Constraint::Min(1),
    ])
    .split(area);
    navigation(f, a, parts[0], t);
    let title = if a.page == Page::Tools {
        a.result
            .as_ref()
            .map(|r| r.title.as_str())
            .unwrap_or("Choose a test")
    } else {
        a.page.title()
    };
    let intro = vec![
        Line::from(Span::styled(clean(title), Style::default().fg(t.fg).bold())),
        Line::from(label(
            fit_text(&summary(a), area.width.saturating_sub(2)),
            t.muted,
        )),
    ];
    f.render_widget(Paragraph::new(intro), parts[1].inner(Margin::new(1, 0)));
    let action = a.page.primary_action();
    let extra = match a.page {
        Page::Profiles => "  t theme · m monitoring · e external",
        Page::Tools if a.result.is_some() => "  b test catalog · d run diagnostics",
        Page::Tools => "  Enter run selected · d diagnostics",
        Page::Connections | Page::Ports => "  f change view · / filter",
        Page::Bandwidth | Page::Latency => "  Space freeze · [ ] range",
        Page::History => "  Space freeze · Enter details",
        Page::Pihole => "  Space freeze · r refresh · p pause polling",
        _ => "  / filter · Enter full details",
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("a {}", action.1),
                Style::default().fg(t.accent).bold(),
            ),
            label(extra, t.muted),
        ])),
        parts[2].inner(Margin::new(1, 0)),
    );
    if a.page == Page::Profiles {
        settings(f, a, parts[3], t);
        return;
    }
    if disconnected(f, a, parts[3], t) {
        return;
    }
    if a.page == Page::Tools
        && a.result.as_ref().is_some_and(|r| {
            !r.findings.is_empty() && r.columns.first().is_some_and(|c| c == "Status")
        })
    {
        findings(f, a, parts[3], t);
        return;
    }
    if matches!(
        a.page,
        Page::Bandwidth | Page::Latency | Page::History | Page::Pihole
    ) && parts[3].height >= 12
    {
        let content = parts[3];
        if content.width >= 112 {
            let panes =
                Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)])
                    .spacing(2)
                    .split(content);
            chart(f, a, panes[0], t);
            records(f, a, panes[1], t, false);
        } else {
            let panes = Layout::vertical([Constraint::Min(7), Constraint::Length(6)])
                .spacing(1)
                .split(content);
            chart(f, a, panes[0], t);
            record_table(f, a, panes[1], t);
        }
    } else {
        records(f, a, parts[3], t, true);
    }
}

fn summary(a: &App) -> String {
    match a.page {
        Page::Interfaces => format!(
            "{} interfaces · primary {} · gateway {}",
            a.snapshot.interfaces.len(),
            a.snapshot.primary.as_deref().unwrap_or("unavailable"),
            a.snapshot.gateway().unwrap_or("unavailable")
        ),
        Page::Wifi => format!(
            "{} networks from the last scan · {} connected",
            a.wifi.len(),
            a.wifi.iter().filter(|n| n.connected).count()
        ),
        Page::Dns => format!(
            "{} active resolvers · {} · last response {}",
            a.snapshot.dns.len(),
            if a.snapshot.dns_source.is_empty() {
                "source unavailable"
            } else {
                &a.snapshot.dns_source
            },
            ms(a.dns_ms)
        ),
        Page::Connections | Page::Ports => format!(
            "{} sockets · {} listeners · view: {}",
            a.snapshot.connections.len(),
            a.snapshot
                .connections
                .iter()
                .filter(|c| c.listening())
                .count(),
            ["All", "Established", "Listening", "External"]
                [usize::from(a.connection_filter.min(3))]
        ),
        Page::Routes => format!(
            "{} routes · all visible IPv4 / IPv6 tables · gateway {}",
            a.snapshot.routes.len(),
            a.snapshot.gateway().unwrap_or("unavailable")
        ),
        Page::Neighbors => format!(
            "{} observed devices · {} labels · presence follows recent traffic",
            a.history.devices.len(),
            a.history.known_devices.len()
        ),
        Page::Bandwidth => {
            let (rx, tx, _, _) = a.snapshot.traffic_totals();
            format!(
                "{} · ↓ {}  ↑ {} · counter deltas",
                a.snapshot.primary.as_deref().unwrap_or("all links"),
                rate(rx),
                rate(tx)
            )
        }
        Page::Latency => format!(
            "{} targets observed · monitoring {} · missing replies stay visible",
            a.probes.len(),
            on_off(a.config.monitoring_enabled)
        ),
        Page::Events => format!(
            "{} observations · newest first · timestamps in UTC",
            a.history.events.len()
        ),
        Page::History => format!(
            "{} saved bandwidth tests · each result retains its backend",
            a.history.tests.len()
        ),
        Page::Profiles => "Appearance, collection and access · saved network configurations".into(),
        Page::System => format!(
            "{} of {} optional tools available · core local monitoring works independently",
            a.snapshot
                .capabilities
                .iter()
                .filter(|c| c.available)
                .count(),
            a.snapshot.capabilities.len()
        ),
        Page::Tools => a
            .result
            .as_ref()
            .map(|r| {
                format!(
                    "Observed {} UTC · {} findings · b returns to tests",
                    r.at.format("%H:%M:%S"),
                    r.findings.len()
                )
            })
            .unwrap_or_else(|| "Pick a question, run a bounded test, inspect the evidence.".into()),
        Page::Tailscale => a
            .tailscale_result
            .as_ref()
            .and_then(|r| r.tailscale.as_ref())
            .map(|ts| {
                format!(
                    "{} · {} · {} peers · {}",
                    ts.state,
                    ts.self_name,
                    ts.peers.len(),
                    ts.health
                        .first()
                        .map(String::as_str)
                        .unwrap_or("No daemon health warnings")
                )
            })
            .unwrap_or_else(|| "Read local tailnet status and inspect peer paths.".into()),
        Page::Pihole => a
            .pihole_result
            .as_ref()
            .and_then(|r| r.pihole.as_ref())
            .map(|p| {
                format!(
                    "Blocking {} · {:.1}% blocked · {} clients · {}",
                    p.blocking,
                    p.percent,
                    p.clients,
                    if a.pihole_polling && !a.paused {
                        "polling every 10s"
                    } else {
                        "polling stopped"
                    }
                )
            })
            .unwrap_or_else(|| "Connect a Pi-hole v6 server to inspect DNS activity.".into()),
        Page::Dashboard => String::new(),
    }
}
fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

fn chart(f: &mut Frame, a: &App, area: Rect, t: Theme) {
    match a.page {
        Page::Latency => crate::charts::latency(f, a, area, t),
        Page::History => crate::charts::speed(f, a, area, t),
        Page::Pihole => crate::charts::pihole(f, a, area, t),
        _ => crate::charts::traffic(f, a, area, t),
    }
}

fn records(f: &mut Frame, a: &App, area: Rect, t: Theme, wide: bool) {
    if wide && area.width >= 100 {
        let panes = Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)])
            .spacing(2)
            .split(area);
        record_table(f, a, panes[0], t);
        detail(f, a, panes[1], t);
    } else if area.height >= 13 {
        let count = a.rows().1.len() as u16;
        let panes = Layout::vertical([
            Constraint::Length((count + 4).clamp(5, area.height / 2)),
            Constraint::Min(6),
        ])
        .spacing(1)
        .split(area);
        record_table(f, a, panes[0], t);
        detail(f, a, panes[1], t);
    } else {
        record_table(f, a, area, t);
    }
}

/// Identity and decision columns are chosen per task, never by equal slicing.
fn columns(page: Page, total: usize, width: u16) -> Vec<(usize, Constraint)> {
    let wide = width >= 70;
    let indices: Vec<usize> = match page {
        Page::Interfaces => {
            if wide {
                vec![0, 2, 3, 5]
            } else {
                vec![0, 2, 3]
            }
        }
        Page::Wifi => {
            if wide {
                vec![0, 1, 2, 4, 5]
            } else {
                vec![0, 1, 4]
            }
        }
        Page::Connections => {
            if wide {
                vec![2, 3, 5]
            } else {
                vec![2, 3]
            }
        }
        Page::Ports => {
            if wide {
                vec![2, 5, 1]
            } else {
                vec![2, 5]
            }
        }
        Page::Routes => {
            if wide {
                vec![1, 2, 3, 6]
            } else {
                vec![1, 2, 3]
            }
        }
        Page::Neighbors => {
            if wide {
                vec![0, 3, 2]
            } else {
                vec![0, 3]
            }
        }
        Page::Bandwidth => vec![0, 1, 2],
        Page::Latency => vec![0, 1, 7],
        Page::History => {
            if wide {
                vec![0, 2, 3, 1]
            } else {
                vec![0, 2, 3]
            }
        }
        Page::Tailscale if total == 9 => {
            if wide {
                vec![0, 1, 2, 3]
            } else {
                vec![0, 1, 3]
            }
        }
        Page::Events => {
            if wide {
                vec![0, 1, 2]
            } else {
                vec![0, 1]
            }
        }
        Page::Profiles => {
            if wide {
                vec![0, 1, 2]
            } else {
                vec![0, 1]
            }
        }
        Page::System => {
            if wide {
                vec![0, 1, 2]
            } else {
                vec![0, 1]
            }
        }
        Page::Tools => vec![0, 1],
        _ => (0..total.min(if wide { 4 } else { 3 })).collect(),
    };
    indices
        .into_iter()
        .filter(|i| *i < total)
        .map(|i| {
            let w = match (page, i) {
                (Page::Interfaces, 0) | (Page::Bandwidth, 0) | (Page::Profiles, 1) => {
                    Constraint::Length(13)
                }
                (Page::Interfaces, 2)
                | (Page::Wifi, 1)
                | (Page::Wifi, 2)
                | (Page::Wifi, 5)
                | (Page::Routes, 6) => Constraint::Length(7),
                (Page::Wifi, 4) | (Page::Routes, 3) | (Page::System, 1) | (Page::Tailscale, 1) => {
                    Constraint::Length(10)
                }
                (Page::Latency, 1)
                | (Page::Latency, 7)
                | (Page::History, 2)
                | (Page::History, 3) => Constraint::Length(11),
                (Page::History, 0) => Constraint::Length(14),
                (Page::Events, 0) => Constraint::Length(9),
                (Page::System, 0) => Constraint::Length(13),
                (Page::Dns, 0) => Constraint::Length(12),
                (Page::Tools, 0) => Constraint::Percentage(48),
                _ => Constraint::Min(12),
            };
            (i, w)
        })
        .collect()
}

fn record_table(f: &mut Frame, a: &App, area: Rect, t: Theme) {
    let (headers, rows) = a.rows();
    let selected = a.selected.min(rows.len().saturating_sub(1));
    let title = if a.page == Page::Profiles {
        "SAVED PROFILES"
    } else if a.page == Page::Tools && a.result.is_none() {
        "TEST CATALOG"
    } else {
        "RECORDS"
    };
    let suffix = if a.filter.is_empty() {
        format!(
            "{} / {}",
            if rows.is_empty() { 0 } else { selected + 1 },
            rows.len()
        )
    } else {
        format!("{} matches · / {}", rows.len(), a.filter)
    };
    let compact = area.height < 8;
    let block = surface(format!("{title}  ·  {suffix}"), t).padding(Padding::new(
        1,
        1,
        if compact { 0 } else { 1 },
        0,
    ));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if rows.is_empty() {
        let (title, body) = empty_copy(a);
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(title, Style::default().fg(t.fg).bold())),
                Line::from(""),
                Line::from(label(body, t.muted)),
            ])
            .wrap(Wrap { trim: false }),
            inner,
        );
        return;
    }
    let cols = columns(a.page, headers.len(), inner.width);
    let header = Row::new(cols.iter().map(|(i, _)| Cell::from(headers[*i].clone())))
        .style(Style::default().fg(t.muted))
        .bottom_margin(if compact { 0 } else { 1 });
    let visible = inner
        .height
        .saturating_sub(if compact { 1 } else { 2 })
        .max(1) as usize;
    let offset = selected.saturating_sub(visible - 1);
    let items =
        rows.iter()
            .enumerate()
            .skip(offset)
            .take(visible)
            .map(|(n, row)| {
                Row::new(cols.iter().map(|(i, _)| {
                    Cell::from(clean(row.get(*i).map(String::as_str).unwrap_or("—")))
                }))
                .style(Style::default().fg(t.fg).bg(if n == selected {
                    t.border
                } else {
                    t.panel
                }))
            });
    f.render_widget(
        Table::new(items, cols.iter().map(|(_, w)| *w))
            .header(header)
            .column_spacing(2),
        inner,
    );
}

fn empty_copy(a: &App) -> (&'static str, &'static str) {
    if !a.filter.is_empty() {
        return ("No matches", "Esc clears the filter. / changes the search.");
    }
    match a.page {
        Page::Wifi => (
            "No Wi-Fi scan yet",
            "a scans nearby networks using the local radio.",
        ),
        Page::Latency => (
            "No probes recorded",
            "a starts monitoring. External targets require external access (e).",
        ),
        Page::History => (
            "No saved bandwidth tests",
            "a opens a short iperf3 test against a server you choose.",
        ),
        Page::Profiles => (
            "No saved profiles",
            "a saves a named DNS and MTU configuration.",
        ),
        Page::Neighbors => (
            "No devices observed",
            "Neighbors appear after local traffic. a opens explicit LAN discovery.",
        ),
        Page::Events => (
            "No events recorded",
            "Link changes and completed tasks appear here.",
        ),
        Page::System => (
            "Capabilities not collected",
            "a refreshes the local snapshot and checks installed tools.",
        ),
        Page::Ports => (
            "No matching listeners",
            "f changes the socket view; r refreshes local state.",
        ),
        _ => (
            "Nothing observed yet",
            "r refreshes local state. Ctrl+K opens available actions.",
        ),
    }
}

fn detail(f: &mut Frame, a: &App, area: Rect, t: Theme) {
    let (headers, rows) = a.rows();
    let selected = a.selected.min(rows.len().saturating_sub(1));
    let mut lines = Vec::new();
    if a.page == Page::Pihole {
        if let Some(result) = &a.pihole_result {
            let endpoint = result
                .pihole
                .as_ref()
                .map(|p| p.endpoint.as_str())
                .unwrap_or("—");
            let lines=vec![
                Line::from(label(endpoint,t.fg)),
                Line::from(label(format!("Observed {} UTC",result.at.format("%H:%M:%S")),t.muted)),
                Line::from(""),
                Line::from(label("r refresh · p pause / resume collection",t.accent)),
                Line::from(""),
                Line::from(label("Ctrl+K → Pause Pi-hole for 60 seconds",t.fg)),
                Line::from(label("Ctrl+K → Resume Pi-hole blocking",t.fg)),
                Line::from(label("Ctrl+K → Disconnect Pi-hole monitoring",t.fg)),
                Line::from(""),
                Line::from(label("Blocking changes are previewed before confirmation. Credentials stay in this session.",t.muted)),
            ];
            f.render_widget(
                Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .block(surface("SERVICE CONTROLS", t)),
                area,
            );
            return;
        }
    }
    if a.page == Page::Tailscale {
        if let Some(result) = &a.tailscale_result {
            if let Some(ts) = &result.tailscale {
                lines.extend([
                    Line::from(label("THIS DEVICE", t.muted)),
                    Line::from(label(clean(&ts.ips.join(" · ")), t.fg)),
                    Line::from(label(format!("Exit node: {}", clean(&ts.exit_node)), t.fg)),
                    Line::from(label(
                        format!("Observed {} UTC", result.at.format("%H:%M:%S")),
                        t.muted,
                    )),
                    Line::from(""),
                ]);
            }
        }
    }
    if let Some(row) = rows.get(selected) {
        for (key, value) in headers.iter().zip(row) {
            lines.push(Line::from(label(clean(key), t.muted)));
            lines.push(Line::from(label(
                if value.is_empty() {
                    "—".into()
                } else {
                    clean(value)
                },
                t.fg,
            )));
        }
        if a.page == Page::Interfaces {
            if let Some(i) = a
                .snapshot
                .interfaces
                .iter()
                .find(|i| row.first() == Some(&i.name))
            {
                lines.push(Line::from(""));
                lines.push(Line::from(label(
                    format!("MAC  {}", if i.mac.is_empty() { "—" } else { &i.mac }),
                    t.muted,
                )));
                lines.push(Line::from(label(
                    format!("Counters  ↓ {}  ↑ {}", bytes(i.rx_bytes), bytes(i.tx_bytes)),
                    t.fg,
                )));
                lines.push(Line::from(label(
                    format!(
                        "Errors {} · dropped {}",
                        i.rx_errors + i.tx_errors,
                        i.dropped
                    ),
                    t.muted,
                )));
            }
        }
        if a.page == Page::Tools && a.result.is_none() {
            lines.push(Line::from(""));
            lines.push(Line::from(label(
                "Enter opens the selected test.",
                t.accent,
            )));
            lines.push(Line::from(label(
                "Review the target before running. Results remain in saved diagnostic history.",
                t.muted,
            )));
        }
    } else {
        lines.push(Line::from(label(
            "Select a record to inspect its fields.",
            t.muted,
        )));
    }
    let note = match a.page {
        Page::Tools if a.result.is_none() => {
            "Ctrl+K also opens advanced tools and configuration controls."
        }
        Page::Ports => "ALL means a wildcard bind. It does not establish public exposure.",
        Page::Connections => "Process details depend on your user's permissions.",
        Page::Dns => {
            "Preset rows are choices, not active resolvers. Ctrl+K applies a preset after review."
        }
        Page::Routes => "System changes apply only after a preview and confirmation.",
        Page::System => "Missing tools affect their own tests. Install only the backends you need.",
        Page::Neighbors => "STALE is a neighbor-cache state, not proof that a device is offline.",
        _ => "Enter expands the full record · c copies the selected row",
    };
    lines.push(Line::from(""));
    lines.push(Line::from(label(note, t.muted)));
    if a.page == Page::Tools {
        if let Some(r) = &a.result {
            lines.extend(
                r.metrics
                    .iter()
                    .map(|(k, v)| Line::from(label(format!("{k}: {v}"), t.accent))),
            );
            lines.extend(r.notes.iter().map(|n| Line::from(label(clean(n), t.muted))));
        }
    }
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(surface(
                if a.page == Page::Tools && a.result.is_none() {
                    "ABOUT THIS TEST"
                } else {
                    "SELECTION  ·  Enter expands"
                },
                t,
            )),
        area,
    );
}

fn findings(f: &mut Frame, a: &App, area: Rect, t: Theme) {
    let Some(result) = &a.result else {
        return;
    };
    let panes = if area.width >= 95 {
        Layout::horizontal([Constraint::Percentage(43), Constraint::Percentage(57)])
            .spacing(2)
            .split(area)
    } else {
        Layout::vertical([Constraint::Percentage(36), Constraint::Percentage(64)])
            .spacing(1)
            .split(area)
    };
    let (_, rows) = a.rows();
    let selected = a.selected.min(rows.len().saturating_sub(1));
    let b = surface(format!("FINDINGS  ·  {}", rows.len()), t);
    let inner = b.inner(panes[0]);
    f.render_widget(b, panes[0]);
    let visible = (inner.height / 3).max(1) as usize;
    let offset = selected.saturating_sub(visible - 1);
    for (n, row) in rows.iter().enumerate().skip(offset).take(visible) {
        let current = result
            .findings
            .iter()
            .find(|finding| row.get(1) == Some(&finding.title));
        let color = current
            .map(|finding| super::severity_color(finding.severity, t))
            .unwrap_or(t.muted);
        let lines = vec![
            Line::from(vec![
                label(if n == selected { "› " } else { "  " }, t.accent),
                Span::styled(
                    row.first().cloned().unwrap_or_default(),
                    Style::default().fg(color).bold(),
                ),
            ]),
            Line::from(label(
                format!("  {}", row.get(1).map(String::as_str).unwrap_or("")),
                t.fg,
            )),
        ];
        f.render_widget(
            Paragraph::new(lines).style(Style::default().bg(if n == selected {
                t.border
            } else {
                t.panel
            })),
            Rect::new(
                inner.x,
                inner.y + ((n - offset) * 3) as u16,
                inner.width,
                2.min(inner.height),
            ),
        );
    }
    let current = rows.get(selected).and_then(|r| r.get(1)).and_then(|title| {
        result
            .findings
            .iter()
            .find(|finding| &finding.title == title)
    });
    let lines = if let Some(finding) = current {
        if panes[1].height < 15 {
            vec![
                Line::from(label("NEXT STEP", t.accent)),
                Line::from(label(clean(&finding.next_step), t.fg)),
                Line::from(""),
                Line::from(label("EVIDENCE", t.muted)),
                Line::from(label(clean(&finding.evidence), t.fg)),
            ]
        } else {
            vec![
                Line::from(Span::styled(
                    clean(&finding.title),
                    Style::default()
                        .fg(super::severity_color(finding.severity, t))
                        .bold(),
                )),
                Line::from(""),
                Line::from(label("EVIDENCE", t.muted)),
                Line::from(label(clean(&finding.evidence), t.fg)),
                Line::from(""),
                Line::from(label("NEXT STEP", t.accent)),
                Line::from(label(clean(&finding.next_step), t.fg)),
                Line::from(""),
                Line::from(label(
                    "No repair was applied. Ctrl+K opens tools and settings.",
                    t.muted,
                )),
            ]
        }
    } else {
        vec![Line::from(label(
            "No matching findings. Esc clears the filter.",
            t.muted,
        ))]
    };
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            surface("INVESTIGATION", t).padding(Padding::new(
                1,
                1,
                if panes[1].height < 15 { 0 } else { 1 },
                0,
            )),
        ),
        panes[1],
    );
}

fn settings(f: &mut Frame, a: &App, area: Rect, t: Theme) {
    let compact = area.height < 22;
    let panes = if area.width >= 100 {
        Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)])
            .spacing(3)
            .split(area)
    } else {
        Layout::vertical([
            Constraint::Length(if compact { 6 } else { 17 }),
            Constraint::Min(5),
        ])
        .spacing(1)
        .split(area)
    };
    let lines = if compact {
        vec![
            Line::from(vec![
                label("t  Theme       ", t.muted),
                label(a.config.theme.clone(), t.fg),
            ]),
            Line::from(vec![
                label("m  Monitoring  ", t.muted),
                label(on_off(a.config.monitoring_enabled), t.accent),
            ]),
            Line::from(vec![
                label("e  External    ", t.muted),
                label(
                    if a.config.external_enabled {
                        "enabled"
                    } else {
                        "ask first"
                    },
                    t.accent,
                ),
            ]),
            Line::from(vec![
                label("p  Collection  ", t.muted),
                label(if a.paused { "paused" } else { "running" }, t.fg),
            ]),
        ]
    } else {
        vec![
            Line::from(vec![
                label("t  Theme       ", t.muted),
                label(a.config.theme.clone(), t.fg),
            ]),
            Line::from(vec![
                label("   Charts      ", t.muted),
                label(format!("dithered · {:?}", a.graphics.borrow().mode), t.fg),
            ]),
            Line::from(""),
            Line::from(vec![
                label("m  Monitoring  ", t.muted),
                label(on_off(a.config.monitoring_enabled), t.accent),
            ]),
            Line::from(vec![
                label("e  External    ", t.muted),
                label(
                    if a.config.external_enabled {
                        "enabled"
                    } else {
                        "ask first"
                    },
                    t.accent,
                ),
            ]),
            Line::from(vec![
                label("p  Collection  ", t.muted),
                label(if a.paused { "paused" } else { "running" }, t.fg),
            ]),
            Line::from(vec![
                label("   Refresh     ", t.muted),
                label(format!("every {}s", a.config.refresh_seconds), t.fg),
            ]),
            Line::from(vec![
                label("   Retention   ", t.muted),
                label(format!("{} samples", a.config.retention_samples), t.fg),
            ]),
            Line::from(""),
            Line::from(label("Ctrl+K → Add latency target", t.accent)),
            Line::from(label(
                format!("Targets: {}", a.config.targets.join(", ")),
                t.muted,
            )),
            Line::from(""),
            Line::from(label("PROFILE ACTIONS", t.muted)),
            Line::from(label("a saves a profile. Ctrl+K → Apply / Delete.", t.fg)),
            Line::from(label(
                "Network changes are previewed before confirmation.",
                t.muted,
            )),
        ]
    };
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            surface("PREFERENCES", t).padding(Padding::new(1, 1, if compact { 0 } else { 1 }, 0)),
        ),
        panes[0],
    );
    records(f, a, panes[1], t, false);
}

fn disconnected(f: &mut Frame, a: &App, area: Rect, t: Theme) -> bool {
    let text=match a.page {
        Page::Pihole if a.pihole_result.is_none()=>Some(("Connect your DNS server","Use a Pi-hole v6 URL and an application password. Credentials stay in this session. Once connected, query activity and blocking status appear here.","a connect · Ctrl+K Pi-hole actions")),
        Page::Tailscale if a.tailscale_result.is_none()=>Some(("Inspect your tailnet","NEXUS reads the local tailscale CLI. Refresh to see this device, peer addresses, direct or relay paths, and daemon health.","a refresh local status · Ctrl+K Tailscale actions")),
        _=>None,
    };
    let Some((title, body, action)) = text else {
        return false;
    };
    let width = area.width.min(76);
    let inner = Rect::new(
        area.x + 1,
        area.y + 1,
        width.saturating_sub(2),
        area.height.saturating_sub(1),
    );
    let mut lines = vec![
        Line::from(Span::styled(title, Style::default().fg(t.fg).bold())),
        Line::from(""),
        Line::from(label(body, t.muted)),
        Line::from(""),
        Line::from(label(action, t.accent)),
    ];
    if let Some(busy) = &a.busy {
        lines.push(Line::from(""));
        lines.push(Line::from(label(format!("Running: {busy}"), t.warn)));
    }
    if a.notification.contains("failed") {
        lines.push(Line::from(""));
        lines.push(Line::from(label(clean(&a.notification), t.bad)));
    }
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(surface("SERVICE", t)),
        inner,
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{Config, History},
        model::Interface,
    };
    use ratatui::{backend::TestBackend, Terminal};
    #[test]
    fn secondary_navigation_keeps_selection_visible_and_clickable() {
        for page in Page::ALL {
            for width in [40, 60, 80, 110, 140, 200] {
                let buttons = tabs(page, width);
                assert!(buttons.iter().any(|(p, _, _)| *p == page));
                for (p, x, w) in buttons {
                    assert_eq!(subtab_hit(page, x + w / 2, width), Some(p));
                }
            }
        }
    }
    #[test]
    fn visible_details_follow_filtered_and_sorted_rows() {
        let mut app = App::new(Config::default(), History::default());
        app.started = std::time::Instant::now() - std::time::Duration::from_secs(1);
        app.page = Page::Interfaces;
        app.snapshot.interfaces = vec![
            Interface {
                name: "wlan0".into(),
                mac: "02:00:00:00:00:01".into(),
                ..Default::default()
            },
            Interface {
                name: "eth0".into(),
                mac: "02:00:00:00:00:02".into(),
                ..Default::default()
            },
        ];
        app.filter = "eth0".into();
        app.sort = 1;
        let mut terminal = Terminal::new(TestBackend::new(140, 42)).unwrap();
        terminal.draw(|f| super::super::draw(f, &app)).unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(text.contains("02:00:00:00:00:02"));
        assert!(!text.contains("02:00:00:00:00:01"));
    }
}
