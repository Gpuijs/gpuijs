//! `background-image` gradient layers that the element's own `Fill` can't draw: painted under its
//! content, bottom layer first, each sized, placed and tiled over the box as CSS does, with px
//! stops and repeating gradients resolved against the tile's gradient line.

use gpui::{
    Background, Bounds, ContentMask, Corners, Hsla, LinearColorStop, Pixels, Size, Window,
    conic_gradient, fill, linear_color_stop, linear_gradient_stops, point, px, radial_gradient,
    size,
};

use crate::css::props::{BgLayer, BgShape, Lp, RadialSize, fill_positions};

/// Tiles past this many are not drawn (a 4px pattern over a whole page).
// shortcut: very fine patterns over big boxes are skipped; draw them as one textured quad if needed.
const MAX_TILES: usize = 4096;

fn resolve(l: Lp, of: f32) -> f32 {
    match l {
        Lp::Px(p) => p,
        Lp::Fraction(f) => f * of,
    }
}

/// Tile origins along one axis of length `room`: one at `at`, or every `tile` through the box.
fn tiles(room: f32, tile: f32, at: Lp, repeat: bool) -> Vec<f32> {
    let first = match at {
        Lp::Px(p) => p,
        Lp::Fraction(f) => (room - tile) * f,
    };
    if !repeat {
        return vec![first];
    }
    let start = first - (first / tile).ceil() * tile;
    let count = ((room - start) / tile).ceil().max(0.0) as usize;
    (0..count.min(MAX_TILES + 1)).map(|i| start + i as f32 * tile).collect()
}

/// The stops as gpui takes them: positions as fractions of `line` px, and a repeating gradient's
/// period. Fully transparent stops take their neighbors' color, so straight-alpha interpolation
/// fades the way premultiplied CSS does instead of through gray.
fn stops(layer: &BgLayer, line: f32) -> (Vec<LinearColorStop>, f32) {
    let mut at: Vec<Option<f32>> = layer
        .stops
        .iter()
        .map(|s| {
            s.1.map(|p| match p {
                Lp::Px(p) => p / line.max(1e-3),
                Lp::Fraction(f) => f,
            })
        })
        .collect();
    if at.len() == 1 {
        at.push(None);
    }
    fill_positions(&mut at);
    let color = |i: usize| layer.stops[i.min(layer.stops.len() - 1)].0;
    let mut out = Vec::new();
    for (i, at) in at.iter().enumerate() {
        let (c, at) = (color(i), at.unwrap_or(0.0));
        if c.a > 0.0 {
            out.push(linear_color_stop(c, at));
            continue;
        }
        let clear = |n: Hsla| Hsla { a: 0.0, ..n };
        if i > 0 {
            out.push(linear_color_stop(clear(color(i - 1)), at));
        }
        if i + 1 < layer.stops.len() {
            out.push(linear_color_stop(clear(color(i + 1)), at));
        }
        if out.is_empty() {
            out.push(linear_color_stop(c, at));
        }
    }
    let (first, last) = (out[0].percentage, out[out.len() - 1].percentage);
    let period = last - first;
    if !layer.repeating || period <= 1e-4 {
        return (out, 0.0);
    }
    // shortcut: the pattern starts at the first stop's position, taken as 0.
    for s in &mut out {
        s.percentage = (s.percentage - first) / period;
    }
    (out, period)
}

/// The quad to paint over a `tile` and its gradient (the tile clips it).
pub fn quad(layer: &BgLayer, tile: Bounds<Pixels>) -> (Bounds<Pixels>, Background) {
    let (w, h) = (f32::from(tile.size.width), f32::from(tile.size.height));
    let center = |c: [Lp; 2]| (resolve(c[0], w), resolve(c[1], h));
    let around = |(cx, cy): (f32, f32), (hx, hy): (f32, f32)| {
        Bounds::new(tile.origin + point(px(cx - hx), px(cy - hy)), size(px(2.0 * hx), px(2.0 * hy)))
    };
    match layer.shape {
        BgShape::Linear(angle) => {
            let a = angle.to_radians();
            let line = (w * a.sin()).abs() + (h * a.cos()).abs();
            let (stops, period) = stops(layer, line);
            (tile, linear_gradient_stops(angle, &stops).repeating(period).color_space(layer.space))
        }
        BgShape::Radial { circle, center: c, size: extent } => {
            let (cx, cy) = center(c);
            let (sx, sy) = ((cx).min(w - cx), (cy).min(h - cy));
            let (fx, fy) = ((cx).max(w - cx), (cy).max(h - cy));
            let (rx, ry) = match (extent, circle) {
                (RadialSize::Radii([x, y]), false) => (resolve(x, w), resolve(y, h)),
                (RadialSize::Radii([r, _]), true) => (resolve(r, w), resolve(r, w)),
                (RadialSize::ClosestSide, false) => (sx, sy),
                (RadialSize::FarthestSide, false) => (fx, fy),
                (RadialSize::ClosestCorner, false) => (sx * 2f32.sqrt(), sy * 2f32.sqrt()),
                (RadialSize::FarthestCorner, false) => (fx * 2f32.sqrt(), fy * 2f32.sqrt()),
                (RadialSize::ClosestSide, true) => (sx.min(sy), sx.min(sy)),
                (RadialSize::FarthestSide, true) => (fx.max(fy), fx.max(fy)),
                (RadialSize::ClosestCorner, true) => (sx.hypot(sy), sx.hypot(sy)),
                (RadialSize::FarthestCorner, true) => (fx.hypot(fy), fx.hypot(fy)),
            };
            let (rx, ry) = (rx.max(1e-3), ry.max(1e-3));
            let (stops, period) = stops(layer, rx);
            // gpui's radial gradient reaches t = 1 at its quad's half size × √2 (an ellipse) or at
            // the length of the half size (a circle): a quad that puts that on the radii.
            let half = (rx / 2f32.sqrt(), ry / 2f32.sqrt());
            (around((cx, cy), half), radial_gradient(circle, &stops).repeating(period).color_space(layer.space))
        }
        BgShape::Conic { from, center: c } => {
            let (cx, cy) = center(c);
            let half = (cx.max(w - cx), cy.max(h - cy));
            let (stops, period) = stops(layer, 1.0);
            (around((cx, cy), half), conic_gradient(from, &stops).repeating(period).color_space(layer.space))
        }
    }
}

/// Paints `layers` (top first) over `bounds`; a layer filling it once takes its corner radii.
// shortcut: tiles of a smaller layer ignore the corner radii; the box clips them square.
pub fn paint(layers: &[BgLayer], bounds: Bounds<Pixels>, radii: Corners<Pixels>, window: &mut Window) {
    let Size { width, height } = bounds.size;
    let (w, h) = (f32::from(width), f32::from(height));
    window.with_content_mask(Some(ContentMask { bounds, ..Default::default() }), |window| {
        for layer in layers.iter().rev() {
            let tw = layer.size[0].map_or(w, |s| resolve(s, w));
            let th = layer.size[1].map_or(h, |s| resolve(s, h));
            if tw < 0.5 || th < 0.5 {
                continue;
            }
            let xs = tiles(w, tw, layer.at[0], layer.repeat[0]);
            let ys = tiles(h, th, layer.at[1], layer.repeat[1]);
            if xs.len() * ys.len() > MAX_TILES {
                crate::css::warn_once("background patterns of more than 4096 tiles (not drawn)");
                continue;
            }
            let whole = xs.len() == 1 && ys.len() == 1 && xs[0] == 0.0 && ys[0] == 0.0 && tw == w && th == h;
            for &x in &xs {
                for &y in &ys {
                    let tile = Bounds::new(bounds.origin + point(px(x), px(y)), size(px(tw), px(th)));
                    let (at, background) = quad(layer, tile);
                    window.with_content_mask(Some(ContentMask { bounds: tile, ..Default::default() }), |window| {
                        let q = fill(at, background);
                        window.paint_quad(if whole && at == tile { q.corner_radii(radii) } else { q });
                    });
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_cover_the_box_from_the_position() {
        assert_eq!(tiles(100.0, 30.0, Lp::Px(0.0), false), vec![0.0]);
        assert_eq!(tiles(100.0, 30.0, Lp::Fraction(1.0), false), vec![70.0]);
        assert_eq!(tiles(100.0, 30.0, Lp::Px(10.0), true), vec![-20.0, 10.0, 40.0, 70.0]);
        assert_eq!(tiles(60.0, 30.0, Lp::Px(0.0), true), vec![0.0, 30.0]);
    }

    fn layer(shape: BgShape, stops: Vec<(Hsla, Option<Lp>)>, repeating: bool) -> BgLayer {
        BgLayer { shape, stops, repeating, size: [None, None], at: [Lp::Px(0.0); 2], repeat: [true; 2], space: gpui::ColorSpace::Srgb }
    }

    #[test]
    fn px_stops_and_repeats_resolve_against_the_line() {
        let (black, white) = (gpui::black(), gpui::white());
        let stripes = layer(
            BgShape::Linear(90.0),
            vec![(black, Some(Lp::Px(0.0))), (black, Some(Lp::Px(5.0))), (white, Some(Lp::Px(5.0))), (white, Some(Lp::Px(10.0)))],
            true,
        );
        let (s, period) = stops(&stripes, 100.0);
        assert!((period - 0.1).abs() < 1e-6, "{period}");
        let at: Vec<f32> = s.iter().map(|s| s.percentage).collect();
        assert_eq!(at, vec![0.0, 0.5, 0.5, 1.0]);
    }

    #[test]
    fn a_transparent_stop_fades_the_neighbor_color() {
        let blue = gpui::blue();
        let dot = layer(BgShape::Linear(0.0), vec![(blue, Some(Lp::Px(2.0))), (gpui::transparent_black(), Some(Lp::Px(3.0)))], false);
        let (s, _) = stops(&dot, 10.0);
        assert_eq!(s.len(), 2);
        assert_eq!(s[1].color, Hsla { a: 0.0, ..blue });
        assert!((s[1].percentage - 0.3).abs() < 1e-6);
    }

    #[test]
    fn a_radial_gradient_off_center_reaches_the_farthest_corner() {
        let tile = Bounds::new(point(px(0.0), px(0.0)), size(px(100.0), px(50.0)));
        let l = layer(
            BgShape::Radial { circle: true, center: [Lp::Px(0.0), Lp::Px(0.0)], size: RadialSize::FarthestCorner },
            vec![(gpui::black(), None), (gpui::white(), None)],
            false,
        );
        let (at, _) = quad(&l, tile);
        // A circle about the top left corner reaching the bottom right one: gpui puts t = 1 at
        // the length of the half size.
        let half = f32::from(at.size.width) / 2.0;
        assert!((half * 2f32.sqrt() - 100f32.hypot(50.0)).abs() < 1e-3);
        assert_eq!(at.center(), point(px(0.0), px(0.0)));
    }
}
