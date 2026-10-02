//! Paths: curves through the path point children of a path timeline
//! (`tl_update_path`, `spline_*`, `tl_path_offset_get_position`).

use crate::math::{self, Mat4, Vec3};

/// A point on a path with its frame: position, roll angle, scale, tangent
/// and normal. Stored like the original's 11-element arrays so that blending
/// two points blends every component.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PathPoint(pub [f64; 11]);

const ANGLE: usize = 3;
const SCALE: usize = 4;
const TANGENT: usize = 5;
const NORMAL: usize = 8;

/// Samples taken per spline stretch when measuring its length.
const LENGTH_STEPS: usize = 20;

impl PathPoint {
    pub fn new(position: Vec3, angle: f64, scale: f64) -> Self {
        let mut p = [0.0; 11];
        p[..3].copy_from_slice(&position);
        p[ANGLE] = angle;
        p[SCALE] = scale;
        Self(p)
    }

    pub fn position(&self) -> Vec3 {
        [self.0[0], self.0[1], self.0[2]]
    }

    /// Roll around the path in degrees.
    pub fn angle(&self) -> f64 {
        self.0[ANGLE]
    }

    pub fn scale(&self) -> f64 {
        self.0[SCALE]
    }

    pub fn tangent(&self) -> Vec3 {
        [self.0[TANGENT], self.0[TANGENT + 1], self.0[TANGENT + 2]]
    }

    pub fn normal(&self) -> Vec3 {
        [self.0[NORMAL], self.0[NORMAL + 1], self.0[NORMAL + 2]]
    }

    fn set_position(&mut self, p: Vec3) {
        self.0[..3].copy_from_slice(&p);
    }

    fn set_frame(&mut self, tangent: Vec3, normal: Vec3) {
        self.0[TANGENT..TANGENT + 3].copy_from_slice(&tangent);
        self.0[NORMAL..NORMAL + 3].copy_from_slice(&normal);
    }

    fn lerp(&self, other: &PathPoint, t: f64) -> PathPoint {
        let mut out = [0.0; 11];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = self.0[i] + (other.0[i] - self.0[i]) * t;
        }
        PathPoint(out)
    }
}

/// Shape settings of a path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathShape {
    pub closed: bool,
    pub smooth: bool,
    /// Extra samples per segment.
    pub detail: usize,
}

/// A path sampled at equal distances.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PathTable {
    /// Samples in the path's own space.
    pub points: Vec<PathPoint>,
    /// Length of the curve in path offset units (about 95% of the true arc
    /// length, see [`PathTable::build`]).
    pub length: f64,
    pub closed: bool,
    pub smooth: bool,
}

/// `percent`
fn percent(value: f64, start: f64, end: f64) -> f64 {
    if start == end {
        return if value < start { 1.0 } else { 0.0 };
    }
    ((value - start) / (end - start)).clamp(0.0, 1.0)
}

/// `mod_fix`: remainder that is never negative.
fn wrap(index: f64, count: usize) -> usize {
    index.rem_euclid(count as f64) as usize
}

/// `spline_subdivide`: inserts midpoints so that the control points become
/// the handles of quadratic curves between midpoints.
fn subdivide(points: &[PathPoint], closed: bool) -> Vec<PathPoint> {
    let amount = points.len();
    let mut out = Vec::with_capacity(amount * 2 + 1);
    if closed {
        out.push(points[0].lerp(&points[amount - 1], 0.5));
    }
    for (i, point) in points.iter().enumerate() {
        let next = if closed { (i + 1) % amount } else { (i + 1).min(amount - 1) };
        out.push(*point);
        if i > 0 || closed {
            out.push(point.lerp(&points[next], 0.5));
        }
    }
    out
}

/// `spline_get_point`: point at parameter `t`, where every two units of `t`
/// span one curve. `amount` limits the number of points taken into account;
/// 0 means all of them.
fn spline_point(t: f64, points: &[PathPoint], closed: bool, smooth: bool, amount: usize) -> PathPoint {
    let len = points.len();
    let amount = if amount == 0 { len } else { amount };

    let seg = (t / 2.0).floor() * 2.0;
    let mut curve_t = percent(t, seg, seg + 2.0);
    let index = |offset: f64| -> usize {
        if closed {
            wrap(seg + offset, amount)
        } else {
            (seg + offset).clamp(0.0, amount as f64 - 1.0) as usize
        }
    };
    let (p0, p1, p2) = (index(0.0), index(1.0), index(2.0));

    if amount == len {
        curve_t = percent(t, p0 as f64, p2 as f64);
    }

    // Before the start or past the end of an open path: continue straight.
    if !closed && (t <= 0.0 || t >= amount as f64 - 1.0) {
        let (i, pos) = if t <= 0.0 || len < 2 {
            let away = if len > 1 {
                math::normalize(math::sub(points[0].position(), points[1].position()))
            } else {
                [0.0; 3]
            };
            (0, math::add(points[0].position(), math::scale(away, t.abs())))
        } else {
            let i = len - 1;
            let tangent = points[i - 1].tangent();
            (i, math::add(points[i].position(), math::scale(tangent, (t - i as f64).abs())))
        };
        let mut point = points[i];
        point.set_position(pos);
        return point;
    }

    if smooth {
        let a = points[p0].lerp(&points[p1], curve_t);
        let b = points[p1].lerp(&points[p2], curve_t);
        a.lerp(&b, curve_t)
    } else if curve_t < 0.5 {
        points[p0].lerp(&points[p1], curve_t * 2.0)
    } else {
        points[p1].lerp(&points[p2], (curve_t - 0.5) * 2.0)
    }
}

/// `spline_make_frames`: gives every sample a tangent and a normal, carrying
/// the normal along the curve so that it does not flip, and rolling it by the
/// angle of the path points.
fn make_frames(points: &mut [PathPoint], closed: bool, smooth: bool) {
    let len = points.len();
    if len < 2 {
        return;
    }

    let mut p = spline_point(0.0, points, closed, smooth, 0);
    let mut pn = spline_point(1.0, points, closed, smooth, 0);
    let mut t = math::direction(p.position(), pn.position());
    let mut n = if t[2] == 1.0 || t[2] == -1.0 { [t[2], 0.0, 0.0] } else { math::normal(t, 0.0) };
    points[0].set_frame(t, n);
    p = pn;

    for i in 1..len {
        pn = spline_point(i as f64 + 1.0, points, closed, smooth, 0);
        let tn = math::direction(p.position(), pn.position());

        // Turn the previous normal by the change in direction.
        let axis = math::normalize(math::cross(t, tn));
        let cos = math::dot(t, tn);
        let nn = math::normalize(Mat4::axis_cos(axis, cos).transform_vector(n));

        // Roll around the tangent by the point's angle.
        let rn = math::rotate_axis_angle(nn, tn, pn.angle().to_radians());
        points[i].set_frame(tn, rn);

        p = pn;
        n = nn;
        t = tn;
    }

    // Make the end of a closed path meet its start.
    if closed && len >= 3 {
        let before = points[len - 3];
        let first = points[0];
        let tangent = math::normalize(math::add(before.tangent(), first.tangent()));
        let normal = math::normalize(math::add(before.normal(), first.normal()));
        points[len - 2].set_frame(tangent, normal);
        points[len - 1].set_frame(first.tangent(), first.normal());
    }
}

impl PathTable {
    /// Builds the table from the path's control points (`tl_update_path`).
    /// Fewer than two points give an empty table.
    pub fn build(control_points: &[PathPoint], shape: PathShape) -> PathTable {
        let mut table = PathTable { points: Vec::new(), length: 1.0, closed: shape.closed, smooth: shape.smooth };
        let count = control_points.len();
        if count < 2 {
            return table;
        }

        let samples = count + (count - 1 + shape.closed as usize) * shape.detail;
        let spline = subdivide(control_points, shape.closed);

        // Length of each stretch between consecutive spline points.
        //
        // The original samples each stretch with `for (j = 0; j <= 1; j += 0.05)`,
        // which stops at 0.95 because the sum of twenty 0.05s is just above
        // 1. Lengths therefore come out about 5% short. Path offsets saved in
        // projects are expressed in these units, so this is kept as is.
        let mut distances = Vec::with_capacity(spline.len());
        let mut total = 0.0;
        for i in 0..spline.len() {
            let mut previous = spline_point(i as f64, &spline, shape.closed, shape.smooth, 0);
            let mut distance = 0.0;
            for step in 0..LENGTH_STEPS {
                let j = step as f64 * 0.05;
                let sample = spline_point(i as f64 + j, &spline, shape.closed, shape.smooth, 0);
                distance += math::distance(previous.position(), sample.position());
                previous = sample;
            }
            let distance = distance.max(0.001);
            distances.push(distance);
            total += distance;
        }
        table.length = total;

        // Walk along the curve in equal steps.
        for i in 0..samples {
            let mut remaining = if samples > 1 { i as f64 / (samples - 1) as f64 * total } else { 0.0 };
            let mut j = 0;
            while j + 1 < distances.len() && remaining > distances[j] && remaining > 0.01 {
                remaining -= distances[j];
                j += 1;
            }
            let t = j as f64 + remaining / distances[j];
            table.points.push(spline_point(t, &spline, shape.closed, shape.smooth, 0));
        }

        make_frames(&mut table.points, shape.closed, shape.smooth);
        table
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// `tl_path_offset_get_position`: the point `offset` units along the
    /// path. Offsets outside an open path continue in a straight line; on a
    /// closed path they wrap around.
    pub fn at_offset(&self, offset: f64) -> PathPoint {
        if self.points.is_empty() {
            return PathPoint::default();
        }
        let last = self.points.len() - 1;
        let t = offset / self.length * last as f64;
        spline_point(t, &self.points, self.closed, self.smooth, if self.closed { last } else { 0 })
    }

    /// Transform that places an object at `offset` along the path, oriented
    /// along it, in the path's own space.
    pub fn transform_at(&self, offset: f64) -> Mat4 {
        let mut point = self.at_offset(offset);
        let rotation = Mat4::rotate_to(math::normalize(point.tangent()), math::normalize(point.normal()));

        // Beyond the ends of an open path the object stays at the end.
        if !self.closed {
            if offset <= 0.0 {
                point = self.at_offset(0.0);
            } else if offset >= self.length {
                point = self.at_offset(self.length);
            }
        }
        rotation.then(&Mat4::translation(point.position()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn straight() -> PathTable {
        let points = [
            PathPoint::new([0.0, 0.0, 0.0], 0.0, 1.0),
            PathPoint::new([10.0, 0.0, 0.0], 0.0, 1.0),
            PathPoint::new([20.0, 0.0, 0.0], 0.0, 1.0),
        ];
        PathTable::build(&points, PathShape { closed: false, smooth: true, detail: 6 })
    }

    fn close(a: Vec3, b: Vec3, eps: f64) -> bool {
        math::distance(a, b) < eps
    }

    #[test]
    fn too_few_points_give_an_empty_table() {
        let shape = PathShape { closed: false, smooth: true, detail: 6 };
        assert!(PathTable::build(&[], shape).is_empty());
        assert!(PathTable::build(&[PathPoint::new([1.0; 3], 0.0, 1.0)], shape).is_empty());
        assert_eq!(PathTable::default().at_offset(5.0), PathPoint::default());
    }

    #[test]
    fn straight_path_has_the_expected_length_and_samples() {
        let table = straight();
        assert_eq!(table.points.len(), 3 + 2 * 6);
        assert!(table.length > 18.5 && table.length < 20.0, "{}", table.length);
        assert!(close(table.points[0].position(), [0.0; 3], 1e-9));
        assert!(close(table.points.last().unwrap().position(), [20.0, 0.0, 0.0], 0.05));
        for point in &table.points {
            assert!(close(point.tangent(), [1.0, 0.0, 0.0], 1e-6), "{:?}", point.tangent());
            assert!(math::dot(point.normal(), point.tangent()).abs() < 1e-6);
            assert!((math::length(point.normal()) - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn offsets_move_along_the_path() {
        let table = straight();
        assert!(close(table.at_offset(0.0).position(), [0.0; 3], 1e-6));
        assert!(close(table.at_offset(table.length / 2.0).position(), [10.0, 0.0, 0.0], 1.0));
        // Past the end the point continues along the last tangent.
        let beyond = table.at_offset(table.length * 2.0).position();
        assert!(beyond[0] > 20.0 && beyond[1].abs() < 1e-6);
        // The transform keeps objects at the end.
        let m = table.transform_at(table.length * 2.0);
        assert!(close(m.position(), [20.0, 0.0, 0.0], 0.1));
        let start = table.transform_at(-5.0);
        assert!(close(start.position(), [0.0; 3], 1e-6));
        // Local +Y is carried onto the path direction.
        assert!(close(m.transform_vector([0.0, 1.0, 0.0]), [1.0, 0.0, 0.0], 1e-6));
    }

    #[test]
    fn closed_path_returns_to_its_start() {
        let points = [
            PathPoint::new([0.0, 0.0, 0.0], 0.0, 1.0),
            PathPoint::new([10.0, 0.0, 0.0], 0.0, 1.0),
            PathPoint::new([10.0, 10.0, 0.0], 0.0, 1.0),
            PathPoint::new([0.0, 10.0, 0.0], 0.0, 1.0),
        ];
        let table = PathTable::build(&points, PathShape { closed: true, smooth: false, detail: 4 });
        assert_eq!(table.points.len(), 4 + 4 * 4);
        assert!(table.length > 37.0 && table.length < 40.0, "{}", table.length);
        let first = table.points[0].position();
        let last = table.points.last().unwrap().position();
        assert!(close(first, last, 0.5), "{first:?} {last:?}");
        // Wrapping: one full lap later is the same place.
        let a = table.at_offset(3.0).position();
        let b = table.at_offset(3.0 + table.length).position();
        assert!(close(a, b, 0.5), "{a:?} {b:?}");
        // All samples lie in the plane of the square.
        assert!(table.points.iter().all(|p| p.position()[2].abs() < 1e-9));
    }

    #[test]
    fn smooth_path_cuts_corners_and_sharp_path_does_not() {
        let points = [
            PathPoint::new([0.0, 0.0, 0.0], 0.0, 1.0),
            PathPoint::new([10.0, 0.0, 0.0], 0.0, 1.0),
            PathPoint::new([10.0, 10.0, 0.0], 0.0, 1.0),
        ];
        let corner = [10.0, 0.0, 0.0];
        let nearest = |table: &PathTable| {
            table.points.iter().map(|p| math::distance(p.position(), corner)).fold(f64::MAX, f64::min)
        };
        let smooth = PathTable::build(&points, PathShape { closed: false, smooth: true, detail: 20 });
        let sharp = PathTable::build(&points, PathShape { closed: false, smooth: false, detail: 20 });
        assert!(nearest(&smooth) > 1.0, "{}", nearest(&smooth));
        assert!(nearest(&sharp) < 0.5, "{}", nearest(&sharp));
        assert!(smooth.length < sharp.length);
    }

    #[test]
    fn point_scale_and_angle_are_interpolated() {
        let points = [PathPoint::new([0.0; 3], 0.0, 1.0), PathPoint::new([10.0, 0.0, 0.0], 90.0, 3.0)];
        let table = PathTable::build(&points, PathShape { closed: false, smooth: true, detail: 6 });
        let mid = table.at_offset(table.length / 2.0);
        assert!((mid.scale() - 2.0).abs() < 0.25, "{}", mid.scale());
        assert!((mid.angle() - 45.0).abs() < 10.0, "{}", mid.angle());
    }
}
