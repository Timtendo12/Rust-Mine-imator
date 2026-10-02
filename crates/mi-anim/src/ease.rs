//! Transitions between keyframes (`ease.gml`, `ease_bezier_curve.gml`).
//!
//! A keyframe names the transition used on the way to the next keyframe.
//! The curves are the classic Penner easing equations.

use std::f64::consts::PI;

macro_rules! transitions {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        /// Shape of the interpolation between two keyframes, in the order of
        /// `transition_list`.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Transition { $($variant),+ }

        impl Transition {
            pub const ALL: &'static [Transition] = &[$(Transition::$variant),+];

            /// Name stored in the `TRANSITION` value.
            pub const fn name(self) -> &'static str {
                match self { $(Transition::$variant => $name),+ }
            }

            /// Unknown names behave as linear, as in the original.
            pub fn from_name(name: &str) -> Transition {
                match name { $($name => Transition::$variant,)+ _ => Transition::Linear }
            }
        }
    };
}

transitions! {
    Linear => "linear",
    Instant => "instant",
    Bezier => "bezier",
    EaseInQuad => "easeinquad",
    EaseOutQuad => "easeoutquad",
    EaseInOutQuad => "easeinoutquad",
    EaseInCubic => "easeincubic",
    EaseOutCubic => "easeoutcubic",
    EaseInOutCubic => "easeinoutcubic",
    EaseInQuart => "easeinquart",
    EaseOutQuart => "easeoutquart",
    EaseInOutQuart => "easeinoutquart",
    EaseInQuint => "easeinquint",
    EaseOutQuint => "easeoutquint",
    EaseInOutQuint => "easeinoutquint",
    EaseInSine => "easeinsine",
    EaseOutSine => "easeoutsine",
    EaseInOutSine => "easeinoutsine",
    EaseInExpo => "easeinexpo",
    EaseOutExpo => "easeoutexpo",
    EaseInOutExpo => "easeinoutexpo",
    EaseInCirc => "easeincirc",
    EaseOutCirc => "easeoutcirc",
    EaseInOutCirc => "easeinoutcirc",
    EaseInElastic => "easeinelastic",
    EaseOutElastic => "easeoutelastic",
    EaseInOutElastic => "easeinoutelastic",
    EaseInBack => "easeinback",
    EaseOutBack => "easeoutback",
    EaseInOutBack => "easeinoutback",
    EaseInBounce => "easeinbounce",
    EaseOutBounce => "easeoutbounce",
    EaseInOutBounce => "easeinoutbounce",
}

/// Control points of a custom bezier transition: `EASE_IN_X/Y` and
/// `EASE_OUT_X/Y` of the keyframe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BezierHandles {
    pub ease_in: [f64; 2],
    pub ease_out: [f64; 2],
}

impl Default for BezierHandles {
    /// The defaults of the four values, which give a symmetric ease in and
    /// out.
    fn default() -> Self {
        Self { ease_in: [1.0, 0.0], ease_out: [0.0, 1.0] }
    }
}

fn bounce_out(x: f64) -> f64 {
    if x < 1.0 / 2.75 {
        7.5625 * x * x
    } else if x < 2.0 / 2.75 {
        let x = x - 1.5 / 2.75;
        7.5625 * x * x + 0.75
    } else if x < 2.5 / 2.75 {
        let x = x - 2.25 / 2.75;
        7.5625 * x * x + 0.9375
    } else {
        let x = x - 2.625 / 2.75;
        7.5625 * x * x + 0.984375
    }
}

impl Transition {
    /// Maps linear progress `x` (0..1) to eased progress. Progress outside
    /// the range is clamped. Elastic and back transitions overshoot 0..1 on
    /// purpose.
    ///
    /// [`Transition::Bezier`] needs its handles; without them (here) it is
    /// linear. Use [`Transition::ease_with`].
    pub fn ease(self, x: f64) -> f64 {
        use Transition::*;

        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }

        let x2 = x * 2.0;
        let xm1 = x - 1.0;
        const BACK: f64 = 1.70158;
        const BACK_IN_OUT: f64 = BACK * 1.525;

        match self {
            Linear | Bezier => x,
            Instant => 0.0,

            EaseInQuad => x * x,
            EaseOutQuad => -x * (x - 2.0),
            EaseInOutQuad => {
                if x2 < 1.0 {
                    0.5 * x2 * x2
                } else {
                    -0.5 * ((x2 - 1.0) * (x2 - 3.0) - 1.0)
                }
            }

            EaseInCubic => x * x * x,
            EaseOutCubic => xm1 * xm1 * xm1 + 1.0,
            EaseInOutCubic => {
                if x2 < 1.0 {
                    0.5 * x2 * x2 * x2
                } else {
                    0.5 * ((x2 - 2.0).powi(3) + 2.0)
                }
            }

            EaseInQuart => x * x * x * x,
            EaseOutQuart => -(xm1.powi(4) - 1.0),
            EaseInOutQuart => {
                if x2 < 1.0 {
                    0.5 * x2.powi(4)
                } else {
                    -0.5 * ((x2 - 2.0).powi(4) - 2.0)
                }
            }

            EaseInQuint => x.powi(5),
            EaseOutQuint => xm1.powi(5) + 1.0,
            EaseInOutQuint => {
                if x2 < 1.0 {
                    0.5 * x2.powi(5)
                } else {
                    0.5 * ((x2 - 2.0).powi(5) + 2.0)
                }
            }

            EaseInSine => -(x * (PI / 2.0)).cos() + 1.0,
            EaseOutSine => (x * (PI / 2.0)).sin(),
            EaseInOutSine => -0.5 * ((PI * x).cos() - 1.0),

            EaseInExpo => 2f64.powf(10.0 * (x - 1.0)),
            EaseOutExpo => -(2f64.powf(-10.0 * x)) + 1.0,
            EaseInOutExpo => {
                if x2 < 1.0 {
                    0.5 * 2f64.powf(10.0 * (x2 - 1.0))
                } else {
                    0.5 * (-(2f64.powf(-10.0 * (x2 - 1.0))) + 2.0)
                }
            }

            EaseInCirc => -((1.0 - x * x).sqrt() - 1.0),
            EaseOutCirc => (1.0 - xm1 * xm1).sqrt(),
            EaseInOutCirc => {
                if x2 < 1.0 {
                    -0.5 * ((1.0 - x2 * x2).sqrt() - 1.0)
                } else {
                    0.5 * ((1.0 - (x2 - 2.0) * (x2 - 2.0)).max(0.0).sqrt() + 1.0)
                }
            }

            EaseInElastic => {
                let p = 0.3;
                let s = p / (2.0 * PI) * 1f64.asin();
                -(2f64.powf(10.0 * xm1) * ((xm1 - s) * (2.0 * PI) / p).sin())
            }
            EaseOutElastic => {
                let p = 0.3;
                let s = p / (2.0 * PI) * 1f64.asin();
                2f64.powf(-10.0 * x) * ((x - s) * (2.0 * PI) / p).sin() + 1.0
            }
            EaseInOutElastic => {
                let p = 0.3 * 1.5;
                let s = p / (2.0 * PI) * 1f64.asin();
                let t = x2 - 1.0;
                if x2 < 1.0 {
                    -0.5 * (2f64.powf(10.0 * t) * ((t - s) * (2.0 * PI) / p).sin())
                } else {
                    2f64.powf(-10.0 * t) * ((t - s) * (2.0 * PI) / p).sin() * 0.5 + 1.0
                }
            }

            EaseInBack => x * x * ((BACK + 1.0) * x - BACK),
            EaseOutBack => xm1 * xm1 * ((BACK + 1.0) * xm1 + BACK) + 1.0,
            EaseInOutBack => {
                if x2 < 1.0 {
                    0.5 * (x2 * x2 * ((BACK_IN_OUT + 1.0) * x2 - BACK_IN_OUT))
                } else {
                    let t = x2 - 2.0;
                    0.5 * (t * t * ((BACK_IN_OUT + 1.0) * t + BACK_IN_OUT) + 2.0)
                }
            }

            EaseInBounce => 1.0 - bounce_out(1.0 - x),
            EaseOutBounce => bounce_out(x),
            EaseInOutBounce => {
                if x < 0.5 {
                    (1.0 - bounce_out(1.0 - x * 2.0)) * 0.5
                } else {
                    bounce_out(x * 2.0 - 1.0) * 0.5 + 0.5
                }
            }
        }
    }

    /// Like [`Transition::ease`], with the handles a bezier transition needs.
    pub fn ease_with(self, x: f64, handles: BezierHandles) -> f64 {
        match self {
            Transition::Bezier => ease_bezier(handles, x),
            other => other.ease(x),
        }
    }
}

fn lerp2(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// `bezier_curve_cubic` in two dimensions.
fn bezier_cubic(p1: [f64; 2], p2: [f64; 2], p3: [f64; 2], p4: [f64; 2], t: f64) -> [f64; 2] {
    let t1 = lerp2(p1, p2, t);
    let t2 = lerp2(p2, p3, t);
    let t3 = lerp2(p3, p4, t);
    lerp2(lerp2(t1, t2, t), lerp2(t2, t3, t), t)
}

/// `ease_bezier_curve`: finds the curve parameter whose X equals `t` by
/// bisection (10 steps, tolerance 0.001) and returns the Y there. The curve
/// runs from (0, 0) to (1, 1).
pub fn ease_bezier(handles: BezierHandles, t: f64) -> f64 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }

    let curve = |param: f64| bezier_cubic([0.0, 0.0], handles.ease_in, handles.ease_out, [1.0, 1.0], param);
    let (mut lower, mut upper) = (0.0, 1.0);
    let mut param = 0.5;
    let mut point = curve(param);
    for _ in 0..10 {
        if (t - point[0]).abs() < 0.001 {
            break;
        }
        if t > point[0] {
            lower = param;
        } else {
            upper = param;
        }
        param = (upper + lower) / 2.0;
        point = curve(param);
    }
    point[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn names_round_trip_and_unknown_is_linear() {
        assert_eq!(Transition::ALL.len(), 33);
        for &t in Transition::ALL {
            assert_eq!(Transition::from_name(t.name()), t);
        }
        assert_eq!(Transition::from_name("wobble"), Transition::Linear);
    }

    #[test]
    fn every_transition_starts_at_zero_and_ends_at_one() {
        for &t in Transition::ALL {
            assert_eq!(t.ease(0.0), 0.0, "{t:?}");
            assert_eq!(t.ease(1.0), 1.0, "{t:?}");
            assert_eq!(t.ease(-3.0), 0.0, "{t:?}");
            assert_eq!(t.ease(7.0), 1.0, "{t:?}");
            for i in 1..100 {
                assert!(t.ease(i as f64 / 100.0).is_finite(), "{t:?}");
            }
        }
    }

    #[test]
    fn instant_holds_until_the_end() {
        assert_eq!(Transition::Instant.ease(0.999), 0.0);
    }

    /// Reference values worked out from the formulas in `ease.gml`.
    #[test]
    fn reference_values() {
        use Transition::*;
        let cases: &[(Transition, f64, f64)] = &[
            (Linear, 0.3, 0.3),
            (EaseInQuad, 0.5, 0.25),
            (EaseOutQuad, 0.5, 0.75),
            (EaseInOutQuad, 0.25, 0.125),
            (EaseInOutQuad, 0.75, 0.875),
            (EaseInCubic, 0.5, 0.125),
            (EaseOutCubic, 0.5, 0.875),
            (EaseInOutCubic, 0.25, 0.0625),
            (EaseInOutCubic, 0.75, 0.9375),
            (EaseInQuart, 0.5, 0.0625),
            (EaseOutQuart, 0.5, 0.9375),
            (EaseInOutQuart, 0.25, 0.03125),
            (EaseInOutQuart, 0.75, 0.96875),
            (EaseInQuint, 0.5, 0.03125),
            (EaseOutQuint, 0.5, 0.96875),
            (EaseInOutQuint, 0.25, 0.015625),
            (EaseInOutQuint, 0.75, 0.984375),
            (EaseInSine, 0.5, 1.0 - (PI / 4.0).cos()),
            (EaseOutSine, 0.5, (PI / 4.0).sin()),
            (EaseInOutSine, 0.5, 0.5),
            (EaseInExpo, 0.5, 0.03125),
            (EaseOutExpo, 0.5, 0.96875),
            (EaseInOutExpo, 0.25, 0.015625),
            (EaseInOutExpo, 0.75, 0.984375),
            (EaseInCirc, 0.6, 0.2),
            (EaseOutCirc, 0.4, 0.8),
            (EaseInOutCirc, 0.3, 0.1),
            (EaseInOutCirc, 0.7, 0.9),
            (EaseInBack, 0.5, 0.25 * (2.70158 * 0.5 - 1.70158)),
            (EaseOutBack, 0.5, 0.25 * (2.70158 * -0.5 + 1.70158) + 1.0),
            (EaseOutBounce, 0.2, 7.5625 * 0.04),
            (EaseOutBounce, 0.5, 7.5625 * (0.5f64 - 1.5 / 2.75).powi(2) + 0.75),
            (EaseInBounce, 0.8, 1.0 - 7.5625 * 0.04),
            (EaseInOutBounce, 0.6, 7.5625 * 0.04 * 0.5 + 0.5),
            (EaseInOutBounce, 0.4, (1.0 - 7.5625 * 0.04) * 0.5),
        ];
        for &(transition, x, expected) in cases {
            let got = transition.ease(x);
            assert!(close(got, expected), "{transition:?}({x}) = {got}, expected {expected}");
        }
    }

    #[test]
    fn in_and_out_mirror_each_other() {
        use Transition::*;
        let pairs = [
            (EaseInQuad, EaseOutQuad),
            (EaseInCubic, EaseOutCubic),
            (EaseInQuart, EaseOutQuart),
            (EaseInQuint, EaseOutQuint),
            (EaseInSine, EaseOutSine),
            (EaseInExpo, EaseOutExpo),
            (EaseInCirc, EaseOutCirc),
            (EaseInElastic, EaseOutElastic),
            (EaseInBack, EaseOutBack),
            (EaseInBounce, EaseOutBounce),
        ];
        for (ease_in, ease_out) in pairs {
            for i in 1..20 {
                let x = i as f64 / 20.0;
                assert!(close(ease_in.ease(x), 1.0 - ease_out.ease(1.0 - x)), "{ease_in:?} at {x}");
            }
        }
    }

    #[test]
    fn elastic_and_back_overshoot() {
        assert!(Transition::EaseInBack.ease(0.2) < 0.0);
        assert!(Transition::EaseOutBack.ease(0.8) > 1.0);
        assert!((1..100).any(|i| Transition::EaseOutElastic.ease(i as f64 / 100.0) > 1.0));
    }

    #[test]
    fn default_bezier_handles_ease_in_and_out() {
        let handles = BezierHandles::default();
        assert_eq!(ease_bezier(handles, 0.0), 0.0);
        assert_eq!(ease_bezier(handles, 1.0), 1.0);
        assert!((ease_bezier(handles, 0.5) - 0.5).abs() < 0.01);
        assert!(ease_bezier(handles, 0.25) < 0.25);
        assert!(ease_bezier(handles, 0.75) > 0.75);
    }

    #[test]
    fn bezier_with_handles_on_the_diagonal_is_linear() {
        let handles = BezierHandles { ease_in: [0.25, 0.25], ease_out: [0.75, 0.75] };
        for i in 0..=20 {
            let x = i as f64 / 20.0;
            assert!((ease_bezier(handles, x) - x).abs() < 0.002, "{x}");
        }
    }

    #[test]
    fn bezier_ease_in_starts_slow() {
        let handles = BezierHandles { ease_in: [0.6, 0.0], ease_out: [1.0, 1.0] };
        assert!(Transition::Bezier.ease_with(0.3, handles) < 0.15);
        let late = Transition::Bezier.ease_with(0.9, handles);
        assert!(late > 0.6 && late < 0.9, "{late}");
        // Without handles a bezier transition falls back to linear.
        assert_eq!(Transition::Bezier.ease(0.3), 0.3);
    }
}
