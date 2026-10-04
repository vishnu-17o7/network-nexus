//! Vector paths rasterized at terminal pixel resolution, sent using Kitty graphics.
//! Text-only terminals never receive image escape codes. Only NEXUS-owned IDs are deleted.
use crate::{charts::Series, ui::Theme};
use base64::{engine::general_purpose::STANDARD, Engine};
use ratatui::{layout::Rect, style::Color};
use std::{
    collections::{hash_map::DefaultHasher, BTreeMap},
    hash::{Hash, Hasher},
    io::{self, Write},
    path::Path,
};
use tiny_skia::{FillRule, LineCap, Paint, PathBuilder, Pixmap, Stroke, Transform};

const IMAGE_BASE: u32 = 0x4e58_0000;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Text,
    Kitty,
    Preview,
}
impl Mode {
    pub fn detect(setting: &str) -> Self {
        Self::from_environment(
            setting,
            &std::env::var("TERM").unwrap_or_default(),
            &std::env::var("TERM_PROGRAM").unwrap_or_default(),
            std::env::var_os("TMUX").is_some() || std::env::var_os("STY").is_some(),
        )
    }
    fn from_environment(setting: &str, term: &str, program: &str, multiplexed: bool) -> Self {
        if setting == "text" || multiplexed {
            return Self::Text;
        }
        if setting == "kitty"
            || term == "xterm-kitty"
            || term == "xterm-ghostty"
            || program == "ghostty"
        {
            Self::Kitty
        } else {
            Self::Text
        }
    }
}

/// Cubic controls stay within adjacent measured values. No invented extrema.
#[derive(Clone, Copy, Debug)]
pub struct Cubic {
    pub start: (f64, f64),
    pub c1: (f64, f64),
    pub c2: (f64, f64),
    pub end: (f64, f64),
}
pub fn curves(points: &[(f64, f64)]) -> Vec<Cubic> {
    if points.len() < 2 {
        return Vec::new();
    }
    let slopes: Vec<_> = points
        .windows(2)
        .map(|p| (p[1].1 - p[0].1) / (p[1].0 - p[0].0))
        .collect();
    let mut tangents = vec![0.0; points.len()];
    tangents[0] = slopes[0];
    tangents[points.len() - 1] = slopes[slopes.len() - 1];
    for i in 1..points.len() - 1 {
        let (a, b) = (slopes[i - 1], slopes[i]);
        if a * b > 0.0 {
            let h0 = points[i].0 - points[i - 1].0;
            let h1 = points[i + 1].0 - points[i].0;
            let w0 = 2.0 * h1 + h0;
            let w1 = h1 + 2.0 * h0;
            tangents[i] = (w0 + w1) / (w0 / a + w1 / b);
        }
    }
    points
        .windows(2)
        .enumerate()
        .filter_map(|(i, p)| {
            let h = p[1].0 - p[0].0;
            if h <= 0.0 || !h.is_finite() {
                return None;
            }
            let low = p[0].1.min(p[1].1);
            let high = p[0].1.max(p[1].1);
            Some(Cubic {
                start: p[0],
                c1: (
                    p[0].0 + h / 3.0,
                    (p[0].1 + tangents[i] * h / 3.0).clamp(low, high),
                ),
                c2: (
                    p[1].0 - h / 3.0,
                    (p[1].1 - tangents[i + 1] * h / 3.0).clamp(low, high),
                ),
                end: p[1],
            })
        })
        .collect()
}
pub fn smooth_points(points: &[(f64, f64)], domain: f64, columns: u16) -> Vec<(f64, f64)> {
    let mut result = points.first().copied().into_iter().collect::<Vec<_>>();
    for c in curves(points) {
        let steps = (((c.end.0 - c.start.0) / domain.max(1.0) * f64::from(columns) * 2.0).ceil()
            as usize)
            .clamp(1, 1024);
        for i in 1..=steps {
            if i == steps {
                result.push(c.end);
                continue;
            }
            let t = i as f64 / steps as f64;
            let u = 1.0 - t;
            result.push((
                c.start.0 + (c.end.0 - c.start.0) * t,
                u * u * u * c.start.1
                    + 3.0 * u * u * t * c.c1.1
                    + 3.0 * u * t * t * c.c2.1
                    + t * t * t * c.end.1,
            ));
        }
    }
    result
}

#[derive(Clone)]
pub struct Plot {
    pub id: u32,
    pub area: Rect,
    pub domain: f64,
    pub maximum: f64,
    pub series: Vec<Series>,
    pub theme: Theme,
}
fn rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::White => (255, 255, 255),
        _ => (0, 0, 0),
    }
}
impl Plot {
    fn fingerprint(&self, cells: (u16, u16)) -> u64 {
        let mut h = DefaultHasher::new();
        (
            self.area.x,
            self.area.y,
            self.area.width,
            self.area.height,
            cells,
        )
            .hash(&mut h);
        self.domain.to_bits().hash(&mut h);
        self.maximum.to_bits().hash(&mut h);
        rgb(self.theme.panel).hash(&mut h);
        rgb(self.theme.border).hash(&mut h);
        for s in &self.series {
            rgb(s.color).hash(&mut h);
            for segment in &s.segments {
                segment.len().hash(&mut h);
                for (x, y) in segment {
                    x.to_bits().hash(&mut h);
                    y.to_bits().hash(&mut h);
                }
            }
        }
        h.finish()
    }
    fn dimensions(&self, cells: (u16, u16)) -> (u32, u32) {
        // Bound memory and work on unusually large displays.
        (
            u32::from(self.area.width)
                .saturating_mul(u32::from(cells.0))
                .clamp(8, 2400),
            u32::from(self.area.height)
                .saturating_mul(u32::from(cells.1))
                .clamp(8, 1400),
        )
    }
    pub fn raster(&self, cells: (u16, u16)) -> Option<Pixmap> {
        let (w, h) = self.dimensions(cells);
        let mut pix = Pixmap::new(w, h)?;
        let (r, g, b) = rgb(self.theme.panel);
        pix.fill(tiny_skia::Color::from_rgba8(r, g, b, 255));
        let map = |(x, y): (f64, f64)| {
            (
                3.0 + (x / self.domain.max(1.0) * (w - 6) as f64) as f32,
                (h - 3) as f32 - (y / self.maximum.max(1.0) * (h - 6) as f64) as f32,
            )
        };
        let mut grid = PathBuilder::new();
        for i in 0..=4 {
            let y = 3.0 + (h - 6) as f32 * i as f32 / 4.0;
            grid.move_to(0.0, y);
            grid.line_to(w as f32, y);
        }
        let (r, g, b) = rgb(self.theme.border);
        let mut paint = Paint::default();
        paint.set_color_rgba8(r, g, b, 100);
        pix.stroke_path(
            &grid.finish()?,
            &paint,
            &Stroke {
                width: 0.8,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
        // Shade every segment first so later fills cannot obscure another trace.
        for s in &self.series {
            let (r, g, b) = rgb(crate::dither::shade(s.color, self.theme.panel));
            paint.set_color_rgba8(r, g, b, 255);
            paint.anti_alias = false;
            let mut dots = PathBuilder::new();
            for segment in &s.segments {
                for p in crate::dither::points(
                    segment,
                    self.domain,
                    self.maximum,
                    (w / 4) as u16,
                    (h / 4) as u16,
                ) {
                    let (x, y) = map(p);
                    if let Some(rect) = tiny_skia::Rect::from_xywh(x.round(), y.round(), 1.6, 1.6) {
                        dots.push_rect(rect);
                    }
                }
            }
            if let Some(path) = dots.finish() {
                pix.fill_path(
                    &path,
                    &paint,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
        paint.anti_alias = true;
        for s in &self.series {
            let (r, g, b) = rgb(s.color);
            for points in &s.segments {
                let Some(first) = points.first() else {
                    continue;
                };
                let (x, y) = map(*first);
                if points.len() == 1 {
                    paint.set_color_rgba8(r, g, b, 255);
                    let dot = PathBuilder::from_circle(x, y, 2.5)?;
                    pix.fill_path(&dot, &paint, FillRule::Winding, Transform::identity(), None);
                    continue;
                }
                let mut path = PathBuilder::new();
                path.move_to(x, y);
                for c in curves(points) {
                    let (x1, y1) = map(c.c1);
                    let (x2, y2) = map(c.c2);
                    let (x3, y3) = map(c.end);
                    path.cubic_to(x1, y1, x2, y2, x3, y3);
                }
                let line = path.finish()?;
                paint.set_color_rgba8(r, g, b, 255);
                pix.stroke_path(
                    &line,
                    &paint,
                    &Stroke {
                        width: 2.0,
                        line_cap: LineCap::Round,
                        ..Default::default()
                    },
                    Transform::identity(),
                    None,
                );
            }
            if let Some(last) = s.segments.last().and_then(|p| p.last()) {
                let (x, y) = map(*last);
                paint.set_color_rgba8(r, g, b, 255);
                pix.fill_path(
                    &PathBuilder::from_circle(x, y, 3.0)?,
                    &paint,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
        Some(pix)
    }
    pub fn svg(&self, cells: (u16, u16)) -> String {
        use std::fmt::Write;
        let (w, h) = self.dimensions(cells);
        let (r, g, b) = rgb(self.theme.panel);
        let mut out=format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\"><rect width=\"100%\" height=\"100%\" fill=\"rgb({r},{g},{b})\"/>");
        let map = |(x, y): (f64, f64)| {
            (
                3.0 + x / self.domain.max(1.0) * (w - 6) as f64,
                (h - 3) as f64 - y / self.maximum.max(1.0) * (h - 6) as f64,
            )
        };
        for s in &self.series {
            let (r, g, b) = rgb(crate::dither::shade(s.color, self.theme.panel));
            let mut d = String::new();
            for segment in &s.segments {
                for p in crate::dither::points(
                    segment,
                    self.domain,
                    self.maximum,
                    (w / 4) as u16,
                    (h / 4) as u16,
                ) {
                    let (x, y) = map(p);
                    let _ = write!(d, "M{x:.0},{y:.0}h1.6v1.6h-1.6z");
                }
            }
            let _ = write!(out, "<path d=\"{d}\" fill=\"rgb({r},{g},{b})\"/>");
        }
        for s in &self.series {
            let (r, g, b) = rgb(s.color);
            for points in &s.segments {
                let Some(first) = points.first() else {
                    continue;
                };
                let (x, y) = map(*first);
                let mut d = format!("M{x:.3},{y:.3}");
                for c in curves(points) {
                    let (a, b) = map(c.c1);
                    let (e, f) = map(c.c2);
                    let (g, h) = map(c.end);
                    let _ = write!(d, " C{a:.3},{b:.3} {e:.3},{f:.3} {g:.3},{h:.3}");
                }
                let _=write!(out,"<path d=\"{d}\" stroke=\"rgb({r},{g},{b})\" stroke-width=\"2\" stroke-linecap=\"round\" fill=\"none\"/>");
                if points.len() == 1 {
                    let _ = write!(
                        out,
                        "<circle cx=\"{x:.3}\" cy=\"{y:.3}\" r=\"2.5\" fill=\"rgb({r},{g},{b})\"/>"
                    );
                }
            }
        }
        out.push_str("</svg>");
        out
    }
}

#[derive(Default)]
pub struct Graphics {
    pub mode: Mode,
    pub requests: BTreeMap<u32, Plot>,
    shown: BTreeMap<u32, u64>,
    pub cell_size: (u16, u16),
}
impl Graphics {
    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
            cell_size: (10, 23),
            ..Default::default()
        }
    }
    pub fn begin_frame(&mut self) {
        self.requests.clear();
    }
    pub fn enabled(&self) -> bool {
        self.mode != Mode::Text
    }
    pub fn request(&mut self, plot: Plot) {
        self.requests.insert(plot.id, plot);
    }
    pub fn set_cell_size(&mut self, width: u16, height: u16, columns: u16, rows: u16) {
        if columns > 0 && rows > 0 && width >= columns && height >= rows {
            self.cell_size = ((width / columns).clamp(4, 48), (height / rows).clamp(8, 96));
        }
    }
    pub fn flush(&mut self, out: &mut impl Write) -> io::Result<()> {
        let stale: Vec<_> = self
            .shown
            .keys()
            .filter(|id| !self.requests.contains_key(id) || self.mode != Mode::Kitty)
            .copied()
            .collect();
        for id in stale {
            delete_image(out, id)?;
            self.shown.remove(&id);
        }
        if self.mode != Mode::Kitty {
            return Ok(());
        }
        for (&id, plot) in &self.requests {
            let hash = plot.fingerprint(self.cell_size);
            if self.shown.get(&id) == Some(&hash) {
                continue;
            }
            let Some(pix) = plot.raster(self.cell_size) else {
                continue;
            };
            let png = pix.encode_png().map_err(io::Error::other)?;
            transmit(out, id, plot.area, &png)?;
            self.shown.insert(id, hash);
        }
        out.flush()
    }
    pub fn preview_assets(&self, buffer: &Path) -> anyhow::Result<Vec<serde_json::Value>> {
        let stem = buffer
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("preview");
        let dir = buffer.parent().unwrap_or(Path::new("."));
        let mut images = Vec::new();
        for (&id, plot) in &self.requests {
            let name = format!("{stem}-chart-{id}.png");
            if let Some(pix) = plot.raster((10, 23)) {
                crate::config::private_atomic(&dir.join(&name), &pix.encode_png()?)?;
                crate::config::private_atomic(
                    &dir.join(format!("{stem}-chart-{id}.svg")),
                    plot.svg((10, 23)).as_bytes(),
                )?;
                images.push(serde_json::json!({"path":name,"x":plot.area.x,"y":plot.area.y,"width":plot.area.width,"height":plot.area.height}));
            }
        }
        Ok(images)
    }
}
fn delete_image(out: &mut impl Write, id: u32) -> io::Result<()> {
    write!(out, "\x1b_Ga=d,d=I,i={},q=2;\x1b\\", IMAGE_BASE + id)
}
pub fn cleanup(out: &mut impl Write) -> io::Result<()> {
    for id in 1..=4 {
        delete_image(out, id)?;
    }
    out.flush()
}
fn transmit(out: &mut impl Write, id: u32, area: Rect, png: &[u8]) -> io::Result<()> {
    let encoded = STANDARD.encode(png);
    let chunks: Vec<_> = encoded.as_bytes().chunks(4096).collect();
    write!(out, "\x1b7\x1b[{};{}H", area.y + 1, area.x + 1)?;
    for (i, chunk) in chunks.iter().enumerate() {
        if i == 0 {
            write!(
                out,
                "\x1b_Ga=T,f=100,t=d,i={},p=1,c={},r={},C=1,z=1,q=2,m={};",
                IMAGE_BASE + id,
                area.width,
                area.height,
                u8::from(chunks.len() > 1)
            )?;
        } else {
            write!(out, "\x1b_Gm={},q=2;", u8::from(i + 1 < chunks.len()))?;
        }
        out.write_all(chunk)?;
        out.write_all(b"\x1b\\")?;
    }
    out.write_all(b"\x1b8")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn curves_preserve_samples_and_do_not_overshoot_spikes() {
        let points = [
            (0., 3.),
            (1., 3.),
            (2., 90.),
            (2.1, 4.),
            (9., 5.),
            (10., 0.),
        ];
        let dense = smooth_points(&points, 10., 200);
        for p in points {
            assert!(dense.contains(&p));
        }
        for c in curves(&points) {
            let lo = c.start.1.min(c.end.1);
            let hi = c.start.1.max(c.end.1);
            assert!((lo..=hi).contains(&c.c1.1));
            assert!((lo..=hi).contains(&c.c2.1));
        }
    }
    #[test]
    fn auto_detection_falls_back_in_multiplexers_and_unknown_terminals() {
        assert_eq!(
            Mode::from_environment("auto", "xterm-kitty", "", false),
            Mode::Kitty
        );
        assert_eq!(
            Mode::from_environment("auto", "xterm-256color", "", false),
            Mode::Text
        );
        assert_eq!(
            Mode::from_environment("auto", "xterm-kitty", "", true),
            Mode::Text
        );
        assert_eq!(
            Mode::from_environment("text", "xterm-kitty", "", false),
            Mode::Text
        );
    }
    #[test]
    fn unchanged_frames_are_cached_and_hidden_plots_are_removed() {
        let a = crate::app::App::new(
            crate::config::Config::default(),
            crate::config::History::default(),
        );
        let t = Theme::from_app(&a);
        let mut renderer = Graphics::new(Mode::Kitty);
        renderer.request(Plot {
            id: 1,
            area: Rect::new(2, 3, 20, 8),
            domain: 2.0,
            maximum: 10.0,
            series: vec![Series {
                name: "RTT".into(),
                color: t.violet,
                segments: vec![vec![(0.0, 2.0), (1.0, 8.0), (2.0, 3.0)]],
            }],
            theme: t,
        });
        let mut wire = Vec::new();
        renderer.flush(&mut wire).unwrap();
        assert!(String::from_utf8_lossy(&wire).contains("a=T,f=100"));
        wire.clear();
        renderer.flush(&mut wire).unwrap();
        assert!(wire.is_empty());
        renderer.begin_frame();
        renderer.flush(&mut wire).unwrap();
        assert_eq!(
            String::from_utf8(wire).unwrap(),
            format!("\x1b_Ga=d,d=I,i={},q=2;\x1b\\", IMAGE_BASE + 1)
        );
    }
    #[test]
    fn image_transport_chunks_preserve_payload_and_cursor() {
        let data = vec![17; 8000];
        let mut out = Vec::new();
        transmit(&mut out, 1, Rect::new(3, 4, 50, 10), &data).unwrap();
        let wire = String::from_utf8(out).unwrap();
        assert!(wire.starts_with("\x1b7\x1b[5;4H"));
        assert!(wire.ends_with("\x1b8"));
        let mut payload = String::new();
        for part in wire.split("\x1b_G").skip(1) {
            let (header, body) = part.split_once(';').unwrap();
            assert!(header.contains("q=2"));
            let chunk = body.split("\x1b\\").next().unwrap();
            assert!(chunk.len() <= 4096);
            payload.push_str(chunk);
        }
        assert_eq!(STANDARD.decode(payload).unwrap(), data);
    }
}
