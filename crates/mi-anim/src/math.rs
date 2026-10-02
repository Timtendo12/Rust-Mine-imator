//! Vector and matrix maths with the conventions of the original.
//!
//! The world is Z-up. Matrices follow GameMaker: sixteen numbers with the
//! translation in elements 12..14, applied to row vectors, so
//! `a.then(&b)` (`matrix_multiply(a, b)`) transforms by `a` first and `b`
//! second. Angles are in degrees and, as in GameMaker, positive angles turn
//! clockwise when looking down the axis from its positive end (+X towards
//! -Y for a Z rotation). The formulas are ported from
//! `CppProject/Render/Matrix.cpp` and the `matrix_*` / `vec3_*` scripts, so
//! results match the original to floating point precision.

pub type Vec3 = [f64; 3];

pub const X: usize = 0;
pub const Y: usize = 1;
pub const Z: usize = 2;

pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn scale(a: Vec3, s: f64) -> Vec3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

pub fn mul(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2]]
}

pub fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

pub fn length(a: Vec3) -> f64 {
    dot(a, a).sqrt()
}

pub fn distance(a: Vec3, b: Vec3) -> f64 {
    length(sub(a, b))
}

/// `vec3_normalize`: the zero vector stays zero.
pub fn normalize(a: Vec3) -> Vec3 {
    let len = length(a);
    if len == 0.0 {
        a
    } else {
        scale(a, 1.0 / len)
    }
}

pub fn lerp(a: Vec3, b: Vec3, t: f64) -> Vec3 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// `vec3_direction`: unit vector from `from` to `to`, or +Y when the points
/// coincide.
pub fn direction(from: Vec3, to: Vec3) -> Vec3 {
    let v = normalize(sub(to, from));
    if length(v) == 0.0 {
        [0.0, 1.0, 0.0]
    } else {
        v
    }
}

/// GameMaker's `lengthdir_x`.
pub fn lengthdir_x(len: f64, degrees: f64) -> f64 {
    len * degrees.to_radians().cos()
}

/// GameMaker's `lengthdir_y` (screen Y points down, hence the sign).
pub fn lengthdir_y(len: f64, degrees: f64) -> f64 {
    -len * degrees.to_radians().sin()
}

/// `lengthdir_z`
pub fn lengthdir_z(len: f64, degrees: f64) -> f64 {
    -lengthdir_y(len, degrees)
}

/// `vec3_normal`: a unit vector perpendicular to `v`, turned around it by
/// `degrees`.
pub fn normal(v: Vec3, degrees: f64) -> Vec3 {
    let [x, y, z] = v;
    if z.abs() != 1.0 {
        let cx = lengthdir_x(1.0, degrees) / (x * x + y * y + z * z).sqrt();
        let cy = lengthdir_y(1.0, degrees);
        normalize([-cx * x * z - cy * y, cy * x - cx * y * z, cx * (x * x + y * y)])
    } else {
        [lengthdir_x(1.0, degrees), lengthdir_y(1.0, degrees), 0.0]
    }
}

/// `vec3_rotate_axis_angle`: rotates `v` around the unit vector `axis`.
pub fn rotate_axis_angle(v: Vec3, axis: Vec3, radians: f64) -> Vec3 {
    let along = scale(axis, dot(v, axis));
    let rest = sub(v, along);
    add(along, add(scale(rest, radians.cos()), scale(cross(axis, rest), radians.sin())))
}

/// `point3D_project_plane`: projects `pos` on the plane through `plane_pos`
/// with unit normal `n`.
pub fn project_plane(pos: Vec3, plane_pos: Vec3, n: Vec3) -> Vec3 {
    sub(pos, scale(n, dot(n, pos) - dot(n, plane_pos)))
}

/// `point3D_angle_deg`: angle between two vectors in degrees.
pub fn angle_deg(from: Vec3, to: Vec3) -> f64 {
    let d = length(from) * length(to);
    if d < 0.0001 {
        0.0
    } else {
        (dot(from, to) / d).clamp(-1.0, 1.0).acos().to_degrees()
    }
}

/// `point3D_angle_signed`: like [`angle_deg`], negative when the rotation
/// from `from` to `to` runs clockwise seen along `axis`.
pub fn angle_signed(from: Vec3, to: Vec3, axis: Vec3) -> f64 {
    let side = dot(axis, cross(from, to));
    // GML's sign() gives 0 for 0, which makes parallel vectors yield 0.
    let sign = if side > 0.0 {
        1.0
    } else if side < 0.0 {
        -1.0
    } else {
        0.0
    };
    angle_deg(from, to) * sign
}

/// 4×4 transform in GameMaker layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4(pub [f64; 16]);

impl Default for Mat4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mat4 {
    pub const IDENTITY: Mat4 =
        Mat4([1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);

    /// Builds from rows written the mathematical way (translation in the
    /// last column), like the constructor of the original's `Matrix`.
    #[allow(clippy::too_many_arguments)]
    const fn from_rows(
        m0: f64, m4: f64, m8: f64, m12: f64,
        m1: f64, m5: f64, m9: f64, m13: f64,
        m2: f64, m6: f64, m10: f64, m14: f64,
        m3: f64, m7: f64, m11: f64, m15: f64,
    ) -> Mat4 {
        Mat4([m0, m1, m2, m3, m4, m5, m6, m7, m8, m9, m10, m11, m12, m13, m14, m15])
    }

    pub const fn translation(p: Vec3) -> Mat4 {
        Mat4::from_rows(1.0, 0.0, 0.0, p[0], 0.0, 1.0, 0.0, p[1], 0.0, 0.0, 1.0, p[2], 0.0, 0.0, 0.0, 1.0)
    }

    pub const fn scaling(s: Vec3) -> Mat4 {
        Mat4::from_rows(s[0], 0.0, 0.0, 0.0, 0.0, s[1], 0.0, 0.0, 0.0, 0.0, s[2], 0.0, 0.0, 0.0, 0.0, 1.0)
    }

    /// `Matrix::Rotation(axis, angle)` for a unit axis.
    fn axis_rotation(axis: Vec3, degrees: f64) -> Mat4 {
        let (s, c) = degrees.to_radians().sin_cos();
        let [x, y, z] = axis;
        Mat4::from_rows(
            x * x + (1.0 - x * x) * c, x * y * (1.0 - c) - z * s, x * z * (1.0 - c) + y * s, 0.0,
            x * y * (1.0 - c) + z * s, y * y + (1.0 - y * y) * c, y * z * (1.0 - c) - x * s, 0.0,
            x * z * (1.0 - c) - y * s, y * z * (1.0 - c) + x * s, z * z + (1.0 - z * z) * c, 0.0,
            0.0, 0.0, 0.0, 1.0,
        )
    }

    /// `Matrix::Rotation(pitch, roll, yaw)`: yaw around Z, then pitch around
    /// X, then roll around Y, for `rot = [x, y, z]` in degrees.
    fn euler(rot: Vec3) -> Mat4 {
        Mat4::axis_rotation([0.0, 0.0, 1.0], rot[Z])
            .then(&Mat4::axis_rotation([1.0, 0.0, 0.0], rot[X]))
            .then(&Mat4::axis_rotation([0.0, 1.0, 0.0], rot[Y]))
    }

    /// `matrix_build` / `matrix_create`: rotates, then scales along the
    /// (unrotated) axes, then translates. Scaling an object along its own
    /// axes is done by the caller by applying a scale matrix first, which is
    /// what the timeline transform does.
    pub fn build(pos: Vec3, rot: Vec3, sca: Vec3) -> Mat4 {
        let no_pos = pos == [0.0; 3];
        let no_rot = rot == [0.0; 3];
        let no_scale = sca == [1.0; 3];

        match (no_pos, no_rot, no_scale) {
            (true, true, true) => Mat4::IDENTITY,
            (false, true, true) => Mat4::translation(pos),
            (true, true, false) => Mat4::scaling(sca),
            (true, false, true) => Mat4::euler(rot).transposed(),
            _ => Mat4::scaling(sca).then(&Mat4::euler(rot)).transposed().then(&Mat4::translation(pos)),
        }
    }

    /// `matrix_multiply(self, other)`: applies `self` first, then `other`.
    pub fn then(&self, other: &Mat4) -> Mat4 {
        let (a, b) = (&self.0, &other.0);
        let mut p = [0.0; 16];
        for i in 0..4 {
            for j in 0..4 {
                p[i * 4 + j] = (0..4).map(|k| a[i * 4 + k] * b[k * 4 + j]).sum();
            }
        }
        Mat4(p)
    }

    pub fn transposed(&self) -> Mat4 {
        let m = &self.0;
        let mut t = [0.0; 16];
        for i in 0..4 {
            for j in 0..4 {
                t[i * 4 + j] = m[j * 4 + i];
            }
        }
        Mat4(t)
    }

    /// `matrix_position`
    pub fn position(&self) -> Vec3 {
        [self.0[12], self.0[13], self.0[14]]
    }

    pub fn set_position(&mut self, p: Vec3) {
        self.0[12] = p[0];
        self.0[13] = p[1];
        self.0[14] = p[2];
    }

    /// `point3D_mul_matrix`: transforms a point (translation applies).
    pub fn transform_point(&self, p: Vec3) -> Vec3 {
        let m = &self.0;
        [
            m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
            m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
            m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
        ]
    }

    /// `vec3_mul_matrix`: transforms a direction (no translation).
    pub fn transform_vector(&self, v: Vec3) -> Vec3 {
        let m = &self.0;
        [
            m[0] * v[0] + m[4] * v[1] + m[8] * v[2],
            m[1] * v[0] + m[5] * v[1] + m[9] * v[2],
            m[2] * v[0] + m[6] * v[1] + m[10] * v[2],
        ]
    }

    /// Lengths of the three basis vectors.
    pub fn axis_scale(&self) -> Vec3 {
        let m = &self.0;
        [
            (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt(),
            (m[4] * m[4] + m[5] * m[5] + m[6] * m[6]).sqrt(),
            (m[8] * m[8] + m[9] * m[9] + m[10] * m[10]).sqrt(),
        ]
    }

    /// `matrix_remove_rotation`: keeps scale and translation.
    pub fn remove_rotation(&mut self) {
        let s = self.axis_scale();
        let m = &mut self.0;
        m[0] = s[0];
        m[1] = 0.0;
        m[2] = 0.0;
        m[4] = 0.0;
        m[5] = s[1];
        m[6] = 0.0;
        m[8] = 0.0;
        m[9] = 0.0;
        m[10] = s[2];
    }

    /// `matrix_remove_scale`: makes the basis vectors unit length. A matrix
    /// with a zero axis is left alone.
    pub fn remove_scale(&mut self) {
        let s = self.axis_scale();
        if s[0] * s[1] * s[2] > 0.0 {
            let m = &mut self.0;
            for axis in 0..3 {
                for row in 0..3 {
                    m[axis * 4 + row] /= s[axis];
                }
            }
        }
    }

    /// `matrix_inverse`. A singular matrix yields the identity.
    pub fn inverse(&self) -> Mat4 {
        let m = &self.0;
        let mut inv = [0.0; 16];

        inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15] + m[9] * m[7] * m[14] + m[13] * m[6] * m[11] - m[13] * m[7] * m[10];
        inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15] - m[8] * m[7] * m[14] - m[12] * m[6] * m[11] + m[12] * m[7] * m[10];
        inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15] + m[8] * m[7] * m[13] + m[12] * m[5] * m[11] - m[12] * m[7] * m[9];
        inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14] - m[8] * m[6] * m[13] - m[12] * m[5] * m[10] + m[12] * m[6] * m[9];
        inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15] - m[9] * m[3] * m[14] - m[13] * m[2] * m[11] + m[13] * m[3] * m[10];
        inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15] + m[8] * m[3] * m[14] + m[12] * m[2] * m[11] - m[12] * m[3] * m[10];
        inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15] - m[8] * m[3] * m[13] - m[12] * m[1] * m[11] + m[12] * m[3] * m[9];
        inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14] + m[8] * m[2] * m[13] + m[12] * m[1] * m[10] - m[12] * m[2] * m[9];
        inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15] + m[5] * m[3] * m[14] + m[13] * m[2] * m[7] - m[13] * m[3] * m[6];
        inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15] - m[4] * m[3] * m[14] - m[12] * m[2] * m[7] + m[12] * m[3] * m[6];
        inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15] + m[4] * m[3] * m[13] + m[12] * m[1] * m[7] - m[12] * m[3] * m[5];
        inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14] - m[4] * m[2] * m[13] - m[12] * m[1] * m[6] + m[12] * m[2] * m[5];
        inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11] - m[5] * m[3] * m[10] - m[9] * m[2] * m[7] + m[9] * m[3] * m[6];
        inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11] + m[4] * m[3] * m[10] + m[8] * m[2] * m[7] - m[8] * m[3] * m[6];
        inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11] - m[4] * m[3] * m[9] - m[8] * m[1] * m[7] + m[8] * m[3] * m[5];
        inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10] + m[4] * m[2] * m[9] + m[8] * m[1] * m[6] - m[8] * m[2] * m[5];

        let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
        if det == 0.0 {
            return Mat4::IDENTITY;
        }
        Mat4(inv.map(|v| v / det))
    }

    /// `matrix_create_rotate_to`: rotation whose Y axis is `tangent` and
    /// whose Z axis is `normal`.
    pub fn rotate_to(tangent: Vec3, normal: Vec3) -> Mat4 {
        let b = normalize(cross(tangent, normal));
        Mat4([
            b[0], b[1], b[2], 0.0,
            tangent[0], tangent[1], tangent[2], 0.0,
            normal[0], normal[1], normal[2], 0.0,
            0.0, 0.0, 0.0, 1.0,
        ])
    }

    /// `matrix_create_axis_angle`: rotation around `axis` where the angle is
    /// given by its cosine.
    pub fn axis_cos(axis: Vec3, cos: f64) -> Mat4 {
        let sq = mul(axis, axis);
        let rest = 1.0 - cos;
        let xy = axis[X] * axis[Y] * rest;
        let yz = axis[Y] * axis[Z] * rest;
        let zx = axis[X] * axis[Z] * rest;
        let s = (1.0 - cos * cos).max(0.0).sqrt();
        let a = scale(axis, s);
        Mat4([
            sq[X] + (1.0 - sq[X]) * cos, xy + a[Z], zx - a[Y], 0.0,
            xy - a[Z], sq[Y] + (1.0 - sq[Y]) * cos, yz + a[X], 0.0,
            zx + a[Y], yz - a[X], sq[Z] + (1.0 - sq[Z]) * cos, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ])
    }

    /// Single precision copy for the renderer.
    pub fn to_f32(&self) -> [f32; 16] {
        self.0.map(|v| v as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3) -> bool {
        distance(a, b) < 1e-9
    }

    fn mat_close(a: &Mat4, b: &Mat4) -> bool {
        a.0.iter().zip(&b.0).all(|(x, y)| (x - y).abs() < 1e-9)
    }

    #[test]
    fn build_translates_last() {
        let m = Mat4::build([10.0, 20.0, 30.0], [0.0; 3], [2.0, 2.0, 2.0]);
        assert!(close(m.transform_point([1.0, 1.0, 1.0]), [12.0, 22.0, 32.0]));
        assert_eq!(m.position(), [10.0, 20.0, 30.0]);
    }

    #[test]
    fn rotation_directions() {
        // Positive Z rotation turns +X towards -Y.
        let rz = Mat4::build([0.0; 3], [0.0, 0.0, 90.0], [1.0; 3]);
        assert!(close(rz.transform_point([1.0, 0.0, 0.0]), [0.0, -1.0, 0.0]));
        // Positive X rotation turns +Y towards -Z.
        let rx = Mat4::build([0.0; 3], [90.0, 0.0, 0.0], [1.0; 3]);
        assert!(close(rx.transform_point([0.0, 1.0, 0.0]), [0.0, 0.0, -1.0]));
        // Positive Y rotation turns +Z towards -X.
        let ry = Mat4::build([0.0; 3], [0.0, 90.0, 0.0], [1.0; 3]);
        assert!(close(ry.transform_point([0.0, 0.0, 1.0]), [-1.0, 0.0, 0.0]));
    }

    #[test]
    fn build_scales_after_rotating() {
        // A point on the local Y axis is turned onto +X and then stretched by
        // the X scale.
        let m = Mat4::build([0.0; 3], [0.0, 0.0, 90.0], [2.0, 1.0, 1.0]);
        assert!(close(m.transform_point([0.0, 1.0, 0.0]), [2.0, 0.0, 0.0]));
        assert!(close(m.transform_point([1.0, 0.0, 0.0]), [0.0, -1.0, 0.0]));
        // Scaling along the object's own axes: scale first, then the rest.
        let local = Mat4::scaling([2.0, 1.0, 1.0]).then(&Mat4::build([0.0; 3], [0.0, 0.0, 90.0], [1.0; 3]));
        assert!(close(local.transform_point([1.0, 0.0, 0.0]), [0.0, -2.0, 0.0]));
    }

    #[test]
    fn rotation_order_is_y_then_x_then_z() {
        // With all three angles the point is rolled (Y) first, pitched (X)
        // second and yawed (Z) last.
        let rot = [30.0, 40.0, 50.0];
        let combined = Mat4::build([0.0; 3], rot, [1.0; 3]);
        let ry = Mat4::build([0.0; 3], [0.0, rot[1], 0.0], [1.0; 3]);
        let rx = Mat4::build([0.0; 3], [rot[0], 0.0, 0.0], [1.0; 3]);
        let rz = Mat4::build([0.0; 3], [0.0, 0.0, rot[2]], [1.0; 3]);
        assert!(mat_close(&combined, &ry.then(&rx).then(&rz)));
    }

    #[test]
    fn then_applies_left_to_right() {
        let move_x = Mat4::translation([5.0, 0.0, 0.0]);
        let turn = Mat4::build([0.0; 3], [0.0, 0.0, 90.0], [1.0; 3]);
        // Move, then rotate around the origin.
        assert!(close(move_x.then(&turn).transform_point([0.0; 3]), [0.0, -5.0, 0.0]));
        // Rotate, then move.
        assert!(close(turn.then(&move_x).transform_point([0.0; 3]), [5.0, 0.0, 0.0]));
    }

    #[test]
    fn inverse_undoes_a_transform() {
        let m = Mat4::build([3.0, -4.0, 5.0], [20.0, 30.0, 40.0], [1.0, 2.0, 0.5]);
        assert!(mat_close(&m.then(&m.inverse()), &Mat4::IDENTITY));
        assert_eq!(Mat4([0.0; 16]).inverse(), Mat4::IDENTITY);
    }

    #[test]
    fn removing_rotation_and_scale() {
        let rotation = Mat4::build([1.0, 2.0, 3.0], [10.0, 20.0, 30.0], [1.0; 3]);
        let mut m = Mat4::scaling([2.0, 3.0, 4.0]).then(&rotation);
        assert!(close(m.axis_scale(), [2.0, 3.0, 4.0]));

        let mut unscaled = m;
        unscaled.remove_scale();
        assert!(close(unscaled.axis_scale(), [1.0; 3]));
        assert_eq!(unscaled.position(), [1.0, 2.0, 3.0]);

        m.remove_rotation();
        assert!(mat_close(&m, &Mat4::build([1.0, 2.0, 3.0], [0.0; 3], [2.0, 3.0, 4.0])));

        let mut flat = Mat4::scaling([1.0, 0.0, 1.0]);
        flat.remove_scale();
        assert_eq!(flat, Mat4::scaling([1.0, 0.0, 1.0]));
    }

    #[test]
    fn vector_helpers() {
        assert_eq!(direction([1.0; 3], [1.0; 3]), [0.0, 1.0, 0.0]);
        assert!(close(direction([0.0; 3], [0.0, 0.0, 5.0]), [0.0, 0.0, 1.0]));
        assert_eq!(normalize([0.0; 3]), [0.0; 3]);

        let n = normal([1.0, 0.0, 0.0], 0.0);
        assert!(dot(n, [1.0, 0.0, 0.0]).abs() < 1e-12 && (length(n) - 1.0).abs() < 1e-12);
        assert!(close(normal([0.0, 0.0, 1.0], 0.0), [1.0, 0.0, 0.0]));

        let r = rotate_axis_angle([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], std::f64::consts::FRAC_PI_2);
        assert!(close(r, [0.0, 1.0, 0.0]));

        assert!(close(project_plane([3.0, 4.0, 5.0], [0.0; 3], [0.0, 0.0, 1.0]), [3.0, 4.0, 0.0]));
        assert!((angle_deg([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]) - 90.0).abs() < 1e-9);
        assert!((angle_signed([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]) + 90.0).abs() < 1e-9);
        assert_eq!(angle_signed([1.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 0.0, 1.0]), 0.0);
    }

    #[test]
    fn rotate_to_maps_axes() {
        let m = Mat4::rotate_to([0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
        assert!(close(m.transform_vector([0.0, 1.0, 0.0]), [0.0, 0.0, 1.0]));
        assert!(close(m.transform_vector([0.0, 0.0, 1.0]), [1.0, 0.0, 0.0]));
    }

    #[test]
    fn lengthdir_matches_gamemaker() {
        assert!((lengthdir_x(2.0, 0.0) - 2.0).abs() < 1e-12);
        assert!((lengthdir_y(2.0, 90.0) + 2.0).abs() < 1e-12);
        assert!((lengthdir_z(2.0, 90.0) - 2.0).abs() < 1e-12);
    }
}
