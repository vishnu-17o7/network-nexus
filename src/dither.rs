//! Stable ordered shading under measured traces. No noise, gap bridging or invented extrema.
use ratatui::style::Color;

const BAYER: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

pub fn shade(foreground: Color, background: Color) -> Color {
    fn rgb(c: Color) -> (u8, u8, u8) {
        match c {
            Color::Rgb(r, g, b) => (r, g, b),
            Color::White => (255, 255, 255),
            _ => (0, 0, 0),
        }
    }
    let (r, g, b) = rgb(foreground);
    let (br, bg, bb) = rgb(background);
    let mix = |a: u8, b: u8| ((u16::from(a) * 45 + u16::from(b) * 55) / 100) as u8;
    Color::Rgb(mix(r, br), mix(g, bg), mix(b, bb))
}

/// Returns data-space dots on a fixed grid, clipped separately to one continuous segment.
/// At most 600 x 350 candidates: bounded even on an unusually large terminal.
pub fn points(
    segment: &[(f64, f64)],
    domain: f64,
    maximum: f64,
    width: u16,
    height: u16,
) -> Vec<(f64, f64)> {
    if segment.len() < 2
        || !domain.is_finite()
        || domain <= 0.0
        || !maximum.is_finite()
        || maximum <= 0.0
        || width < 2
        || height < 2
    {
        return Vec::new();
    }
    let width = width.min(600);
    let height = height.min(350);
    let curve = crate::graphics::smooth_points(segment, domain, width);
    let mut result = Vec::new();
    let mut index = 0;
    for gx in 0..width {
        let x = f64::from(gx) * domain / f64::from(width - 1);
        if x < curve[0].0 || x > curve[curve.len() - 1].0 {
            continue;
        }
        while index + 1 < curve.len() - 1 && curve[index + 1].0 < x {
            index += 1;
        }
        let (x0, y0) = curve[index];
        let (x1, y1) = curve[index + 1];
        if x1 <= x0 {
            continue;
        }
        let top = (y0 + (y1 - y0) * (x - x0) / (x1 - x0)).clamp(0.0, maximum);
        for gy in 1..height {
            let y = f64::from(gy) * maximum / f64::from(height - 1);
            if y >= top {
                break;
            }
            // A restrained ramp: sparse at baseline, denser just below the trace.
            let threshold = (2.0 + 6.0 * y / top) as u8;
            if BAYER[(height - 1 - gy) as usize % 4][gx as usize % 4] < threshold {
                result.push((x, y));
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shading_is_stable_clipped_and_does_not_bridge_missing_samples() {
        let left = vec![(1.0, 4.0), (3.0, 8.0)];
        let right = vec![(7.0, 3.0), (9.0, 6.0)];
        let a = points(&left, 10.0, 10.0, 100, 60);
        assert!(!a.is_empty());
        assert_eq!(a, points(&left, 10.0, 10.0, 100, 60));
        assert!(a
            .iter()
            .all(|(x, y)| *x >= 1.0 && *x <= 3.0 && *y > 0.0 && *y < 8.0));
        let b = points(&right, 10.0, 10.0, 100, 60);
        assert!(a.iter().chain(&b).all(|(x, _)| *x <= 3.0 || *x >= 7.0));
        assert!(points(&[(2.0, 5.0)], 10.0, 10.0, 100, 60).is_empty());
    }
    #[test]
    fn zero_and_invalid_ranges_have_no_fill() {
        assert!(points(&[(0.0, 0.0), (1.0, 0.0)], 1.0, 10.0, 100, 60).is_empty());
        assert!(points(&[(0.0, 1.0), (1.0, 2.0)], 0.0, 10.0, 100, 60).is_empty());
        assert!(points(&[(0.0, 1.0), (1.0, 2.0)], 1.0, f64::NAN, 100, 60).is_empty());
    }
}
