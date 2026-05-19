//! Easing curves and interpolation helpers. Pure math — no state, no allocation.

/// An easing curve mapping raw progress `t ∈ [0, 1]` to eased progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Easing {
    Linear,
    EaseOutCubic,
    EaseInCubic,
    EaseInOutCubic,
    EaseOutElastic,
}

impl Easing {
    /// Parse an easing from its snake_case name. `None` for an unknown name.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "linear" => Some(Easing::Linear),
            "ease_out_cubic" => Some(Easing::EaseOutCubic),
            "ease_in_cubic" => Some(Easing::EaseInCubic),
            "ease_in_out_cubic" => Some(Easing::EaseInOutCubic),
            "ease_out_elastic" => Some(Easing::EaseOutElastic),
            _ => None,
        }
    }

    /// The snake_case name of this easing — inverse of [`Easing::from_str`].
    pub fn name(self) -> &'static str {
        match self {
            Easing::Linear => "linear",
            Easing::EaseOutCubic => "ease_out_cubic",
            Easing::EaseInCubic => "ease_in_cubic",
            Easing::EaseInOutCubic => "ease_in_out_cubic",
            Easing::EaseOutElastic => "ease_out_elastic",
        }
    }
}

/// Linear interpolation between two `f32` values.
pub fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Linear interpolation between two `u8` values, rounded.
pub fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

/// Lerp each channel of an RGBA `[u8; 4]` value.
pub fn lerp_rgba(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    [
        lerp_u8(a[0], b[0], t),
        lerp_u8(a[1], b[1], t),
        lerp_u8(a[2], b[2], t),
        lerp_u8(a[3], b[3], t),
    ]
}

/// Map raw `t ∈ [0, 1]` through the chosen easing curve. `t` is clamped first.
pub fn apply_easing(easing: Easing, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match easing {
        Easing::Linear => t,
        Easing::EaseOutCubic => 1.0 - (1.0 - t).powi(3),
        Easing::EaseInCubic => t * t * t,
        Easing::EaseInOutCubic => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
            }
        }
        Easing::EaseOutElastic => {
            if t == 0.0 || t == 1.0 {
                return t;
            }
            let c4 = (2.0 * std::f32::consts::PI) / 3.0;
            2.0_f32.powf(-10.0 * t) * ((t * 10.0 - 10.75) * c4).sin() + 1.0
        }
    }
}

/// Signed pixel offset for a decaying oscillation ("shake") effect.
///
/// - `t`: progress `0 → 1` over the shake duration (raw, not eased).
/// - `amplitude`: peak displacement (e.g. pixels).
/// - `cycles`: number of full oscillations over the duration.
///
/// Amplitude decays linearly to zero at `t = 1` so the target settles cleanly.
pub fn shake_offset(t: f32, amplitude: f32, cycles: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    (t * cycles * 2.0 * std::f32::consts::PI).sin() * amplitude * (1.0 - t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_f32_midpoint() {
        assert!((lerp_f32(0.0, 10.0, 0.5) - 5.0).abs() < 0.001);
        assert_eq!(lerp_f32(0.0, 10.0, 0.0), 0.0);
        assert_eq!(lerp_f32(0.0, 10.0, 1.0), 10.0);
    }

    #[test]
    fn lerp_u8_full_range() {
        assert_eq!(lerp_u8(0, 255, 0.0), 0);
        assert_eq!(lerp_u8(0, 255, 1.0), 255);
        assert_eq!(lerp_u8(0, 100, 0.5), 50);
    }

    #[test]
    fn lerp_rgba_identity() {
        let c = [255u8, 128, 64, 200];
        assert_eq!(lerp_rgba(c, c, 0.5), c);
    }

    #[test]
    fn apply_easing_boundary_values() {
        for easing in [
            Easing::Linear,
            Easing::EaseOutCubic,
            Easing::EaseInCubic,
            Easing::EaseInOutCubic,
            Easing::EaseOutElastic,
        ] {
            let t0 = apply_easing(easing, 0.0);
            let t1 = apply_easing(easing, 1.0);
            assert!(t0.abs() < 0.001, "{easing:?} at t=0 should be ~0, got {t0}");
            assert!(
                (t1 - 1.0).abs() < 0.001,
                "{easing:?} at t=1 should be ~1, got {t1}"
            );
        }
    }

    #[test]
    fn easing_monotonic_in_midrange() {
        for easing in [Easing::Linear, Easing::EaseOutCubic, Easing::EaseInCubic] {
            let t_mid = apply_easing(easing, 0.5);
            assert!(
                t_mid > 0.0 && t_mid < 1.0,
                "{easing:?} midpoint {t_mid} must be in (0,1)"
            );
        }
    }

    #[test]
    fn easing_from_str_round_trips() {
        for name in [
            "linear",
            "ease_out_cubic",
            "ease_in_cubic",
            "ease_in_out_cubic",
            "ease_out_elastic",
        ] {
            let e = Easing::from_str(name).unwrap_or_else(|| panic!("missing easing: {name}"));
            assert_eq!(e.name(), name);
        }
        assert!(Easing::from_str("bogus").is_none());
    }

    #[test]
    fn shake_settles_at_end() {
        assert_eq!(shake_offset(1.0, 5.0, 3.0), 0.0);
    }
}
