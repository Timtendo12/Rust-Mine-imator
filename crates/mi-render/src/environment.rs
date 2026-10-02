//! Sky state derived from the background settings: where the sun is and
//! what colour the light, the ambient light and the sky have
//! (`background_sky_update_sun`, `background_sky_night_alpha`,
//! `background_sky_rise_set_alpha`, `app_update_animate`), and the colours
//! that depend on where the camera looks (`background_sky_update`,
//! `render_world_background`).

use glam::Vec3;

/// Colours as linear-looking 0..1 triples of the stored 8-bit values; the
/// shader applies gamma itself, like the original.
pub type Rgb = [f32; 3];

/// Background settings the lighting depends on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkySettings {
    /// Time of day as an angle in degrees; 0 is noon.
    pub sky_time: f32,
    /// Rotation of the sun's path around the vertical axis in degrees.
    pub sky_rotation: f32,
    pub sunlight_strength: f32,
    pub sunlight_color: Rgb,
    pub ambient_color: Rgb,
    pub night_color: Rgb,
    pub sky_color: Rgb,
    /// Redden the light at sunrise and sunset.
    pub twilight: bool,
    pub render_distance: f32,
}

/// Lighting of a frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lighting {
    /// Unit vector from the scene towards the sun.
    pub sun_direction: Vec3,
    /// Sun colour multiplied by its strength.
    pub sun_color: Rgb,
    pub ambient_color: Rgb,
    /// Colour the sky is cleared with.
    pub sky_color: Rgb,
    /// 0 by day, 1 at night.
    pub night: f32,
}

fn lengthdir_x(len: f32, degrees: f32) -> f32 {
    len * degrees.to_radians().cos()
}

fn lengthdir_y(len: f32, degrees: f32) -> f32 {
    -len * degrees.to_radians().sin()
}

fn lengthdir_z(len: f32, degrees: f32) -> f32 {
    -lengthdir_y(len, degrees)
}

/// `percent`
fn percent(value: f32, start: f32, end: f32) -> f32 {
    if start == end {
        return if value < start { 1.0 } else { 0.0 };
    }
    ((value - start) / (end - start)).clamp(0.0, 1.0)
}

fn smoothstep(x: f32) -> f32 {
    x * x * (3.0 - 2.0 * x)
}

fn merge(a: Rgb, b: Rgb, amount: f32) -> Rgb {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * amount)
}

/// `point_direction` of the camera's view on the ground plane, relative to
/// the sky's rotation: the angle the sunrise and sunset glows are judged by.
pub fn camera_sky_angle(from: Vec3, to: Vec3, sky_rotation: f32) -> f32 {
    (-(to.y - from.y)).atan2(to.x - from.x).to_degrees() - sky_rotation
}

/// `angle_difference_fix`: the signed difference of two angles, -180..180.
fn angle_difference(a: f32, b: f32) -> f32 {
    (a - b + 180.0).rem_euclid(360.0) - 180.0
}

/// How much of the glow in direction `towards` (degrees) a camera looking
/// along `angle` sees: 1 straight at it, 0 with its back to it.
fn facing(angle: f32, towards: f32) -> f32 {
    (1.0 - angle_difference(angle, towards).abs() / 180.0).clamp(0.0, 1.0)
}

const SUNSET_START: Rgb = [0xB2 as f32 / 255.0, 0x35 as f32 / 255.0, 0x3B as f32 / 255.0];
const SUNSET_END: Rgb = [0xC0 as f32 / 255.0, 0x4E as f32 / 255.0, 0x37 as f32 / 255.0];

impl SkySettings {
    /// The colour of the fog when the project does not set one
    /// (`background_fog_color_final`): the sky colour, hazy by day, nearly
    /// black at night and reddened towards the rising or setting sun.
    /// `camera_angle` is [`camera_sky_angle`].
    pub fn fog_color(&self, camera_angle: f32) -> Rgb {
        let night = self.night_alpha();
        let mut color = self.sky_color;
        color = merge(color, merge(color, [0.0; 3], 0.95), night);
        color = merge(color, [1.0; 3], (1.0 - night) * 0.58);
        color = merge(color, merge([0.0, 0.0, 1.0], [0.0; 3], 0.3), night * 0.05);
        if self.twilight {
            let set = self.rise_set_alpha(false);
            color = merge(color, merge(SUNSET_START, SUNSET_END, set), set * facing(camera_angle, 90.0));
            let rise = self.rise_set_alpha(true);
            color = merge(color, merge(SUNSET_START, SUNSET_END, rise), rise * facing(camera_angle, 270.0));
        }
        color
    }

    /// What the sunrise or sunset adds to the whole background when the
    /// camera looks towards it (`render_world_background`).
    pub fn twilight_glow(&self, camera_angle: f32, fog_color: Rgb) -> Rgb {
        if !self.twilight {
            return [0.0; 3];
        }
        let mut glow = [0.0; 3];
        glow = merge(glow, fog_color, self.rise_set_alpha(false) * facing(camera_angle, 90.0) * 0.25);
        glow = merge(glow, fog_color, self.rise_set_alpha(true) * facing(camera_angle, 270.0) * 0.25);
        glow
    }

    /// How visible the sun disc is: it fades out just below the horizon.
    pub fn sun_visibility(&self) -> f32 {
        percent(self.sun_direction().z, -0.15, 0.0)
    }

    /// How visible the moon is, opposite the sun.
    pub fn moon_visibility(&self) -> f32 {
        percent(-self.sun_direction().z, -0.15, 0.0)
    }

    /// Direction towards the sun.
    pub fn sun_direction(&self) -> Vec3 {
        let range = self.render_distance / 8.0;
        let flat = lengthdir_x(1.0, self.sky_time + 90.0);
        let mut x = lengthdir_x(range, self.sky_rotation - 90.0) * flat;
        let y = lengthdir_y(range, self.sky_rotation - 90.0) * flat;
        let z = lengthdir_z(range, self.sky_time + 90.0);
        // Exactly overhead the direction would have no horizontal part.
        if self.sky_time.rem_euclid(360.0) == 0.0 {
            x += 0.1;
        }
        Vec3::new(x, y, z).normalize_or_zero()
    }

    /// How far into the night it is: 0 while the sun is well above the
    /// horizon, 1 once it is below.
    pub fn night_alpha(&self) -> f32 {
        let height = self.sun_direction().z.max(0.0);
        smoothstep(percent(1.0 - height, 0.85, 1.0))
    }

    /// Strength of the sunrise (`rise`) or sunset glow.
    pub fn rise_set_alpha(&self, rise: bool) -> f32 {
        let time = self.sky_time.rem_euclid(360.0);
        let d = percent(self.sun_direction().z, -0.175, 0.325);
        let mut alpha = if d > 0.5 { percent(d, 1.0, 0.5) } else { percent(d, 0.0, 0.5) };
        if (rise && time < 180.0) || (!rise && time > 180.0) {
            alpha = 0.0;
        }
        smoothstep(alpha)
    }

    /// The lighting these settings result in.
    pub fn lighting(&self) -> Lighting {
        let night = self.night_alpha();
        let glow = self.rise_set_alpha(true).max(self.rise_set_alpha(false));
        let tint = if self.twilight { [1.0, 0.0, 0.0] } else { [1.0, 1.0, 1.0] };
        let twilight_color = merge(self.sunlight_color, tint, glow * 0.75);
        let sun = merge(twilight_color, [0.0; 3], night);
        Lighting {
            sun_direction: self.sun_direction(),
            sun_color: sun.map(|c| c * self.sunlight_strength),
            ambient_color: merge(self.ambient_color, self.night_color, night),
            // The night sky is #020204.
            sky_color: merge(self.sky_color, [2.0 / 255.0, 2.0 / 255.0, 4.0 / 255.0], night),
            night,
        }
    }
}

impl Default for SkySettings {
    /// The background of a new project.
    fn default() -> Self {
        let rgb = |r: u8, g: u8, b: u8| [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0];
        Self {
            sky_time: -45.0,
            sky_rotation: 0.0,
            sunlight_strength: 1.0,
            sunlight_color: rgb(255, 247, 228),
            ambient_color: rgb(102, 112, 140),
            night_color: rgb(14, 14, 24),
            sky_color: rgb(120, 167, 255),
            twilight: true,
            render_distance: 30000.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(sky_time: f32) -> SkySettings {
        SkySettings { sky_time, ..Default::default() }
    }

    #[test]
    fn sun_is_overhead_at_noon_and_below_at_midnight() {
        assert!(at(0.0).sun_direction().z > 0.999);
        assert!(at(180.0).sun_direction().z < -0.999);
        // The default time is mid-morning or mid-afternoon: 45 degrees up.
        let default = SkySettings::default().sun_direction();
        assert!((default.z - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-3, "{default:?}");
        assert!((default.length() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn sky_rotation_turns_the_sun_path() {
        let a = SkySettings { sky_time: 90.0, sky_rotation: 0.0, ..Default::default() }.sun_direction();
        let b = SkySettings { sky_time: 90.0, sky_rotation: 90.0, ..Default::default() }.sun_direction();
        assert!(a.z.abs() < 1e-4 && b.z.abs() < 1e-4);
        assert!(a.dot(b).abs() < 1e-4, "rotating by 90 degrees gives a perpendicular direction");
    }

    #[test]
    fn day_and_night() {
        let day = at(0.0).lighting();
        assert_eq!(day.night, 0.0);
        assert_eq!(day.sky_color, SkySettings::default().sky_color);
        assert!(day.sun_color[0] > 0.9);

        let night = at(180.0).lighting();
        assert_eq!(night.night, 1.0);
        assert_eq!(night.sun_color, [0.0; 3]);
        let expected = SkySettings::default().night_color;
        assert!((0..3).all(|i| (night.ambient_color[i] - expected[i]).abs() < 1e-6));
        assert!(night.sky_color[2] < 0.02);
    }

    #[test]
    fn twilight_reddens_the_sun_near_the_horizon() {
        // The sun sets at +90 and rises at -90 (270).
        let sunset = at(85.0);
        assert!(sunset.rise_set_alpha(false) > 0.5);
        assert_eq!(sunset.rise_set_alpha(true), 0.0);
        let sunrise = at(275.0);
        assert!(sunrise.rise_set_alpha(true) > 0.5);
        assert_eq!(sunrise.rise_set_alpha(false), 0.0);
        assert_eq!(at(0.0).rise_set_alpha(false), 0.0);

        let with = sunset.lighting().sun_color;
        let without = SkySettings { twilight: false, ..sunset }.lighting().sun_color;
        assert!(with[1] < without[1] && with[2] < without[2]);
    }

    #[test]
    fn fog_takes_the_sky_colour_and_twilight_depends_on_the_view() {
        // By day: the sky colour, 58% of the way to white.
        let day = at(0.0);
        let sky = day.sky_color;
        let fog = day.fog_color(0.0);
        assert!((0..3).all(|i| (fog[i] - (sky[i] + (1.0 - sky[i]) * 0.58)).abs() < 1e-5), "{fog:?}");
        assert_eq!(day.twilight_glow(90.0, fog), [0.0; 3]);
        // At night nearly black.
        assert!(at(180.0).fog_color(0.0).iter().all(|&c| c < 0.1));

        // At sunset the fog is red when looking at the sun (90 degrees)
        // and not when looking away from it.
        let sunset = at(85.0);
        let towards = sunset.fog_color(90.0);
        let away = sunset.fog_color(270.0);
        assert!(towards[0] > towards[2] * 2.0, "{towards:?}");
        assert!(away[2] >= away[0], "{away:?}");
        let glow = sunset.twilight_glow(90.0, towards);
        assert!(glow[0] > 0.1 && glow[0] > glow[2]);
        assert_eq!(sunset.twilight_glow(270.0, away), [0.0; 3]);
        assert_eq!(SkySettings { twilight: false, ..sunset }.twilight_glow(90.0, towards), [0.0; 3]);
    }

    #[test]
    fn camera_angles_and_disc_visibility() {
        // Looking along +X is 0 degrees, along -Y is 90 (y points down in
        // `point_direction`).
        assert!(camera_sky_angle(Vec3::ZERO, Vec3::X, 0.0).abs() < 1e-4);
        assert!((camera_sky_angle(Vec3::ZERO, -Vec3::Y, 0.0) - 90.0).abs() < 1e-4);
        assert!((camera_sky_angle(Vec3::ZERO, -Vec3::Y, 30.0) - 60.0).abs() < 1e-4);
        assert!((angle_difference(350.0, 10.0) + 20.0).abs() < 1e-4);
        assert_eq!((at(0.0).sun_visibility(), at(0.0).moon_visibility()), (1.0, 0.0));
        assert_eq!((at(180.0).sun_visibility(), at(180.0).moon_visibility()), (0.0, 1.0));
        // At the horizon both are visible.
        assert!(at(90.0).sun_visibility() > 0.999 && at(90.0).moon_visibility() > 0.999);
    }

    #[test]
    fn strength_scales_the_sun() {
        let strong = SkySettings { sky_time: 0.0, sunlight_strength: 2.0, ..Default::default() }.lighting();
        assert!((strong.sun_color[0] - 2.0).abs() < 1e-5);
    }
}
