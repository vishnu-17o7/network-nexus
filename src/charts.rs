//! Time series rendered at terminal sub-cell resolution; no invented interpolation.
use crate::{app::App, ui::Theme};
use ratatui::{prelude::*, widgets::*};

pub struct Series {
    pub name: String,
    pub color: Color,
    pub segments: Vec<Vec<(f64, f64)>>,
}
/// Missing / invalid samples break a line rather than implying a successful reply.
pub fn segments(values: impl IntoIterator<Item = (f64, Option<f64>)>) -> Vec<Vec<(f64, f64)>> {
    let mut out = Vec::new();
    let mut current = Vec::new();
    for (x, y) in values {
        if let Some(y) = y
            .filter(|v| v.is_finite() && *v >= 0.0)
            .filter(|_| x.is_finite())
        {
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
fn plot(
    f: &mut Frame,
    area: Rect,
    t: Theme,
    title: &str,
    unit: &str,
    domain: (f64, bool),
    series: &[Series],
) {
    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(t.fg).bold()))
        .borders(Borders::TOP)
        .border_style(Style::default().fg(t.border))
        .padding(Padding::new(1, 1, 1, 0))
        .style(Style::default().bg(t.panel));
    if series.iter().all(|s| s.segments.is_empty()) {
        f.render_widget(
            Paragraph::new("No measurements yet · start a test or monitor")
                .style(Style::default().fg(t.muted))
                .wrap(Wrap { trim: false })
                .block(block),
            area,
        );
        return;
    }
    let max = series
        .iter()
        .flat_map(|s| &s.segments)
        .flatten()
        .map(|(_, y)| *y)
        .fold(1.0, f64::max)
        * 1.12;
    let mut datasets = Vec::new();
    for s in series {
        for (i, points) in s.segments.iter().enumerate() {
            let mut d = Dataset::default()
                .marker(symbols::Marker::Braille)
                .graph_type(if points.len() > 1 {
                    GraphType::Line
                } else {
                    GraphType::Scatter
                })
                .style(Style::default().fg(s.color))
                .data(points);
            if i == 0 {
                d = d.name(s.name.as_str());
            }
            datasets.push(d);
        }
    }
    let (window, time) = domain;
    let duration = window.max(1.0);
    let age = |seconds: f64| {
        if seconds >= 3600.0 {
            format!("-{:.1}h", seconds / 3600.0)
        } else if seconds >= 60.0 {
            format!("-{:.0}m", seconds / 60.0)
        } else {
            format!("-{seconds:.0}s")
        }
    };
    let x_labels = if time {
        vec![age(duration), age(duration / 2.0), "latest".into()]
    } else {
        vec![
            "1".into(),
            format!("{:.0}", duration / 2.0 + 1.0),
            format!("{:.0}", duration + 1.0),
        ]
    };
    let number = |v: f64| {
        if v >= 100.0 {
            format!("{v:.0}")
        } else {
            format!("{v:.1}")
        }
    };
    let chart = Chart::new(datasets)
        .block(block)
        .x_axis(
            Axis::default()
                .style(Style::default().fg(t.border))
                .bounds([0.0, duration])
                .labels(
                    x_labels
                        .into_iter()
                        .map(|s| Span::styled(s, Style::default().fg(t.muted)))
                        .collect::<Vec<_>>(),
                ),
        )
        .y_axis(
            Axis::default()
                .title(Span::styled(
                    format!(" {unit}"),
                    Style::default().fg(t.muted),
                ))
                .style(Style::default().fg(t.border))
                .bounds([0.0, max])
                .labels(
                    ["0".into(), number(max / 2.0), number(max)]
                        .into_iter()
                        .map(|s| Span::styled(s, Style::default().fg(t.muted)))
                        .collect::<Vec<_>>(),
                ),
        )
        .legend_position(None);
    f.render_widget(chart, area);
}
pub fn traffic(f: &mut Frame, a: &App, area: Rect, t: Theme) {
    let samples: Vec<_> = a.history.samples.iter().rev().take(180).rev().collect();
    let origin = samples.first().map(|s| s.at);
    let x = |s: &crate::model::Sample| {
        origin
            .map(|at| (s.at - at).num_milliseconds().max(0) as f64 / 1000.0)
            .unwrap_or(0.0)
    };
    let peak = samples
        .iter()
        .map(|s| s.rx.max(s.tx))
        .filter(|v| v.is_finite())
        .fold(0.0, f64::max);
    let (scale, unit) = if peak >= 1e9 {
        (1e9, "GB/s")
    } else if peak >= 1e6 {
        (1e6, "MB/s")
    } else if peak >= 1e3 {
        (1e3, "KB/s")
    } else {
        (1.0, "B/s")
    };
    let series = [
        Series {
            name: "Download".into(),
            color: t.accent,
            segments: segments(samples.iter().map(|s| (x(s), Some(s.rx / scale)))),
        },
        Series {
            name: "Upload".into(),
            color: t.violet,
            segments: segments(samples.iter().map(|s| (x(s), Some(s.tx / scale)))),
        },
    ];
    let title = format!(
        " TRAFFIC · {}",
        a.snapshot.primary.as_deref().unwrap_or("all links")
    );
    plot(
        f,
        area,
        t,
        &title,
        unit,
        (samples.last().map(|s| x(s)).unwrap_or(1.0), true),
        &series,
    );
    legend(f, area, t, "↓ download", "↑ upload");
}
pub fn latency(f: &mut Frame, a: &App, area: Rect, t: Theme) {
    let probe = a.internet_probe();
    let values: Vec<_> = probe
        .map(|p| p.history.iter().rev().take(90).rev().copied().collect())
        .unwrap_or_default();
    let interval = 1.0;
    let series = [Series {
        name: "Latency".into(),
        color: t.violet,
        segments: segments(
            values
                .iter()
                .enumerate()
                .map(|(i, v)| (i as f64 * interval, *v)),
        ),
    }];
    let title = format!(
        " LATENCY · {} · probe # · gaps = no reply",
        probe.map(|p| p.target.as_str()).unwrap_or("not monitoring")
    );
    plot(
        f,
        area,
        t,
        &title,
        "ms",
        (values.len().saturating_sub(1) as f64 * interval, false),
        &series,
    );
}
pub fn speed(f: &mut Frame, a: &App, area: Rect, t: Theme) {
    let tests: Vec<_> = a.history.tests.iter().take(30).rev().collect();
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
            name: "Download".into(),
            color: t.accent,
            segments: points("Download Mbps"),
        },
        Series {
            name: "Upload".into(),
            color: t.violet,
            segments: points("Upload Mbps"),
        },
    ];
    plot(
        f,
        area,
        t,
        " SPEED TESTS · compare matching backends",
        "Mbps",
        (tests.len().saturating_sub(1) as f64, false),
        &series,
    );
    legend(f, area, t, "↓ download", "↑ upload");
}
fn legend(f: &mut Frame, area: Rect, t: Theme, first: &str, second: &str) {
    if area.width < 42 || area.height < 5 {
        return;
    }
    let width = 28.min(area.width);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(first, Style::default().fg(t.accent)),
            Span::raw("   "),
            Span::styled(second, Style::default().fg(t.violet)),
        ])),
        Rect::new(area.right() - width, area.y + 1, width, 1),
    );
}

pub fn pihole(f: &mut Frame, a: &App, area: Rect, t: Theme) {
    let points = a
        .pihole_result
        .as_ref()
        .and_then(|r| r.pihole.as_ref())
        .map(|p| p.history.as_slice())
        .unwrap_or(&[]);
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
        area,
        t,
        " PI-HOLE ACTIVITY · API history buckets",
        "queries",
        (
            points.last().map(|p| p.timestamp - start).unwrap_or(1.0),
            true,
        ),
        &series,
    );
    legend(f, area, t, "all queries", "blocked");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_values_and_nonfinite_values_break_lines() {
        let result = segments([
            (0.0, Some(4.0)),
            (1.0, Some(5.0)),
            (2.0, None),
            (3.0, Some(7.0)),
            (4.0, Some(f64::NAN)),
            (5.0, Some(-1.0)),
            (6.0, Some(8.0)),
        ]);
        assert_eq!(
            result,
            vec![
                vec![(0.0, 4.0), (1.0, 5.0)],
                vec![(3.0, 7.0)],
                vec![(6.0, 8.0)]
            ]
        );
    }
}
