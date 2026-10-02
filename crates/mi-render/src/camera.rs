//! Cameras (`render_update_camera`, `render_set_projection`,
//! `camera_work_set_from`).
//!
//! The world is Z-up and left-handed: seen from a camera looking along +Y
//! with Z up, +X is to the left. (Minecraft is right-handed with Y up; the
//! program swaps Y and Z, which flips the handedness, so worlds look the
//! same as in the game.) Depth runs from 0 at the near plane to 1 at the far
//! plane.

use glam::{Mat4, Vec3};

/// Near clipping distance (`clip_near`).
pub const CLIP_NEAR: f32 = 1.0;

/// A camera ready for rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub from: Vec3,
    pub to: Vec3,
    pub up: Vec3,
    /// Vertical field of view in degrees.
    pub fov: f32,
    pub near: f32,
    pub far: f32,
}

fn lengthdir_x(len: f32, degrees: f32) -> f32 {
    len * degrees.to_radians().cos()
}

fn lengthdir_y(len: f32, degrees: f32) -> f32 {
    -len * degrees.to_radians().sin()
}

impl Camera {
    pub fn view(&self) -> Mat4 {
        Mat4::look_at_lh(self.from, self.to, self.up)
    }

    /// `aspect` is width divided by height.
    pub fn projection(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_lh(self.fov.max(1.0).to_radians(), aspect, self.near, self.far)
    }

    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        self.projection(aspect) * self.view()
    }

    /// Camera of a camera timeline from its world matrix (GameMaker layout):
    /// it looks along its local +Y with its local +Z up.
    pub fn from_matrix(matrix: &[f32; 16], fov: f32, far: f32) -> Camera {
        let m = Mat4::from_cols_array(matrix);
        let from = m.transform_point3(Vec3::ZERO);
        Camera {
            from,
            to: m.transform_point3(Vec3::Y),
            up: Vec3::new(matrix[8], matrix[9], matrix[10]),
            fov: fov.max(1.0),
            near: CLIP_NEAR,
            far,
        }
    }
}

/// The editor's free camera (`cam_work_*`). It orbits a focus point; panning
/// moves the camera without moving the focus, so the direction it looks in
/// is kept separately from the orbit angles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkCamera {
    pub focus: Vec3,
    /// Orbit angle around the vertical axis in degrees.
    pub angle_xy: f32,
    /// Orbit elevation in degrees; positive puts the camera above the focus.
    pub angle_z: f32,
    pub roll: f32,
    /// Distance from the focus.
    pub zoom: f32,
    /// Direction the camera looks in, as the orbit angle it corresponds to.
    pub look_xy: f32,
    /// Elevation of the look direction; negative looks down.
    pub look_z: f32,
}

impl Default for WorkCamera {
    /// `camera_work_reset`
    fn default() -> Self {
        Self::orbiting(Vec3::new(0.0, 0.0, 16.0), 315.0, 5.0, 0.0, 100.0)
    }
}

/// Limit of the elevation angles, just short of straight up or down.
const MAX_ELEVATION: f32 = 89.9;

impl WorkCamera {
    /// Field of view of the work camera, which is fixed.
    pub const FOV: f32 = 45.0;

    /// A camera looking at `focus` from the given orbit position, which is
    /// how projects store it.
    pub fn orbiting(focus: Vec3, angle_xy: f32, angle_z: f32, roll: f32, zoom: f32) -> Self {
        Self {
            focus,
            angle_xy,
            angle_z,
            roll,
            zoom,
            look_xy: angle_xy,
            look_z: (-angle_z).clamp(-MAX_ELEVATION, MAX_ELEVATION),
        }
    }

    /// Position of the camera (`camera_work_set_from`).
    pub fn position(&self) -> Vec3 {
        let flat = lengthdir_x(1.0, self.angle_z);
        self.focus
            + Vec3::new(
                lengthdir_x(self.zoom, self.angle_xy) * flat,
                lengthdir_y(self.zoom, self.angle_xy) * flat,
                -lengthdir_y(self.zoom, self.angle_z),
            )
    }

    fn look_direction(&self) -> Vec3 {
        let flat = lengthdir_x(1.0, self.look_z);
        Vec3::new(
            lengthdir_x(1.0, self.look_xy + 180.0) * flat,
            lengthdir_y(1.0, self.look_xy + 180.0) * flat,
            -lengthdir_y(1.0, self.look_z),
        )
    }

    /// The camera for rendering (`render_update_camera`).
    pub fn camera(&self, far: f32) -> Camera {
        let from = self.position();
        let direction = self.look_direction();

        // Up vector perpendicular to the view direction, rolled.
        let (x, y, z) = (direction.x, direction.y, direction.z);
        let cx = lengthdir_x(1.0, -self.roll) / direction.length();
        let cy = lengthdir_y(1.0, -self.roll);
        let up = Vec3::new(-cx * x * z - cy * y, cy * x - cx * y * z, cx * (x * x + y * y));

        Camera { from, to: from + direction, up, fov: Self::FOV, near: CLIP_NEAR, far }
    }

    /// Orbits around the focus by a mouse movement in pixels
    /// (`camera_control_rotate`).
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        let (mx, my) = (-dx / 4.0, dy / 4.0);
        self.angle_xy += mx;
        self.angle_z = (self.angle_z + my).clamp(-MAX_ELEVATION, MAX_ELEVATION);
        self.look_xy += mx;
        self.look_z = (self.look_z - my).clamp(-MAX_ELEVATION, MAX_ELEVATION);
    }

    /// Zooms by mouse wheel steps; positive steps move away
    /// (`view_update`). The distance stays within the clipping range.
    pub fn zoom_by(&mut self, steps: f32, far: f32) {
        self.zoom = (self.zoom * (1.0 + 0.25 * steps)).clamp(CLIP_NEAR, far.max(CLIP_NEAR));
    }

    /// Slides the camera sideways and up or down by a mouse movement in
    /// pixels, keeping the direction it looks in (`camera_control_pan`).
    /// `move_speed` is the program's movement speed setting.
    pub fn pan(&mut self, dx: f32, dy: f32, move_speed: f32) {
        let scale = 0.075 * (self.zoom / 50.0) * 4.0 * move_speed;
        let mx = -(dx / 8.0) * scale;
        let my = (dy / 8.0) * scale;

        // Right and up of the view, from the look angles.
        let direction = self.look_direction();
        let right = Vec3::Z.cross(direction).normalize_or_zero();
        let up = direction.cross(right);
        let from = self.position() + right * mx + up * my;

        // The focus stays; the orbit is recomputed around it
        // (`camera_work_set_angle`).
        let offset = from - self.focus;
        let flat = (offset.x * offset.x + offset.y * offset.y).sqrt();
        self.angle_xy = (-offset.y).atan2(offset.x).to_degrees();
        self.angle_z = offset.z.atan2(flat).to_degrees().clamp(-MAX_ELEVATION, MAX_ELEVATION);
        self.zoom = offset.length();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ndc(camera: &Camera, point: Vec3) -> Vec3 {
        camera.view_projection(16.0 / 9.0).project_point3(point)
    }

    #[test]
    fn work_camera_looks_at_its_focus() {
        let work = WorkCamera::default();
        let camera = work.camera(30000.0);
        assert!((camera.from.distance(work.focus) - 100.0).abs() < 1e-3);
        // Above the focus for a positive elevation.
        assert!(camera.from.z > work.focus.z);
        let centre = ndc(&camera, work.focus);
        assert!(centre.x.abs() < 1e-4 && centre.y.abs() < 1e-4, "{centre:?}");
        assert!(centre.z > 0.0 && centre.z < 1.0);
    }

    #[test]
    fn up_is_up_and_x_is_left() {
        // Looking along +Y from behind the origin.
        let camera = Camera { from: Vec3::new(0.0, -100.0, 0.0), to: Vec3::ZERO, up: Vec3::Z, fov: 45.0, near: 1.0, far: 1000.0 };
        assert!(ndc(&camera, Vec3::new(0.0, 0.0, 10.0)).y > 0.0);
        assert!(ndc(&camera, Vec3::new(10.0, 0.0, 0.0)).x < 0.0);
        // Nearer points have smaller depth.
        assert!(ndc(&camera, Vec3::new(0.0, -50.0, 0.0)).z < ndc(&camera, Vec3::new(0.0, 50.0, 0.0)).z);
    }

    #[test]
    fn roll_tilts_the_horizon() {
        let level = WorkCamera::default().camera(1000.0);
        let rolled = WorkCamera { roll: 90.0, ..Default::default() }.camera(1000.0);
        assert_eq!(WorkCamera::default().look_z, -5.0);
        assert!(level.up.z > 0.9);
        assert!(rolled.up.z.abs() < 0.1);
        assert!(level.up.dot(level.to - level.from).abs() < 1e-4);
    }

    #[test]
    fn timeline_camera_looks_along_its_y_axis() {
        // Identity matrix moved to (5, 0, 0).
        let mut matrix = Mat4::IDENTITY.to_cols_array();
        matrix[12] = 5.0;
        let camera = Camera::from_matrix(&matrix, 60.0, 500.0);
        assert_eq!(camera.from, Vec3::new(5.0, 0.0, 0.0));
        assert_eq!(camera.to, Vec3::new(5.0, 1.0, 0.0));
        assert_eq!(camera.up, Vec3::Z);
        assert_eq!(Camera::from_matrix(&matrix, 0.0, 500.0).fov, 1.0);
    }

    #[test]
    fn orbiting_keeps_the_focus_centred() {
        let mut work = WorkCamera::default();
        work.orbit(120.0, -40.0);
        assert_eq!(work.angle_xy, 285.0);
        assert_eq!(work.angle_z, -5.0);
        let camera = work.camera(30000.0);
        let centre = ndc(&camera, work.focus);
        assert!(centre.x.abs() < 1e-3 && centre.y.abs() < 1e-3, "{centre:?}");
        assert!((camera.from.distance(work.focus) - 100.0).abs() < 1e-3);

        // Elevation stops short of the poles.
        work.orbit(0.0, 10000.0);
        assert_eq!(work.angle_z, 89.9);
        assert_eq!(work.look_z, -89.9);
    }

    #[test]
    fn zooming_changes_the_distance_within_limits() {
        let mut work = WorkCamera::default();
        work.zoom_by(1.0, 30000.0);
        assert_eq!(work.zoom, 125.0);
        work.zoom_by(-1.0, 30000.0);
        assert_eq!(work.zoom, 93.75);
        for _ in 0..100 {
            work.zoom_by(-1.0, 30000.0);
        }
        assert_eq!(work.zoom, CLIP_NEAR);
        for _ in 0..100 {
            work.zoom_by(1.0, 500.0);
        }
        assert_eq!(work.zoom, 500.0);
    }

    #[test]
    fn panning_slides_the_camera_without_turning_it() {
        let mut work = WorkCamera::orbiting(Vec3::ZERO, 90.0, 0.0, 0.0, 100.0);
        let before = work.camera(30000.0);
        work.pan(80.0, 0.0, 1.0);
        let after = work.camera(30000.0);

        let moved = after.from - before.from;
        let direction = (before.to - before.from).normalize();
        assert!(moved.length() > 1.0);
        assert!(moved.dot(direction).abs() < 1e-3, "moves across the view, not along it");
        assert!(moved.z.abs() < 1e-3, "a horizontal drag does not change the height");
        let direction_after = (after.to - after.from).normalize();
        assert!(direction.distance(direction_after) < 1e-4, "the view direction is unchanged");
        assert_eq!(work.focus, Vec3::ZERO);

        // Dragging right moves the scene right, so the camera moves left on
        // screen: the old position is now on the right.
        assert!(ndc(&after, before.from + direction * 100.0).x > 0.0);

        // A vertical drag moves the camera along its up axis.
        let mut work = WorkCamera::orbiting(Vec3::ZERO, 90.0, 0.0, 0.0, 100.0);
        work.pan(0.0, 80.0, 1.0);
        assert!(work.camera(30000.0).from.z > 1.0);
    }
}
