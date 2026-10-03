//! Reading sound files with symphonia: MP3, Ogg Vorbis, WAV, FLAC and AAC
//! in MP4, brought to stereo at 44.1 kHz.

use crate::{AudioError, Clip, SAMPLE_RATE};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Decodes a sound file. `extension` helps to recognise the format.
pub fn decode(bytes: Vec<u8>, extension: Option<&str>) -> Result<Clip, AudioError> {
    let failed = |error: Error| AudioError::Decode(error.to_string());
    let stream = MediaSourceStream::new(Box::new(std::io::Cursor::new(bytes)), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = extension {
        hint.with_extension(extension);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, stream, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(failed)?;
    let mut format = probed.format;
    let track = format.tracks().iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL).ok_or(AudioError::NoAudio)?;
    let track_id = track.id;
    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(failed)?;

    // Interleaved samples as decoded, with the layout of the file.
    let mut raw: Vec<f32> = Vec::new();
    let mut channels = 0usize;
    let mut rate = 0u32;
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            // The end of the file arrives as an I/O error.
            Err(Error::IoError(_)) | Err(Error::ResetRequired) => break,
            Err(error) => return Err(failed(error)),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                channels = spec.channels.count();
                rate = spec.rate;
                let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                buffer.copy_interleaved_ref(decoded);
                raw.extend_from_slice(buffer.samples());
            }
            // A damaged packet is skipped, like players do.
            Err(Error::DecodeError(_)) => continue,
            Err(Error::IoError(_)) => break,
            Err(error) => return Err(failed(error)),
        }
    }
    if channels == 0 || rate == 0 || raw.is_empty() {
        return Err(AudioError::NoAudio);
    }
    Ok(Clip { samples: to_stereo_44k(&raw, channels, rate) })
}

/// Mono becomes both channels, of more than two the first two are kept;
/// other sample rates are resampled by linear interpolation.
fn to_stereo_44k(raw: &[f32], channels: usize, rate: u32) -> Vec<f32> {
    let frames = raw.len() / channels;
    let frame = |index: usize| -> [f32; 2] {
        let left = raw[index * channels];
        [left, if channels > 1 { raw[index * channels + 1] } else { left }]
    };
    if rate == SAMPLE_RATE {
        return (0..frames).flat_map(frame).collect();
    }
    let out_frames = (frames as u64 * SAMPLE_RATE as u64 / rate as u64) as usize;
    let step = rate as f64 / SAMPLE_RATE as f64;
    let mut out = Vec::with_capacity(out_frames * 2);
    for index in 0..out_frames {
        let position = index as f64 * step;
        let before = (position as usize).min(frames - 1);
        let after = (before + 1).min(frames - 1);
        let share = (position - before as f64) as f32;
        let (a, b) = (frame(before), frame(after));
        out.push(a[0] + (b[0] - a[0]) * share);
        out.push(a[1] + (b[1] - a[1]) * share);
    }
    out
}
