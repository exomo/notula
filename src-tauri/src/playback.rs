use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};
use manguebyte_core::AudioBuffer;
use manguebyte_dsp::event::NoteRef;
use manguebyte_dsp::{ProcessSpec, Processor};
use manguebyte_sampler::{Instrument, Sampler};
use notula_model::Score;
use notula_playback::{PlaybackEvent, PlaybackEventKind, TempoMap, events};
use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};

enum AudioCommand {
    Play(Vec<PlaybackEvent>, TempoMap),
    Stop,
}

enum AudioEngine {
    Soundfont(Box<Synthesizer>),
    Sfz(Sampler<f32>),
}

pub struct PlaybackController {
    sender: Sender<AudioCommand>,
    shutdown: Sender<()>,
    thread: Option<JoinHandle<()>>,
    instrument_path: PathBuf,
    playing: Arc<AtomicBool>,
    position: Arc<AtomicU64>,
}

impl PlaybackController {
    pub fn load(path: &Path) -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel();
        let (shutdown, shutdown_receiver) = mpsc::channel();
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let playing = Arc::new(AtomicBool::new(false));
        let position = Arc::new(AtomicU64::new(0));
        let instrument_path = path.to_owned();
        let path = instrument_path.clone();
        let playing_on_thread = Arc::clone(&playing);
        let position_on_thread = Arc::clone(&position);
        let thread = thread::Builder::new()
            .name("notula-audio".to_owned())
            .spawn(move || {
                run_audio_thread(
                    &path,
                    receiver,
                    shutdown_receiver,
                    ready_sender,
                    playing_on_thread,
                    position_on_thread,
                );
            })
            .map_err(|error| error.to_string())?;
        match ready_receiver.recv().map_err(|error| error.to_string())? {
            Ok(()) => {}
            Err(error) => {
                let _ = shutdown.send(());
                let _ = thread.join();
                return Err(error);
            }
        }
        Ok(Self {
            sender,
            shutdown,
            thread: Some(thread),
            instrument_path,
            playing,
            position,
        })
    }

    pub fn play(&self, score: &Score) -> Result<(), String> {
        self.sender
            .send(AudioCommand::Play(events(score), TempoMap::new(score)))
            .map_err(|error| error.to_string())
    }

    pub fn stop(&self) -> Result<(), String> {
        self.sender
            .send(AudioCommand::Stop)
            .map_err(|error| error.to_string())
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    pub fn position(&self) -> u64 {
        self.position.load(Ordering::Relaxed)
    }

    pub fn instrument_path(&self) -> &Path {
        &self.instrument_path
    }
}

impl Drop for PlaybackController {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run_audio_thread(
    path: &Path,
    receiver: Receiver<AudioCommand>,
    shutdown: Receiver<()>,
    ready: mpsc::SyncSender<Result<(), String>>,
    playing: Arc<AtomicBool>,
    position: Arc<AtomicU64>,
) {
    let result = (|| {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| "No audio output device is available.".to_owned())?;
        let supported = device
            .default_output_config()
            .map_err(|error| error.to_string())?;
        if supported.channels() < 2 {
            return Err(
                "Playback requires an audio output device with at least two channels.".to_owned(),
            );
        }
        let format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let sample_rate = config.sample_rate.0 as i32;
        let engine = match path.extension().and_then(|extension| extension.to_str()) {
            Some(extension) if extension.eq_ignore_ascii_case("sfz") => {
                let instrument = Instrument::open(path).map_err(|error| error.to_string())?;
                for warning in instrument.warnings() {
                    eprintln!("SFZ warning: {warning}");
                }
                let (mut sampler, _loader) = Sampler::with_instrument(instrument, 128);
                sampler.prepare(&ProcessSpec {
                    sample_rate: f64::from(sample_rate),
                    max_block_size: 512,
                    num_channels: 2,
                });
                AudioEngine::Sfz(sampler)
            }
            Some(extension)
                if extension.eq_ignore_ascii_case("sf2")
                    || extension.eq_ignore_ascii_case("sf3") =>
            {
                let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
                let soundfont =
                    Arc::new(SoundFont::new(&mut file).map_err(|error| error.to_string())?);
                AudioEngine::Soundfont(Box::new(
                    Synthesizer::new(&soundfont, &SynthesizerSettings::new(sample_rate))
                        .map_err(|error| error.to_string())?,
                ))
            }
            _ => return Err("Choose an SF2, SF3, or SFZ instrument file.".to_owned()),
        };
        let stream = build_stream(
            &device, &config, format, receiver, engine, playing, position,
        )?;
        stream.play().map_err(|error| error.to_string())?;
        Ok(stream)
    })();
    match result {
        Ok(_stream) => {
            let _ = ready.send(Ok(()));
            let _ = shutdown.recv();
        }
        Err(error) => {
            let _ = ready.send(Err(error));
        }
    }
}

fn build_stream(
    device: &cpal::Device,
    config: &StreamConfig,
    format: SampleFormat,
    receiver: Receiver<AudioCommand>,
    engine: AudioEngine,
    playing: Arc<AtomicBool>,
    position: Arc<AtomicU64>,
) -> Result<Stream, String> {
    macro_rules! build_typed_stream {
        ($sample:ty) => {
            build_typed_stream::<$sample>(device, config, receiver, engine, playing, position)
        };
    }
    match format {
        SampleFormat::I8 => build_typed_stream!(i8),
        SampleFormat::I16 => build_typed_stream!(i16),
        SampleFormat::I32 => build_typed_stream!(i32),
        SampleFormat::I64 => build_typed_stream!(i64),
        SampleFormat::U8 => build_typed_stream!(u8),
        SampleFormat::U16 => build_typed_stream!(u16),
        SampleFormat::U32 => build_typed_stream!(u32),
        SampleFormat::U64 => build_typed_stream!(u64),
        SampleFormat::F32 => build_typed_stream!(f32),
        SampleFormat::F64 => build_typed_stream!(f64),
        _ => Err(format!("Unsupported audio sample format: {format}")),
    }
}

fn build_typed_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    receiver: Receiver<AudioCommand>,
    mut engine: AudioEngine,
    playing: Arc<AtomicBool>,
    position: Arc<AtomicU64>,
) -> Result<Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let sample_rate = u64::from(config.sample_rate.0);
    let channels = usize::from(config.channels);
    let mut events = Vec::new();
    let mut event_index = 0;
    let mut frame_position = 0_u64;
    let mut tail_until_frame = 0_u64;
    let mut tempo_map = TempoMap::default();
    let mut left = [0.0_f32; 512];
    let mut right = [0.0_f32; 512];
    let mut sfz_buffer = AudioBuffer::<f32>::new(2, 512);
    let playing_for_callback = Arc::clone(&playing);
    let position_for_callback = Arc::clone(&position);

    device
        .build_output_stream(
            config,
            move |output: &mut [T], _| {
                output.fill(T::from_sample(0.0));
                for command in receiver.try_iter() {
                    match command {
                        AudioCommand::Play(next_events, next_tempo_map) => {
                            reset_engine(&mut engine);
                            events = next_events;
                            tempo_map = next_tempo_map;
                            event_index = 0;
                            frame_position = 0;
                            tail_until_frame = u64::MAX;
                            playing_for_callback.store(!events.is_empty(), Ordering::Relaxed);
                            position_for_callback.store(0, Ordering::Relaxed);
                        }
                        AudioCommand::Stop => {
                            events.clear();
                            event_index = 0;
                            tail_until_frame = 0;
                            reset_engine(&mut engine);
                            playing_for_callback.store(false, Ordering::Relaxed);
                        }
                    }
                }

                if events.is_empty() {
                    return;
                }

                let frame_count = output.len() / channels;
                let mut written = 0;
                while written < frame_count {
                    let current_tick =
                        frame_position_to_tick(frame_position, sample_rate, &tempo_map);
                    while let Some(event) = events.get(event_index).copied() {
                        if event.tick > current_tick {
                            break;
                        }
                        send_event(&mut engine, event);
                        event_index += 1;
                    }
                    if event_index == events.len() && playing_for_callback.load(Ordering::Relaxed) {
                        playing_for_callback.store(false, Ordering::Relaxed);
                        tail_until_frame = frame_position + sample_rate * 2;
                    }
                    if !playing_for_callback.load(Ordering::Relaxed)
                        && frame_position >= tail_until_frame
                    {
                        events.clear();
                        break;
                    }

                    let next_event_frame = events
                        .get(event_index)
                        .map(|event| {
                            (tempo_map.seconds_at(event.tick) * sample_rate as f64).ceil() as u64
                        })
                        .unwrap_or(u64::MAX);
                    let until_event = next_event_frame.saturating_sub(frame_position).max(1);
                    let chunk = (frame_count - written)
                        .min(512)
                        .min(usize::try_from(until_event).unwrap_or(usize::MAX));
                    match &mut engine {
                        AudioEngine::Soundfont(synthesizer) => {
                            synthesizer.render(&mut left[..chunk], &mut right[..chunk]);
                        }
                        AudioEngine::Sfz(sampler) => {
                            sfz_buffer.set_num_frames(chunk);
                            sampler.process(&mut sfz_buffer);
                            left[..chunk].copy_from_slice(&sfz_buffer.channel(0)[..chunk]);
                            right[..chunk].copy_from_slice(&sfz_buffer.channel(1)[..chunk]);
                            sfz_buffer.set_num_frames(512);
                        }
                    }
                    for index in 0..chunk {
                        let output_index = (written + index) * channels;
                        output[output_index] = T::from_sample(left[index]);
                        output[output_index + 1] = T::from_sample(right[index]);
                    }
                    written += chunk;
                    frame_position += chunk as u64;
                }

                if playing_for_callback.load(Ordering::Relaxed) {
                    position_for_callback.store(
                        frame_position_to_tick(frame_position, sample_rate, &tempo_map),
                        Ordering::Relaxed,
                    );
                }
            },
            |error| eprintln!("Audio output error: {error}"),
            None,
        )
        .map_err(|error| error.to_string())
}

fn send_event(engine: &mut AudioEngine, event: PlaybackEvent) {
    match engine {
        AudioEngine::Soundfont(synthesizer) => {
            let command = match event.kind {
                PlaybackEventKind::NoteOn => 0x90,
                PlaybackEventKind::NoteOff => 0x80,
            };
            synthesizer.process_midi_message(
                i32::from(event.channel),
                command,
                i32::from(event.note),
                i32::from(event.velocity),
            );
        }
        AudioEngine::Sfz(sampler) => {
            let note = NoteRef::new(event.channel, event.note);
            match event.kind {
                PlaybackEventKind::NoteOn => {
                    sampler.note_on(note, f32::from(event.velocity) / 127.0);
                }
                PlaybackEventKind::NoteOff => sampler.note_off(note, 0.0),
            }
        }
    }
}

fn reset_engine(engine: &mut AudioEngine) {
    match engine {
        AudioEngine::Soundfont(synthesizer) => {
            for channel in 0..16 {
                synthesizer.process_midi_message(channel, 0xB0, 0x7B, 0);
            }
        }
        AudioEngine::Sfz(sampler) => sampler.reset(),
    }
}

fn frame_position_to_tick(frame: u64, sample_rate: u64, tempo_map: &TempoMap) -> u64 {
    tempo_map.tick_at(frame as f64 / sample_rate as f64)
}

#[cfg(test)]
mod tests {
    use manguebyte_sampler::MemorySource;

    use super::*;

    #[test]
    fn sfz_instrument_renders_score_note_events() {
        let instrument = Instrument::from_sfz(
            "<region> sample=*sine lokey=60 hikey=60 ampeg_release=0.05",
            &mut MemorySource::new(),
        )
        .unwrap();
        let (mut sampler, _loader) = Sampler::with_instrument(instrument, 8);
        sampler.prepare(&ProcessSpec {
            sample_rate: 48_000.0,
            max_block_size: 512,
            num_channels: 2,
        });
        let mut engine = AudioEngine::Sfz(sampler);

        send_event(
            &mut engine,
            PlaybackEvent {
                tick: 0,
                channel: 0,
                note: 60,
                velocity: 96,
                kind: PlaybackEventKind::NoteOn,
            },
        );
        let AudioEngine::Sfz(sampler) = &mut engine else {
            unreachable!();
        };
        let mut block = AudioBuffer::<f32>::new(2, 512);
        sampler.process(&mut block);

        assert!(block.peak() > 0.1);
        assert_eq!(sampler.active_voices(), 1);
    }
}
