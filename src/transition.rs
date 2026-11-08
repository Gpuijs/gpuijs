//! CSS transitions and keyframe animations for the properties gpuijs can interpolate: colors,
//! box-shadow, opacity and the transform properties (translate, rotate, scale).

use std::time::Duration;

use gpui::{
    AbsoluteLength, BoxShadow, DefiniteLength, Fill, Hsla, Length, Pixels, Point, Rgba,
    StyleRefinement, point,
};

#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    /// CSS property names, or `all`.
    pub properties: Vec<String>,
    pub duration: f32,
    pub delay: f32,
    pub easing: Easing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    CubicBezier(f32, f32, f32, f32),
    Steps(u32),
}

impl Transition {
    /// Eased progress in `0.0..=1.0` after `elapsed`.
    pub fn progress(&self, elapsed: Duration) -> f32 {
        let x = ((elapsed.as_secs_f32() - self.delay) / self.duration).clamp(0.0, 1.0);
        self.easing.apply(x)
    }

    pub fn is_done(&self, elapsed: Duration) -> bool {
        elapsed.as_secs_f32() >= self.delay + self.duration
    }

    fn animates(&self, property: &str) -> bool {
        self.properties.iter().any(|p| p == "all" || p == property)
    }
}

impl Easing {
    pub fn apply(self, x: f32) -> f32 {
        match self {
            Easing::CubicBezier(x1, y1, x2, y2) => cubic_bezier(x1, y1, x2, y2, x),
            Easing::Steps(n) => (x * n as f32).floor() / n as f32,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Direction {
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

/// A running `@keyframes` animation, its keyframes already resolved for the element.
#[derive(Clone, Debug, PartialEq)]
pub struct Animation {
    pub name: String,
    /// Offsets in `0.0..=1.0`, sorted; each look holds only what that keyframe sets.
    pub frames: Vec<(f32, Look)>,
    pub duration: f32,
    pub delay: f32,
    pub easing: Easing,
    /// `f32::INFINITY` for `infinite`.
    pub iterations: f32,
    pub direction: Direction,
    pub fill_backwards: bool,
    pub fill_forwards: bool,
}

impl Animation {
    /// What the animation shows `elapsed` seconds after it started, over the element's own `base`
    /// look; `None` when it has no effect then (before or after it runs, without fill).
    pub fn sample(&self, base: &Look, elapsed: f32) -> Option<Look> {
        let t = elapsed - self.delay;
        let (iteration, progress) = if t < 0.0 {
            if !self.fill_backwards {
                return None;
            }
            (0.0, 0.0)
        } else if self.duration <= 0.0 || t >= self.duration * self.iterations {
            if !self.fill_forwards {
                return None;
            }
            // Ends where the last (possibly partial) iteration ends.
            let last = (self.iterations - 1.0).max(0.0).ceil();
            let end = self.iterations - last;
            (last, if end > 0.0 { end } else { 1.0 })
        } else {
            let x = t / self.duration;
            (x.floor(), x.fract())
        };
        let odd = iteration as u64 % 2 == 1;
        let reversed = match self.direction {
            Direction::Normal => false,
            Direction::Reverse => true,
            Direction::Alternate => odd,
            Direction::AlternateReverse => !odd,
        };
        let p = if reversed { 1.0 - progress } else { progress };
        Some(self.at(base, p))
    }

    pub fn is_done(&self, elapsed: f32) -> bool {
        elapsed >= self.delay + self.duration * self.iterations
    }

    /// The look at progress `p` within one iteration; missing 0%/100% keyframes are the base.
    fn at(&self, base: &Look, p: f32) -> Look {
        let mut frames: Vec<(f32, Look)> = Vec::with_capacity(self.frames.len() + 2);
        if self.frames.first().is_none_or(|f| f.0 > 0.0) {
            frames.push((0.0, base.clone()));
        }
        frames.extend(self.frames.iter().map(|(o, l)| (*o, base.under(l))));
        if self.frames.last().is_none_or(|f| f.0 < 1.0) {
            frames.push((1.0, base.clone()));
        }
        let next = frames
            .iter()
            .position(|f| f.0 >= p)
            .unwrap_or(frames.len() - 1);
        if next == 0 {
            return frames[0].1.clone();
        }
        let (a, b) = (&frames[next - 1], &frames[next]);
        let local = if b.0 > a.0 {
            (p - a.0) / (b.0 - a.0)
        } else {
            1.0
        };
        // As in CSS, the timing function applies within each keyframe interval.
        Look::mix(&a.1, &b.1, self.easing.apply(local), &everything())
    }
}

/// A transition over every property, for interpolating keyframes.
fn everything() -> Transition {
    Transition {
        properties: vec!["all".into()],
        duration: 1.0,
        delay: 0.0,
        easing: Easing::CubicBezier(0.0, 0.0, 1.0, 1.0),
    }
}

/// `y` for `x` on the CSS cubic-bezier curve from (0,0) to (1,1).
fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    if x <= 0.0 || x >= 1.0 {
        return x.clamp(0.0, 1.0);
    }
    let at = |t: f32, a: f32, b: f32| {
        3.0 * (1.0 - t) * (1.0 - t) * t * a + 3.0 * (1.0 - t) * t * t * b + t * t * t
    };
    // x(t) is monotonic for valid easing curves, so bisection always converges.
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..30 {
        let mid = (lo + hi) / 2.0;
        if at(mid, x1, x2) < x {
            lo = mid
        } else {
            hi = mid
        }
    }
    at((lo + hi) / 2.0, y1, y2)
}

/// The values a transition interpolates, read from and written back to a style.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Look {
    bg: Option<Hsla>,
    color: Option<Hsla>,
    border: Option<Hsla>,
    shadows: Vec<BoxShadow>,
    opacity: Option<f32>,
    translate: Option<Point<DefiniteLength>>,
    rotate: Option<f32>,
    scale: Option<Point<f32>>,
    /// Layout lengths in px (the sidebar's `transition-[left,right,width]`); others jump.
    width: Option<Pixels>,
    height: Option<Pixels>,
    /// top, right, bottom, left
    inset: [Option<Pixels>; 4],
}

fn px_of(l: Option<Length>) -> Option<Pixels> {
    match l {
        Some(Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(p)))) => Some(p),
        _ => None,
    }
}

impl Look {
    pub fn of(s: &StyleRefinement) -> Self {
        Look {
            // A gradient counts as its average color: a transition away from one (MUI's buttons
            // drop theirs on hover) starts from about what was on screen, not from transparent.
            bg: s.background.as_ref().and_then(|Fill::Color(b)| {
                b.solid_color().or_else(|| {
                    let [from, to] = b.gradient_stops()?;
                    mix_color(Some(from.color), Some(to.color), 0.5)
                })
            }),
            color: s.text.as_ref().and_then(|t| t.color),
            border: s.border_color,
            shadows: s.box_shadow.clone().unwrap_or_default(),
            opacity: s.opacity,
            translate: s.translate,
            rotate: s.rotate,
            scale: s.scale,
            width: px_of(s.size.width),
            height: px_of(s.size.height),
            inset: [s.inset.top, s.inset.right, s.inset.bottom, s.inset.left].map(px_of),
        }
    }

    /// This look with the element's computed color where its own is unset — `of` reads `None`
    /// for an inherited `color` — so a transition interpolates between two real colors instead
    /// of fading through fully transparent, which reads as the text blinking out on hover.
    pub fn resolved(mut self, color: Hsla) -> Self {
        if self.color.is_none() {
            self.color = Some(color);
        }
        self
    }

    /// Whether going from this look to `to` changes a property `tr` animates. Others take their new
    /// value at once and leave a running transition alone, as in CSS: react-spring writing an
    /// element's opacity every frame mustn't restart its height transition (MUI's Collapse).
    pub fn moves(&self, to: &Look, tr: &Transition) -> bool {
        let on = |names: &[&str]| names.iter().any(|&n| tr.animates(n));
        let transform = |name| on(&[name, "transform"]);
        (on(&["background-color"]) && self.bg != to.bg)
            || (on(&["color"]) && self.color != to.color)
            || (on(&["border-color"]) && self.border != to.border)
            || (on(&["box-shadow"]) && self.shadows != to.shadows)
            || (on(&["opacity"]) && self.opacity != to.opacity)
            || (transform("translate") && self.translate != to.translate)
            || (transform("rotate") && self.rotate != to.rotate)
            || (transform("scale") && self.scale != to.scale)
            || (on(&["width"]) && self.width != to.width)
            || (on(&["height"]) && self.height != to.height)
            || ["top", "right", "bottom", "left"]
                .iter()
                .enumerate()
                .any(|(i, &side)| on(&[side, "inset"]) && self.inset[i] != to.inset[i])
    }

    /// This look with whatever `over` sets taking precedence (a keyframe over the element's own).
    pub fn under(&self, over: &Look) -> Look {
        Look {
            bg: over.bg.or(self.bg),
            color: over.color.or(self.color),
            border: over.border.or(self.border),
            shadows: match over.shadows.is_empty() {
                true => self.shadows.clone(),
                false => over.shadows.clone(),
            },
            opacity: over.opacity.or(self.opacity),
            translate: over.translate.or(self.translate),
            rotate: over.rotate.or(self.rotate),
            scale: over.scale.or(self.scale),
            width: over.width.or(self.width),
            height: over.height.or(self.height),
            inset: [0, 1, 2, 3].map(|i| over.inset[i].or(self.inset[i])),
        }
    }

    /// `from` moving towards `to` at `t`; properties the transition does not cover are already at `to`.
    pub fn mix(from: &Look, to: &Look, t: f32, tr: &Transition) -> Look {
        let length = |on: bool, a: Option<Pixels>, b: Option<Pixels>| match (on, a, b) {
            (true, Some(a), Some(b)) => Some(a + (b - a) * t),
            _ => b,
        };
        let color = |name: &str, a: Option<Hsla>, b: Option<Hsla>| match tr.animates(name) {
            true => mix_color(a, b, t),
            false => b,
        };
        let n = from.shadows.len().max(to.shadows.len());
        let shadow = |list: &[BoxShadow], other: &[BoxShadow], i: usize| {
            // A missing shadow animates from/to nothing: no size, same color fully transparent.
            list.get(i).cloned().unwrap_or_else(|| BoxShadow {
                color: other[i].color.opacity(0.0),
                offset: Default::default(),
                blur_radius: Default::default(),
                spread_radius: Default::default(),
            })
        };
        Look {
            bg: color("background-color", from.bg, to.bg),
            color: color("color", from.color, to.color),
            border: color("border-color", from.border, to.border),
            shadows: match tr.animates("box-shadow") {
                true => (0..n)
                    .map(|i| {
                        let (a, b) = (
                            shadow(&from.shadows, &to.shadows, i),
                            shadow(&to.shadows, &from.shadows, i),
                        );
                        BoxShadow {
                            color: mix_color(Some(a.color), Some(b.color), t).unwrap_or(b.color),
                            offset: a.offset + (b.offset - a.offset) * t,
                            blur_radius: a.blur_radius + (b.blur_radius - a.blur_radius) * t,
                            spread_radius: a.spread_radius
                                + (b.spread_radius - a.spread_radius) * t,
                        }
                    })
                    .collect(),
                false => to.shadows.clone(),
            },
            opacity: match (tr.animates("opacity"), from.opacity, to.opacity) {
                (true, a, b) if a.is_some() || b.is_some() => {
                    let (a, b) = (a.unwrap_or(1.0), b.unwrap_or(1.0));
                    Some(a + (b - a) * t)
                }
                _ => to.opacity,
            },
            translate: match tr.animates("translate") || tr.animates("transform") {
                true => mix_translate(from.translate, to.translate, t),
                false => to.translate,
            },
            rotate: match (
                tr.animates("rotate") || tr.animates("transform"),
                from.rotate,
                to.rotate,
            ) {
                (true, a, b) if a.is_some() || b.is_some() => {
                    let (a, b) = (a.unwrap_or(0.0), b.unwrap_or(0.0));
                    Some(a + (b - a) * t)
                }
                _ => to.rotate,
            },
            width: length(tr.animates("width"), from.width, to.width),
            height: length(tr.animates("height"), from.height, to.height),
            inset: [0, 1, 2, 3].map(|i| {
                let side = ["top", "right", "bottom", "left"][i];
                let on = tr.animates(side) || tr.animates("inset");
                length(on, from.inset[i], to.inset[i])
            }),
            scale: match (
                tr.animates("scale") || tr.animates("transform"),
                from.scale,
                to.scale,
            ) {
                (true, a, b) if a.is_some() || b.is_some() => {
                    let (a, b) = (a.unwrap_or(point(1.0, 1.0)), b.unwrap_or(point(1.0, 1.0)));
                    Some(point(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t))
                }
                _ => to.scale,
            },
        }
    }

    pub fn apply(&self, s: &mut StyleRefinement) {
        // A gradient stays a gradient; only solid backgrounds are interpolated.
        if let Some(bg) = self.bg
            && s.background
                .as_ref()
                .is_none_or(|Fill::Color(b)| b.solid_color().is_some())
        {
            s.background = Some(bg.into());
        }
        if let Some(c) = self.color {
            s.text.get_or_insert_with(Default::default).color = Some(c);
        }
        s.border_color = self.border.or(s.border_color);
        s.box_shadow = Some(self.shadows.clone());
        s.opacity = self.opacity;
        s.translate = self.translate;
        s.rotate = self.rotate;
        s.scale = self.scale;
        // Lengths only go back where the style had a px value (what `of` read).
        let set = |slot: &mut Option<Length>, v: Option<Pixels>| {
            if let (Some(v), Some(_)) = (v, px_of(*slot)) {
                *slot = Some(v.into());
            }
        };
        set(&mut s.size.width, self.width);
        set(&mut s.size.height, self.height);
        set(&mut s.inset.top, self.inset[0]);
        set(&mut s.inset.right, self.inset[1]);
        set(&mut s.inset.bottom, self.inset[2]);
        set(&mut s.inset.left, self.inset[3]);
    }
}

/// Lengths of the same kind interpolate; a missing side is zero of the other's kind. A px value
/// and a percentage can't be mixed without the box's size, so that jumps to the end.
fn mix_translate(
    a: Option<Point<DefiniteLength>>,
    b: Option<Point<DefiniteLength>>,
    t: f32,
) -> Option<Point<DefiniteLength>> {
    let zero_like = |l: DefiniteLength| match l {
        DefiniteLength::Fraction(_) => DefiniteLength::Fraction(0.0),
        _ => DefiniteLength::Absolute(AbsoluteLength::Pixels(gpui::px(0.0))),
    };
    let (a, b) = match (a, b) {
        (Some(a), Some(b)) => (a, b),
        (Some(a), None) => (a, point(zero_like(a.x), zero_like(a.y))),
        (None, Some(b)) => (point(zero_like(b.x), zero_like(b.y)), b),
        (None, None) => return None,
    };
    let axis = |a: DefiniteLength, b: DefiniteLength| match (a, b) {
        (DefiniteLength::Fraction(a), DefiniteLength::Fraction(b)) => {
            DefiniteLength::Fraction(a + (b - a) * t)
        }
        (
            DefiniteLength::Absolute(AbsoluteLength::Pixels(a)),
            DefiniteLength::Absolute(AbsoluteLength::Pixels(b)),
        ) => DefiniteLength::Absolute(AbsoluteLength::Pixels(a + (b - a) * t)),
        // A zero on one side takes the other side's kind.
        (DefiniteLength::Absolute(AbsoluteLength::Pixels(z)), DefiniteLength::Fraction(b))
            if z == gpui::px(0.0) =>
        {
            DefiniteLength::Fraction(b * t)
        }
        (DefiniteLength::Fraction(a), DefiniteLength::Absolute(AbsoluteLength::Pixels(z)))
            if z == gpui::px(0.0) && a == 0.0 =>
        {
            DefiniteLength::Absolute(AbsoluteLength::Pixels(z))
        }
        (DefiniteLength::Fraction(a), DefiniteLength::Absolute(AbsoluteLength::Pixels(z)))
            if z == gpui::px(0.0) =>
        {
            DefiniteLength::Fraction(a * (1.0 - t))
        }
        (_, b) => b,
    };
    Some(point(axis(a.x, b.x), axis(a.y, b.y)))
}

/// Interpolates in premultiplied sRGB, as browsers do, so fading from transparent does not darken.
fn mix_color(a: Option<Hsla>, b: Option<Hsla>, t: f32) -> Option<Hsla> {
    let (a, b) = match (a, b) {
        (Some(a), Some(b)) => (a, b),
        (Some(a), None) => (a, a.opacity(0.0)),
        (None, Some(b)) => (b.opacity(0.0), b),
        (None, None) => return None,
    };
    let (a, b) = (Rgba::from(a), Rgba::from(b));
    let alpha = a.a + (b.a - a.a) * t;
    let channel = |x: f32, y: f32| {
        let premultiplied = x * a.a + (y * b.a - x * a.a) * t;
        if alpha > 0.0 {
            premultiplied / alpha
        } else {
            y
        }
    };
    Some(
        Rgba {
            r: channel(a.r, b.r),
            g: channel(a.g, b.g),
            b: channel(a.b, b.b),
            a: alpha,
        }
        .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px};

    fn tr(properties: &[&str]) -> Transition {
        Transition {
            properties: properties.iter().map(|p| p.to_string()).collect(),
            duration: 0.15,
            delay: 0.0,
            easing: Easing::CubicBezier(0.4, 0.0, 0.2, 1.0),
        }
    }

    #[test]
    fn only_a_change_it_animates_moves_a_transition() {
        let at = |height: f32, opacity: f32| Look {
            height: Some(px(height)),
            opacity: Some(opacity),
            ..Default::default()
        };
        let height = tr(&["height"]);
        assert!(!at(0.0, 0.2).moves(&at(0.0, 0.3), &height), "react-spring's opacity, per frame");
        assert!(at(0.0, 0.2).moves(&at(120.0, 0.2), &height));
        assert!(at(0.0, 0.2).moves(&at(0.0, 0.3), &tr(&["all"])));
    }

    #[test]
    fn easing_and_progress() {
        let t = tr(&["all"]);
        assert_eq!(t.progress(Duration::ZERO), 0.0);
        assert!((t.progress(Duration::from_millis(150)) - 1.0).abs() < 1e-4);
        let mid = t.progress(Duration::from_millis(75));
        assert!(
            mid > 0.6 && mid < 0.9,
            "ease-out heavy curve is past halfway at half time: {mid}"
        );
        let linear = cubic_bezier(0.0, 0.0, 1.0, 1.0, 0.3);
        assert!((linear - 0.3).abs() < 1e-3);
    }

    #[test]
    fn ring_grows_from_nothing_and_fades_in() {
        let ring = BoxShadow {
            color: Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.7,
                a: 0.5,
            },
            offset: point(px(0.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(3.),
        };
        let from = Look::default();
        let to = Look {
            shadows: vec![ring.clone()],
            ..Default::default()
        };
        let half = Look::mix(&from, &to, 0.5, &tr(&["color", "box-shadow"]));
        assert_eq!(half.shadows[0].spread_radius, px(1.5));
        assert!((half.shadows[0].color.a - 0.25).abs() < 1e-3);
        assert!(
            (half.shadows[0].color.l - 0.7).abs() < 1e-3,
            "premultiplied mixing keeps the lightness"
        );
        let skipped = Look::mix(&from, &to, 0.5, &tr(&["color"]));
        assert_eq!(
            skipped.shadows,
            vec![ring],
            "box-shadow not listed: jumps to the end value"
        );
    }

    fn anim(
        frames: Vec<(f32, Look)>,
        fill: (bool, bool),
        direction: Direction,
        iterations: f32,
    ) -> Animation {
        Animation {
            name: "a".into(),
            frames,
            duration: 1.0,
            delay: 0.5,
            easing: Easing::CubicBezier(0.0, 0.0, 1.0, 1.0),
            iterations,
            direction,
            fill_backwards: fill.0,
            fill_forwards: fill.1,
        }
    }

    fn opacity(o: f32) -> Look {
        Look {
            opacity: Some(o),
            ..Default::default()
        }
    }

    #[test]
    fn keyframes_sample_with_delay_fill_and_direction() {
        let base = opacity(1.0);
        // `from { opacity: 0 }` alone: the 100% frame is the element's own look.
        let fade_in = anim(
            vec![(0.0, opacity(0.0))],
            (false, false),
            Direction::Normal,
            1.0,
        );
        assert_eq!(
            fade_in.sample(&base, 0.0),
            None,
            "no backwards fill during the delay"
        );
        assert_eq!(fade_in.sample(&base, 1.0).unwrap().opacity, Some(0.5));
        assert_eq!(
            fade_in.sample(&base, 2.0),
            None,
            "no effect after it ends without fill"
        );
        assert!(fade_in.is_done(1.5) && !fade_in.is_done(1.4));

        let held = anim(
            vec![(0.0, opacity(0.0))],
            (true, true),
            Direction::Normal,
            1.0,
        );
        assert_eq!(held.sample(&base, 0.0).unwrap().opacity, Some(0.0));
        assert_eq!(held.sample(&base, 9.0).unwrap().opacity, Some(1.0));

        // `50% { opacity: .5 }`, alternating forever (Tailwind's animate-pulse shape).
        let pulse = anim(
            vec![(0.5, opacity(0.5))],
            (false, false),
            Direction::Alternate,
            f32::INFINITY,
        );
        assert_eq!(pulse.sample(&base, 0.75).unwrap().opacity, Some(0.75));
        assert_eq!(pulse.sample(&base, 1.0).unwrap().opacity, Some(0.5));
        let second = pulse.sample(&base, 1.75).unwrap().opacity.unwrap();
        assert!(
            (second - 0.75).abs() < 1e-5,
            "reversed iteration mirrors the first: {second}"
        );
        assert!(!pulse.is_done(1000.0));
    }

    #[test]
    fn inherited_color_transitions_between_real_colors() {
        let (fg, accent) = (
            Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.1,
                a: 1.0,
            },
            Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.9,
                a: 1.0,
            },
        );
        // The base look's `color` is inherited, so `of` reads None; `resolved` stands in the
        // element's computed color, and a `transition-all` hover no longer blinks the text out.
        let mut hover = StyleRefinement::default();
        hover.text.get_or_insert_with(Default::default).color = Some(accent);
        let from = Look::of(&StyleRefinement::default()).resolved(fg);
        let to = Look::of(&hover).resolved(fg);
        let all = tr(&["all"]);
        assert_eq!(Look::mix(&from, &to, 0.0, &all).color, Some(fg));
        assert_eq!(Look::mix(&from, &to, 1.0, &all).color, Some(accent));
    }

    #[test]
    fn transform_transitions_interpolate() {
        let from = Look {
            rotate: Some(0.0),
            ..Default::default()
        };
        let to = Look {
            rotate: Some(90.0),
            scale: Some(point(0.5, 0.5)),
            ..Default::default()
        };
        let half = Look::mix(&from, &to, 0.5, &tr(&["transform"]));
        assert_eq!(half.rotate, Some(45.0));
        assert_eq!(half.scale, Some(point(0.75, 0.75)));
        let slide = Look::mix(
            &Look {
                translate: Some(point(DefiniteLength::Fraction(-0.5), px(8.0).into())),
                ..Default::default()
            },
            &Look::default(),
            0.5,
            &tr(&["translate"]),
        );
        assert_eq!(
            slide.translate,
            Some(point(DefiniteLength::Fraction(-0.25), px(4.0).into()))
        );
    }

    #[test]
    fn layout_lengths_transition_when_listed() {
        let mut wide = StyleRefinement::default();
        wide.size.width = Some(px(256.0).into());
        let mut narrow = StyleRefinement::default();
        narrow.size.width = Some(px(48.0).into());
        let half = Look::mix(&Look::of(&wide), &Look::of(&narrow), 0.5, &tr(&["width"]));
        let mut s = narrow.clone();
        half.apply(&mut s);
        assert_eq!(s.size.width, Some(px(152.0).into()));
        let skipped = Look::mix(&Look::of(&wide), &Look::of(&narrow), 0.5, &tr(&["color"]));
        let mut s = narrow.clone();
        skipped.apply(&mut s);
        assert_eq!(
            s.size.width,
            Some(px(48.0).into()),
            "width not listed: jumps"
        );
    }
}
