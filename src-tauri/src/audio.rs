//! Sound in the editor: the decoded sounds of the open project and their
//! playback along with the animation (`action_tl_play_start`,
//! `audio_stop_all`).
//!
//! Playback mixes everything from the frame it starts at to the end of the
//! animation into one stream and plays that, so the sounds stay in step
//! with each other however many there are.

use mi_audio::{Clip, Placed};
use mi_core::SaveId;
use mi_project::Project;
use std::collections::HashMap;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

/// The decoded sound resources of the open project.
#[derive(Default)]
pub struct Sounds {
    clips: HashMap<SaveId, Arc<Clip>>,
    /// Resources that could not be decoded, so they are not tried again.
    failed: Vec<SaveId>,
}

impl Sounds {
    /// Decodes the sound resources of `project` that are not known yet and
    /// tells the project how long they are. Returns what went wrong.
    pub fn load_missing(&mut self, project: &mut Project) -> Vec<String> {
        let mut errors = Vec::new();
        let pending: Vec<(SaveId, Option<std::path::PathBuf>)> = project
            .resources()
            .iter()
            .filter(|r| r.kind == mi_core::ResType::Sound)
            .filter(|r| !self.clips.contains_key(&r.id) && !self.failed.contains(&r.id))
            .map(|r| (r.id.clone(), project.resource_path(r)))
            .collect();
        for (id, path) in pending {
            let Some(path) = path else { continue };
            let extension = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
            let decoded = std::fs::read(&path)
                .map_err(|e| e.to_string())
                .and_then(|bytes| mi_audio::decode(bytes, extension.as_deref()).map_err(|e| e.to_string()));
            match decoded {
                Ok(clip) => {
                    self.clips.insert(id, Arc::new(clip));
                }
                Err(error) => {
                    errors.push(format!("{}: {error}", path.display()));
                    self.failed.push(id);
                }
            }
        }
        for (id, clip) in &self.clips {
            project.set_sound_seconds(id, clip.seconds());
        }
        errors
    }

    /// The sounds of `project` between two frames, mixed. `hidden` includes
    /// hidden audio timelines.
    pub fn mix(&self, project: &Project, from_frame: f64, to_frame: f64, hidden: bool) -> Vec<f32> {
        let tempo = project.file().info.tempo.max(1.0);
        let sounds = project.placed_sounds(hidden);
        let placed: Vec<Placed> = sounds
            .iter()
            .filter_map(|sound| {
                Some(Placed {
                    clip: self.clips.get(&sound.resource)?,
                    time: sound.position as f64 / tempo,
                    volume: sound.volume,
                    pitch: sound.pitch,
                    start: sound.start,
                    end: sound.end,
                })
            })
            .collect();
        mi_audio::mix(&placed, from_frame / tempo, ((to_frame - from_frame) / tempo).max(0.0))
    }

    /// Whether any sound of the project could play.
    pub fn any(&self, project: &Project) -> bool {
        project.placed_sounds(false).iter().any(|sound| self.clips.contains_key(&sound.resource))
    }

    /// The loudest sample of each of `buckets` stretches of a sound.
    pub fn peaks(&self, resource: &SaveId, buckets: usize) -> Option<Vec<f32>> {
        self.clips.get(resource).map(|clip| clip.peaks(buckets))
    }
}

enum Message {
    Play(Vec<f32>),
    Stop,
}

/// Plays mixed streams on the default sound device, from a thread of its
/// own (the device handle cannot move between threads).
pub struct Player {
    sender: Mutex<Option<Sender<Message>>>,
}

impl Default for Player {
    fn default() -> Self {
        Self { sender: Mutex::new(None) }
    }
}

impl Player {
    fn send(&self, message: Message) {
        let mut sender = self.sender.lock().unwrap_or_else(|e| e.into_inner());
        if sender.is_none() {
            // Nothing to stop before anything played.
            if matches!(message, Message::Stop) {
                return;
            }
            let (tx, rx) = mpsc::channel();
            std::thread::Builder::new().name("audio".into()).spawn(move || run(rx)).ok();
            *sender = Some(tx);
        }
        if let Some(tx) = sender.as_ref() {
            let _ = tx.send(message);
        }
    }

    /// Plays a stereo stream at 44.1 kHz, replacing what is playing.
    pub fn play(&self, samples: Vec<f32>) {
        self.send(Message::Play(samples));
    }

    pub fn stop(&self) {
        self.send(Message::Stop);
    }
}

fn run(messages: mpsc::Receiver<Message>) {
    // The device is opened when the first sound plays. Without one (no
    // sound card, a remote session) playback is silent.
    let mut output: Option<(rodio::OutputStream, rodio::OutputStreamHandle)> = None;
    let mut sink: Option<rodio::Sink> = None;
    for message in messages {
        if let Some(playing) = sink.take() {
            playing.stop();
        }
        let Message::Play(samples) = message else { continue };
        if output.is_none() {
            match rodio::OutputStream::try_default() {
                Ok(opened) => output = Some(opened),
                Err(error) => {
                    eprintln!("No sound device: {error}");
                    continue;
                }
            }
        }
        let Some((_, handle)) = &output else { continue };
        match rodio::Sink::try_new(handle) {
            Ok(new_sink) => {
                new_sink.append(rodio::buffer::SamplesBuffer::new(2, mi_audio::SAMPLE_RATE, samples));
                sink = Some(new_sink);
            }
            Err(error) => {
                eprintln!("Could not play sound: {error}");
                // The device may be gone; open it again next time.
                output = None;
            }
        }
    }
}
