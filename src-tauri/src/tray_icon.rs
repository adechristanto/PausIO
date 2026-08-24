//! A small, mostly-transparent white eye glyph for the menu-bar/tray icon.
//!
//! The tray previously used the full-color app icon directly as the icon, marked
//! as a macOS "template" image (`icon_as_template(true)`). Template mode
//! discards color entirely and uses only the alpha channel as a silhouette —
//! and that icon's backdrop is roughly 77% opaque, so the result was a solid
//! black blob (the "black dot" reported in the menu bar), not a recognizable
//! mark.
//!
//! This renders a proper glyph instead: an eye outline with a pupil, matching
//! the motif already used for the dashboard's eye-break button icon
//! (`M2 12s3.64-7 10-7 10 7 10 7-3.64 7-10 7S2 12 2 12z` plus a center pupil
//! circle), with a mostly-transparent background and anti-aliased edges.
//!
//! The ink is fixed white on every platform and macOS template mode is
//! deliberately off: the icon must read as PausIO's own mark, not follow the
//! system theme's recoloring.
//!
//! Deliberately a single static shape, not a per-state animation: an earlier
//! attempt at procedurally-rendered, per-state tray icons was tried and
//! reverted (see git history) because cycling shapes read as an unbranded
//! dot rather than a recognizable mark. One legible glyph, always the same,
//! is the right trade-off here.

use tauri::image::Image;

/// Rendered at a higher resolution than any platform's menu bar actually
/// needs (macOS fixes status-item icons at 18pt regardless of buffer size)
/// purely so downscaling produces crisp edges on HiDPI displays.
const SIZE: u32 = 44;

/// Signed distance to an almond/eye "lens" shape formed by the intersection
/// of two circles centered above and below the icon's center — negative
/// inside the lens, positive outside. Coordinates are normalized so the
/// lens roughly spans x in [-0.9, 0.9] and y in [-0.43, 0.43].
fn lens_distance(x: f32, y: f32) -> f32 {
    let top = (x * x + (y + 0.62) * (y + 0.62)).sqrt() - 1.05;
    let bottom = (x * x + (y - 0.62) * (y - 0.62)).sqrt() - 1.05;
    top.max(bottom)
}

/// Analytic ~1px anti-aliasing: full coverage well inside a shape, zero well
/// outside, a one-pixel-wide linear falloff across the edge. `signed_distance`
/// is in the same normalized units as `lens_distance`/pupil radius, so it is
/// converted to pixels via `scale` (the same factor used to build x/y) before
/// the falloff is applied — otherwise the 0.5 threshold spans a fraction of
/// the whole shape instead of a single pixel, blurring everything into a
/// soft blob.
fn coverage(signed_distance: f32, scale: f32) -> f32 {
    (0.5 - signed_distance * scale).clamp(0.0, 1.0)
}

/// Renders the tray glyph: white ink at varying alpha, fully transparent
/// background. White is the product's default tray color on every platform;
/// macOS template mode is intentionally not used, so the system theme never
/// recolors the mark.
pub fn render() -> Image<'static> {
    const STROKE_HALF_WIDTH: f32 = 0.115;
    const PUPIL_RADIUS: f32 = 0.30;
    let half = SIZE as f32 / 2.0;
    // Scale factor so the lens (which spans roughly ±0.9 in x) sits inside
    // the canvas with a small margin rather than touching the edges.
    let scale = half * 0.8;

    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for row in 0..SIZE {
        for col in 0..SIZE {
            let x = ((col as f32 + 0.5) - half) / scale;
            let y = ((row as f32 + 0.5) - half) / scale;
            let lens = lens_distance(x, y);
            let outline = coverage(lens.abs() - STROKE_HALF_WIDTH, scale);
            let pupil = coverage((x * x + y * y).sqrt() - PUPIL_RADIUS, scale);
            let alpha = (outline + pupil).clamp(0.0, 1.0);
            rgba.extend_from_slice(&[255, 255, 255, (alpha * 255.0).round() as u8]);
        }
    }
    Image::new_owned(rgba, SIZE, SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha_at(image: &Image<'static>, x: u32, y: u32) -> u8 {
        let row = image
            .rgba()
            .chunks_exact(4)
            .nth((y * SIZE + x) as usize)
            .unwrap();
        row[3]
    }

    #[test]
    fn renders_the_declared_size() {
        let image = render();
        assert_eq!(image.width(), SIZE);
        assert_eq!(image.height(), SIZE);
        assert_eq!(image.rgba().len(), (SIZE * SIZE * 4) as usize);
    }

    #[test]
    fn corners_are_fully_transparent() {
        let image = render();
        for &(x, y) in &[(0, 0), (SIZE - 1, 0), (0, SIZE - 1), (SIZE - 1, SIZE - 1)] {
            assert_eq!(
                alpha_at(&image, x, y),
                0,
                "corner ({x}, {y}) should be transparent"
            );
        }
    }

    #[test]
    fn center_pupil_is_opaque() {
        let image = render();
        // SIZE is even, so no pixel center lands exactly on the geometric
        // center — near-full rather than exactly 255 is expected here.
        assert!(alpha_at(&image, SIZE / 2, SIZE / 2) > 240);
    }

    #[test]
    fn most_of_the_canvas_stays_transparent() {
        // The previous full-color icon used as a template was ~77% opaque —
        // this asserts the new glyph is the opposite: mostly see-through.
        let image = render();
        let opaque = image
            .rgba()
            .chunks_exact(4)
            .filter(|pixel| pixel[3] > 200)
            .count();
        let total = (SIZE * SIZE) as usize;
        assert!(
            opaque * 100 < total * 40,
            "expected under 40% opaque coverage, got {}%",
            opaque * 100 / total
        );
    }

    #[test]
    fn ink_is_always_pure_white() {
        // The tray glyph is fixed white by product decision: it must not
        // follow the system theme, so every non-transparent pixel is white.
        let image = render();
        for pixel in image.rgba().chunks_exact(4) {
            assert_eq!((pixel[0], pixel[1], pixel[2]), (255, 255, 255));
        }
    }

    #[test]
    fn edges_show_partial_alpha_proving_anti_aliasing() {
        let image = render();
        let has_partial_alpha = image
            .rgba()
            .chunks_exact(4)
            .any(|pixel| pixel[3] > 0 && pixel[3] < 255);
        assert!(
            has_partial_alpha,
            "expected at least one anti-aliased edge pixel"
        );
    }
}
