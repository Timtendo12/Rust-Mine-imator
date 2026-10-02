//! Simplex noise in one and two dimensions, as the original gets it from
//! Stefan Gustavson's public domain `SimplexNoise1234` (`simplex_lib`), and
//! the camera shake built on it (`render_update_camera`).
//!
//! The arithmetic is single precision like the C code, so that shaking
//! cameras move the same way.

use crate::math::Mat4;
use mi_core::ValueId;
use mi_format::ValueSet;

/// Ken Perlin's permutation of 0..=255.
const PERM: [u8; 256] = [
    151, 160, 137, 91, 90, 15, 131, 13, 201, 95, 96, 53, 194, 233, 7, 225, 140, 36, 103, 30, 69, 142, 8, 99, 37, 240,
    21, 10, 23, 190, 6, 148, 247, 120, 234, 75, 0, 26, 197, 62, 94, 252, 219, 203, 117, 35, 11, 32, 57, 177, 33, 88,
    237, 149, 56, 87, 174, 20, 125, 136, 171, 168, 68, 175, 74, 165, 71, 134, 139, 48, 27, 166, 77, 146, 158, 231, 83,
    111, 229, 122, 60, 211, 133, 230, 220, 105, 92, 41, 55, 46, 245, 40, 244, 102, 143, 54, 65, 25, 63, 161, 1, 216,
    80, 73, 209, 76, 132, 187, 208, 89, 18, 169, 200, 196, 135, 130, 116, 188, 159, 86, 164, 100, 109, 198, 173, 186,
    3, 64, 52, 217, 226, 250, 124, 123, 5, 202, 38, 147, 118, 126, 255, 82, 85, 212, 207, 206, 59, 227, 47, 16, 58,
    17, 182, 189, 28, 42, 223, 183, 170, 213, 119, 248, 152, 2, 44, 154, 163, 70, 221, 153, 101, 155, 167, 43, 172, 9,
    129, 22, 39, 253, 19, 98, 108, 110, 79, 113, 224, 232, 178, 185, 112, 104, 218, 246, 97, 228, 251, 34, 242, 193,
    238, 210, 144, 12, 191, 179, 162, 241, 81, 51, 145, 235, 249, 14, 239, 107, 49, 192, 214, 31, 181, 199, 106, 157,
    184, 84, 204, 176, 115, 121, 50, 45, 127, 4, 150, 254, 138, 236, 205, 93, 222, 114, 67, 29, 24, 72, 243, 141, 128,
    195, 78, 66, 215, 61, 156, 180,
];

/// The table repeats, so sums of two indices wrap around.
fn perm(index: i32) -> i32 {
    PERM[(index & 0xff) as usize] as i32
}

/// `FASTFLOOR`
fn floor(x: f32) -> i32 {
    let truncated = x as i32;
    if truncated as f32 <= x {
        truncated
    } else {
        truncated - 1
    }
}

fn grad1(hash: i32, x: f32) -> f32 {
    let h = hash & 15;
    let grad = 1.0 + (h & 7) as f32;
    if h & 8 != 0 {
        -grad * x
    } else {
        grad * x
    }
}

fn grad2(hash: i32, x: f32, y: f32) -> f32 {
    let h = hash & 7;
    let (u, v) = if h < 4 { (x, y) } else { (y, x) };
    (if h & 1 != 0 { -u } else { u }) + (if h & 2 != 0 { -2.0 * v } else { 2.0 * v })
}

/// `snoise1`: noise along a line, within about ±0.63.
pub fn simplex1(x: f32) -> f32 {
    let i0 = floor(x);
    let i1 = i0 + 1;
    let x0 = x - i0 as f32;
    let x1 = x0 - 1.0;

    let mut t0 = 1.0 - x0 * x0;
    t0 *= t0;
    let n0 = t0 * t0 * grad1(perm(i0), x0);

    let mut t1 = 1.0 - x1 * x1;
    t1 *= t1;
    let n1 = t1 * t1 * grad1(perm(i1), x1);
    0.25 * (n0 + n1)
}

/// `snoise2`: noise over a plane, within ±1.
pub fn simplex2(x: f32, y: f32) -> f32 {
    // The constants are doubles in the C code: products with them are
    // computed in double precision and then stored as floats.
    const F2: f64 = 0.366025403;
    const G2: f64 = 0.211324865;

    let s = ((x + y) as f64 * F2) as f32;
    let i = floor(x + s);
    let j = floor(y + s);

    let t = ((i + j) as f32 as f64 * G2) as f32;
    let x0 = x - (i as f32 - t);
    let y0 = y - (j as f32 - t);

    let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };
    let x1 = (x0 as f64 - i1 as f64 + G2) as f32;
    let y1 = (y0 as f64 - j1 as f64 + G2) as f32;
    let x2 = ((x0 - 1.0) as f64 + 2.0 * G2) as f32;
    let y2 = ((y0 - 1.0) as f64 + 2.0 * G2) as f32;

    let (ii, jj) = (i & 0xff, j & 0xff);
    let corner = |x: f32, y: f32, hash: i32| -> f32 {
        let t = 0.5 - x * x - y * y;
        if t < 0.0 {
            0.0
        } else {
            let t = t * t;
            t * t * grad2(hash, x, y)
        }
    };
    let n0 = corner(x0, y0, perm(ii + perm(jj)));
    let n1 = corner(x1, y1, perm(ii + i1 + perm(jj + j1)));
    let n2 = corner(x2, y2, perm(ii + 1 + perm(jj + 1)));
    40.0 * (n0 + n1 + n2)
}

/// The shake of a camera with the given values, `seconds` into the
/// animation: a transform to apply before the camera's own matrix. It
/// turns the camera, or moves it when the shake mode is set. `None` while
/// the camera does not shake.
pub fn camera_shake(values: &ValueSet, seconds: f64) -> Option<Mat4> {
    if !values.flag(ValueId::CamShake) {
        return None;
    }
    let number = |id| values.number(id);
    let shake = [
        simplex1((seconds * number(ValueId::CamShakeSpeedX)) as f32) as f64 * number(ValueId::CamShakeStrengthX),
        simplex2((seconds * number(ValueId::CamShakeSpeedY)) as f32, 1000.0) as f64 * number(ValueId::CamShakeStrengthY),
        simplex2((seconds * number(ValueId::CamShakeSpeedZ)) as f32, 2000.0) as f64 * number(ValueId::CamShakeStrengthZ),
    ];
    Some(if number(ValueId::CamShakeMode) != 0.0 {
        Mat4::build(shake, [0.0; 3], [1.0; 3])
    } else {
        Mat4::build([0.0; 3], shake, [1.0; 3])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mi_core::Value;

    #[test]
    fn noise_is_zero_on_the_lattice_and_smooth_between() {
        // Gradient noise vanishes at whole numbers.
        for x in [-3.0, 0.0, 1.0, 17.0, 300.0] {
            assert_eq!(simplex1(x), 0.0);
        }
        assert_eq!(simplex2(0.0, 0.0), 0.0);
        // Halfway between 0 and 1: both ends contribute (1 - 1/4)^4 times
        // their gradient (perm 151 -> 8, perm 160 -> 1) times ±1/2.
        let expected = 0.25 * 0.31640625 * (8.0 * 0.5 + 1.0 * -0.5);
        assert!((simplex1(0.5) - expected).abs() < 1e-6, "{}", simplex1(0.5));
        // Bounded and continuous.
        let mut previous = simplex1(0.0);
        for step in 1..2000 {
            let x = step as f32 * 0.01;
            let value = simplex1(x);
            assert!(value.abs() <= 0.64 && (value - previous).abs() < 0.08, "{x}: {value}");
            previous = value;
        }
        let mut previous = simplex2(0.0, 1000.0);
        let mut extent = 0.0f32;
        for step in 1..2000 {
            let x = step as f32 * 0.01;
            let value = simplex2(x, 1000.0);
            assert!(value.abs() <= 1.0 && (value - previous).abs() < 0.08, "{x}: {value}");
            extent = extent.max(value.abs());
            previous = value;
        }
        assert!(extent > 0.5, "the noise uses its range: {extent}");
        // The table wraps every 256 units.
        assert!((simplex1(3.3) - simplex1(259.3)).abs() < 1e-4);
    }

    #[test]
    fn cameras_shake_only_when_asked_and_in_their_mode() {
        let defaults = mi_format::project::ProjectFile::new(0.0, 1.0).defaults;
        assert!(camera_shake(&defaults, 1.3).is_none());

        let mut values = defaults.clone();
        values[ValueId::CamShake] = Value::Bool(true);
        values[ValueId::CamShakeStrengthX] = Value::Number(2.0);
        // Position, the default mode: it moves by the noise times the strength.
        let shift = camera_shake(&values, 1.3).unwrap().position();
        assert!((shift[0] - simplex1(1.3) as f64 * 2.0).abs() < 1e-9);
        assert!((shift[1] - simplex2(1.3, 1000.0) as f64).abs() < 1e-9);
        assert!((shift[2] - simplex2(1.3, 2000.0) as f64).abs() < 1e-9);
        // The speed scales time.
        values[ValueId::CamShakeSpeedX] = Value::Number(3.0);
        let faster = camera_shake(&values, 1.3).unwrap().position();
        assert!((faster[0] - simplex1(3.9) as f64 * 2.0).abs() < 1e-9);
        // At the start nothing has moved yet along X.
        assert_eq!(camera_shake(&values, 0.0).unwrap().position()[0], 0.0);
        // Rotation: the camera stays where it is and turns.
        values[ValueId::CamShakeMode] = Value::Number(0.0);
        let turn = camera_shake(&values, 1.3).unwrap();
        assert_eq!(turn.position(), [0.0; 3]);
        assert!(turn != Mat4::IDENTITY);
    }
}
