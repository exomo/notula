//! Editing operations a user performs, each applied as a single undoable step.

use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::duration::{Duration, NoteValue, TimeSignature, TimeSignatureDisplay};
use crate::error::ModelError;
use crate::history::Document;
use crate::ids::{EventId, StaffId};
use crate::markings::{Articulation, Dynamic, Hairpin, Markings};
use crate::pitch::{Pitch, Step};
use crate::score::{Clef, Event, EventKind, Score, Voice, VoiceLocation};

/// An insertion point: a position in time within one voice of one staff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    pub staff: StaffId,
    pub measure: usize,
    pub voice: usize,
    pub tick: u32,
}

impl Cursor {
    fn at(self) -> VoiceLocation {
        VoiceLocation {
            staff: self.staff,
            measure: self.measure,
            voice: self.voice,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Located {
    /// Positioned at the start of the event.
    pub cursor: Cursor,
    /// Index within the voice of the measure.
    pub index: usize,
}

/// Result of entering an event: the new event and where entry continues.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub event: EventId,
    pub next: Cursor,
}

pub fn locate(score: &Score, id: EventId) -> Option<Located> {
    for staff in score.staves() {
        for (measure, content) in staff.measures.iter().enumerate() {
            for (voice, events) in content.voices.iter().enumerate() {
                let mut tick = 0;
                for (index, event) in events.events.iter().enumerate() {
                    if event.id == id {
                        let cursor = Cursor {
                            staff: staff.id,
                            measure,
                            voice,
                            tick,
                        };
                        return Some(Located { cursor, index });
                    }
                    tick += event.duration.ticks();
                }
            }
        }
    }
    None
}

fn find(score: &Score, id: EventId) -> Result<(Located, Event), ModelError> {
    let located = locate(score, id).ok_or(ModelError::EventNotFound(id))?;
    let event = score.voice(located.cursor.at())?.events[located.index].clone();
    Ok((located, event))
}

/// Total length of the events in a voice, which is where new events are appended.
pub fn voice_end(
    score: &Score,
    staff: StaffId,
    measure: usize,
    voice: usize,
) -> Result<u32, ModelError> {
    let at = VoiceLocation {
        staff,
        measure,
        voice,
    };
    Ok(score.voice(at)?.used_ticks())
}

/// Returns a note beginning exactly at the cursor, if one exists.
pub fn note_at_cursor(score: &Score, cursor: Cursor) -> Option<EventId> {
    let voice = score.voice(cursor.at()).ok()?;
    let mut onset = 0;
    for event in &voice.events {
        if onset == cursor.tick && matches!(event.kind, EventKind::Chord { .. }) {
            return Some(event.id);
        }
        onset += event.duration.ticks();
    }
    None
}

/// Returns the last valid insertion/overwrite position in a measure.
pub fn cursor_at_content_end(score: &Score, cursor: Cursor) -> Option<Cursor> {
    let voice = score.voice(cursor.at()).ok()?;
    let used = voice.used_ticks();
    let capacity = score.measure_capacity(cursor.measure).ok()?;
    let tick = if used >= capacity {
        let last = voice.events.last()?;
        used - last.duration.ticks()
    } else {
        used
    };
    Some(Cursor { tick, ..cursor })
}

/// Resolves a layout hit against the clicked staff's own voice, not shared score columns.
pub fn cursor_at_staff_hit(score: &Score, cursor: Cursor) -> Option<Cursor> {
    let voice = score.voice(cursor.at()).ok()?;
    if voice.events.is_empty() || note_at_cursor(score, cursor).is_some() {
        return Some(Cursor {
            tick: if voice.events.is_empty() {
                0
            } else {
                cursor.tick
            },
            ..cursor
        });
    }
    cursor_at_content_end(score, cursor)
}

/// Moves a tick back to the start of the event containing it, or to the end of the voice.
fn snap(voice: &Voice, tick: u32) -> u32 {
    let mut onset = 0;
    for event in &voice.events {
        let end = onset + event.duration.ticks();
        if tick < end {
            return onset;
        }
        onset = end;
    }
    onset
}

pub fn snap_cursor(score: &Score, cursor: Cursor) -> Result<Cursor, ModelError> {
    score.measure_capacity(cursor.measure)?;
    let tick = snap(score.voice(cursor.at())?, cursor.tick);
    Ok(Cursor { tick, ..cursor })
}

fn rest_durations(mut ticks: u32) -> Result<Vec<Duration>, ModelError> {
    const VALUES: [NoteValue; 7] = [
        NoteValue::Whole,
        NoteValue::Half,
        NoteValue::Quarter,
        NoteValue::Eighth,
        NoteValue::Sixteenth,
        NoteValue::ThirtySecond,
        NoteValue::SixtyFourth,
    ];
    let mut rests = Vec::new();
    for value in VALUES {
        while ticks >= value.ticks() {
            rests.push(Duration::new(value, 0));
            ticks -= value.ticks();
        }
    }
    if ticks == 0 {
        Ok(rests)
    } else {
        Err(ModelError::Invalid(
            "the remaining time cannot be written as rests",
        ))
    }
}

/// Overwrites whatever is at the cursor, padding with rests where a longer event was replaced.
fn enter_event(
    doc: &mut Document,
    cursor: Cursor,
    id: Option<EventId>,
    kind: EventKind,
    markings: Option<Markings>,
    duration: Duration,
) -> Result<Entry, ModelError> {
    duration.validate()?;
    let at = cursor.at();
    let length = duration.ticks();

    let (index, removed, fillers, growth, next, inherited) = {
        let score = doc.score();
        let capacity = score.measure_capacity(cursor.measure)?;
        let voice = score.voice(at)?;
        let tick = snap(voice, cursor.tick);
        if tick + length > capacity {
            return Err(ModelError::MeasureOverfull);
        }

        let mut onset = 0;
        let mut index = 0;
        for event in &voice.events {
            if onset >= tick {
                break;
            }
            onset += event.duration.ticks();
            index += 1;
        }

        let mut covered = 0;
        let mut removed = 0;
        for event in &voice.events[index..] {
            if covered >= length {
                break;
            }
            covered += event.duration.ticks();
            removed += 1;
        }
        let fillers = rest_durations(covered.saturating_sub(length))?;

        // A new note keeps the volume marks of what it replaces, but not its articulations.
        let inherited = voice
            .events
            .get(index)
            .filter(|_| removed > 0)
            .map(|replaced| Markings {
                dynamic: replaced.markings.dynamic,
                hairpin: replaced.markings.hairpin,
                articulations: Vec::new(),
            })
            .unwrap_or_default();

        let measure_full = tick + length == capacity;
        let growth = (measure_full && cursor.measure + 1 == score.measures.len())
            .then(|| score.blank_measure(score.measures.len()));
        let next = if measure_full {
            Cursor {
                measure: cursor.measure + 1,
                tick: 0,
                ..cursor
            }
        } else {
            Cursor {
                tick: tick + length,
                ..cursor
            }
        };
        (index, removed, fillers, growth, next, inherited)
    };

    let event_id = id.unwrap_or_else(|| doc.new_event_id());
    let mut commands = Vec::new();
    for _ in 0..removed {
        commands.push(Command::RemoveEvent { at, index });
    }
    commands.push(Command::InsertEvent {
        at,
        index,
        event: Event {
            id: event_id,
            duration,
            kind,
            markings: markings.unwrap_or(inherited),
        },
    });
    for (offset, rest) in fillers.into_iter().enumerate() {
        let event = Event::new(doc.new_event_id(), rest, EventKind::Rest);
        commands.push(Command::InsertEvent {
            at,
            index: index + 1 + offset,
            event,
        });
    }
    if let Some(data) = growth {
        commands.push(Command::InsertMeasure {
            index: next.measure,
            data,
        });
    }
    doc.execute(Command::Batch { commands })?;
    Ok(Entry {
        event: event_id,
        next,
    })
}

pub fn enter_note(
    doc: &mut Document,
    cursor: Cursor,
    pitch: Pitch,
    duration: Duration,
) -> Result<Entry, ModelError> {
    pitch.validate()?;
    let kind = EventKind::Chord {
        pitches: vec![pitch],
        tied: false,
    };
    enter_event(doc, cursor, None, kind, None, duration)
}

pub fn enter_rest(
    doc: &mut Document,
    cursor: Cursor,
    duration: Duration,
) -> Result<Entry, ModelError> {
    enter_event(doc, cursor, None, EventKind::Rest, None, duration)
}

fn default_reference(clef: Clef) -> i16 {
    match clef {
        Clef::Treble => 34,             // B4
        Clef::Bass => 22,               // D3
        Clef::Alto | Clef::Tenor => 28, // C4
    }
}

/// Staff position of the last note before the cursor, which letter entry stays close to.
fn reference_position(score: &Score, cursor: Cursor) -> Result<i16, ModelError> {
    let staff = score.staff(cursor.staff)?;
    let top_of = |event: &Event| match &event.kind {
        EventKind::Chord { pitches, .. } => pitches.iter().map(|p| p.diatonic_index()).max(),
        EventKind::Rest => None,
    };

    let mut last = None;
    let mut onset = 0;
    let current = score.voice(cursor.at())?;
    for event in &current.events {
        if onset >= cursor.tick {
            break;
        }
        last = top_of(event).or(last);
        onset += event.duration.ticks();
    }
    if last.is_none() {
        for content in staff.measures[..cursor.measure].iter().rev() {
            let Some(voice) = content.voices.get(cursor.voice) else {
                continue;
            };
            if let Some(found) = voice.events.iter().rev().find_map(top_of) {
                last = Some(found);
                break;
            }
        }
    }
    Ok(last.unwrap_or_else(|| default_reference(staff.clef)))
}

/// Enters the note with the given letter in the octave closest to the previous note.
pub fn enter_step(
    doc: &mut Document,
    cursor: Cursor,
    step: Step,
    duration: Duration,
) -> Result<Entry, ModelError> {
    enter_step_with_alter(doc, cursor, step, duration, None)
}

pub fn enter_step_with_alter(
    doc: &mut Document,
    cursor: Cursor,
    step: Step,
    duration: Duration,
    alter: Option<i8>,
) -> Result<Entry, ModelError> {
    let score = doc.score();
    let key = score
        .measures
        .get(cursor.measure)
        .ok_or(ModelError::MeasureOutOfRange(cursor.measure))?
        .key_signature;
    let reference = reference_position(score, cursor)?;
    let octave = (reference - i16::from(step.index()) + 3).div_euclid(7);
    let pitch = Pitch::new(
        step,
        alter.unwrap_or_else(|| key.alter(step)),
        i8::try_from(octave).unwrap_or(i8::MAX),
    );
    enter_note(doc, cursor, pitch, duration)
}

/// Replaces a note with a rest of the same length; returns false if it already was a rest.
pub fn delete_event(doc: &mut Document, id: EventId) -> Result<bool, ModelError> {
    let (located, event) = find(doc.score(), id)?;
    if event.kind == EventKind::Rest {
        return Ok(false);
    }
    let rest = Event {
        kind: EventKind::Rest,
        markings: event.markings.clone().for_rest(),
        ..event
    };
    doc.execute(Command::ReplaceEvent {
        at: located.cursor.at(),
        index: located.index,
        event: rest,
    })?;
    Ok(true)
}

/// Changes the length of an event, overwriting what follows.
pub fn set_duration(
    doc: &mut Document,
    id: EventId,
    duration: Duration,
) -> Result<Entry, ModelError> {
    let (located, event) = find(doc.score(), id)?;
    let markings = Some(event.markings);
    enter_event(
        doc,
        located.cursor,
        Some(id),
        event.kind,
        markings,
        duration,
    )
}

fn edit_pitches(
    doc: &mut Document,
    id: EventId,
    change: impl Fn(Pitch) -> Result<Pitch, ModelError>,
) -> Result<(), ModelError> {
    let (located, event) = find(doc.score(), id)?;
    let EventKind::Chord { pitches, tied } = &event.kind else {
        return Err(ModelError::Invalid("a rest has no pitch"));
    };
    let pitches = pitches
        .iter()
        .map(|&pitch| change(pitch))
        .collect::<Result<Vec<_>, _>>()?;
    let kind = EventKind::Chord {
        pitches,
        tied: *tied,
    };
    doc.execute(Command::ReplaceEvent {
        at: located.cursor.at(),
        index: located.index,
        event: Event { kind, ..event },
    })
}

/// Moves by semitones, spelled with sharps going up and flats going down.
pub fn transpose_semitones(
    doc: &mut Document,
    id: EventId,
    semitones: i8,
) -> Result<(), ModelError> {
    edit_pitches(doc, id, |pitch| {
        Pitch::from_midi(pitch.midi() + i16::from(semitones), semitones > 0)
            .ok_or(ModelError::Invalid("pitch is outside the MIDI range"))
    })
}

pub fn transpose_chord_pitch(
    doc: &mut Document,
    id: EventId,
    pitch: Pitch,
    semitones: i8,
) -> Result<Pitch, ModelError> {
    let (located, event) = find(doc.score(), id)?;
    let EventKind::Chord { pitches, tied } = &event.kind else {
        return Err(ModelError::Invalid("a rest has no pitch"));
    };
    let index = pitches
        .iter()
        .position(|&existing| existing == pitch)
        .ok_or(ModelError::Invalid("pitch is not part of the chord"))?;
    let changed = Pitch::from_midi(pitch.midi() + i16::from(semitones), semitones > 0)
        .ok_or(ModelError::Invalid("pitch is outside the MIDI range"))?;
    let mut pitches = pitches.clone();
    pitches[index] = changed;
    let kind = EventKind::Chord {
        pitches,
        tied: *tied,
    };
    doc.execute(Command::ReplaceEvent {
        at: located.cursor.at(),
        index: located.index,
        event: Event { kind, ..event },
    })?;
    Ok(changed)
}

pub fn transpose_octaves(doc: &mut Document, id: EventId, octaves: i8) -> Result<(), ModelError> {
    edit_pitches(doc, id, |pitch| {
        let moved = Pitch {
            octave: pitch.octave.saturating_add(octaves),
            ..pitch
        };
        moved.validate()?;
        Ok(moved)
    })
}

pub fn set_alter(doc: &mut Document, id: EventId, alter: i8) -> Result<(), ModelError> {
    edit_pitches(doc, id, |pitch| {
        let changed = Pitch { alter, ..pitch };
        changed.validate()?;
        Ok(changed)
    })
}

/// Stacks the given letter on top of a chord, in the first octave above its highest note.
pub fn add_note_above(doc: &mut Document, id: EventId, step: Step) -> Result<(), ModelError> {
    add_note_above_with_alter(doc, id, step, None)
}

pub fn add_note_above_with_alter(
    doc: &mut Document,
    id: EventId,
    step: Step,
    alter: Option<i8>,
) -> Result<(), ModelError> {
    let (located, event) = find(doc.score(), id)?;
    let EventKind::Chord { pitches, tied } = &event.kind else {
        return Err(ModelError::Invalid("a rest cannot hold a chord"));
    };
    let top = pitches
        .iter()
        .map(|p| p.diatonic_index())
        .max()
        .unwrap_or_default();
    let target = top + 1 + (i16::from(step.index()) - (top + 1)).rem_euclid(7);
    let key = doc.score().measures[located.cursor.measure].key_signature;
    let pitch = Pitch::new(
        step,
        alter.unwrap_or_else(|| key.alter(step)),
        i8::try_from(target.div_euclid(7)).unwrap_or(i8::MAX),
    );
    pitch.validate()?;

    let mut pitches = pitches.clone();
    pitches.push(pitch);
    let kind = EventKind::Chord {
        pitches,
        tied: *tied,
    };
    doc.execute(Command::ReplaceEvent {
        at: located.cursor.at(),
        index: located.index,
        event: Event { kind, ..event },
    })
}

pub fn add_pitch_to_chord(doc: &mut Document, id: EventId, pitch: Pitch) -> Result<(), ModelError> {
    pitch.validate()?;
    let (located, event) = find(doc.score(), id)?;
    let EventKind::Chord { pitches, tied } = &event.kind else {
        return Err(ModelError::Invalid("a rest cannot hold a chord"));
    };
    if pitches.contains(&pitch) {
        return Ok(());
    }
    let mut pitches = pitches.clone();
    pitches.push(pitch);
    let kind = EventKind::Chord {
        pitches,
        tied: *tied,
    };
    doc.execute(Command::ReplaceEvent {
        at: located.cursor.at(),
        index: located.index,
        event: Event { kind, ..event },
    })
}

pub fn remove_pitch_from_chord(
    doc: &mut Document,
    id: EventId,
    pitch: Pitch,
) -> Result<bool, ModelError> {
    let (located, event) = find(doc.score(), id)?;
    let EventKind::Chord { pitches, tied } = &event.kind else {
        return Err(ModelError::Invalid("a rest cannot hold a chord"));
    };
    let Some(index) = pitches.iter().position(|&existing| existing == pitch) else {
        return Ok(false);
    };
    if pitches.len() == 1 {
        return Ok(false);
    }
    let mut pitches = pitches.clone();
    pitches.remove(index);
    let kind = EventKind::Chord {
        pitches,
        tied: *tied,
    };
    doc.execute(Command::ReplaceEvent {
        at: located.cursor.at(),
        index: located.index,
        event: Event { kind, ..event },
    })?;
    Ok(true)
}

pub fn toggle_pitch_in_chord(
    doc: &mut Document,
    id: EventId,
    pitch: Pitch,
) -> Result<(), ModelError> {
    let (_, event) = find(doc.score(), id)?;
    let EventKind::Chord { pitches, .. } = &event.kind else {
        return Err(ModelError::Invalid("a rest cannot hold a chord"));
    };
    let contains_pitch = pitches.contains(&pitch);
    if contains_pitch {
        remove_pitch_from_chord(doc, id, pitch)?;
    } else {
        add_pitch_to_chord(doc, id, pitch)?;
    }
    Ok(())
}

/// Applies `change` to the markings of an event as one undoable step.
fn edit_markings(
    doc: &mut Document,
    id: EventId,
    change: impl FnOnce(&mut Markings),
) -> Result<(), ModelError> {
    let (located, mut event) = find(doc.score(), id)?;
    change(&mut event.markings);
    doc.execute(Command::ReplaceEvent {
        at: located.cursor.at(),
        index: located.index,
        event,
    })
}

/// Sets or, with `None`, removes the dynamic mark at an event.
pub fn set_dynamic(
    doc: &mut Document,
    id: EventId,
    dynamic: Option<Dynamic>,
) -> Result<(), ModelError> {
    edit_markings(doc, id, |markings| markings.dynamic = dynamic)
}

/// Sets or, with `None`, removes the start or end of a hairpin at an event.
pub fn set_hairpin(
    doc: &mut Document,
    id: EventId,
    hairpin: Option<Hairpin>,
) -> Result<(), ModelError> {
    edit_markings(doc, id, |markings| markings.hairpin = hairpin)
}

/// Adds the articulation, or removes it if the event already has it.
pub fn toggle_articulation(
    doc: &mut Document,
    id: EventId,
    articulation: Articulation,
) -> Result<(), ModelError> {
    edit_markings(doc, id, |markings| {
        match markings
            .articulations
            .iter()
            .position(|a| *a == articulation)
        {
            Some(position) => {
                markings.articulations.remove(position);
            }
            None => markings.articulations.push(articulation),
        }
    })
}

/// The previous or next event in the same voice, crossing measure boundaries.
pub fn adjacent_event(score: &Score, id: EventId, forward: bool) -> Option<EventId> {
    let located = locate(score, id)?;
    let staff = score.staff(located.cursor.staff).ok()?;
    let voice_in = |measure: usize| {
        staff
            .measures
            .get(measure)?
            .voices
            .get(located.cursor.voice)
    };
    let measure = located.cursor.measure;

    if forward {
        let events = &voice_in(measure)?.events;
        if let Some(event) = events.get(located.index + 1) {
            return Some(event.id);
        }
        (measure + 1..staff.measures.len()).find_map(|m| voice_in(m)?.events.first().map(|e| e.id))
    } else {
        if located.index > 0 {
            return Some(voice_in(measure)?.events[located.index - 1].id);
        }
        (0..measure)
            .rev()
            .find_map(|m| voice_in(m)?.events.last().map(|e| e.id))
    }
}

/// Moves the insertion cursor one event at a time, preserving empty measures as stops.
pub fn move_cursor(
    score: &Score,
    cursor: Cursor,
    selected: Option<EventId>,
    forward: bool,
) -> Option<Cursor> {
    let staff = score.staff(cursor.staff).ok()?;
    let adjacent_measure = if forward {
        cursor
            .measure
            .checked_add(1)
            .filter(|&m| m < staff.measures.len())
    } else {
        cursor.measure.checked_sub(1)
    };
    let cursor_in = |measure: usize, tick: u32| {
        let voices = &staff.measures.get(measure)?.voices;
        Some(Cursor {
            measure,
            voice: cursor.voice.min(voices.len().checked_sub(1)?),
            tick,
            ..cursor
        })
    };
    let voice_at = |measure: usize| {
        let target = cursor_in(measure, 0)?;
        staff.measures.get(measure)?.voices.get(target.voice)
    };

    if let Some(id) = selected {
        let located = locate(score, id)?;
        let voice = staff
            .measures
            .get(located.cursor.measure)?
            .voices
            .get(located.cursor.voice)?;
        let event = voice.events.get(located.index)?;
        let has_local_neighbor = if forward {
            located.index + 1 < voice.events.len()
        } else {
            located.index > 0
        };
        if forward {
            let tick = located.cursor.tick + event.duration.ticks();
            let capacity = score.measure_capacity(located.cursor.measure).ok()?;
            if tick == capacity && located.cursor.measure + 1 < staff.measures.len() {
                return cursor_in(located.cursor.measure + 1, 0);
            }
            return Some(Cursor {
                tick,
                ..located.cursor
            });
        }
        if !has_local_neighbor && let Some(measure) = adjacent_measure {
            let empty = staff
                .measures
                .get(measure)?
                .voices
                .get(cursor_in(measure, 0)?.voice)?
                .events
                .is_empty();
            if empty {
                return cursor_in(measure, 0);
            }
        }
        if located.index > 0 {
            let tick = voice.events[..located.index - 1]
                .iter()
                .map(|previous| previous.duration.ticks())
                .sum();
            return Some(Cursor {
                tick,
                ..located.cursor
            });
        }
        let Some(measure) = adjacent_measure else {
            return Some(located.cursor);
        };
        voice_at(measure)?;
        return cursor_before_measure_end(score, cursor_in(measure, 0)?, measure);
    }

    let voice = voice_at(cursor.measure)?;
    let mut onset = 0;
    if forward {
        for event in &voice.events {
            let end = onset + event.duration.ticks();
            if end > cursor.tick || onset >= cursor.tick {
                return Some(Cursor {
                    tick: end,
                    ..cursor
                });
            }
            onset = end;
        }
        let measure = adjacent_measure?;
        voice_at(measure)?;
        return cursor_in(measure, 0);
    }

    let mut previous = None;
    for event in &voice.events {
        if onset >= cursor.tick {
            break;
        }
        previous = Some(onset);
        onset += event.duration.ticks();
    }
    if let Some(tick) = previous {
        return Some(Cursor { tick, ..cursor });
    }
    let measure = adjacent_measure?;
    let previous_voice = voice_at(measure)?;
    if previous_voice.events.is_empty() {
        return cursor_in(measure, 0);
    }
    cursor_before_measure_end(score, cursor_in(measure, 0)?, measure)
}

fn cursor_before_measure_end(score: &Score, cursor: Cursor, measure: usize) -> Option<Cursor> {
    cursor_at_content_end(score, Cursor { measure, ..cursor })
}

pub fn move_cursor_to_measure(score: &Score, cursor: Cursor, forward: bool) -> Option<Cursor> {
    let measure = if forward {
        cursor.measure.checked_add(1)?
    } else {
        cursor.measure.checked_sub(1)?
    };
    score.measure_capacity(measure).ok()?;
    let voices = &score
        .staff(cursor.staff)
        .ok()?
        .measures
        .get(measure)?
        .voices;
    Some(Cursor {
        measure,
        voice: cursor.voice.min(voices.len().checked_sub(1)?),
        tick: 0,
        ..cursor
    })
}

pub fn move_cursor_to_staff(score: &Score, cursor: Cursor, down: bool) -> Option<Cursor> {
    let staves: Vec<_> = score.staves().collect();
    let current = staves.iter().position(|staff| staff.id == cursor.staff)?;
    let target = if down {
        *staves.get(current.checked_add(1)?)?
    } else {
        *staves.get(current.checked_sub(1)?)?
    };
    let capacity = score.measure_capacity(cursor.measure).ok()?;
    let target_voices = &target.measures.get(cursor.measure)?.voices;
    let voice_index = cursor.voice.min(target_voices.len().checked_sub(1)?);
    let voice = target_voices.get(voice_index)?;
    if voice.events.is_empty() {
        return Some(Cursor {
            staff: target.id,
            voice: voice_index,
            tick: 0,
            ..cursor
        });
    }
    let clamped_tick = cursor.tick.min(capacity).min(voice.used_ticks());
    let mut candidates = vec![0];
    let mut onset = 0;
    for event in &voice.events {
        candidates.push(onset);
        onset += event.duration.ticks();
    }
    candidates.push(onset);
    let tick = candidates
        .into_iter()
        .min_by_key(|candidate| candidate.abs_diff(clamped_tick))?;
    Some(Cursor {
        staff: target.id,
        voice: voice_index,
        tick,
        ..cursor
    })
}

pub fn append_measure(doc: &mut Document) -> Result<(), ModelError> {
    let last = doc.score().measures.len() - 1;
    insert_measure_after(doc, last)
}

/// Adds a new staff-wide voice and returns its index.
pub fn add_voice(doc: &mut Document, staff: StaffId, measure: usize) -> Result<usize, ModelError> {
    let staff_data = doc.score().staff(staff)?;
    let voice_count = staff_data
        .measures
        .get(measure)
        .map(|measure| measure.voices.len())
        .ok_or(ModelError::MeasureOutOfRange(measure))?;
    if staff_data.measures[measure..]
        .iter()
        .any(|content| content.voices.len() < voice_count)
    {
        return Err(ModelError::Invalid(
            "voice indices must remain available after their start measure",
        ));
    }
    let voices = vec![Voice::default(); doc.score().measures.len() - measure];
    doc.execute(Command::InsertVoice {
        staff,
        measure,
        index: voice_count,
        voices,
    })?;
    Ok(voice_count)
}

/// Removes a staff-wide additional voice as one undoable edit.
pub fn remove_voice(doc: &mut Document, staff: StaffId, voice: usize) -> Result<(), ModelError> {
    let staff_data = doc.score().staff(staff)?;
    let measure = staff_data
        .measures
        .iter()
        .position(|content| content.voices.len() > voice)
        .ok_or(ModelError::VoiceOutOfRange(voice))?;
    doc.execute(Command::RemoveVoice {
        staff,
        measure,
        index: voice,
    })
}

/// Ends an additional voice at `tick`, moving its suffix into the primary voice.
pub fn end_voice_at(
    doc: &mut Document,
    staff: StaffId,
    measure: usize,
    voice: usize,
    tick: u32,
) -> Result<(), ModelError> {
    if voice == 0 {
        return Err(ModelError::Invalid("the original voice cannot be ended"));
    }
    let at = VoiceLocation {
        staff,
        measure,
        voice,
    };
    let capacity = doc.score().measure_capacity(measure)?;
    if tick > capacity {
        return Err(ModelError::Invalid("voice end is beyond the measure"));
    }
    let target_events = doc.score().voice(at)?.events.clone();
    let mut onset = 0;
    let mut first_removed = target_events.len();
    for (index, event) in target_events.iter().enumerate() {
        if onset == tick {
            first_removed = index;
            break;
        }
        let end = onset + event.duration.ticks();
        if onset < tick && tick < end {
            return Err(ModelError::Invalid("voice must end at an event boundary"));
        }
        onset = end;
    }
    if first_removed == target_events.len() && tick > onset {
        return Err(ModelError::Invalid("voice must end at an event boundary"));
    }

    let suffix = target_events[first_removed..].to_vec();
    let primary_at = VoiceLocation { voice: 0, ..at };
    let primary_ticks = doc.score().voice(primary_at)?.used_ticks();
    let mut primary_index = doc.score().voice(primary_at)?.events.len();
    if !suffix.is_empty() && primary_ticks > tick {
        return Err(ModelError::Invalid(
            "primary voice already contains music after this point",
        ));
    }

    let mut commands = Vec::new();
    let padding = if primary_ticks < tick {
        rest_durations(tick - primary_ticks)?
    } else {
        Vec::new()
    };
    for duration in padding {
        let event = Event::new(doc.new_event_id(), duration, EventKind::Rest);
        commands.push(Command::InsertEvent {
            at: primary_at,
            index: primary_index,
            event,
        });
        primary_index += 1;
    }
    for event in suffix {
        commands.push(Command::InsertEvent {
            at: primary_at,
            index: primary_index,
            event,
        });
        primary_index += 1;
    }
    commands.extend(
        (first_removed..target_events.len())
            .rev()
            .map(|index| Command::RemoveEvent { at, index }),
    );
    commands.push(Command::SetVoiceEnd {
        at,
        end_tick: Some(tick),
    });
    if measure + 1 < doc.score().measures.len() {
        commands.push(Command::RemoveVoice {
            staff,
            measure: measure + 1,
            index: voice,
        });
    }
    doc.execute(Command::Batch { commands })
}

/// Inserts an empty measure that takes over the signatures active at its position.
pub fn insert_measure_at(doc: &mut Document, index: usize) -> Result<(), ModelError> {
    if index > doc.score().measures.len() {
        return Err(ModelError::MeasureOutOfRange(index));
    }
    let data = doc.score().blank_measure(index);
    doc.execute(Command::InsertMeasure { index, data })
}

/// Inserts an empty measure after `measure`.
pub fn insert_measure_after(doc: &mut Document, measure: usize) -> Result<(), ModelError> {
    if measure >= doc.score().measures.len() {
        return Err(ModelError::MeasureOutOfRange(measure));
    }
    insert_measure_at(doc, measure + 1)
}

/// Removes a measure and all its staff content as a single undoable edit.
pub fn remove_measure(doc: &mut Document, measure: usize) -> Result<(), ModelError> {
    doc.execute(Command::RemoveMeasure { index: measure })
}

/// Changes the time signature from `measure` up to the next time signature change.
pub fn change_time_signature(
    doc: &mut Document,
    measure: usize,
    time_signature: TimeSignature,
) -> Result<(), ModelError> {
    change_time_signature_with_display(doc, measure, time_signature, TimeSignatureDisplay::Numeric)
}

pub fn change_time_signature_with_display(
    doc: &mut Document,
    measure: usize,
    time_signature: TimeSignature,
    display: TimeSignatureDisplay,
) -> Result<(), ModelError> {
    time_signature.validate()?;
    let score = doc.score();
    let old_info = score
        .measures
        .get(measure)
        .ok_or(ModelError::MeasureOutOfRange(measure))?;
    let candidate = crate::score::MeasureInfo {
        time_signature,
        time_signature_display: display,
        ..*old_info
    };
    candidate.validate()?;
    let old = old_info.time_signature;
    let old_display = old_info.time_signature_display;
    if old == time_signature && old_display == display {
        return Ok(());
    }
    let commands = score.measures[measure..]
        .iter()
        .take_while(|info| info.time_signature == old && info.time_signature_display == old_display)
        .enumerate()
        .flat_map(|(offset, _)| {
            [
                Command::SetTimeSignature {
                    measure: measure + offset,
                    time_signature,
                },
                Command::SetTimeSignatureDisplay {
                    measure: measure + offset,
                    display,
                },
            ]
        })
        .collect();
    doc.execute(Command::Batch { commands })
}

/// Changes the key signature from `measure` through the current key section.
pub fn change_key_signature(
    doc: &mut Document,
    measure: usize,
    key_signature: crate::score::KeySignature,
) -> Result<(), ModelError> {
    key_signature.validate()?;
    let score = doc.score();
    let old = score
        .measures
        .get(measure)
        .ok_or(ModelError::MeasureOutOfRange(measure))?
        .key_signature;
    if old == key_signature {
        return Ok(());
    }
    let commands = score.measures[measure..]
        .iter()
        .take_while(|info| info.key_signature == old)
        .enumerate()
        .map(|(offset, _)| Command::SetKeySignature {
            measure: measure + offset,
            key_signature,
        })
        .collect();
    doc.execute(Command::Batch { commands })
}

/// Shortens a measure to what is written in it, as for a pickup (anacrusis);
/// a measure that is already shortened gets its full length back.
pub fn toggle_pickup(doc: &mut Document, measure: usize) -> Result<(), ModelError> {
    let score = doc.score();
    let info = *score
        .measures
        .get(measure)
        .ok_or(ModelError::MeasureOutOfRange(measure))?;

    let length = if info.length.is_some() {
        None
    } else {
        let written = score
            .staves()
            .flat_map(|staff| &staff.measures[measure].voices)
            .map(Voice::used_ticks)
            .max()
            .unwrap_or(0);
        if written == 0 {
            return Err(ModelError::Invalid(
                "enter the pickup notes first, then shorten the measure",
            ));
        }
        if written >= info.capacity() {
            return Err(ModelError::Invalid("the measure is already full"));
        }
        Some(written)
    };
    doc.execute(Command::SetMeasureLength { measure, length })
}
