//! Exporting the animation (`action_toolbar_exportmovie_save`,
//! `export_update`): every frame is rendered at the project's video size
//! and written to a numbered PNG file, or piped as raw pixels to an
//! `ffmpeg` process that encodes the video.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// What the animation is saved as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// One PNG file per frame.
    Png,
    Mp4,
    Mov,
    Wmv,
}

impl Format {
    pub fn from_name(name: &str) -> Option<Format> {
        Some(match name {
            "png" => Format::Png,
            "mp4" => Format::Mp4,
            "mov" => Format::Mov,
            "wmv" => Format::Wmv,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MovieOptions {
    pub format: Format,
    pub frames_per_second: f64,
    /// As the original's export dialog gives it; the encoder gets five
    /// times this, as in the original.
    pub bit_rate: u64,
    /// A sound file to put into the video.
    pub audio: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("ffmpeg was not found. Install ffmpeg or put ffmpeg next to the program to export videos; image sequences work without it")]
    NoEncoder,
    #[error("the video encoder failed: {0}")]
    Encoder(String),
    #[error("frame {0} could not be rendered")]
    Render(usize),
    #[error("could not write {path}: {reason}")]
    Write { path: String, reason: String },
}

/// The timeline positions of the exported frames: from `start`, one every
/// `tempo / frames_per_second` project frames, up to and including `end`.
pub fn frame_markers(start: f64, end: f64, tempo: f64, frames_per_second: f64) -> Vec<f64> {
    let step = tempo / frames_per_second.max(1.0);
    let mut markers = Vec::new();
    loop {
        let marker = start + markers.len() as f64 * step;
        // Accumulated rounding must not drop the last frame.
        if marker > end + 1e-9 {
            break;
        }
        markers.push(marker);
    }
    markers
}

/// File of frame `index` (from 0) of an image sequence saved as `path`:
/// `name_001.png`, numbered from 1 with as many digits as the number of
/// frames `(end - start) / tempo * frames_per_second`, rounded up, has.
pub fn sequence_file(path: &Path, index: usize, total_frames: usize) -> PathBuf {
    let digits = total_frames.to_string().len();
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!("{stem}_{:0digits$}.png", index + 1))
}

/// The number the original sizes the frame numbers of a sequence by.
pub fn sequence_total(start: f64, end: f64, tempo: f64, frames_per_second: f64) -> usize {
    (((end - start) / tempo.max(1.0)) * frames_per_second).ceil().max(0.0) as usize
}

/// Arguments for ffmpeg to encode raw RGBA frames from its input into
/// `path`, with the settings the original gives its encoder: the default
/// codec of the container, five times the bit rate, a key frame every 12
/// frames.
pub fn ffmpeg_args(options: &MovieOptions, width: u32, height: u32, path: &Path) -> Vec<String> {
    let (codec, container) = match options.format {
        Format::Mp4 => ("libx264", "mp4"),
        Format::Mov => ("libx264", "mov"),
        // "wmv" files are ASF with Microsoft's MPEG-4 version 3.
        Format::Wmv | Format::Png => ("msmpeg4", "asf"),
    };
    let mut args: Vec<String> = ["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba"].map(String::from).into();
    args.extend(["-s".into(), format!("{width}x{height}"), "-r".into(), format!("{}", options.frames_per_second)]);
    args.extend(["-i", "-"].map(String::from));
    match &options.audio {
        Some(audio) => {
            // 320 kbit/s like the original: AAC, or Windows Media Audio
            // in "wmv" files.
            let audio_codec = if container == "asf" { "wmav2" } else { "aac" };
            args.extend(["-i".into(), audio.to_string_lossy().into_owned()]);
            args.extend(["-c:a", audio_codec, "-b:a", "320k"].map(String::from));
        }
        None => args.push("-an".into()),
    }
    args.extend(["-c:v", codec].map(String::from));
    args.extend(["-b:v".into(), (options.bit_rate * 5).to_string(), "-g".into(), "12".into()]);
    if codec == "libx264" {
        args.extend(["-tune", "zerolatency"].map(String::from));
    }
    // The pixel format of the stream halves the colour resolution, which
    // needs even sizes.
    args.extend(["-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2", "-pix_fmt", "yuv420p", "-f", container].map(String::from));
    args.push(path.to_string_lossy().into_owned());
    args
}

/// The ffmpeg program: the one named by `MI_FFMPEG`, one next to this
/// program, or the one on the search path.
pub fn find_ffmpeg() -> PathBuf {
    if let Some(path) = std::env::var_os("MI_FFMPEG") {
        return PathBuf::from(path);
    }
    let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    if let Some(beside) = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join(name))) {
        if beside.is_file() {
            return beside;
        }
    }
    PathBuf::from(name)
}

fn start_encoder(options: &MovieOptions, width: u32, height: u32, path: &Path) -> Result<Child, ExportError> {
    let mut command = Command::new(find_ffmpeg());
    command.args(ffmpeg_args(options, width, height, path)).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window.
        command.creation_flags(0x0800_0000);
    }
    command.spawn().map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => ExportError::NoEncoder,
        _ => ExportError::Encoder(error.to_string()),
    })
}

/// Waits for the encoder and turns a failure into its error output.
fn finish_encoder(mut child: Child) -> Result<(), ExportError> {
    drop(child.stdin.take());
    let output = child.wait_with_output().map_err(|e| ExportError::Encoder(e.to_string()))?;
    if output.status.success() {
        return Ok(());
    }
    let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(ExportError::Encoder(if message.is_empty() { output.status.to_string() } else { message }))
}

/// How an export ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Cancelled,
}

/// Exports the frames at `markers` as `path`. `render` gives the RGBA
/// pixels (`width` × `height`) of the frame at a marker; `progress` is
/// called before each frame with its index and returns whether to go on.
/// A cancelled video keeps the frames encoded so far, as in the original.
pub fn export(
    options: &MovieOptions,
    path: &Path,
    (width, height): (u32, u32),
    markers: &[f64],
    sequence_total: usize,
    mut render: impl FnMut(f64) -> Option<Vec<u8>>,
    mut progress: impl FnMut(usize) -> bool,
) -> Result<Outcome, ExportError> {
    let mut encoder = match options.format {
        Format::Png => None,
        _ => Some(start_encoder(options, width, height, path)?),
    };
    let mut outcome = Outcome::Done;
    let mut failure = None;
    for (index, &marker) in markers.iter().enumerate() {
        if !progress(index) {
            outcome = Outcome::Cancelled;
            break;
        }
        let Some(pixels) = render(marker).filter(|p| p.len() == width as usize * height as usize * 4) else {
            failure = Some(ExportError::Render(index));
            break;
        };
        let written = match &mut encoder {
            Some(child) => match child.stdin.as_mut().map(|stdin| stdin.write_all(&pixels)) {
                Some(Ok(())) => Ok(()),
                // The encoder stopped reading: its own message says why.
                _ => Err(None),
            },
            None => {
                let file = sequence_file(path, index, sequence_total);
                image::save_buffer(&file, &pixels, width, height, image::ColorType::Rgba8)
                    .map_err(|e| Some(ExportError::Write { path: file.to_string_lossy().into_owned(), reason: e.to_string() }))
            }
        };
        match written {
            Ok(()) => {}
            Err(error) => {
                failure = error.or(Some(ExportError::Encoder("the encoder stopped".into())));
                break;
            }
        }
    }
    let finished = encoder.map(finish_encoder).unwrap_or(Ok(()));
    match (failure, finished) {
        // What the encoder says is more useful than "it stopped".
        (Some(ExportError::Encoder(_)), Err(error)) | (None, Err(error)) => Err(error),
        (Some(error), _) => Err(error),
        (None, Ok(())) => Ok(outcome),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mi-export-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn frames_follow_the_export_frame_rate() {
        // A 24 fps project of 48 frames at 24 fps: every frame, both ends.
        let same = frame_markers(0.0, 48.0, 24.0, 24.0);
        assert_eq!((same.len(), same[1], *same.last().unwrap()), (49, 1.0, 48.0));
        // At 60 fps the marker moves 0.4 project frames a frame.
        let fast = frame_markers(0.0, 48.0, 24.0, 60.0);
        assert_eq!(fast.len(), 121);
        assert!((fast[1] - 0.4).abs() < 1e-12 && (fast[120] - 48.0).abs() < 1e-9);
        // A region, and a project without animation.
        assert_eq!(frame_markers(10.0, 12.0, 24.0, 12.0), [10.0, 12.0]);
        assert_eq!(frame_markers(0.0, 0.0, 24.0, 30.0), [0.0]);
    }

    #[test]
    fn sequence_files_are_numbered_from_one() {
        let path = Path::new("out/movie.png");
        assert_eq!(sequence_total(0.0, 48.0, 24.0, 60.0), 120);
        assert_eq!(sequence_file(path, 0, 120), Path::new("out/movie_001.png"));
        assert_eq!(sequence_file(path, 119, 120), Path::new("out/movie_120.png"));
        assert_eq!(sequence_file(path, 4, 9), Path::new("out/movie_5.png"));
        // As in the original, the last frame of a round total has a digit more.
        assert_eq!(sequence_file(path, 9, 9), Path::new("out/movie_10.png"));
    }

    #[test]
    fn encoder_settings() {
        let options = MovieOptions { format: Format::Mp4, frames_per_second: 30.0, bit_rate: 2_500_000, audio: None };
        let args = ffmpeg_args(&options, 1280, 720, Path::new("a.mp4")).join(" ");
        assert!(args.contains("-f rawvideo -pix_fmt rgba -s 1280x720 -r 30 -i -"), "{args}");
        assert!(args.contains("-c:v libx264 -b:v 12500000 -g 12 -tune zerolatency"), "{args}");
        assert!(args.ends_with("-pix_fmt yuv420p -f mp4 a.mp4"), "{args}");
        let wmv = ffmpeg_args(&MovieOptions { format: Format::Wmv, ..options.clone() }, 2, 2, Path::new("a.wmv")).join(" ");
        assert!(wmv.contains("-c:v msmpeg4") && wmv.ends_with("-f asf a.wmv") && !wmv.contains("zerolatency"), "{wmv}");
        // Without sound the video has no audio stream; with it the sound
        // file is a second input.
        assert!(args.contains("-i - -an -c:v"), "{args}");
        let sound = MovieOptions { audio: Some(PathBuf::from("mix.wav")), ..options.clone() };
        let with_sound = ffmpeg_args(&sound, 2, 2, Path::new("a.mp4")).join(" ");
        assert!(with_sound.contains("-i - -i mix.wav -c:a aac -b:a 320k -c:v libx264"), "{with_sound}");
        let wma = ffmpeg_args(&MovieOptions { format: Format::Wmv, ..sound }, 2, 2, Path::new("a.wmv")).join(" ");
        assert!(wma.contains("-c:a wmav2"), "{wma}");
        assert_eq!(Format::from_name("mov"), Some(Format::Mov));
        assert_eq!(Format::from_name("gif"), None);
    }

    fn solid(width: u32, height: u32, marker: f64) -> Option<Vec<u8>> {
        let shade = (marker * 20.0) as u8;
        Some([shade, 255 - shade, 0, 255].repeat((width * height) as usize))
    }

    #[test]
    fn image_sequences_stop_when_cancelled() {
        let dir = temp_dir("png");
        let path = dir.join("clip.png");
        let options = MovieOptions { format: Format::Png, frames_per_second: 24.0, bit_rate: 0, audio: None };
        let markers = frame_markers(0.0, 11.0, 24.0, 24.0);
        let outcome = export(&options, &path, (4, 2), &markers, 11, |m| solid(4, 2, m), |_| true).unwrap();
        assert_eq!(outcome, Outcome::Done);
        assert!(dir.join("clip_01.png").is_file() && dir.join("clip_12.png").is_file());
        let frame = image::open(dir.join("clip_03.png")).unwrap().to_rgba8();
        assert_eq!((frame.dimensions(), frame.get_pixel(0, 0).0), ((4, 2), [40, 215, 0, 255]));

        let path = dir.join("short.png");
        let outcome = export(&options, &path, (4, 2), &markers, 11, |m| solid(4, 2, m), |i| i < 3).unwrap();
        assert_eq!(outcome, Outcome::Cancelled);
        assert!(dir.join("short_03.png").is_file() && !dir.join("short_04.png").exists());

        // A frame of the wrong size is an error, not a broken file.
        let error = export(&options, &path, (4, 2), &markers, 11, |_| Some(vec![0; 3]), |_| true).unwrap_err();
        assert!(matches!(error, ExportError::Render(0)));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Runs only where ffmpeg is installed.
    #[test]
    fn videos_are_encoded_by_ffmpeg() {
        if Command::new(find_ffmpeg()).arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_err() {
            eprintln!("ffmpeg is not installed; skipping");
            return;
        }
        let dir = temp_dir("video");
        let markers = frame_markers(0.0, 11.0, 24.0, 24.0);
        for (format, name) in [(Format::Mp4, "clip.mp4"), (Format::Mov, "clip.mov"), (Format::Wmv, "clip.wmv")] {
            let options = MovieOptions { format, frames_per_second: 24.0, bit_rate: 350_000, audio: None };
            let path = dir.join(name);
            // An odd size, which the encoder pads.
            let outcome = export(&options, &path, (33, 18), &markers, 11, |m| solid(33, 18, m), |_| true).unwrap();
            assert_eq!(outcome, Outcome::Done);
            assert!(std::fs::metadata(&path).unwrap().len() > 500, "{name}");
        }
        // With sound the video gets an audio stream.
        let wav = dir.join("mix.wav");
        std::fs::write(&wav, mi_audio::wav_bytes(&[0.25, -0.25].repeat(22_050))).unwrap();
        let options = MovieOptions { format: Format::Mp4, frames_per_second: 24.0, bit_rate: 350_000, audio: Some(wav) };
        let path = dir.join("sound.mp4");
        export(&options, &path, (32, 18), &markers, 11, |m| solid(32, 18, m), |_| true).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        // The sound track is announced by an "mp4a" sample entry.
        assert!(bytes.windows(4).any(|w| w == b"mp4a"));
        assert!(!std::fs::read(dir.join("clip.mp4")).unwrap().windows(4).any(|w| w == b"mp4a"));

        // The encoder's own message is reported when it cannot write.
        let options = MovieOptions { format: Format::Mp4, frames_per_second: 24.0, bit_rate: 350_000, audio: None };
        let missing = dir.join("no-such-folder").join("clip.mp4");
        let error = export(&options, &missing, (32, 18), &markers, 11, |m| solid(32, 18, m), |_| true).unwrap_err();
        assert!(matches!(error, ExportError::Encoder(ref message) if !message.is_empty()), "{error:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
