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

/// The editor's free camera: it orbits a focus point (`cam_work_*`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkCamera {
    pub focus: Vec3,
    /// Angle around the vertical axis in degrees.
    pub angle_xy: f32,
    /// Elevation in degrees; positive looks down on the focus.
    pub angle_z: f32,
    pub roll: f32,
    /// Distance from the focus.
    pub zoom: f32,
}

impl Default for WorkCamera {
    /// `camera_work_reset`
    fn default() -> Self {
        Self { focus: Vec3::new(0.0, 0.0, 16.0), angle_xy: 315.0, angle_z: 5.0, roll: 0.0, zoom: 100.0 }
    }
}

impl WorkCamera {
    /// Field of view of the work camera, which is fixed.
    pub const FOV: f32 = 45.0;

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

    /// The camera for rendering. The look direction is the reverse of the
    /// orbit direction, with the elevation limited to just short of straight
    /// up or down as the original does when loading.
    pub fn camera(&self, far: f32) -> Camera {
        let from = self.position();
        let look_xy = self.angle_xy;
        let look_z = (-self.angle_z).clamp(-89.9, 89.9);
        let flat = lengthdir_x(1.0, look_z);
        let direction = Vec3::new(
            lengthdir_x(1.0, look_xy + 180.0) * flat,
            lengthdir_y(1.0, look_xy + 180.0) * flat,
            -lengthdir_y(1.0, look_z),
        );

        // Up vector perpendicular to the view direction, rolled.
        let (x, y, z) = (direction.x, direction.y, direction.z);
        let cx = lengthdir_x(1.0, -self.roll) / direction.length();
        let cy = lengthdir_y(1.0, -self.roll);
        let up = Vec3::new(-cx * x * z - cy * y, cy * x - cx * y * z, cx * (x * x + y * y));

        Camera { from, to: from + direction, up, fov: Self::FOV, near: CLIP_NEAR, far }
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
}
