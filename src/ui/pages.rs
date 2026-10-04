//! Hallmark · native workbench · design-system: design.md · designed-as-app.
//! Task-specific views share the continuous body surface.
use super::{fit_text, label, property_lines, surface, wrap_cells, Theme};
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
        Constraint::Length(if tight { 1 } else { 2 }),
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
                format!("[a] {}", action.1),
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
            "Primary {} · gateway {}",
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
    if a.rows().1.is_empty() {
        record_table(f, a, area, t);
    } else if wide && area.width >= 100 {
        let panes = Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)])
            .spacing(2)
            .split(area);
        record_table(f, a, panes[0], t);
        detail(f, a, panes[1], t);
    } else if area.height >= 13 {
        let count = a.rows().1.len() as u16;
        let panes = Layout::vertical([
            Constraint::Length((count + 2).clamp(4, area.height / 2)),
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
                (Page::Interfaces, 5)
                | (Page::Interfaces, 6)
                | (Page::Bandwidth, 1)
                | (Page::Bandwidth, 2) => Constraint::Length(14),
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

fn record_title(page: Page) -> &'static str {
    match page {
        Page::Interfaces | Page::Bandwidth => "Interfaces",
        Page::Wifi => "Nearby networks",
        Page::Dns => "Resolvers & presets",
        Page::Connections => "Connections",
        Page::Ports => "Listening sockets",
        Page::Routes => "Routing table",
        Page::Neighbors => "Observed devices",
        Page::Latency => "Probe targets",
        Page::History => "Saved tests",
        Page::Events => "Recent events",
        Page::Profiles => "Saved profiles",
        Page::System => "Optional backends",
        Page::Tailscale => "Tailnet peers",
        Page::Pihole => "DNS activity",
        Page::Tools => "Test catalog",
        Page::Dashboard => "Findings",
    }
}

fn numeric_column(header: &str) -> bool {
    matches!(
        header,
        "Signal"
            | "Channel"
            | "MTU"
            | "RX / s"
            | "TX / s"
            | "Download"
            | "Upload"
            | "Last"
            | "Loss / sent"
            | "Download Mbps"
            | "Upload Mbps"
            | "Metric"
            | "PID"
            | "UID"
    )
}

fn record_table(f: &mut Frame, a: &App, area: Rect, t: Theme) {
    let (headers, rows) = a.rows();
    let selected = a.selected.min(rows.len().saturating_sub(1));
    let title = if a.page == Page::Tools && a.result.is_some() {
        "Test results"
    } else {
        record_title(a.page)
    };
    let suffix = if a.filter.is_empty() {
        format!("{} entries", rows.len())
    } else {
        format!("{} matches · {}", rows.len(), clean(&a.filter))
    };
    let compact = area.height < 16;
    let block = surface(
        fit_text(&format!("{title} · {suffix}"), area.width.saturating_sub(2)),
        t,
    )
    .padding(Padding::new(1, 1, u16::from(!compact), 0));
    let mut inner = block.inner(area);
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
    let header_height = if compact { 1 } else { 2 };
    let overflow = rows.len() > inner.height.saturating_sub(header_height) as usize;
    if overflow {
        inner.height = inner.height.saturating_sub(1);
    }
    let visible = inner.height.saturating_sub(header_height).max(1) as usize;
    let offset = selected.saturating_sub(visible - 1);
    let cols = columns(a.page, headers.len(), inner.width.saturating_sub(2));
    let widths = Layout::horizontal(cols.iter().map(|(_, w)| *w))
        .spacing(2)
        .split(Rect::new(
            inner.x,
            inner.y,
            inner.width.saturating_sub(2),
            1,
        ));
    let cell = |value: String, n: usize, header: &str| {
        Cell::from(
            Line::from(fit_text(&clean(&value), widths[n].width)).alignment(
                if numeric_column(header) {
                    Alignment::Right
                } else {
                    Alignment::Left
                },
            ),
        )
    };
    let header = Row::new(
        cols.iter()
            .enumerate()
            .map(|(n, (i, _))| cell(headers[*i].clone(), n, &headers[*i])),
    )
    .style(Style::default().fg(t.muted))
    .bottom_margin(u16::from(!compact));
    let items = rows.iter().map(|row| {
        Row::new(cols.iter().enumerate().map(|(n, (i, _))| {
            cell(
                row.get(*i).cloned().unwrap_or_else(|| "—".into()),
                n,
                &headers[*i],
            )
        }))
        .style(Style::default().fg(t.fg))
    });
    let mut state = TableState::default()
        .with_selected(Some(selected))
        .with_offset(offset);
    f.render_stateful_widget(
        Table::new(items, cols.iter().map(|(_, w)| *w))
            .header(header)
            .column_spacing(2)
            .highlight_symbol("› ")
            .highlight_spacing(HighlightSpacing::Always)
            .row_highlight_style(Style::default().fg(t.fg).bg(t.border)),
        inner,
        &mut state,
    );
    if overflow {
        f.render_widget(
            Paragraph::new(fit_text(
                &format!(
                    "{}–{} of {} · ↑ ↓ scroll",
                    offset + 1,
                    (offset + visible).min(rows.len()),
                    rows.len()
                ),
                inner.width,
            ))
            .style(Style::default().fg(t.muted)),
            Rect::new(inner.x, inner.bottom(), inner.width, 1),
        );
    }
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
    let Some(row) = rows.get(selected) else {
        return;
    };
    let title = match a.page {
        Page::Interfaces | Page::Bandwidth => "Link details",
        Page::Wifi => "Network details",
        Page::Dns => "Resolver details",
        Page::Latency => "Probe statistics",
        Page::Connections | Page::Ports => "Connection details",
        Page::Routes => "Route details",
        Page::Neighbors => "Device details",
        Page::Profiles => "Profile details",
        Page::Events => "Event details",
        Page::History => "Test details",
        Page::System => "Backend details",
        Page::Tailscale => "Peer details",
        Page::Pihole => "Service controls",
        _ => "About this test",
    };
    let b = surface(title, t).padding(Padding::new(1, 1, u16::from(area.height >= 16), 0));
    let inner = b.inner(area);
    f.render_widget(b, area);
    let order: Vec<usize> = match a.page {
        Page::Interfaces => vec![0, 2, 1, 3, 4, 5, 6],
        Page::Wifi => vec![0, 5, 4, 1, 3, 2],
        Page::Connections | Page::Ports => vec![2, 3, 1, 5, 0, 4, 6],
        Page::Routes => vec![1, 2, 3, 5, 6, 4, 0],
        Page::Neighbors => vec![3, 0, 2, 1, 4, 5],
        _ => (0..headers.len()).collect(),
    };
    let mut fields: Vec<(String, String)> = order
        .into_iter()
        .filter_map(|i| Some((headers.get(i)?.clone(), row.get(i)?.clone())))
        .collect();
    if a.page == Page::Interfaces {
        if let Some(i) = a
            .snapshot
            .interfaces
            .iter()
            .find(|i| row.first() == Some(&i.name))
        {
            fields.extend([
                ("MAC".into(), i.mac.clone()),
                (
                    "Link speed".into(),
                    i.speed_mbps
                        .map(|n| format!("{n} Mbps"))
                        .unwrap_or_else(|| "Unavailable".into()),
                ),
                ("Received".into(), bytes(i.rx_bytes)),
                ("Sent".into(), bytes(i.tx_bytes)),
                (
                    "Errors / drops".into(),
                    format!("{} / {}", i.rx_errors + i.tx_errors, i.dropped),
                ),
            ]);
        }
    }
    if a.page == Page::Tailscale {
        if let Some(ts) = a
            .tailscale_result
            .as_ref()
            .and_then(|r| r.tailscale.as_ref())
        {
            fields.push(("This device".into(), ts.ips.join(", ")));
            fields.push(("Exit node".into(), ts.exit_node.clone()));
        }
    }
    let mut lines = Vec::new();
    if a.page == Page::Pihole {
        if let Some(r) = &a.pihole_result {
            let endpoint = r
                .pihole
                .as_ref()
                .map(|p| p.endpoint.as_str())
                .unwrap_or("—");
            lines.extend(property_lines("Server", endpoint, inner.width, t));
            lines.extend(property_lines(
                "Updated",
                &format!("{} UTC", r.at.format("%H:%M:%S")),
                inner.width,
                t,
            ));
            lines.push(Line::from(""));
            lines.push(Line::from(label(
                "[r] Refresh  [p] Pause polling",
                t.accent,
            )));
            lines.push(Line::from(""));
            for text in [
                "Ctrl+K → Pause / resume blocking",
                "Ctrl+K → Disconnect Pi-hole",
                "Blocking changes require confirmation.",
            ] {
                lines.extend(
                    wrap_cells(text, inner.width)
                        .into_iter()
                        .map(|s| Line::from(label(s, t.muted))),
                );
            }
        }
    } else {
        for (key, value) in fields {
            lines.extend(property_lines(&clean(&key), &clean(&value), inner.width, t));
        }
        let note = match a.page {
            Page::Tools if a.result.is_none() => {
                Some("Enter opens this test. Review the target before running.")
            }
            Page::Profiles => Some("Ctrl+K → Apply or delete profile. Changes are previewed."),
            Page::Ports => Some("Wildcard binds do not establish public exposure."),
            Page::Connections => Some("Process details depend on your permissions."),
            Page::Dns => Some("Presets are choices, not active resolvers. Apply from Ctrl+K."),
            Page::Neighbors => Some("STALE is a cache state, not proof of an offline device."),
            Page::System => Some("Only the related features need a missing backend."),
            _ => None,
        };
        if let Some(note) = note {
            lines.push(Line::from(""));
            lines.extend(
                wrap_cells(note, inner.width)
                    .into_iter()
                    .map(|s| Line::from(label(s, t.muted))),
            );
        }
        if a.page == Page::Tools {
            if let Some(r) = &a.result {
                for (k, v) in &r.metrics {
                    lines.extend(property_lines(k, v, inner.width, t));
                }
                for n in &r.notes {
                    lines.extend(
                        wrap_cells(&clean(n), inner.width)
                            .into_iter()
                            .map(|s| Line::from(label(s, t.muted))),
                    );
                }
            }
        }
    }
    let overflow = lines.len() > inner.height as usize;
    let body = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(u16::from(overflow)),
    );
    f.render_widget(Paragraph::new(lines), body);
    if overflow && inner.height > 0 {
        f.render_widget(
            Paragraph::new("Enter opens all fields").style(Style::default().fg(t.accent)),
            Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
        );
    }
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
    let compact = panes[0].height < 12;
    let b = surface(format!("Findings · {}", rows.len()), t).padding(Padding::new(
        1,
        1,
        u16::from(!compact),
        0,
    ));
    let inner = b.inner(panes[0]);
    f.render_widget(b, panes[0]);
    let row_height = if compact { 1 } else { 3 };
    let visible = (inner.height / row_height).max(1) as usize;
    let offset = selected.saturating_sub(visible - 1);
    for (n, row) in rows.iter().enumerate().skip(offset).take(visible) {
        let current = result
            .findings
            .iter()
            .find(|finding| row.get(1) == Some(&finding.title));
        let color = current
            .map(|finding| super::severity_color(finding.severity, t))
            .unwrap_or(t.muted);
        let lines = if compact {
            vec![Line::from(vec![
                label(if n == selected { "› " } else { "  " }, t.fg),
                label(
                    format!("{:<5} ", row.first().cloned().unwrap_or_default()),
                    if n == selected { t.fg } else { color },
                ),
                label(
                    fit_text(
                        row.get(1).map(String::as_str).unwrap_or(""),
                        inner.width.saturating_sub(8),
                    ),
                    t.fg,
                ),
            ])]
        } else {
            vec![
                Line::from(vec![
                    label(if n == selected { "› " } else { "  " }, t.fg),
                    Span::styled(
                        row.first().cloned().unwrap_or_default(),
                        Style::default()
                            .fg(if n == selected { t.fg } else { color })
                            .bold(),
                    ),
                ]),
                Line::from(label(
                    format!("  {}", row.get(1).map(String::as_str).unwrap_or("")),
                    t.fg,
                )),
            ]
        };
        f.render_widget(
            Paragraph::new(lines).style(Style::default().bg(if n == selected {
                t.border
            } else {
                t.panel
            })),
            Rect::new(
                inner.x,
                inner.y + ((n - offset) as u16 * row_height),
                inner.width,
                if compact { 1 } else { 2.min(inner.height) },
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
            surface("Selected finding", t).padding(Padding::new(
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
            Constraint::Length(if compact { 6 } else { 19 }),
            Constraint::Min(5),
        ])
        .spacing(1)
        .split(area)
    };
    let b = surface("Application", t).padding(Padding::new(1, 1, u16::from(!compact), 0));
    let inner = b.inner(panes[0]);
    f.render_widget(b, panes[0]);
    let mut lines = Vec::new();
    let renderer = if a.graphics.borrow().enabled() {
        "Pixel · dithered"
    } else {
        "Text · dithered"
    };
    if !compact {
        lines.push(Line::from(Span::styled(
            "Appearance",
            Style::default().fg(t.fg).bold(),
        )));
    }
    lines.extend(property_lines("[t] Theme", &a.config.theme, inner.width, t));
    if !compact {
        lines.extend(property_lines("Graphs", renderer, inner.width, t));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Collection",
            Style::default().fg(t.fg).bold(),
        )));
    }
    lines.extend(property_lines(
        "[m] Monitoring",
        on_off(a.config.monitoring_enabled),
        inner.width,
        t,
    ));
    lines.extend(property_lines(
        "[p] Snapshot",
        if a.paused { "Paused" } else { "Live" },
        inner.width,
        t,
    ));
    if !compact {
        lines.extend(property_lines(
            "Refresh",
            &format!("Every {}s", a.config.refresh_seconds),
            inner.width,
            t,
        ));
        lines.extend(property_lines(
            "Retention",
            &format!("{} samples", a.config.retention_samples),
            inner.width,
            t,
        ));
        lines.extend(property_lines(
            "Targets",
            &a.config.targets.join(", "),
            inner.width,
            t,
        ));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Access",
            Style::default().fg(t.fg).bold(),
        )));
    }
    lines.extend(property_lines(
        "[e] External",
        if a.config.external_enabled {
            "Enabled"
        } else {
            "Ask before tests"
        },
        inner.width,
        t,
    ));
    if !compact {
        lines.push(Line::from(""));
        lines.extend(
            wrap_cells("Ctrl+K → Graph renderer · Add latency target", inner.width)
                .into_iter()
                .map(|s| Line::from(label(s, t.muted))),
        );
    }
    f.render_widget(Paragraph::new(lines), inner);
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
