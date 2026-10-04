// Hallmark · modern-minimal · native workbench · design-system: design.md
// Pre-emit critique: P4 H4 E4 S5 R5 V4. Terminal semantics replace CSS-only gates.
use crate::{
    app::{App, Modal, Page},
    model::{ms, rate},
};
use ratatui::{prelude::*, widgets::*};
mod pages;
pub use pages::subtab_hit as clicked_subtab;

#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: Color,
    pub panel: Color,
    pub fg: Color,
    pub muted: Color,
    pub border: Color,
    pub accent: Color,
    pub good: Color,
    pub warn: Color,
    pub bad: Color,
    pub violet: Color,
}
impl Theme {
    pub fn from_app(app: &App) -> Self {
        let mut t = match app.config.theme.as_str() {
            "oled" => Self {
                bg: Color::Black,
                panel: Color::Rgb(8, 8, 10),
                fg: Color::Rgb(240, 240, 245),
                muted: Color::Rgb(125, 135, 150),
                border: Color::Rgb(43, 48, 57),
                accent: Color::Rgb(71, 222, 204),
                good: Color::Rgb(106, 221, 144),
                warn: Color::Rgb(255, 202, 110),
                bad: Color::Rgb(255, 115, 132),
                violet: Color::Rgb(173, 151, 255),
            },
            "catppuccin" => Self {
                bg: Color::Rgb(30, 30, 46),
                panel: Color::Rgb(36, 36, 54),
                fg: Color::Rgb(205, 214, 244),
                muted: Color::Rgb(147, 153, 178),
                border: Color::Rgb(69, 71, 90),
                accent: Color::Rgb(137, 220, 235),
                good: Color::Rgb(166, 227, 161),
                warn: Color::Rgb(249, 226, 175),
                bad: Color::Rgb(243, 139, 168),
                violet: Color::Rgb(203, 166, 247),
            },
            "tokyo-night" => Self {
                bg: Color::Rgb(26, 27, 38),
                panel: Color::Rgb(31, 33, 47),
                fg: Color::Rgb(192, 202, 245),
                muted: Color::Rgb(137, 147, 180),
                border: Color::Rgb(52, 59, 88),
                accent: Color::Rgb(125, 207, 255),
                good: Color::Rgb(158, 206, 106),
                warn: Color::Rgb(224, 175, 104),
                bad: Color::Rgb(247, 118, 142),
                violet: Color::Rgb(187, 154, 247),
            },
            "gruvbox" => Self {
                bg: Color::Rgb(40, 40, 40),
                panel: Color::Rgb(50, 48, 47),
                fg: Color::Rgb(235, 219, 178),
                muted: Color::Rgb(168, 153, 132),
                border: Color::Rgb(80, 73, 69),
                accent: Color::Rgb(131, 165, 152),
                good: Color::Rgb(184, 187, 38),
                warn: Color::Rgb(250, 189, 47),
                bad: Color::Rgb(251, 110, 94),
                violet: Color::Rgb(211, 134, 155),
            },
            "light" => Self {
                bg: Color::Rgb(242, 245, 249),
                panel: Color::White,
                fg: Color::Rgb(32, 43, 65),
                muted: Color::Rgb(98, 113, 138),
                border: Color::Rgb(208, 217, 230),
                accent: Color::Rgb(0, 116, 143),
                good: Color::Rgb(24, 128, 82),
                warn: Color::Rgb(158, 104, 0),
                bad: Color::Rgb(190, 45, 68),
                violet: Color::Rgb(112, 74, 168),
            },
            _ => Self {
                bg: Color::Rgb(13, 18, 26),
                panel: Color::Rgb(18, 25, 35),
                fg: Color::Rgb(226, 232, 240),
                muted: Color::Rgb(133, 144, 157),
                border: Color::Rgb(43, 56, 72),
                accent: Color::Rgb(80, 216, 215),
                good: Color::Rgb(126, 218, 151),
                warn: Color::Rgb(244, 193, 112),
                bad: Color::Rgb(247, 126, 145),
                violet: Color::Rgb(174, 155, 250),
            },
        };
        for (key, value) in &app.config.custom_colors {
            if let Some(v) = value
                .strip_prefix('#')
                .filter(|v| v.len() == 6)
                .and_then(|v| u32::from_str_radix(v, 16).ok())
            {
                let c = Color::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8);
                let target = match key.as_str() {
                    "background" => &mut t.bg,
                    "panel" => &mut t.panel,
                    "foreground" => &mut t.fg,
                    "accent" => &mut t.accent,
                    "muted" => &mut t.muted,
                    "border" => &mut t.border,
                    _ => continue,
                };
                *target = c;
            }
        }
        // A short entrance fade; data values themselves are never interpolated.
        let progress = (app.started.elapsed().as_secs_f64() / 0.35).min(1.0);
        let blend = |color: Color| match (t.bg, color) {
            (Color::Rgb(br, bg, bb), Color::Rgb(r, g, b)) => Color::Rgb(
                (br as f64 + (r as f64 - br as f64) * progress) as u8,
                (bg as f64 + (g as f64 - bg as f64) * progress) as u8,
                (bb as f64 + (b as f64 - bb as f64) * progress) as u8,
            ),
            _ => color,
        };
        t.fg = blend(t.fg);
        t.muted = blend(t.muted);
        t.accent = blend(t.accent);
        t
    }
}
fn block<'a>(title: impl Into<Line<'a>>, t: Theme) -> Block<'a> {
    Block::default()
        .title(title)
        .title_style(Style::default().fg(t.muted).bold())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.panel).fg(t.fg))
        .padding(Padding::horizontal(1))
}
fn fit_text(value: &str, width: u16) -> String {
    if Line::from(value).width() <= width as usize {
        return value.into();
    }
    let mut result = String::new();
    let mut used = 0;
    for c in value.chars() {
        let w = Span::raw(c.to_string()).width();
        if used + w + 1 > width as usize {
            break;
        }
        result.push(c);
        used += w;
    }
    if width > 0 {
        result.push('…');
    }
    result
}
fn label<'a>(value: impl Into<std::borrow::Cow<'a, str>>, color: Color) -> Span<'a> {
    Span::styled(value, Style::default().fg(color))
}

/// Keep the editing end visible without splitting a UTF-8 character.
fn input_tail(value: &str, width: u16) -> String {
    if Line::from(value).width() <= width as usize {
        return value.into();
    }
    if width == 0 {
        return String::new();
    }
    let mut suffix = Vec::new();
    let mut used = 1;
    for c in value.chars().rev() {
        let w = Span::raw(c.to_string()).width();
        if used + w > width as usize {
            break;
        }
        suffix.push(c);
        used += w;
    }
    format!("…{}", suffix.into_iter().rev().collect::<String>())
}

fn wrap_cells(value: &str, width: u16) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }
    let mut result = Vec::new();
    for paragraph in value.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            if !line.is_empty() && Line::from(format!("{line} {word}")).width() > width as usize {
                result.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            for c in word.chars() {
                if Line::from(format!("{line}{c}")).width() > width as usize && !line.is_empty() {
                    result.push(std::mem::take(&mut line));
                }
                line.push(c);
            }
        }
        result.push(line);
    }
    result
}

fn property_lines(key: &str, value: &str, width: u16, t: Theme) -> Vec<Line<'static>> {
    let key_width = (width / 3).clamp(8, 16).min(width.saturating_sub(5));
    let value_width = width.saturating_sub(key_width + 2);
    let value = if value.trim().is_empty() {
        "—"
    } else {
        value
    };
    wrap_cells(value, value_width)
        .into_iter()
        .enumerate()
        .map(|(n, part)| {
            Line::from(vec![
                label(
                    format!(
                        "{:<width$}  ",
                        if n == 0 {
                            fit_text(key, key_width)
                        } else {
                            String::new()
                        },
                        width = key_width as usize
                    ),
                    t.muted,
                ),
                label(part, t.fg),
            ])
        })
        .collect()
}

pub fn draw(frame: &mut Frame, app: &App) {
    app.graphics.borrow_mut().begin_frame();
    let t = Theme::from_app(app);
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(t.bg).fg(t.fg)),
        area,
    );
    if area.width < 40 || area.height < 12 {
        frame.render_widget(Paragraph::new("NEXUS · Network Control Center\n\nResize the terminal to at least 40 × 12.\nRecommended: 110 × 32 or larger.\n\nq / Ctrl+C exits safely.").style(Style::default().fg(t.accent)).block(block(" Small terminal ",t)),area);
        return;
    }
    let layout = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(2),
    ])
    .split(area);
    header(frame, app, layout[0], t);
    let main = layout[1];
    // Fill the whole content surface so layout gaps match cards and chart layers.
    frame.render_widget(Block::default().style(Style::default().bg(t.panel)), main);
    if app.page == Page::Dashboard {
        dashboard(frame, app, main, t);
    } else {
        pages::draw(frame, app, main, t);
    }
    footer(frame, app, layout[2], t);
    if let Some(modal) = &app.modal {
        overlay(frame, app, modal, t);
    }
}

fn header(f: &mut Frame, a: &App, area: Rect, t: Theme) {
    let brand_width = if area.width >= 85 {
        20
    } else if area.width >= 60 {
        9
    } else {
        0
    };
    let tab_width = area.width.saturating_sub(brand_width);
    let labels = if area.width < 60 {
        ["Home", "Test", "View", "Live", "Set"]
    } else if area.width >= 70 {
        ["Overview", "Diagnose", "Inspect", "Monitor", "Settings"]
    } else {
        ["Home", "Test", "Inspect", "Live", "Setup"]
    };
    let active = match a.page {
        Page::Dashboard => 0,
        Page::Tools => 1,
        Page::Latency | Page::Bandwidth | Page::Events | Page::History => 3,
        Page::Profiles | Page::System => 4,
        _ => 2,
    };
    let mut spans = Vec::new();
    for (n, name) in labels.iter().enumerate() {
        spans.push(Span::styled(
            format!(" {name} "),
            if n == active {
                Style::default().bg(t.accent).fg(t.bg).bold()
            } else {
                Style::default().fg(t.fg)
            },
        ));
        spans.push(label(" ", t.bg));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect::new(area.x, area.y, tab_width, 1),
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("NEXUS", Style::default().fg(t.accent).bold()),
            label(
                if brand_width >= 20 {
                    format!("  v{}", env!("CARGO_PKG_VERSION"))
                } else {
                    String::new()
                },
                t.muted,
            ),
        ]))
        .alignment(Alignment::Right),
        Rect::new(area.x + tab_width, area.y, area.width - tab_width, 1),
    );
    let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"][(a.tick / 3) % 10];
    let state = if a.paused {
        "PAUSED"
    } else if a.busy.is_some() {
        "WORKING"
    } else if a.last_snapshot.elapsed().as_secs() > a.config.refresh_seconds.saturating_mul(3) {
        "STALE"
    } else {
        "LIVE"
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            label(
                format!(
                    " {} {state}  ",
                    if a.busy.is_some() { spinner } else { "●" }
                ),
                if state == "STALE" || state == "PAUSED" {
                    t.warn
                } else {
                    t.accent
                },
            ),
            label(
                if a.page == Page::Dashboard {
                    a.page.title()
                } else {
                    "Local collection"
                },
                t.fg,
            ),
            label(
                if area.width < 70 {
                    ""
                } else if a.config.external_enabled {
                    "  ·  external tests enabled"
                } else {
                    "  ·  local-only; tests ask first"
                },
                t.muted,
            ),
        ])),
        Rect::new(area.x, area.y + 1, area.width, 1),
    );
}

pub fn clicked_tab(column: u16, width: u16) -> Option<Page> {
    let labels = if width < 60 {
        ["Home", "Test", "View", "Live", "Set"]
    } else if width >= 70 {
        ["Overview", "Diagnose", "Inspect", "Monitor", "Settings"]
    } else {
        ["Home", "Test", "Inspect", "Live", "Setup"]
    };
    let pages = [
        Page::Dashboard,
        Page::Tools,
        Page::Interfaces,
        Page::Bandwidth,
        Page::Profiles,
    ];
    let mut x = 0;
    for (n, label) in labels.iter().enumerate() {
        let end = x + label.len() as u16 + 2;
        if column >= x && column < end {
            return Some(pages[n]);
        }
        x = end + 1;
    }
    None
}
fn footer(f: &mut Frame, a: &App, area: Rect, t: Theme) {
    let message = a
        .busy
        .as_ref()
        .map(|s| format!("{} {}", if a.mutating { "APPLYING" } else { "RUNNING" }, s))
        .unwrap_or_else(|| a.notification.clone());
    let mut progress = vec![label(" ", t.muted)];
    if a.busy.is_some() {
        for n in 0..6 {
            progress.push(label(
                "▰",
                if (a.tick / 3 + n) % 6 < 2 {
                    t.accent
                } else {
                    t.border
                },
            ));
        }
        progress.push(label("  ", t.muted));
    }
    if a.chart_snapshot.is_some() && a.busy.is_none() {
        progress.push(label(
            "GRAPHS PAUSED · collection continues · Space resumes",
            t.warn,
        ));
    } else {
        progress.push(label(message, t.accent));
    }
    let range_page = matches!(a.page, Page::Dashboard | Page::Bandwidth | Page::Latency);
    let chart_page = range_page || matches!(a.page, Page::Pihole | Page::History);
    let keys = if area.width < 60 {
        " Ctrl+K actions · ? help · q quit"
    } else if a.page == Page::Tools && a.result.is_none() {
        if area.width < 90 {
            " ↑ ↓ select · Enter run · Ctrl+K · ? help · q quit"
        } else {
            " j/k select · Enter run test · / filter · Ctrl+K all actions · ? help · q quit"
        }
    } else if a.page != Page::Dashboard && area.width >= 110 {
        " , . section   a action   / filter   j/k select   Enter details   Ctrl+K all actions   ? help   q quit"
    } else if a.page != Page::Dashboard {
        " , . pages · Enter details · Ctrl+K · ? help · q quit"
    } else if range_page && area.width >= 110 {
        " Space freeze graphs   [ ] range   d diagnose   Ctrl+K actions   Tab page   ? help   q quit"
    } else if range_page && area.width >= 60 {
        " Space freeze · [ ] range · Ctrl+K actions · ? · q"
    } else if chart_page && area.width >= 110 {
        " Space freeze graphs   Ctrl+K actions   Enter details   Tab page   r refresh   ? help   q quit"
    } else if chart_page && area.width >= 60 {
        " Space freeze · Ctrl+K actions · r refresh · ? · q"
    } else if area.width < 85 {
        " Ctrl+K actions · Tab page · Enter · ? · q"
    } else {
        " Ctrl+K actions   / filter   j/k move   Enter details   Tab page   r refresh   ? help   q quit"
    };
    f.render_widget(
        Paragraph::new(vec![Line::from(progress), Line::from(label(keys, t.muted))]),
        area,
    );
}
fn severity_color(level: crate::diagnosis::Severity, t: Theme) -> Color {
    use crate::diagnosis::Severity::*;
    match level {
        Error => t.bad,
        Warning => t.warn,
        Unknown => t.muted,
        Info => t.accent,
        Pass => t.good,
    }
}
fn surface<'a>(title: impl Into<Line<'a>>, t: Theme) -> Block<'a> {
    Block::default()
        .title(title)
        .title_style(Style::default().fg(t.fg).bold())
        .borders(Borders::TOP)
        .border_style(Style::default().fg(t.border))
        .padding(Padding::new(1, 1, 1, 0))
        .style(Style::default().bg(t.panel).fg(t.fg))
}
fn metric(
    f: &mut Frame,
    area: Rect,
    t: Theme,
    title: &str,
    value: String,
    caption: String,
    color: Color,
) {
    f.render_widget(Block::default().style(Style::default().bg(t.panel)), area);
    let inside = area.inner(Margin::new(1, 0));
    if inside.height == 0 {
        return;
    }
    let mut lines = vec![
        Line::from(label(title, t.muted)),
        Line::from(Span::styled(value, Style::default().fg(color).bold())),
    ];
    if inside.height >= 4 && inside.width >= 23 {
        lines.push(Line::from(label(caption, t.muted)));
    }
    f.render_widget(Paragraph::new(lines), inside);
}
fn dashboard(f: &mut Frame, a: &App, area: Rect, t: Theme) {
    let iface = a.snapshot.primary_interface();
    let (rx, tx, _, _) = a.snapshot.traffic_totals();
    let tested = a.assessment.as_ref().and_then(|r| {
        r.metrics
            .get("Internet reachability")
            .map(|v| format!("{v} · tested {}", r.at.format("%H:%M")))
    });
    let status = tested.unwrap_or_else(|| match a.internet {
        Some(true) => "ICMP responding".into(),
        Some(false) => "ICMP probes failed".into(),
        None => "Internet not tested".into(),
    });
    let compact = area.height < 16;
    let summary_height = if area.height >= 28 { 2 } else { 1 };
    let metrics_height = if compact {
        2
    } else if area.height >= 28 {
        4
    } else {
        3
    };
    let graph_height = if compact {
        0
    } else if area.width < 100 && area.height >= 36 {
        (area.height - 18).clamp(20, 30)
    } else {
        (area.height / 2).clamp(8, 22)
    };
    let sections = Layout::vertical([
        Constraint::Length(summary_height),
        Constraint::Length(metrics_height),
        Constraint::Length(graph_height),
        Constraint::Min(3),
    ])
    .split(area);
    let device = iface
        .map(|i| format!("{} · {} / {}", i.name, i.kind, i.state))
        .unwrap_or_else(|| "No default interface".into());
    f.render_widget(
        Paragraph::new(Line::from(vec![
            label(
                " ● ",
                if a.internet == Some(false) {
                    t.warn
                } else {
                    t.accent
                },
            ),
            Span::styled(status, Style::default().fg(t.fg).bold()),
            label(format!("    {device}"), t.muted),
        ])),
        sections[0],
    );
    if compact {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                label(format!(" ↓ {}  ", rate(rx)), t.accent),
                label(format!("↑ {}", rate(tx)), t.violet),
            ])),
            sections[1],
        );
    } else {
        let cards = Layout::horizontal([Constraint::Percentage(25); 4])
            .spacing(1)
            .split(sections[1]);
        let probe = a.internet_probe();
        metric(
            f,
            cards[0],
            t,
            "DOWNLOAD",
            rate(rx),
            if a.paused {
                "snapshot paused"
            } else {
                "live interface rate"
            }
            .into(),
            t.accent,
        );
        metric(
            f,
            cards[1],
            t,
            "UPLOAD",
            rate(tx),
            if a.paused {
                "snapshot paused"
            } else {
                "live interface rate"
            }
            .into(),
            t.violet,
        );
        metric(
            f,
            cards[2],
            t,
            "INTERNET RTT",
            ms(probe.and_then(|p| p.last)),
            probe
                .filter(|p| p.sent > 0)
                .map(|p| format!("{:.1}% loss · session", p.loss()))
                .unwrap_or_else(|| "m starts probes".into()),
            t.fg,
        );
        metric(
            f,
            cards[3],
            t,
            "DNS RESPONSE",
            ms(a.dns_ms),
            "last resolver probe".into(),
            if a.dns_ms.is_some_and(|v| v > 250.) {
                t.warn
            } else {
                t.fg
            },
        );
    }
    if graph_height > 0 {
        if area.width >= 100 {
            let charts =
                Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
                    .spacing(2)
                    .split(sections[2]);
            traffic(f, a, charts[0], t);
            latency_chart(f, a, charts[1], t);
        } else if graph_height >= 20 {
            let charts = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
                .spacing(1)
                .split(sections[2]);
            traffic(f, a, charts[0], t);
            latency_chart(f, a, charts[1], t);
        } else {
            traffic(f, a, sections[2], t);
        }
    }
    let context = sections[3];
    if area.width >= 100 && context.height >= 7 {
        let parts = Layout::horizontal([Constraint::Percentage(63), Constraint::Percentage(37)])
            .spacing(2)
            .split(context);
        findings_panel(f, a, parts[0], t);
        let mut lines = vec![
            Line::from(vec![label("Interface  ", t.muted), label(device, t.fg)]),
            Line::from(vec![
                label("Address    ", t.muted),
                label(
                    iface
                        .map(|i| i.addresses.join(" · "))
                        .filter(|v| !v.is_empty())
                        .unwrap_or_else(|| "Unavailable".into()),
                    t.fg,
                ),
            ]),
            Line::from(vec![
                label("Gateway    ", t.muted),
                label(a.snapshot.gateway().unwrap_or("Unavailable"), t.fg),
            ]),
            Line::from(vec![
                label("Resolvers  ", t.muted),
                label(
                    if a.snapshot.dns.is_empty() {
                        "Unavailable".into()
                    } else {
                        a.snapshot.dns.join(" · ")
                    },
                    t.fg,
                ),
            ]),
        ];
        if context.height >= 9 {
            lines.extend([
                Line::from(""),
                Line::from(label(
                    format!(
                        "{} sockets  ·  {} listeners  ·  {} tunnels",
                        a.snapshot.connections.len(),
                        a.snapshot
                            .connections
                            .iter()
                            .filter(|c| c.listening())
                            .count(),
                        a.snapshot.vpn_interfaces().len()
                    ),
                    t.muted,
                )),
            ]);
        }
        if context.height >= 12 {
            lines.push(Line::from(vec![
                label("Public IP  ", t.muted),
                label(a.last_public_ip.as_deref().unwrap_or("not requested"), t.fg),
            ]));
            lines.push(Line::from(label(
                "Ctrl+K opens interfaces, DNS and VPN tools",
                t.accent,
            )));
        }
        f.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .block(surface(" NETWORK ", t)),
            parts[1],
        );
    } else {
        findings_panel(f, a, context, t);
    }
}
fn findings_panel(f: &mut Frame, a: &App, area: Rect, t: Theme) {
    let findings = a.findings();
    let bad = findings
        .iter()
        .filter(|f| {
            matches!(
                f.severity,
                crate::diagnosis::Severity::Error | crate::diagnosis::Severity::Warning
            )
        })
        .count();
    let b = surface(format!(" FINDINGS · {bad} to check "), t);
    let inner = b.inner(area);
    f.render_widget(b, area);
    let rows = inner.height.saturating_sub(1).max(1) as usize / 3;
    let selected = a.selected.min(findings.len().saturating_sub(1));
    let offset = selected.saturating_sub(rows.saturating_sub(1));
    for (n, finding) in findings.iter().enumerate().skip(offset).take(rows.max(1)) {
        let y = inner.y + ((n - offset) * 3) as u16;
        if y >= inner.bottom() {
            break;
        }
        let color = severity_color(finding.severity, t);
        let line = Line::from(vec![
            label(if n == selected { "› " } else { "  " }, t.accent),
            Span::styled(
                format!("[{}] ", finding.severity.label()),
                Style::default().fg(color).bold(),
            ),
            label(finding.title.clone(), t.fg),
        ]);
        f.render_widget(
            Paragraph::new(vec![
                line,
                Line::from(label(
                    fit_text(&format!("  {}", finding.next_step), inner.width),
                    t.muted,
                )),
            ])
            .wrap(Wrap { trim: false }),
            Rect::new(inner.x, y, inner.width, 2.min(inner.bottom() - y)),
        );
    }
    if inner.height > 2 {
        f.render_widget(
            Paragraph::new(label(
                if a.assessment.is_some() {
                    "j/k select · Enter details · d test again"
                } else {
                    "d diagnose · Ctrl+K inspect endpoint"
                },
                t.accent,
            )),
            Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
        );
    }
}
fn traffic(f: &mut Frame, a: &App, area: Rect, t: Theme) {
    crate::charts::traffic(f, a, area, t);
}
fn latency_chart(f: &mut Frame, a: &App, area: Rect, t: Theme) {
    crate::charts::latency(f, a, area, t);
}
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width.saturating_sub(4));
    let h = height.min(area.height.saturating_sub(2));
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}
fn overlay(f: &mut Frame, a: &App, modal: &Modal, t: Theme) {
    let height = match modal {
        Modal::Form(v) => {
            (v.fields.len() * 3
                + 4
                + wrap_cells(&v.note, f.area().width.saturating_sub(8).min(76))
                    .len()
                    .clamp(1, 3)
                + 2) as u16
        }
        Modal::Palette { .. } => 20,
        Modal::Help => 25,
        Modal::Detail { lines, .. } => (lines.len() + 5).clamp(8, 26) as u16,
        Modal::ConfirmPlan(p) => (p.summary.len() * 2 + 12).clamp(20, 30) as u16,
        _ => 15,
    };
    let area = centered(f.area(), 80, height);
    // A modal owns keyboard focus. Quiet the entire underlay uniformly.
    for cell in &mut f.buffer_mut().content {
        cell.set_fg(t.muted).set_bg(t.bg);
        cell.set_style(Style::default().remove_modifier(Modifier::BOLD));
    }
    f.render_widget(Clear, area);
    match modal {
        Modal::Palette {
            query,
            selected,
            global,
        } => {
            let b = block(
                if *global {
                    " Search local records "
                } else {
                    " Actions "
                },
                t,
            )
            .border_style(Style::default().fg(t.accent))
            .title_style(Style::default().fg(t.fg).bold());
            let inner = b.inner(area);
            f.render_widget(b, area);
            let chunks = Layout::vertical([
                Constraint::Length(2),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(inner);
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    label("› ", t.accent),
                    label(input_tail(query, inner.width.saturating_sub(3)), t.fg),
                    label("▏", t.accent),
                ])),
                chunks[0],
            );
            let items: Vec<(String, String)> = if *global {
                a.search_results(query)
                    .into_iter()
                    .map(|(_, kind, label)| (label, kind))
                    .collect()
            } else {
                a.palette_results(query)
                    .into_iter()
                    .map(|action| (action.name, action.hint.into()))
                    .collect()
            };
            if items.is_empty() {
                f.render_widget(
                    Paragraph::new(vec![
                        Line::from(Span::styled(
                            if *global {
                                "No matching records"
                            } else {
                                "No matching actions"
                            },
                            Style::default().fg(t.fg).bold(),
                        )),
                        Line::from(""),
                        Line::from(label("Try a shorter search.", t.muted)),
                        Line::from(label("Ctrl+U clears the search · Esc closes", t.muted)),
                    ])
                    .wrap(Wrap { trim: false }),
                    chunks[1],
                );
            } else {
                let visible = (chunks[1].height / 2).max(1) as usize;
                let offset = selected.saturating_sub(visible.saturating_sub(1));
                for (index, (name, hint)) in items.iter().enumerate().skip(offset).take(visible) {
                    let active = index == *selected;
                    let lines = vec![
                        Line::from(vec![
                            label(if active { "› " } else { "  " }, t.fg),
                            Span::styled(
                                fit_text(name, inner.width.saturating_sub(2)),
                                Style::default().fg(t.fg).bold(),
                            ),
                        ]),
                        Line::from(label(
                            format!("  {}", fit_text(hint, inner.width.saturating_sub(2))),
                            if active { t.fg } else { t.muted },
                        )),
                    ];
                    f.render_widget(
                        Paragraph::new(lines).style(Style::default().bg(if active {
                            t.border
                        } else {
                            t.panel
                        })),
                        Rect::new(
                            chunks[1].x,
                            chunks[1].y + ((index - offset) * 2) as u16,
                            chunks[1].width,
                            2.min(chunks[1].height),
                        ),
                    );
                }
            }
            let help = if inner.width >= 62 {
                format!(
                    "↑ ↓ select · Enter run · Esc close    {} matches",
                    items.len()
                )
            } else {
                "↑ ↓ select · Enter run · Esc close".into()
            };
            f.render_widget(
                Paragraph::new(fit_text(&help, inner.width)).style(Style::default().fg(t.muted)),
                chunks[2],
            );
        }
        Modal::Form(form) => {
            let b = block(format!(" {} ", form.title), t)
                .border_style(Style::default().fg(t.accent))
                .title_style(Style::default().fg(t.fg).bold());
            let inner = b.inner(area);
            f.render_widget(b, area);
            let note = wrap_cells(&form.note, inner.width);
            let note_height = (note.len() as u16).min(if inner.height < 10 { 1 } else { 3 });
            let error_height = 2;
            let reserve = note_height + error_height + 2;
            let fields_height = inner.height.saturating_sub(reserve);
            let visible = ((fields_height + 1) / 3).max(1) as usize;
            let offset = form.active.saturating_sub(visible.saturating_sub(1));
            for (index, field) in form.fields.iter().enumerate().skip(offset).take(visible) {
                let y = inner.y + ((index - offset) * 3) as u16;
                if y + 2 > inner.y + fields_height {
                    break;
                }
                let active = index == form.active;
                let value = if field.secret {
                    "•".repeat(field.value.chars().count())
                } else {
                    crate::command::clean(&field.value)
                };
                let display = if active {
                    input_tail(&value, inner.width.saturating_sub(3))
                } else {
                    fit_text(&value, inner.width.saturating_sub(2))
                };
                let title = if form.fields.len() > visible {
                    format!("{}  ({}/{})", field.label, index + 1, form.fields.len())
                } else {
                    field.label.clone()
                };
                f.render_widget(
                    Paragraph::new(fit_text(&title, inner.width))
                        .style(Style::default().fg(if active { t.fg } else { t.muted })),
                    Rect::new(inner.x, y, inner.width, 1),
                );
                f.render_widget(
                    Paragraph::new(Line::from(vec![
                        label(if active { "› " } else { "  " }, t.fg),
                        label(display, t.fg),
                        label(if active { "▏" } else { "" }, t.accent),
                    ]))
                    .style(Style::default().bg(if active {
                        t.border
                    } else {
                        t.panel
                    })),
                    Rect::new(inner.x, y + 1, inner.width, 1),
                );
            }
            let mut y = inner.bottom().saturating_sub(reserve);
            if let Some(error) = &form.error {
                f.render_widget(
                    Paragraph::new(format!("Check this value: {error}"))
                        .style(Style::default().fg(t.bad))
                        .wrap(Wrap { trim: false }),
                    Rect::new(inner.x, y, inner.width, error_height),
                );
            }
            y += error_height;
            f.render_widget(
                Paragraph::new(
                    note.into_iter()
                        .take(note_height as usize)
                        .map(Line::from)
                        .collect::<Vec<_>>(),
                )
                .style(Style::default().fg(t.muted)),
                Rect::new(inner.x, y, inner.width, note_height),
            );
            let hint = if inner.width >= 64 {
                "Tab next · Ctrl+U clear · Enter submit · Esc cancel"
            } else {
                "Tab next · Enter submit · Esc cancel"
            };
            f.render_widget(
                Paragraph::new(fit_text(hint, inner.width)).style(Style::default().fg(t.accent)),
                Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
            );
        }
        Modal::ConfirmTool(tool) => {
            let lines = vec![
                Line::from(Span::styled(tool.title(), Style::default().fg(t.fg).bold())),
                Line::from(""),
                Line::from(tool.consent_note()),
                Line::from(""),
                Line::from(label(
                    "Consent applies to this task only. Global external access remains off.",
                    t.muted,
                )),
                Line::from(""),
                Line::from(label("Enter run this test · Esc cancel", t.accent)),
            ];
            f.render_widget(
                Paragraph::new(lines)
                    .block(block(" NETWORK REQUEST PREVIEW ", t))
                    .wrap(Wrap { trim: false }),
                area,
            );
        }
        Modal::ConfirmPlan(plan) => {
            let mut lines = vec![
                Line::from(Span::styled(
                    plan.title.clone(),
                    Style::default().fg(t.fg).bold(),
                )),
                Line::from(""),
            ];
            lines.extend(
                plan.summary
                    .iter()
                    .map(|s| Line::from(label(s.clone(), t.fg))),
            );
            if plan.disruptive {
                lines.push(Line::from(label(
                    "! This can disconnect networking, including an SSH session.",
                    t.warn,
                )));
            }
            if plan.steps.iter().any(|s| s.privileged) {
                lines.push(Line::from(label(
                    "Elevated privileges required · pkexec when running as a normal user",
                    t.warn,
                )));
            }
            lines.push(Line::from(label(
                if plan.undo.is_empty() {
                    "No undo available."
                } else {
                    "Previous values retained for a confirmed revert (u)."
                },
                t.muted,
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(label(
                "Enter apply the displayed change · Esc cancel",
                t.accent,
            )));
            f.render_widget(
                Paragraph::new(lines)
                    .block(block(" CONFIRM CONFIGURATION CHANGE ", t))
                    .wrap(Wrap { trim: false }),
                area,
            );
        }
        Modal::ExternalConsent => {
            f.render_widget(Paragraph::new(vec![Line::from(Span::styled("Enable external service access?",Style::default().fg(t.fg).bold())),Line::from(""),Line::from("Configured latency targets and DNS test names may be probed when live monitoring is on. Tools can contact their displayed endpoints without a per-test consent dialog."),Line::from(""),Line::from("Public-IP and speed tests remain explicit actions. No automatic geolocation or telemetry."),Line::from(""),Line::from(label("This setting is saved locally. e disables it.",t.muted)),Line::from(""),Line::from(label("Enter enable · Esc remain local-only",t.accent))]).block(block(" EXTERNAL ACCESS ",t)).wrap(Wrap{trim:false}),area);
        }
        Modal::Help => {
            let entries = [
                ("Navigate", ""),
                ("Ctrl+K", "Search every action and setting"),
                ("Tab / h l", "Next / previous page"),
                (", / .", "Previous / next page in this section"),
                ("1–9", "Jump to the first nine pages"),
                ("", ""),
                ("Inspect", ""),
                ("↑ ↓ / j k", "Select a row"),
                ("Home / End", "First / last row"),
                ("PageUp / Down", "Move ten rows"),
                ("Enter", "Open full details or selected test"),
                ("/ · S · s", "Filter · global search · sort"),
                ("c", "Copy the selected row"),
                ("", ""),
                ("Measure", ""),
                ("a", "Run the page's primary action"),
                ("d · b", "Diagnose · return to test catalog"),
                ("r · m", "Refresh · toggle latency monitoring"),
                ("Space · [ ]", "Freeze graphs · change time range"),
                ("p · x", "Pause collection · cancel test"),
                ("f", "Change socket filter"),
                ("", ""),
                ("Configure", ""),
                ("t · e", "Cycle theme · external access setting"),
                ("u · E", "Preview revert · export redacted report"),
                ("Esc", "Close dialog or clear the filter"),
                ("q / Ctrl+C", "Quit and restore the terminal"),
            ];
            let width = area.width.saturating_sub(4);
            let mut lines = Vec::new();
            for (key, value) in entries {
                if value.is_empty() {
                    lines.push(Line::from(Span::styled(
                        key,
                        Style::default().fg(t.accent).bold(),
                    )));
                } else {
                    lines.extend(property_lines(key, value, width, t));
                }
            }
            scroll_dialog(f, a, area, t, "Keyboard shortcuts", lines, a.scroll);
        }
        Modal::Detail {
            title,
            lines,
            scroll,
        } => {
            let width = area.width.saturating_sub(4);
            let content = lines
                .iter()
                .flat_map(|s| wrap_cells(&crate::command::clean(s), width))
                .map(|s| Line::from(label(s, t.fg)))
                .collect();
            scroll_dialog(f, a, area, t, title, content, *scroll);
        }
    }
}

fn scroll_dialog(
    f: &mut Frame,
    a: &App,
    area: Rect,
    t: Theme,
    title: &str,
    lines: Vec<Line<'static>>,
    scroll: u16,
) {
    let b = block(
        format!(" {} ", fit_text(title, area.width.saturating_sub(6))),
        t,
    )
    .border_style(Style::default().fg(t.accent))
    .title_style(Style::default().fg(t.fg).bold());
    let inner = b.inner(area);
    f.render_widget(b, area);
    let height = inner.height.saturating_sub(2);
    let max = lines
        .len()
        .saturating_sub(height as usize)
        .min(u16::MAX as usize) as u16;
    a.modal_scroll_max.set(max);
    let offset = scroll.min(max);
    f.render_widget(
        Paragraph::new(lines).scroll((offset, 0)),
        Rect::new(inner.x, inner.y, inner.width, height),
    );
    let hint = if max > 0 {
        format!(
            "↑ ↓ scroll · PgUp/PgDn · Esc close   {}/{}",
            offset + 1,
            max + 1
        )
    } else {
        "Esc close".into()
    };
    f.render_widget(
        Paragraph::new(fit_text(&hint, inner.width)).style(Style::default().fg(t.muted)),
        Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, History};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn screen(app: &App, width: u16, height: u16) -> String {
        let mut terminal =
            Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn editing_long_targets_keeps_cursor_and_secrets_safe() {
        let mut app = App::new(Config::default(), History::default());
        app.dispatch("http");
        if let Some(Modal::Form(f)) = &mut app.modal {
            f.fields[0].value = format!(
                "https://example.net/{}?last=visible",
                "long-path/".repeat(20)
            );
        }
        for width in [40, 60, 80, 140] {
            let text = screen(&app, width, 24);
            assert!(text.contains("last=visible▏"), "cursor hidden at {width}");
        }
        if let Some(Modal::Form(f)) = &mut app.modal {
            f.fields[0].value = "private-test-secret".into();
            f.fields[0].secret = true;
        }
        assert!(!screen(&app, 80, 24).contains("private-test-secret"));
    }

    #[test]
    fn invalid_port_stays_in_form_and_error_clears_on_edit() {
        let mut app = App::new(Config::default(), History::default());
        app.dispatch("tcp");
        if let Some(Modal::Form(f)) = &mut app.modal {
            f.fields[1].value = "invalid".into();
        }
        assert!(matches!(
            app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            crate::app::Effect::None
        ));
        let Some(Modal::Form(form)) = &app.modal else {
            panic!("invalid form closed")
        };
        assert_eq!(form.active, 1);
        assert_eq!(form.fields[1].value, "invalid");
        assert!(form
            .error
            .as_ref()
            .is_some_and(|e| e.contains("1 to 65535")));
        let text = screen(&app, 80, 24);
        assert!(text.contains("Check this value: Port"));
        app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        let Some(Modal::Form(form)) = &app.modal else {
            panic!("form closed")
        };
        assert!(form.error.is_none());
        assert!(form.fields[1].value.is_empty());
    }

    #[test]
    fn help_scroll_and_empty_palette_have_recovery_paths() {
        let mut app = App::new(Config::default(), History::default());
        app.modal = Some(Modal::Help);
        screen(&app, 60, 24);
        app.handle_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
        let last = app.scroll;
        assert!(last > 0);
        for _ in 0..30 {
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        assert_eq!(app.scroll, last);
        assert!(screen(&app, 60, 24).contains("Quit and restore"));
        app.handle_key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
        assert_eq!(app.scroll, 0);
        app.modal = Some(Modal::Palette {
            query: "no-such-action".into(),
            selected: 0,
            global: false,
        });
        assert!(screen(&app, 80, 24).contains("No matching actions"));
        app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        let Some(Modal::Palette { query, .. }) = &app.modal else {
            panic!("palette closed")
        };
        assert!(query.is_empty());
    }
    #[test]
    fn dashboard_background_matches_text_and_graphics_layers() {
        use crate::graphics::Mode;
        for name in [
            "dark",
            "oled",
            "catppuccin",
            "tokyo-night",
            "gruvbox",
            "light",
            "custom",
        ] {
            let mut config = Config {
                theme: name.into(),
                ..Default::default()
            };
            if name == "custom" {
                config
                    .custom_colors
                    .insert("background".into(), "#010203".into());
                config
                    .custom_colors
                    .insert("panel".into(), "#314159".into());
            }
            let mut app = App::new(config, History::default());
            app.started = std::time::Instant::now() - std::time::Duration::from_secs(1);
            for populated in [false, true] {
                if populated {
                    for n in 0..10 {
                        app.history.samples.push(crate::model::Sample {
                            at: chrono::Utc::now() - chrono::Duration::seconds((9 - n) * 2),
                            rx: 1000.0 + n as f64 * 100.0,
                            tx: 100.0,
                            latency: None,
                            loss: None,
                            dns_ms: None,
                        });
                    }
                }
                for mode in [Mode::Text, Mode::Preview] {
                    app.graphics.borrow_mut().mode = mode;
                    for (w, h) in [(140, 42), (110, 32), (80, 24), (60, 48), (40, 12)] {
                        let mut terminal =
                            Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
                        terminal.draw(|f| draw(f, &app)).unwrap();
                        let theme = Theme::from_app(&app);
                        let buffer = terminal.backend().buffer();
                        for y in 3..h - 2 {
                            for x in 0..w {
                                assert_eq!(
                                    buffer[(x, y)].bg,
                                    theme.panel,
                                    "{name} / {mode:?} / {w}x{h}: background seam at {x},{y}"
                                );
                            }
                        }
                        assert_eq!(buffer[(0, 2)].bg, theme.bg);
                        assert_eq!(buffer[(w - 1, h - 1)].bg, theme.bg);
                        let graphics = app.graphics.borrow();
                        if populated && mode == Mode::Preview && h >= 24 {
                            assert!(!graphics.requests.is_empty());
                        }
                        let rgba = match theme.panel {
                            Color::Rgb(r, g, b) => [r, g, b, 255],
                            Color::White => [255; 4],
                            _ => unreachable!(),
                        };
                        for plot in graphics.requests.values() {
                            assert_eq!(plot.theme.panel, theme.panel);
                            let pixels = plot.raster((10, 23)).unwrap();
                            assert_eq!(&pixels.data()[..4], &rgba);
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn plotted_samples_have_axes_two_traces_and_no_filled_bars() {
        let mut app = App::new(Config::default(), History::default());
        app.started = std::time::Instant::now() - std::time::Duration::from_secs(1);
        for n in 0..90 {
            app.history.samples.push(crate::model::Sample {
                at: chrono::Utc::now() - chrono::Duration::seconds((89 - n) * 2),
                rx: 200000.0 + (n as f64 * 0.2).sin() * 100000.0,
                tx: 30000.0,
                latency: None,
                loss: None,
                dns_ms: None,
            });
        }
        let mut probe = crate::model::Probe {
            target: "1.1.1.1".into(),
            ..Default::default()
        };
        for n in 0..90 {
            probe.record(if n == 70 {
                None
            } else {
                Some(10.0 + (n as f64 * 0.2).sin() * 4.0)
            });
        }
        app.probes.insert("1.1.1.1".into(), probe);
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(140, 42)).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        let text = buffer
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(
            text.contains("TRAFFIC")
                && text.contains("LATENCY")
                && text.contains("KiB/s")
                && text.contains("latest")
        );
        let traces: Vec<_> = buffer
            .content
            .iter()
            .filter(|c| {
                c.symbol()
                    .chars()
                    .any(|ch| (0x2801..=0x28ff).contains(&(ch as u32)))
            })
            .collect();
        assert!(traces.len() > 30);
        let theme = Theme::from_app(&app);
        assert!(traces.iter().any(|c| c.fg == theme.accent));
        assert!(traces.iter().any(|c| c.fg == theme.violet));
        assert!(!text.chars().any(|c| ('▁'..='█').contains(&c)));
    }
    #[test]
    fn all_pages_themes_and_terminal_sizes_render() {
        for theme in [
            "dark",
            "oled",
            "catppuccin",
            "tokyo-night",
            "gruvbox",
            "light",
        ] {
            for (w, h) in [
                (240, 80),
                (160, 48),
                (140, 42),
                (110, 32),
                (80, 24),
                (60, 48),
                (58, 16),
                (40, 12),
                (30, 10),
            ] {
                let mut app = App::new(
                    Config {
                        theme: theme.into(),
                        ..Default::default()
                    },
                    History::default(),
                );
                app.tailscale_result = Some(crate::integrations::tailnet_result(
                    crate::integrations::Tailnet {
                        state: "Running".into(),
                        self_name: "devbox.example.ts.net".into(),
                        peers: vec![crate::integrations::Peer {
                            name: "peer.example.ts.net".into(),
                            ips: vec!["100.64.0.2".into()],
                            online: Some(true),
                            path: "DERP fra".into(),
                            ..Default::default()
                        }],
                        ..Default::default()
                    },
                ));
                app.pihole_result = Some(crate::integrations::pihole_result(
                    crate::integrations::PiholeStatus {
                        endpoint: "https://pi.hole".into(),
                        blocking: "enabled".into(),
                        queries: 100,
                        blocked: 20,
                        history: vec![
                            crate::integrations::PiholePoint {
                                timestamp: 1000.0,
                                total: 20.0,
                                blocked: 4.0,
                            },
                            crate::integrations::PiholePoint {
                                timestamp: 1600.0,
                                total: 30.0,
                                blocked: 5.0,
                            },
                        ],
                        ..Default::default()
                    },
                ));
                for n in 0..20 {
                    app.history.samples.push(crate::model::Sample {
                        at: chrono::Utc::now() - chrono::Duration::seconds((19 - n) * 2),
                        rx: (n * 1000) as f64,
                        tx: 1000.0,
                        latency: None,
                        loss: None,
                        dns_ms: None,
                    });
                }
                let backend = ratatui::backend::TestBackend::new(w, h);
                let mut terminal = Terminal::new(backend).unwrap();
                for page in Page::ALL {
                    app.page = page;
                    terminal.draw(|f| draw(f, &app)).unwrap();
                    if w >= 40 && h >= 12 {
                        let theme = Theme::from_app(&app);
                        let buffer = terminal.backend().buffer();
                        for y in 3..h - 2 {
                            for x in 0..w {
                                let bg = buffer[(x, y)].bg;
                                assert!(
                                    bg == theme.panel || bg == theme.border,
                                    "{page:?} / {} / {w}x{h}: background seam at {x},{y}: {bg:?}",
                                    app.config.theme
                                );
                            }
                        }
                    }
                }
                for modal in [
                    Modal::Form(crate::app::Form {
                        id: "pihole-connect".into(),
                        title: "Connect to Pi-hole v6".into(),
                        fields: vec![
                            crate::app::Field {
                                label: "URL".into(),
                                value: "https://pi.hole".into(),
                                secret: false,
                            },
                            crate::app::Field {
                                label: "Application password".into(),
                                value: "example-secret".into(),
                                secret: true,
                            },
                        ],
                        active: 1,
                        note: "Session credentials only".into(),
                        error: None,
                    }),
                    Modal::Help,
                    Modal::ExternalConsent,
                    Modal::Palette {
                        query: "dns".into(),
                        selected: 0,
                        global: false,
                    },
                    Modal::ConfirmTool(crate::tools::Tool::PublicIp),
                ] {
                    app.modal = Some(modal);
                    terminal.draw(|f| draw(f, &app)).unwrap();
                }
            }
        }
    }
}
