//! Lighting effects as a pure function of configuration and time.

use std::collections::BTreeMap;
use std::f64::consts::PI;

use crate::Rgb;
use crate::colour::Colour;
use crate::config::{
    AnimationConfig, Direction, LightingConfig, LightingMode, PresetName, Slide, Transition,
};
use crate::keymap::Keymap;

/// Colour per LED zone id.
pub type Frame = Vec<(u8, Rgb)>;

/// Whether the effect changes over time (static frames are sent once).
pub fn is_animated(cfg: &LightingConfig) -> bool {
    match cfg.mode {
        LightingMode::Preset => cfg
            .preset
            .as_ref()
            .is_some_and(|p| p.name != PresetName::Unknown),
        LightingMode::Animation => cfg.animation.as_ref().is_some_and(|a| !a.slides.is_empty()),
        LightingMode::Static | LightingMode::Unknown => false,
    }
}

/// The frame for `cfg` at `t` seconds after the effect started.
pub fn frame(cfg: &LightingConfig, map: &Keymap, t: f64) -> Frame {
    let mut f = raw_frame(cfg, map, t);
    let b = cfg.brightness.clamp(0.0, 1.0);
    if b < 1.0 {
        for (_, rgb) in &mut f {
            *rgb = scale(*rgb, b);
        }
    }
    f
}

fn raw_frame(cfg: &LightingConfig, map: &Keymap, t: f64) -> Frame {
    match (cfg.mode, &cfg.preset, &cfg.animation) {
        (LightingMode::Preset, Some(p), _) => {
            let c1 = p.colours.first().map_or(cfg.base_colour.0, |c| c.0);
            let c2 = p.colours.get(1).map_or(Rgb::default(), |c| c.0);
            let speed = p.speed;
            map.lit()
                .map(|(name, led)| {
                    let rgb = match p.name {
                        PresetName::Breathing => {
                            scale(c1, f64::midpoint((t * PI * speed * 0.5).sin(), 1.0))
                        }
                        PresetName::Wave => {
                            let phase =
                                (coord(map, name, p.direction) - t * speed * 0.25).rem_euclid(1.0);
                            mix(c2, c1, f64::midpoint((phase * 2.0 * PI).sin(), 1.0))
                        }
                        PresetName::RainbowWave => hsv(
                            (coord(map, name, p.direction) - t * speed * 0.1).rem_euclid(1.0),
                            1.0,
                            1.0,
                        ),
                        PresetName::ColourCycle => {
                            hsv((t * speed * 0.05).rem_euclid(1.0), 1.0, 1.0)
                        }
                        PresetName::Unknown => base_colour(cfg, name),
                    };
                    (led, rgb)
                })
                .collect()
        }
        (LightingMode::Animation, _, Some(anim)) if !anim.slides.is_empty() => {
            animation(cfg, anim, map, t)
        }
        _ => map
            .lit()
            .map(|(name, led)| (led, base_colour(cfg, name)))
            .collect(),
    }
}

fn base_colour(cfg: &LightingConfig, name: &str) -> Rgb {
    cfg.keys.get(name).unwrap_or(&cfg.base_colour).0
}

fn coord(map: &Keymap, name: &str, dir: Direction) -> f64 {
    let (x, y) = map.centre(name);
    let (x, y) = (f64::from(x), f64::from(y));
    match dir {
        Direction::LeftRight => x,
        Direction::RightLeft => 1.0 - x,
        Direction::TopBottom => y,
        Direction::BottomTop => 1.0 - y,
        Direction::Radial => ((x - 0.5).hypot(y - 0.5) * 2.0).min(1.0),
    }
}

fn slide_colour(cfg: &LightingConfig, slide: &Slide, name: &str) -> Rgb {
    slide
        .keys
        .get(name)
        .map_or_else(|| base_colour(cfg, name), |c: &Colour| c.0)
}

fn animation(cfg: &LightingConfig, anim: &AnimationConfig, map: &Keymap, t: f64) -> Frame {
    let slides = &anim.slides;
    let trans_dur = |s: &Slide| {
        if s.transition == Transition::Cut {
            0.0
        } else {
            s.transition_duration.max(0.0)
        }
    };
    let total: f64 = slides.iter().map(|s| s.hold.max(0.0) + trans_dur(s)).sum();
    let t = match (total > 0.0, anim.looped) {
        (false, _) => 0.0,
        (true, true) => t.rem_euclid(total),
        (true, false) => t.min(total),
    };

    let mut start = 0.0;
    for (i, slide) in slides.iter().enumerate() {
        let hold_end = start + slide.hold.max(0.0);
        let dur = trans_dur(slide);
        if t < hold_end {
            return map
                .lit()
                .map(|(name, led)| (led, slide_colour(cfg, slide, name)))
                .collect();
        }
        if t < hold_end + dur {
            let next = &slides[(i + 1) % slides.len()];
            let frac = (t - hold_end) / dur;
            return map
                .lit()
                .map(|(name, led)| {
                    let a = slide_colour(cfg, slide, name);
                    let b = slide_colour(cfg, next, name);
                    (
                        led,
                        transition(
                            slide.transition,
                            a,
                            b,
                            frac,
                            coord_for_wipe(map, name, slide.transition),
                        ),
                    )
                })
                .collect();
        }
        start = hold_end + dur;
    }
    let last = &slides[slides.len() - 1];
    map.lit()
        .map(|(name, led)| (led, slide_colour(cfg, last, name)))
        .collect()
}

fn coord_for_wipe(map: &Keymap, name: &str, tr: Transition) -> f64 {
    let dir = match tr {
        Transition::WipeRight => Direction::RightLeft,
        Transition::WipeTop => Direction::TopBottom,
        Transition::WipeBottom => Direction::BottomTop,
        _ => Direction::LeftRight,
    };
    coord(map, name, dir)
}

fn transition(tr: Transition, a: Rgb, b: Rgb, frac: f64, wipe_coord: f64) -> Rgb {
    match tr {
        Transition::Cut => b,
        Transition::Ease => mix(a, b, frac * frac * (3.0 - 2.0 * frac)),
        Transition::Hsv => mix_hsv(a, b, frac),
        Transition::WipeLeft
        | Transition::WipeRight
        | Transition::WipeTop
        | Transition::WipeBottom => {
            if wipe_coord <= frac {
                b
            } else {
                a
            }
        }
        Transition::Blink if frac < 0.5 => mix(a, Rgb::default(), frac * 2.0),
        Transition::Blink => mix(Rgb::default(), b, (frac - 0.5) * 2.0),
        Transition::Fade | Transition::Unknown => mix(a, b, frac),
    }
}

fn channel(v: f64) -> u8 {
    v.clamp(0.0, 255.0) as u8
}

fn scale(Rgb(r, g, b): Rgb, k: f64) -> Rgb {
    Rgb(
        channel(f64::from(r) * k),
        channel(f64::from(g) * k),
        channel(f64::from(b) * k),
    )
}

/// Linear blend from `a` (t = 0) to `b` (t = 1).
fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let lerp = |x: u8, y: u8| channel(f64::from(x) + (f64::from(y) - f64::from(x)) * t);
    Rgb(lerp(a.0, b.0), lerp(a.1, b.1), lerp(a.2, b.2))
}

fn mix_hsv(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let (h1, s1, v1) = to_hsv(a);
    let (h2, s2, v2) = to_hsv(b);
    let mut dh = h2 - h1;
    if dh > 0.5 {
        dh -= 1.0;
    } else if dh < -0.5 {
        dh += 1.0;
    }
    hsv(
        (h1 + dh * t).rem_euclid(1.0),
        s1 + (s2 - s1) * t,
        v1 + (v2 - v1) * t,
    )
}

/// HSV (all 0..=1) to RGB.
#[allow(clippy::many_single_char_names)]
pub fn hsv(h: f64, s: f64, v: f64) -> Rgb {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match (i as i64).rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Rgb(channel(r * 255.0), channel(g * 255.0), channel(b * 255.0))
}

#[allow(clippy::many_single_char_names)]
fn to_hsv(Rgb(r, g, b): Rgb) -> (f64, f64, f64) {
    let (r, g, b) = (
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if (max - r).abs() < f64::EPSILON {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if (max - g).abs() < f64::EPSILON {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    (h, if max == 0.0 { 0.0 } else { d / max }, max)
}

/// Changed zones between two frames (all of `next` if there is no `prev`).
pub fn diff(prev: Option<&Frame>, next: &Frame) -> Frame {
    let Some(prev) = prev else {
        return next.clone();
    };
    let old: BTreeMap<u8, Rgb> = prev.iter().copied().collect();
    next.iter()
        .copied()
        .filter(|(led, rgb)| old.get(led) != Some(rgb))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{PresetConfig, Slide};

    fn colour(s: &str) -> Colour {
        s.parse().unwrap()
    }

    fn led(map: &Keymap, name: &str) -> u8 {
        map.get(name).unwrap().led.unwrap()
    }

    fn at(f: &Frame, led: u8) -> Rgb {
        f.iter().find(|(l, _)| *l == led).unwrap().1
    }

    #[test]
    fn static_with_override() {
        let map = Keymap::g815();
        let mut cfg = LightingConfig {
            base_colour: colour("#0000ff"),
            ..Default::default()
        };
        cfg.keys.insert("W".into(), colour("#ffffff"));
        let f = frame(&cfg, map, 0.0);
        assert_eq!(f.len(), map.lit().count());
        assert_eq!(at(&f, led(map, "W")), Rgb(255, 255, 255));
        assert_eq!(at(&f, led(map, "A")), Rgb(0, 0, 255));
        assert!(!is_animated(&cfg));
    }

    #[test]
    fn breathing_pulses() {
        let map = Keymap::g815();
        let cfg = LightingConfig {
            mode: LightingMode::Preset,
            preset: Some(PresetConfig {
                colours: vec![colour("#ff0000")],
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(is_animated(&cfg));
        // brightness = (sin(t·π/2)+1)/2 → 0.5 at t=0, 1.0 at t=1
        assert_eq!(at(&frame(&cfg, map, 0.0), led(map, "A")), Rgb(127, 0, 0));
        assert_eq!(at(&frame(&cfg, map, 1.0), led(map, "A")), Rgb(255, 0, 0));
    }

    #[test]
    fn rainbow_wave_varies_left_to_right() {
        let map = Keymap::g815();
        let cfg = LightingConfig {
            mode: LightingMode::Preset,
            preset: Some(PresetConfig {
                name: PresetName::RainbowWave,
                ..Default::default()
            }),
            ..Default::default()
        };
        let f = frame(&cfg, map, 0.0);
        assert_ne!(at(&f, led(map, "ESC")), at(&f, led(map, "NUM_MINUS")));
    }

    #[test]
    fn animation_holds_then_fades() {
        let map = Keymap::g815();
        let red = Slide {
            keys: [("A".into(), colour("#ff0000"))].into(),
            hold: 1.0,
            transition: Transition::Fade,
            transition_duration: 1.0,
        };
        let blue = Slide {
            keys: [("A".into(), colour("#0000ff"))].into(),
            hold: 1.0,
            ..Default::default()
        };
        let cfg = LightingConfig {
            mode: LightingMode::Animation,
            animation: Some(AnimationConfig {
                looped: true,
                slides: vec![red, blue],
            }),
            ..Default::default()
        };
        let a = led(map, "A");
        assert_eq!(at(&frame(&cfg, map, 0.5), a), Rgb(255, 0, 0));
        assert_eq!(at(&frame(&cfg, map, 1.5), a), Rgb(127, 0, 127));
        assert_eq!(at(&frame(&cfg, map, 2.5), a), Rgb(0, 0, 255));
        assert_eq!(at(&frame(&cfg, map, 3.5), a), Rgb(255, 0, 0), "loops");
    }

    #[test]
    fn hsv_round_trip() {
        for c in [Rgb(255, 0, 0), Rgb(0, 128, 255), Rgb(10, 200, 30)] {
            let (h, s, v) = to_hsv(c);
            let back = hsv(h, s, v);
            assert!(
                (i16::from(back.0) - i16::from(c.0)).abs() <= 1,
                "{c:?} → {back:?}"
            );
        }
    }

    #[test]
    fn brightness_scales_everything() {
        let map = Keymap::g815();
        let cfg = LightingConfig {
            base_colour: colour("#ff8000"),
            brightness: 0.5,
            ..Default::default()
        };
        assert_eq!(at(&frame(&cfg, map, 0.0), led(map, "A")), Rgb(127, 64, 0));
        let text = toml::to_string(&LightingConfig::default()).unwrap();
        assert!(
            !text.contains("brightness"),
            "full brightness is not written"
        );
    }

    #[test]
    fn diff_only_changed() {
        let a: Frame = vec![(1, Rgb(1, 1, 1)), (2, Rgb(2, 2, 2))];
        let b: Frame = vec![(1, Rgb(1, 1, 1)), (2, Rgb(9, 9, 9))];
        assert_eq!(diff(Some(&a), &b), [(2, Rgb(9, 9, 9))]);
        assert_eq!(diff(None, &b), b);
    }
}
