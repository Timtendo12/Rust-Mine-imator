//! Sounds: decoding audio files and mixing the sounds of a project into one
//! stream, for playback and for exported videos (`res_load_audio`,
//! `movie_audio_sound_add`, the mixing of `lib_movie_frame`).
//!
//! Everything is stereo at 44.1 kHz, as in the original.

mod decode;

pub use decode::decode;

/// Samples per second of every clip and mix.
pub const SAMPLE_RATE: u32 = 44_100;

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("the sound could not be read: {0}")]
    Decode(String),
    #[error("the file has no sound in it")]
    NoAudio,
}

/// A decoded sound: left and right samples in turn.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Clip {
    pub samples: Vec<f32>,
}

impl Clip {
    /// Number of stereo frames.
    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }

    pub fn seconds(&self) -> f64 {
        self.frames() as f64 / SAMPLE_RATE as f64
    }

    /// The loudest sample of each of `buckets` equal stretches of the
    /// clip, for drawing its waveform.
    pub fn peaks(&self, buckets: usize) -> Vec<f32> {
        let frames = self.frames();
        (0..buckets)
            .map(|bucket| {
                let (from, to) = (bucket * frames / buckets.max(1), (bucket + 1) * frames / buckets.max(1));
                self.samples[from * 2..to * 2].iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()))
            })
            .collect()
    }
}

/// A sound placed on the timeline (a keyframe of an audio timeline).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed<'a> {
    pub clip: &'a Clip,
    /// When it starts, in seconds.
    pub time: f64,
    pub volume: f64,
    /// Speed it is played at; 2 is twice as fast and an octave higher.
    pub pitch: f64,
    /// Seconds into the (pitched) sound that it starts from.
    pub start: f64,
    /// Seconds added to its length: negative cuts the end off, positive
    /// lets it start over.
    pub end: f64,
}

/// How long a sound plays, in seconds (`tl_keyframe_length`).
pub fn play_seconds(clip_seconds: f64, pitch: f64, start: f64, end: f64) -> f64 {
    if pitch == 0.0 {
        return 0.0;
    }
    (clip_seconds / pitch + end - start).max(0.0)
}

impl Placed<'_> {
    pub fn seconds(&self) -> f64 {
        play_seconds(self.clip.seconds(), self.pitch, self.start, self.end)
    }
}

/// Mixes the sounds that play between `from` and `from + seconds` into one
/// stereo stream. Sounds are added up and kept within -1..1; a sound that
/// runs past its end starts over.
pub fn mix(sounds: &[Placed], from: f64, seconds: f64) -> Vec<f32> {
    let rate = SAMPLE_RATE as f64;
    let frames = (seconds.max(0.0) * rate).round() as usize;
    let mut out = vec![0.0f32; frames * 2];
    for sound in sounds {
        let length = sound.seconds();
        let clip_frames = sound.clip.frames();
        if length <= 0.0 || clip_frames == 0 || sound.volume <= 0.0 || sound.pitch <= 0.0 {
            continue;
        }
        // The stretch of the output this sound covers.
        let first = (((sound.time - from) * rate).ceil().max(0.0)) as usize;
        let last = ((((sound.time + length) - from) * rate).ceil().max(0.0) as usize).min(frames);
        // Length of one pass through the sound at its pitch.
        let pass = sound.clip.seconds() / sound.pitch;
        for frame in first..last {
            let elapsed = from + frame as f64 / rate - sound.time;
            let source = ((elapsed + sound.start).rem_euclid(pass) * sound.pitch * rate) as usize;
            if source >= clip_frames {
                continue;
            }
            for channel in 0..2 {
                let value = &mut out[frame * 2 + channel];
                *value = (*value + sound.clip.samples[source * 2 + channel] * sound.volume as f32).clamp(-1.0, 1.0);
            }
        }
    }
    out
}

/// A stereo stream as a 16-bit WAV file.
pub fn wav_bytes(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&2u16.to_le_bytes()); // channels
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 4).to_le_bytes()); // bytes per second
    out.extend_from_slice(&4u16.to_le_bytes()); // bytes per frame
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clip of `seconds` whose left sample counts frames up and whose
    /// right sample is constant.
    fn ramp(seconds: f64) -> Clip {
        let frames = (seconds * SAMPLE_RATE as f64) as usize;
        let samples = (0..frames).flat_map(|i| [i as f32 / frames as f32 * 0.5, 0.25]).collect();
        Clip { samples }
    }

    fn placed(clip: &Clip, time: f64) -> Placed<'_> {
        Placed { clip, time, volume: 1.0, pitch: 1.0, start: 0.0, end: 0.0 }
    }

    #[test]
    fn lengths_follow_pitch_and_trimming() {
        assert_eq!(play_seconds(4.0, 1.0, 0.0, 0.0), 4.0);
        assert_eq!(play_seconds(4.0, 2.0, 0.0, 0.0), 2.0);
        assert_eq!(play_seconds(4.0, 1.0, 1.0, -0.5), 2.5);
        assert_eq!(play_seconds(4.0, 1.0, 0.0, 2.0), 6.0);
        assert_eq!(play_seconds(4.0, 1.0, 9.0, 0.0), 0.0);
        assert_eq!(play_seconds(4.0, 0.0, 0.0, 0.0), 0.0);
        assert!((ramp(1.5).seconds() - 1.5).abs() < 1e-4);
    }

    #[test]
    fn sounds_are_placed_in_time_and_added_up() {
        let clip = ramp(1.0);
        let rate = SAMPLE_RATE as usize;
        // One sound at 0.5 s, mixed over 2 s: silence, the sound, silence.
        let out = mix(&[placed(&clip, 0.5)], 0.0, 2.0);
        assert_eq!(out.len(), rate * 2 * 2);
        assert_eq!(out[rate / 2 * 2 - 1], 0.0);
        assert_eq!(out[rate / 2 * 2 + 1], 0.25);
        assert_eq!(out[(rate * 3 / 2 - 1) * 2 + 1], 0.25);
        assert_eq!(out[(rate * 3 / 2 + 1) * 2 + 1], 0.0);
        // Starting the mix later starts inside the sound.
        let later = mix(&[placed(&clip, 0.5)], 1.0, 1.0);
        assert!((later[0] - 0.25).abs() < 1e-3, "{}", later[0]);
        assert_eq!(later[1], 0.25);

        // Two overlapping sounds add up; volume scales; the sum is limited.
        let quiet = Placed { volume: 0.5, ..placed(&clip, 0.0) };
        let both = mix(&[placed(&clip, 0.0), quiet], 0.0, 1.0);
        assert!((both[101] - 0.375).abs() < 1e-6);
        let loud = Placed { volume: 10.0, ..placed(&clip, 0.0) };
        assert_eq!(mix(&[loud], 0.0, 1.0)[101], 1.0);
        // Silent or stopped sounds add nothing.
        let muted = Placed { volume: 0.0, ..placed(&clip, 0.0) };
        let stopped = Placed { pitch: 0.0, ..placed(&clip, 0.0) };
        assert!(mix(&[muted, stopped], 0.0, 0.5).iter().all(|&s| s == 0.0));
    }

    #[test]
    fn pitch_start_and_end_shape_a_sound() {
        let clip = ramp(1.0);
        let rate = SAMPLE_RATE as usize;
        // Twice as fast: half as long, and halfway through it after 0.25 s.
        let fast = Placed { pitch: 2.0, ..placed(&clip, 0.0) };
        let out = mix(&[fast], 0.0, 1.0);
        assert!((out[rate / 4 * 2] - 0.25).abs() < 1e-3);
        assert_eq!(out[(rate / 2 + 10) * 2 + 1], 0.0);
        // Starting 0.5 s in.
        let skipped = Placed { start: 0.5, ..placed(&clip, 0.0) };
        let out = mix(&[skipped], 0.0, 1.0);
        assert!((out[0] - 0.25).abs() < 1e-3);
        assert_eq!(out[(rate / 2 + 10) * 2 + 1], 0.0);
        // A longer end starts the sound over.
        let repeated = Placed { end: 0.5, ..placed(&clip, 0.0) };
        let out = mix(&[repeated], 0.0, 2.0);
        assert!(out[(rate + 100) * 2] < 0.01 && out[(rate + 100) * 2 + 1] == 0.25);
        assert_eq!(out[(rate * 3 / 2 + 10) * 2 + 1], 0.0);
    }

    #[test]
    fn wav_files_and_peaks() {
        let wav = wav_bytes(&[0.0, 1.0, -1.0, 0.5]);
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(wav.len(), 44 + 8);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 8);
        assert_eq!(i16::from_le_bytes(wav[46..48].try_into().unwrap()), i16::MAX);
        assert_eq!(i16::from_le_bytes(wav[48..50].try_into().unwrap()), -i16::MAX);

        let clip = Clip { samples: vec![0.1, -0.2, 0.0, 0.0, 0.9, 0.0, -0.3, 0.3] };
        assert_eq!(clip.peaks(2), [0.2, 0.9]);
        assert_eq!(Clip::default().peaks(3), [0.0, 0.0, 0.0]);
    }

    #[test]
    fn wav_files_decode_back() {
        // A second of a rising left channel at 22.05 kHz mono, which is
        // brought to 44.1 kHz stereo.
        let mono: Vec<i16> = (0..22_050).map(|i| (i / 2) as i16).collect();
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + mono.len() as u32 * 2).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&22_050u32.to_le_bytes());
        wav.extend_from_slice(&44_100u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(mono.len() as u32 * 2).to_le_bytes());
        for sample in &mono {
            wav.extend_from_slice(&sample.to_le_bytes());
        }
        let clip = decode(wav, Some("wav")).unwrap();
        assert!((clip.seconds() - 1.0).abs() < 0.01, "{}", clip.seconds());
        // Both channels carry the mono signal.
        let middle = clip.frames() / 2;
        assert!((clip.samples[middle * 2] - clip.samples[middle * 2 + 1]).abs() < 1e-6);
        assert!((clip.samples[middle * 2] - 5512.0 / 32768.0).abs() < 0.01, "{}", clip.samples[middle * 2]);

        // What this crate writes, it reads.
        let stereo = wav_bytes(&[0.5, -0.5].repeat(1000));
        let back = decode(stereo, None).unwrap();
        assert_eq!(back.frames(), 1000);
        assert!((back.samples[10] - 0.5).abs() < 1e-3 && (back.samples[11] + 0.5).abs() < 1e-3);
        assert!(decode(b"not a sound".to_vec(), None).is_err());
    }
}
