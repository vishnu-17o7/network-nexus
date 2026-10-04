//! Honest time series: shape-preserving curves, visible gaps, unchanged measurements.
use crate::{
    app::{App, Page},
    graphics::{self, Plot},
    model::{Probe, Sample, ToolResult},
    ui::Theme,
};
use ratatui::{
    prelude::*,
    widgets::{
        canvas::{Canvas, Line as CanvasLine, Points},
        *,
    },
};

#[derive(Clone)]
pub struct Series {
    pub name: String,
    pub color: Color,
    pub segments: Vec<Vec<(f64, f64)>>,
}
#[derive(Clone)]
pub struct Snapshot {
    pub samples: Vec<Sample>,
    pub probe: Option<Probe>,
    pub probes: std::collections::BTreeMap<String, Probe>,
    pub tests: Vec<ToolResult>,
    pub pihole: Vec<crate::integrations::PiholePoint>,
    pub primary: String,
}
impl Snapshot {
    pub fn capture(a: &App) -> Self {
        Self {
            samples: a.history.samples.clone(),
            probe: a.internet_probe().cloned(),
            probes: a.probes.clone(),
            tests: a.history.tests.clone(),
            pihole: a
                .pihole_result
                .as_ref()
                .and_then(|r| r.pihole.as_ref())
                .map(|p| p.history.clone())
                .unwrap_or_default(),
            primary: a
                .snapshot
                .primary
                .clone()
                .unwrap_or_else(|| "all links".into()),
        }
    }
}
pub const WINDOWS: [u64; 3] = [60, 300, 900];
pub const PROBE_WINDOWS: [usize; 3] = [30, 90, 300];
pub fn range_label(a: &App) -> String {
    format!(
        "{}m / {} probes",
        WINDOWS[a.chart_window] / 60,
        PROBE_WINDOWS[a.chart_window]
    )
}

/// Missing, invalid or non-increasing coordinates break the line.
pub fn segments(values: impl IntoIterator<Item = (f64, Option<f64>)>) -> Vec<Vec<(f64, f64)>> {
    let mut out = Vec::new();
    let mut current: Vec<(f64, f64)> = Vec::new();
    for (x, y) in values {
        if let Some(y) = y
            .filter(|v| v.is_finite() && *v >= 0.0)
            .filter(|_| x.is_finite())
        {
            if current.last().is_some_and(|p| x <= p.0) {
                out.push(std::mem::take(&mut current));
            }
            current.push((x, y));
        } else if !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}
/// A rounded, stepped scale prevents the axis from changing for every small fluctuation.
pub fn nice_max(value: f64) -> f64 {
    let v = (value * 1.04).max(1.0);
    if !v.is_finite() {
        return value.max(1.0);
    }
    let step = 10_f64.powf(v.log10().floor());
    [1.0, 1.25, 1.5, 2.0, 2.5, 5.0, 7.5, 10.0]
        .into_iter()
        .find(|n| (*n * step).is_finite() && *n * step >= v)
        .map(|n| n * step)
        .unwrap_or(value.max(1.0))
}
fn number(v: f64) -> String {
    if v == 0.0 {
        "0".into()
    } else if v >= 100.0 {
        format!("{v:.0}")
    } else if v >= 10.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.2}")
    }
}
fn age(v: f64) -> String {
    if v >= 3600.0 {
        format!("-{:.1}h", v / 3600.0)
    } else if v >= 60.0 {
        let seconds = v.round() as u64;
        if seconds % 60 == 0 {
            format!("-{}m", seconds / 60)
        } else {
            format!("-{}m{:02}s", seconds / 60, seconds % 60)
        }
    } else {
        format!("-{v:.0}s")
    }
}
struct Chart<'a> {
    id: u32,
    title: &'a str,
    unit: &'a str,
    domain: f64,
    time: bool,
    first_probe: u64,
    summary: String,
    empty: &'a str,
}
fn plot(f: &mut Frame, a: &App, area: Rect, t: Theme, c: Chart<'_>, series: &[Series]) {
    if area.width < 12 || area.height < 4 {
        return;
    }
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(t.border))
        .title(Span::styled(
            format!(" {} ", c.title),
            Style::default().fg(t.fg).bold(),
        ))
        .style(Style::default().bg(t.panel))
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 3 {
        return;
    }
    let mut summary = Vec::new();
    for s in series {
        summary.push(Span::styled(
            format!("{}  ", s.name),
            Style::default().fg(s.color),
        ));
    }
    if inner.width >= 48 {
        summary.push(Span::styled(c.summary, Style::default().fg(t.muted)));
    }
    f.render_widget(
        Paragraph::new(Line::from(summary)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    if series.iter().all(|s| s.segments.is_empty()) {
        f.render_widget(
            Paragraph::new(c.empty)
                .wrap(Wrap { trim: false })
                .style(Style::default().fg(t.muted)),
            Rect::new(
                inner.x + 1,
                inner.y + 2,
                inner.width.saturating_sub(2),
                inner.height.saturating_sub(2),
            ),
        );
        return;
    }
    let maximum = nice_max(
        series
            .iter()
            .flat_map(|s| s.segments.iter().flatten())
            .map(|p| p.1)
            .fold(0.0, f64::max),
    );
    let gutter = if inner.width >= 40 { 7 } else { 5 };
    let graph = Rect::new(
        inner.x + gutter,
        inner.y + 2,
        inner.width.saturating_sub(gutter + 1),
        inner.height.saturating_sub(4),
    );
    if graph.width < 3 || graph.height < 2 {
        return;
    }
    for (value, y) in [
        (maximum, graph.y),
        (maximum / 2.0, graph.y + graph.height / 2),
        (0.0, graph.bottom() - 1),
    ] {
        f.render_widget(
            Paragraph::new(number(value))
                .alignment(Alignment::Right)
                .style(Style::default().fg(t.muted)),
            Rect::new(inner.x, y, gutter - 1, 1),
        );
    }
    f.render_widget(
        Paragraph::new(c.unit).style(Style::default().fg(t.muted)),
        Rect::new(inner.x, inner.y + 1, gutter, 1),
    );
    let domain = c.domain.max(1.0);
    let labels = if c.time {
        [
            age(domain),
            age(domain / 2.0),
            if a.chart_snapshot.is_some() {
                "paused".into()
            } else {
                "latest".into()
            },
        ]
    } else {
        [
            c.first_probe.to_string(),
            (c.first_probe + domain as u64 / 2).to_string(),
            (c.first_probe + domain as u64).to_string(),
        ]
    };
    for (i, text) in labels.into_iter().enumerate() {
        let width = (text.len() as u16).min(graph.width);
        let x = match i {
            0 => graph.x,
            1 => graph.x + graph.width / 2 - width / 2,
            _ => graph.right() - width,
        };
        f.render_widget(
            Paragraph::new(text).style(Style::default().fg(t.muted)),
            Rect::new(x, graph.bottom() + 1, width, 1),
        );
    }
    let use_graphics = a.modal.is_none() && a.graphics.borrow().enabled();
    if use_graphics {
        a.graphics.borrow_mut().request(Plot {
            id: c.id,
            area: graph,
            domain,
            maximum,
            series: series.to_vec(),
            theme: t,
        });
    } else {
        // Curves remain portable over SSH, Linux consoles and multiplexers.
        let curves: Vec<_> = series
            .iter()
            .flat_map(|s| {
                s.segments.iter().map(move |points| {
                    (
                        s.color,
                        graphics::smooth_points(points, domain, graph.width),
                    )
                })
            })
            .collect();
        f.render_widget(
            Canvas::default()
                .background_color(t.panel)
                .marker(symbols::Marker::Braille)
                .x_bounds([0.0, domain])
                .y_bounds([0.0, maximum])
                .paint(|ctx| {
                    for (color, points) in &curves {
                        let dots = crate::dither::points(
                            points,
                            domain,
                            maximum,
                            graph.width.saturating_mul(2),
                            graph.height.saturating_mul(4),
                        );
                        ctx.draw(&Points {
                            coords: &dots,
                            color: crate::dither::shade(*color, t.panel),
                        });
                    }
                    ctx.layer();
                    for (color, points) in &curves {
                        for p in points.windows(2) {
                            ctx.draw(&CanvasLine {
                                x1: p[0].0,
                                y1: p[0].1,
                                x2: p[1].0,
                                y2: p[1].1,
                                color: *color,
                            });
                        }
                        if points.len() == 1 {
                            ctx.draw(&Points {
                                coords: points,
                                color: *color,
                            });
                        }
                    }
                }),
            graph,
        );
    }
}
fn traffic_values(a: &App) -> &[Sample] {
    a.chart_snapshot
        .as_ref()
        .map(|s| s.samples.as_slice())
        .unwrap_or(&a.history.samples)
}
fn traffic_series(
    samples: &[&Sample],
    start: chrono::DateTime<chrono::Utc>,
    scale: f64,
    rx: bool,
    gap: f64,
) -> Vec<Vec<(f64, f64)>> {
    let mut values = Vec::new();
    let mut previous = None;
    for s in samples {
        let x = (s.at - start).num_milliseconds() as f64 / 1000.0;
        if previous.is_some_and(|p| x - p > gap) {
            values.push((x, None));
        }
        values.push((x, Some(if rx { s.rx } else { s.tx } / scale)));
        previous = Some(x);
    }
    segments(values)
}
pub fn traffic(f: &mut Frame, a: &App, area: Rect, t: Theme) {
    let all = traffic_values(a);
    let duration = WINDOWS[a.chart_window] as f64;
    let end = all.last().map(|s| s.at).unwrap_or_else(chrono::Utc::now);
    let start = end - chrono::Duration::seconds(duration as i64);
    let samples: Vec<_> = all
        .iter()
        .filter(|s| s.at >= start && s.at <= end)
        .collect();
    let peak = samples
        .iter()
        .map(|s| s.rx.max(s.tx))
        .filter(|v| v.is_finite())
        .fold(0.0, f64::max);
    let (scale, unit) = if peak >= 1024_f64.powi(3) {
        (1024_f64.powi(3), "GiB/s")
    } else if peak >= 1024_f64.powi(2) {
        (1024_f64.powi(2), "MiB/s")
    } else if peak >= 1024.0 {
        (1024.0, "KiB/s")
    } else {
        (1.0, "B/s")
    };
    let gap = (a.config.refresh_seconds * 3).max(6) as f64;
    let series = [
        Series {
            name: "↓ Download".into(),
            color: t.accent,
            segments: traffic_series(&samples, start, scale, true, gap),
        },
        Series {
            name: "↑ Upload".into(),
            color: t.violet,
            segments: traffic_series(&samples, start, scale, false, gap),
        },
    ];
    let primary = a
        .chart_snapshot
        .as_ref()
        .map(|s| s.primary.as_str())
        .unwrap_or_else(|| a.snapshot.primary.as_deref().unwrap_or("all links"));
    plot(
        f,
        a,
        area,
        t,
        Chart {
            id: 1,
            title: &format!("TRAFFIC  ·  {primary}  ·  {}m", duration as u64 / 60),
            unit,
            domain: duration,
            time: true,
            first_probe: 0,
            summary: format!("peak {}", crate::model::rate(peak)),
            empty: "Collecting local traffic · the next sample arrives automatically",
        },
        &series,
    );
}
pub fn latency(f: &mut Frame, a: &App, area: Rect, t: Theme) {
    let target = if a.page == Page::Latency {
        let (_, rows) = a.rows();
        rows.get(a.selected.min(rows.len().saturating_sub(1)))
            .and_then(|r| r.first())
            .cloned()
    } else {
        None
    };
    let probe = if a.page == Page::Latency {
        let probes = a
            .chart_snapshot
            .as_ref()
            .map(|s| &s.probes)
            .unwrap_or(&a.probes);
        target.as_ref().and_then(|target| probes.get(target))
    } else if let Some(s) = &a.chart_snapshot {
        s.probe.as_ref()
    } else {
        a.internet_probe()
    };
    let values: Vec<_> = probe
        .map(|p| {
            p.history
                .iter()
                .rev()
                .take(PROBE_WINDOWS[a.chart_window])
                .rev()
                .copied()
                .collect()
        })
        .unwrap_or_default();
    let valid: Vec<_> = values
        .iter()
        .flatten()
        .copied()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .collect();
    let mut sorted = valid.clone();
    sorted.sort_by(f64::total_cmp);
    let summary = if sorted.is_empty() {
        "gaps = no reply".into()
    } else {
        let p95 = sorted[((sorted.len() as f64 * 0.95).ceil() as usize).saturating_sub(1)];
        let loss = 100.0 * (values.len() - valid.len()) as f64 / values.len() as f64;
        format!("p95 {p95:.1}ms · loss {loss:.0}%")
    };
    let series = [Series {
        name: "● RTT".into(),
        color: t.violet,
        segments: segments(values.iter().enumerate().map(|(i, v)| (i as f64, *v))),
    }];
    let first = probe
        .map(|p| p.sent.saturating_sub(values.len() as u64) + 1)
        .unwrap_or(1);
    plot(
        f,
        a,
        area,
        t,
        Chart {
            id: 2,
            title: &format!(
                "LATENCY  ·  {}",
                probe.map(|p| p.target.as_str()).unwrap_or("not monitoring")
            ),
            unit: "ms",
            domain: values.len().saturating_sub(1) as f64,
            time: false,
            first_probe: first,
            summary,
            empty: if values.is_empty() {
                "No probes yet · m starts monitoring\nInternet targets need external access (e)"
            } else {
                "No successful replies in this range · gaps remain visible"
            },
        },
        &series,
    );
}
pub fn speed(f: &mut Frame, a: &App, area: Rect, t: Theme) {
    let history = a
        .chart_snapshot
        .as_ref()
        .map(|s| s.tests.as_slice())
        .unwrap_or(&a.history.tests);
    let tests: Vec<_> = history.iter().take(30).rev().collect();
    let points = |metric: &str| {
        segments(tests.iter().enumerate().map(|(i, test)| {
            (
                i as f64,
                test.metrics.get(metric).and_then(|v| v.parse().ok()),
            )
        }))
    };
    let series = [
        Series {
            name: "↓ Download".into(),
            color: t.accent,
            segments: points("Download Mbps"),
        },
        Series {
            name: "↑ Upload".into(),
            color: t.violet,
            segments: points("Upload Mbps"),
        },
    ];
    plot(
        f,
        a,
        area,
        t,
        Chart {
            id: 3,
            title: "SPEED HISTORY",
            unit: "Mbps",
            domain: tests.len().saturating_sub(1) as f64,
            time: false,
            first_probe: 1,
            summary: "compare matching backends".into(),
            empty: "No speed tests yet · Ctrl+K → Internet speed test",
        },
        &series,
    );
}
pub fn pihole(f: &mut Frame, a: &App, area: Rect, t: Theme) {
    let points = a
        .chart_snapshot
        .as_ref()
        .map(|s| s.pihole.as_slice())
        .unwrap_or_else(|| {
            a.pihole_result
                .as_ref()
                .and_then(|r| r.pihole.as_ref())
                .map(|p| p.history.as_slice())
                .unwrap_or(&[])
        });
    let start = points.first().map(|p| p.timestamp).unwrap_or(0.0);
    let series = [
        Series {
            name: "All queries".into(),
            color: t.accent,
            segments: segments(points.iter().map(|p| (p.timestamp - start, Some(p.total)))),
        },
        Series {
            name: "Blocked".into(),
            color: t.violet,
            segments: segments(
                points
                    .iter()
                    .map(|p| (p.timestamp - start, Some(p.blocked))),
            ),
        },
    ];
    plot(
        f,
        a,
        area,
        t,
        Chart {
            id: 4,
            title: "DNS ACTIVITY",
            unit: "queries",
            domain: points.last().map(|p| p.timestamp - start).unwrap_or(1.0),
            time: true,
            first_probe: 0,
            summary: "API history buckets".into(),
            empty: "Connect to Pi-hole v6 to load query history",
        },
        &series,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_values_and_nonfinite_values_break_lines() {
        assert_eq!(
            segments([
                (0., Some(4.)),
                (1., Some(5.)),
                (2., None),
                (3., Some(7.)),
                (4., Some(f64::NAN)),
                (5., Some(-1.)),
                (6., Some(8.))
            ]),
            vec![vec![(0., 4.), (1., 5.)], vec![(3., 7.)], vec![(6., 8.)]]
        );
    }
    #[test]
    fn duplicate_or_reversed_timestamps_do_not_create_infinite_slopes() {
        let lines = segments([
            (0., Some(2.)),
            (0., Some(3.)),
            (-1., Some(4.)),
            (1., Some(5.)),
        ]);
        assert_eq!(lines.len(), 3);
        assert!(lines
            .iter()
            .flat_map(|p| graphics::curves(p))
            .all(|c| c.c1.1.is_finite()));
    }
    #[test]
    fn axis_is_stable_and_never_clips_the_peak() {
        assert_eq!(nice_max(72.), nice_max(72.1));
        for v in [0., 0.01, 9., 85., 270., 9000.] {
            assert!(nice_max(v) >= v);
        }
    }
    #[test]
    fn pauses_between_samples_are_not_drawn_as_continuous_traffic() {
        let now = chrono::Utc::now();
        let sample = |seconds| Sample {
            at: now + chrono::Duration::seconds(seconds),
            rx: 100.0,
            tx: 20.0,
            latency: None,
            loss: None,
            dns_ms: None,
        };
        let data = [sample(0), sample(2), sample(120), sample(122)];
        let refs = data.iter().collect::<Vec<_>>();
        let lines = traffic_series(&refs, now, 1.0, true, 6.0);
        assert_eq!(
            lines,
            vec![
                vec![(0.0, 100.0), (2.0, 100.0)],
                vec![(120.0, 100.0), (122.0, 100.0)]
            ]
        );
        assert_eq!(age(150.0), "-2m30s");
    }
    #[test]
    fn frozen_graphs_leave_collection_and_history_running() {
        let mut a = App::new(
            crate::config::Config::default(),
            crate::config::History::default(),
        );
        a.update_snapshot(crate::model::Snapshot::default());
        a.dispatch("chart-pause");
        let captured = a.chart_snapshot.as_ref().unwrap().samples.len();
        a.update_snapshot(crate::model::Snapshot::default());
        assert_eq!(a.chart_snapshot.as_ref().unwrap().samples.len(), captured);
        assert_eq!(a.history.samples.len(), captured + 1);
        assert!(!a.paused);
        a.dispatch("chart-pause");
        assert!(a.chart_snapshot.is_none());
    }
    #[test]
    fn latency_selection_uses_the_selected_frozen_target() {
        let mut a = App::new(
            crate::config::Config::default(),
            crate::config::History::default(),
        );
        a.page = Page::Latency;
        a.graphics.borrow_mut().mode = graphics::Mode::Preview;
        for (target, value) in [("192.0.2.1", 2.0), ("1.1.1.1", 20.0)] {
            let mut probe = Probe {
                target: target.into(),
                ..Default::default()
            };
            probe.record(Some(value));
            probe.record(Some(value));
            a.probes.insert(target.into(), probe);
        }
        a.dispatch("chart-pause");
        a.probes.get_mut("192.0.2.1").unwrap().record(Some(200.0));
        a.filter = "192.0.2.1".into();
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(140, 42)).unwrap();
        terminal.draw(|f| crate::ui::draw(f, &a)).unwrap();
        let graphics = a.graphics.borrow();
        let plot = graphics.requests.get(&2).unwrap();
        assert!(plot.series[0]
            .segments
            .iter()
            .flatten()
            .all(|(_, y)| *y == 2.0));
    }
}
