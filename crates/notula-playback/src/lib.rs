//! Converts score events into a time-ordered stream of MIDI note events.

use notula_model::{Articulation, EventKind, Score, TICKS_PER_QUARTER};

pub const DEFAULT_TEMPO_BPM: u16 = 120;
pub const DEFAULT_VELOCITY: u8 = 96;
pub const ACCENTED_VELOCITY: u8 = 120;
const STACCATO_DURATION: (u64, u64) = (1, 2);
const STACCATISSIMO_DURATION: (u64, u64) = (1, 4);
const FERMATA_DURATION: (u64, u64) = (3, 2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackEventKind {
    NoteOff,
    NoteOn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaybackEvent {
    pub tick: u64,
    pub channel: u8,
    pub note: u8,
    pub velocity: u8,
    pub kind: PlaybackEventKind,
}

#[derive(Debug, Clone, Copy)]
struct NoteSpan {
    start: u64,
    end: u64,
    track: usize,
    channel: u8,
    note: u8,
    velocity: u8,
    duration_ratio: (u64, u64),
    tied: bool,
}

/// Builds note-on/off events for all parts, staves, voices, and measures.
pub fn events(score: &Score) -> Vec<PlaybackEvent> {
    let mut spans = Vec::new();
    let mut measure_starts = Vec::with_capacity(score.measures.len());
    let mut score_end = 0_u64;
    for measure in &score.measures {
        measure_starts.push(score_end);
        score_end += u64::from(measure.capacity());
    }

    let mut track = 0;
    for part in &score.parts {
        let max_voices = part
            .staves
            .iter()
            .flat_map(|staff| &staff.measures)
            .map(|measure| measure.voices.len())
            .max()
            .unwrap_or(0);

        for staff in &part.staves {
            for voice_index in 0..max_voices {
                let track_id = track;
                track += 1;
                let channel = midi_channel(track_id);
                for (measure_index, measure) in staff.measures.iter().enumerate() {
                    let Some(voice) = measure.voices.get(voice_index) else {
                        continue;
                    };
                    let mut onset = measure_starts[measure_index];
                    for event in &voice.events {
                        let end = onset + u64::from(event.duration.ticks());
                        if let EventKind::Chord { pitches, tied } = &event.kind {
                            let articulations = &event.markings.articulations;
                            let velocity = if articulations.contains(&Articulation::Accent)
                                || articulations.contains(&Articulation::Marcato)
                            {
                                ACCENTED_VELOCITY
                            } else {
                                DEFAULT_VELOCITY
                            };
                            let duration_ratio = if articulations.contains(&Articulation::Fermata) {
                                FERMATA_DURATION
                            } else if articulations.contains(&Articulation::Staccatissimo) {
                                STACCATISSIMO_DURATION
                            } else if articulations.contains(&Articulation::Staccato) {
                                STACCATO_DURATION
                            } else {
                                (1, 1)
                            };
                            for pitch in pitches {
                                let note = pitch.midi() as u8;
                                if let Some(previous) =
                                    spans.iter_mut().rev().find(|span: &&mut NoteSpan| {
                                        span.channel == channel
                                            && span.track == track_id
                                            && span.note == note
                                            && span.end == onset
                                            && span.tied
                                    })
                                {
                                    previous.end = end;
                                    previous.tied = *tied;
                                } else {
                                    spans.push(NoteSpan {
                                        start: onset,
                                        end,
                                        track: track_id,
                                        channel,
                                        note,
                                        velocity,
                                        duration_ratio,
                                        tied: *tied,
                                    });
                                }
                            }
                        }
                        onset = end;
                    }
                }
            }
        }
    }

    let mut events = Vec::with_capacity(spans.len() * 2);
    for span in spans {
        let sounding_end =
            span.start + (span.end - span.start) * span.duration_ratio.0 / span.duration_ratio.1;
        events.push(PlaybackEvent {
            tick: span.start,
            channel: span.channel,
            note: span.note,
            velocity: span.velocity,
            kind: PlaybackEventKind::NoteOn,
        });
        events.push(PlaybackEvent {
            tick: sounding_end,
            channel: span.channel,
            note: span.note,
            velocity: 0,
            kind: PlaybackEventKind::NoteOff,
        });
    }
    events.sort_by_key(|event| (event.tick, event.kind as u8, event.channel, event.note));
    events
}

fn midi_channel(track: usize) -> u8 {
    let channel = track % 15;
    if channel >= 9 {
        (channel + 1) as u8
    } else {
        channel as u8
    }
}

pub fn tick_duration_seconds(ticks: u64, quarter_notes_per_minute: f64) -> f64 {
    ticks as f64 * 60.0 / (f64::from(TICKS_PER_QUARTER) * quarter_notes_per_minute)
}

#[cfg(test)]
mod tests {
    use notula_model::{
        Articulation, Clef, Command, Document, Duration, EventKind, NoteValue, Pitch, Score, Step,
        Tempo,
        edit::{self, Cursor, enter_note, enter_rest},
    };

    use super::*;

    fn setup(measures: usize) -> (Document, Cursor) {
        let mut doc = Document::new();
        let part = doc.new_part("Piano", &[Clef::Treble]);
        let staff = part.staves[0].id;
        doc.execute(Command::AddPart { index: 0, part }).unwrap();
        for _ in 1..measures {
            edit::append_measure(&mut doc).unwrap();
        }
        let cursor = Cursor {
            staff,
            measure: 0,
            voice: 0,
            tick: 0,
        };
        (doc, cursor)
    }

    fn quarter() -> Duration {
        Duration::new(NoteValue::Quarter, 0)
    }

    fn c4() -> Pitch {
        Pitch::new(Step::C, 0, 4)
    }

    #[test]
    fn emits_notes_for_chords_and_skips_rests() {
        let (mut doc, cursor) = setup(2);
        enter_note(&mut doc, cursor, c4(), quarter()).unwrap();
        enter_rest(
            &mut doc,
            Cursor {
                tick: 960,
                ..cursor
            },
            quarter(),
        )
        .unwrap();
        let next_measure = Cursor {
            measure: 1,
            ..cursor
        };
        let entry = enter_note(&mut doc, next_measure, c4(), quarter()).unwrap();
        edit::add_pitch_to_chord(&mut doc, entry.event, Pitch::new(Step::E, 0, 4)).unwrap();

        let generated = events(doc.score());
        assert_eq!(generated.len(), 6);
        assert!(generated.contains(&PlaybackEvent {
            tick: 0,
            channel: 0,
            note: 60,
            velocity: DEFAULT_VELOCITY,
            kind: PlaybackEventKind::NoteOn,
        }));
        assert!(generated.contains(&PlaybackEvent {
            tick: 960,
            channel: 0,
            note: 60,
            velocity: 0,
            kind: PlaybackEventKind::NoteOff,
        }));
        assert!(generated.contains(&PlaybackEvent {
            tick: 3840,
            channel: 0,
            note: 64,
            velocity: DEFAULT_VELOCITY,
            kind: PlaybackEventKind::NoteOn,
        }));
    }

    #[test]
    fn accented_notes_play_with_a_stronger_attack() {
        let (mut doc, cursor) = setup(1);
        let entry = enter_note(&mut doc, cursor, c4(), quarter()).unwrap();
        edit::toggle_articulation(&mut doc, entry.event, Articulation::Accent).unwrap();

        let attack = events(doc.score())
            .into_iter()
            .find(|event| event.kind == PlaybackEventKind::NoteOn)
            .unwrap();
        assert_eq!(attack.velocity, ACCENTED_VELOCITY);
    }

    #[test]
    fn standard_articulations_shape_playback_attacks_and_releases() {
        let cases = [
            (Articulation::Staccato, 480, DEFAULT_VELOCITY),
            (Articulation::Staccatissimo, 240, DEFAULT_VELOCITY),
            (Articulation::Tenuto, 960, DEFAULT_VELOCITY),
            (Articulation::Accent, 960, ACCENTED_VELOCITY),
            (Articulation::Marcato, 960, ACCENTED_VELOCITY),
            (Articulation::Fermata, 1440, DEFAULT_VELOCITY),
        ];

        for (articulation, expected_release, expected_velocity) in cases {
            let (mut doc, cursor) = setup(1);
            let entry = enter_note(&mut doc, cursor, c4(), quarter()).unwrap();
            edit::toggle_articulation(&mut doc, entry.event, articulation).unwrap();
            let generated = events(doc.score());
            let attack = generated
                .iter()
                .find(|event| event.kind == PlaybackEventKind::NoteOn)
                .unwrap();
            let release = generated
                .iter()
                .find(|event| event.kind == PlaybackEventKind::NoteOff)
                .unwrap();

            assert_eq!(attack.velocity, expected_velocity, "{articulation:?}");
            assert_eq!(release.tick, expected_release, "{articulation:?}");
        }
    }

    #[test]
    fn independent_voices_play_simultaneously_and_end_independently() {
        let (mut doc, cursor) = setup(1);
        let second_voice = edit::add_voice(&mut doc, cursor.staff, 0).unwrap();
        enter_note(&mut doc, cursor, c4(), quarter()).unwrap();
        enter_note(
            &mut doc,
            Cursor {
                voice: second_voice,
                ..cursor
            },
            Pitch::new(Step::E, 0, 4),
            Duration::new(NoteValue::Half, 0),
        )
        .unwrap();

        let generated = events(doc.score());
        assert!(generated.contains(&PlaybackEvent {
            tick: 0,
            channel: 0,
            note: 60,
            velocity: DEFAULT_VELOCITY,
            kind: PlaybackEventKind::NoteOn,
        }));
        assert!(generated.contains(&PlaybackEvent {
            tick: 0,
            channel: 1,
            note: 64,
            velocity: DEFAULT_VELOCITY,
            kind: PlaybackEventKind::NoteOn,
        }));
        assert!(generated.contains(&PlaybackEvent {
            tick: 960,
            channel: 0,
            note: 60,
            velocity: 0,
            kind: PlaybackEventKind::NoteOff,
        }));
        assert!(generated.contains(&PlaybackEvent {
            tick: 1920,
            channel: 1,
            note: 64,
            velocity: 0,
            kind: PlaybackEventKind::NoteOff,
        }));
        assert_eq!(generated.len(), 4);
    }

    #[test]
    fn tied_notes_extend_one_sounding_span() {
        let (mut doc, cursor) = setup(1);
        let first = enter_note(&mut doc, cursor, c4(), quarter()).unwrap();
        let location = notula_model::VoiceLocation {
            staff: first.next.staff,
            measure: first.next.measure,
            voice: first.next.voice,
        };
        let event = doc.score().voice(location).unwrap().events[0].clone();
        doc.execute(Command::ReplaceEvent {
            at: location,
            index: 0,
            event: notula_model::Event {
                kind: EventKind::Chord {
                    pitches: vec![c4()],
                    tied: true,
                },
                ..event
            },
        })
        .unwrap();
        enter_note(&mut doc, first.next, c4(), quarter()).unwrap();

        let generated = events(doc.score());
        assert_eq!(generated.len(), 2);
        assert_eq!(generated[0].tick, 0);
        assert_eq!(generated[0].kind, PlaybackEventKind::NoteOn);
        assert_eq!(generated[1].tick, 1920);
        assert_eq!(generated[1].kind, PlaybackEventKind::NoteOff);
    }

    #[test]
    fn ties_do_not_extend_notes_from_another_staff() {
        let mut doc = Document::new();
        let part = doc.new_part("Piano", &[Clef::Treble, Clef::Bass]);
        let treble = part.staves[0].id;
        let bass = part.staves[1].id;
        doc.execute(Command::AddPart { index: 0, part }).unwrap();
        let treble_cursor = Cursor {
            staff: treble,
            measure: 0,
            voice: 0,
            tick: 0,
        };
        let bass_cursor = Cursor {
            staff: bass,
            ..treble_cursor
        };
        let first = enter_note(&mut doc, treble_cursor, c4(), quarter()).unwrap();
        let location = notula_model::VoiceLocation {
            staff: treble,
            measure: 0,
            voice: 0,
        };
        let first_event = doc.score().voice(location).unwrap().events[0].clone();
        doc.execute(Command::ReplaceEvent {
            at: location,
            index: 0,
            event: notula_model::Event {
                kind: EventKind::Chord {
                    pitches: vec![c4()],
                    tied: true,
                },
                ..first_event
            },
        })
        .unwrap();
        enter_note(&mut doc, bass_cursor, c4(), quarter()).unwrap();
        enter_note(
            &mut doc,
            Cursor {
                tick: 960,
                ..bass_cursor
            },
            c4(),
            quarter(),
        )
        .unwrap();

        let generated = events(doc.score());
        assert_eq!(generated.len(), 6);
        assert!(generated.contains(&PlaybackEvent {
            tick: 1920,
            channel: 1,
            note: 60,
            velocity: 0,
            kind: PlaybackEventKind::NoteOff,
        }));
        assert!(generated.contains(&PlaybackEvent {
            tick: 960,
            channel: 0,
            note: 60,
            velocity: 0,
            kind: PlaybackEventKind::NoteOff,
        }));
        assert_eq!(first.next.tick, 960);
    }

    #[test]
    fn note_off_precedes_note_on_at_same_tick() {
        let (mut doc, cursor) = setup(1);
        enter_note(&mut doc, cursor, c4(), quarter()).unwrap();
        enter_note(
            &mut doc,
            Cursor {
                tick: 960,
                ..cursor
            },
            Pitch::new(Step::D, 0, 4),
            quarter(),
        )
        .unwrap();

        let generated = events(doc.score());
        assert_eq!(generated[1].kind, PlaybackEventKind::NoteOff);
        assert_eq!(generated[2].kind, PlaybackEventKind::NoteOn);
    }

    #[test]
    fn tempo_converts_quarter_note_ticks_to_seconds() {
        assert_eq!(tick_duration_seconds(960, 120.0), 0.5);
        let half_note_tempo = Tempo {
            beat_unit: NoteValue::Half,
            units_per_minute: 120,
        };
        assert_eq!(half_note_tempo.quarter_notes_per_minute(), 240.0);
        assert_eq!(
            tick_duration_seconds(960, half_note_tempo.quarter_notes_per_minute()),
            0.25
        );
        let slow_sixty_fourth_tempo = Tempo {
            beat_unit: NoteValue::SixtyFourth,
            units_per_minute: 1,
        };
        assert_eq!(
            tick_duration_seconds(60, slow_sixty_fourth_tempo.quarter_notes_per_minute()),
            60.0
        );
    }

    #[test]
    fn score_type_is_accessible_for_public_api() {
        let score = Score::new();
        assert!(events(&score).is_empty());
    }
}
