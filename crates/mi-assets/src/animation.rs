//! Animated block textures (`res_load_pack_block_sheet`,
//! `block_texture_get_frame`).
//!
//! An animated texture stacks its images vertically; a `.png.mcmeta` file
//! next to it can give the time per image, an order of images and whether
//! to fade from one to the next. The original resamples every animation to
//! a loop of 64 "sheet frames" that all animated textures step through
//! together, repeating short animations a whole number of times.

use crate::Rgba;
use mi_format::json::{self, Json};

/// Length of the loop all animated textures share (`block_sheet_ani_frames`).
pub const SHEET_FRAMES: u32 = 64;

/// The `animation` section of a `.png.mcmeta` file.
#[derive(Debug, Clone, PartialEq)]
pub struct TextureAnimation {
    /// Sheet frames each image is shown for.
    pub frametime: f64,
    /// Fade into the next image.
    pub interpolate: bool,
    /// Order of the images; all of them in turn when absent.
    pub frames: Option<Vec<usize>>,
}

impl Default for TextureAnimation {
    fn default() -> Self {
        Self { frametime: 1.0, interpolate: false, frames: None }
    }
}

impl TextureAnimation {
    /// Reads a `.png.mcmeta` file; anything missing keeps its default.
    pub fn parse(mcmeta: &[u8]) -> Self {
        let mut animation = Self::default();
        let Ok(Json::Object(root)) = json::parse(mcmeta) else { return animation };
        let Some(section) = root.object("animation") else { return animation };
        if let Some(time) = section.real("frametime") {
            animation.frametime = time;
        }
        if let Some(fade) = section.flag("interpolate") {
            animation.interpolate = fade;
        }
        if let Some(list) = section.array("frames") {
            // An entry is an image index, or an object with one. (A time
            // of its own for an image is not supported, as in the original.)
            let frames: Vec<usize> = list
                .iter()
                .filter_map(|entry| match entry {
                    Json::Object(map) => map.real("index"),
                    Json::Number(index) => Some(*index),
                    _ => None,
                })
                .map(|index| index.max(0.0) as usize)
                .collect();
            if !frames.is_empty() {
                animation.frames = Some(frames);
            }
        }
        animation
    }

    /// Images the animation steps through, of a texture with `images`
    /// images; 1 when the animation would not fit the shared loop (longer
    /// than one and a half times its length), which leaves it still.
    fn steps(&self, images: usize) -> usize {
        let steps = self.frames.as_ref().map_or(images, Vec::len);
        if steps as f64 * self.frametime > SHEET_FRAMES as f64 * 1.5 {
            1
        } else {
            steps.max(1)
        }
    }

    /// Whether a texture with `images` images changes over the loop.
    pub fn is_animated(&self, images: usize) -> bool {
        images > 1 && self.steps(images) > 1
    }

    /// What sheet frame `frame` shows: the image, the one it fades into,
    /// and how far the fade is (0 without interpolation).
    pub fn image_at(&self, images: usize, frame: u32) -> (usize, usize, f64) {
        let images = images.max(1);
        let steps = self.steps(images);
        let length = if steps == 1 { 1.0 } else { steps as f64 * self.frametime };
        let loops = (SHEET_FRAMES as f64 / length).round().max(1.0);
        let unit = 1.0 / SHEET_FRAMES as f64;
        // `snap(frac(f / (frames / loops)), 1 / frames)`
        let progress = (((frame % SHEET_FRAMES) as f64 / (SHEET_FRAMES as f64 / loops)).fract() / unit).round() * unit;

        let position = progress * steps as f64;
        let step = position.floor() as usize;
        let (image, next) = match &self.frames {
            Some(order) => (order[step % order.len()], order[(step + 1) % order.len()]),
            None => (step, step + 1),
        };
        let fade = if self.interpolate { position.fract() } else { 0.0 };
        (image % images, next % images, fade)
    }
}

/// The sheet frame shown `seconds` into the animation at the project's
/// texture animation speed (`block_texture_get_frame`; the original counts
/// time in sixtieths of a second).
pub fn sheet_frame(seconds: f64, speed: f64) -> u32 {
    ((seconds * 60.0 * speed).floor() as i64).rem_euclid(SHEET_FRAMES as i64) as u32
}

/// Number of images stacked in a texture of `width` × `height` pixels.
pub fn image_count(width: u32, height: u32) -> usize {
    if width > 0 && height > width && height % width == 0 {
        (height / width) as usize
    } else {
        1
    }
}

/// The picture a stacked texture shows at a sheet frame.
pub fn frame_image(stacked: &Rgba, animation: &TextureAnimation, frame: u32) -> Rgba {
    let side = stacked.width;
    let images = image_count(stacked.width, stacked.height);
    let (image, next, fade) = animation.image_at(images, frame);
    let bytes = (side * side * 4) as usize;
    let cut = |index: usize| &stacked.pixels[index * bytes..(index + 1) * bytes];
    let mut pixels = cut(image).to_vec();
    if fade > 0.0 && next != image {
        for (value, target) in pixels.iter_mut().zip(cut(next)) {
            *value = (*value as f64 + (*target as f64 - *value as f64) * fade).round() as u8;
        }
    }
    Rgba { width: side, height: side, pixels }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_is_read_with_defaults() {
        assert_eq!(TextureAnimation::parse(b"not json"), TextureAnimation::default());
        assert_eq!(TextureAnimation::parse(b"{}"), TextureAnimation::default());
        let magma = TextureAnimation::parse(br#"{"animation": {"frametime": 8, "interpolate": true, "frames": [0, 1, 2]}}"#);
        assert_eq!(magma, TextureAnimation { frametime: 8.0, interpolate: true, frames: Some(vec![0, 1, 2]) });
        let objects = TextureAnimation::parse(br#"{"animation": {"frames": [{"index": 2, "time": 4}, 0]}}"#);
        assert_eq!(objects.frames, Some(vec![2, 0]));
    }

    #[test]
    fn animations_are_fitted_into_the_shared_loop() {
        // 32 images, 2 sheet frames each: exactly one loop of 64.
        let water = TextureAnimation { frametime: 2.0, ..Default::default() };
        assert!(water.is_animated(32));
        assert_eq!(water.image_at(32, 0), (0, 1, 0.0));
        assert_eq!(water.image_at(32, 1).0, 0);
        assert_eq!(water.image_at(32, 2).0, 1);
        assert_eq!(water.image_at(32, 63).0, 31);
        assert_eq!(water.image_at(32, 64), water.image_at(32, 0));

        // 3 images of 8 frames are 24 frames: repeated 3 times (64 / 24
        // rounds to 3), slightly faster than written.
        let magma = TextureAnimation { frametime: 8.0, interpolate: true, frames: Some(vec![0, 1, 2]) };
        let images: Vec<usize> = (0..64).map(|f| magma.image_at(3, f).0).collect();
        let changes = images.windows(2).filter(|w| w[0] != w[1]).count();
        assert_eq!(changes, 8, "{images:?}");
        // It fades towards the next image, wrapping to the first.
        let (image, next, fade) = magma.image_at(3, 3);
        assert_eq!((image, next), (0, 1));
        assert!(fade > 0.0 && fade < 1.0);
        let last = (0..64).rev().map(|f| magma.image_at(3, f)).find(|step| step.0 == 2).unwrap();
        assert_eq!(last.1, 0);

        // An animation much longer than the loop stands still.
        let slow = TextureAnimation { frametime: 40.0, ..Default::default() };
        assert!(!slow.is_animated(4));
        assert!((0..64).all(|f| slow.image_at(4, f).0 == 0));
        // One image is never animated.
        assert!(!water.is_animated(1));
    }

    #[test]
    fn the_sheet_frame_follows_time_and_speed() {
        assert_eq!(sheet_frame(0.0, 0.25), 0);
        // One second is 60 steps; at the default speed of 0.25, 15 frames.
        assert_eq!(sheet_frame(1.0, 0.25), 15);
        assert_eq!(sheet_frame(5.0, 0.25), 75 % 64);
        assert_eq!(sheet_frame(1.0, 0.0), 0);
        assert_eq!(sheet_frame(-0.1, 0.25), 62);
    }

    #[test]
    fn frames_are_cut_and_faded() {
        // Two 1×1 images: black, then white.
        let stacked = Rgba { width: 1, height: 2, pixels: vec![0, 0, 0, 255, 255, 255, 255, 255] };
        assert_eq!(image_count(1, 2), 2);
        assert_eq!(image_count(16, 16), 1);
        assert_eq!(image_count(16, 24), 1);
        let plain = TextureAnimation { frametime: 32.0, ..Default::default() };
        assert_eq!(frame_image(&stacked, &plain, 0).pixels, [0, 0, 0, 255]);
        assert_eq!(frame_image(&stacked, &plain, 32).pixels, [255, 255, 255, 255]);
        let fading = TextureAnimation { interpolate: true, ..plain };
        let half = frame_image(&stacked, &fading, 16);
        assert_eq!((half.width, half.height), (1, 1));
        assert_eq!(half.pixels, [128, 128, 128, 255]);
    }
}
