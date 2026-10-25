use std::collections::HashSet;
use std::mem;

use serde::{Deserialize, Serialize};

use crate::duration::{TimeSignature, TimeSignatureDisplay};
use crate::error::ModelError;
use crate::ids::{PartId, StaffId};
use crate::score::{
    Clef, Event, KeySignature, MeasureData, MeasureInfo, Part, Score, Staff, StaffMeasure, Tempo,
    TempoDirection, TempoDirectionMark, Voice, VoiceLocation,
};

/// A reversible edit. `apply` leaves the score untouched on error and returns the inverse command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    InsertEvent {
        at: VoiceLocation,
        index: usize,
        event: Event,
    },
    RemoveEvent {
        at: VoiceLocation,
        index: usize,
    },
    InsertVoice {
        staff: StaffId,
        measure: usize,
        index: usize,
        voices: Vec<Voice>,
    },
    RemoveVoice {
        staff: StaffId,
        measure: usize,
        index: usize,
    },
    SetVoiceEnd {
        at: VoiceLocation,
        end_tick: Option<u32>,
    },
    ReplaceEvent {
        at: VoiceLocation,
        index: usize,
        event: Event,
    },
    InsertMeasure {
        index: usize,
        data: MeasureData,
    },
    RemoveMeasure {
        index: usize,
    },
    SetTimeSignature {
        measure: usize,
        time_signature: TimeSignature,
    },
    SetTimeSignatureDisplay {
        measure: usize,
        display: TimeSignatureDisplay,
    },
    SetKeySignature {
        measure: usize,
        key_signature: KeySignature,
    },
    /// `None` restores the full length given by the time signature.
    SetMeasureLength {
        measure: usize,
        length: Option<u32>,
    },
    SetClef {
        staff: StaffId,
        clef: Clef,
    },
    AddPart {
        index: usize,
        part: Part,
    },
    MovePart {
        part: PartId,
        index: usize,
    },
    RemovePart {
        part: PartId,
    },
    SetPartName {
        part: PartId,
        name: String,
    },
    SetPartShortName {
        part: PartId,
        short_name: String,
    },
    SetPartStaves {
        part: PartId,
        staves: Vec<Staff>,
    },
    SetMetadata {
        title: String,
        composer: String,
    },
    SetTempo {
        tempo: Tempo,
    },
    SetTempoDirection {
        measure: usize,
        tick: u32,
        direction: Option<TempoDirection>,
    },
    /// Applied all-or-nothing and undone as a single step.
    Batch {
        commands: Vec<Command>,
    },
}

fn check_fits(content: &StaffMeasure, capacity: u32) -> Result<(), ModelError> {
    if content.voices.is_empty() {
        return Err(ModelError::Invalid("a measure needs at least one voice"));
    }
    if content.voices.iter().any(|voice| {
        voice.used_ticks() > capacity
            || voice
                .end_tick
                .is_some_and(|end| end > capacity || end < voice.used_ticks())
    }) {
        return Err(ModelError::MeasureOverfull);
    }
    Ok(())
}

fn check_measure_index(score: &Score, measure: usize) -> Result<(), ModelError> {
    if measure < score.measures.len() {
        Ok(())
    } else {
        Err(ModelError::MeasureOutOfRange(measure))
    }
}

impl Command {
    pub fn apply(self, score: &mut Score) -> Result<Command, ModelError> {
        match self {
            Command::InsertEvent { at, index, event } => {
                event.validate()?;
                let capacity = score.measure_capacity(at.measure)?;
                let voice = score.voice_mut(at)?;
                if index > voice.events.len() {
                    return Err(ModelError::EventOutOfRange(index));
                }
                let new_end = voice.used_ticks() + event.duration.ticks();
                if new_end > capacity || voice.end_tick.is_some_and(|end| new_end > end) {
                    return Err(ModelError::MeasureOverfull);
                }
                voice.events.insert(index, event);
                Ok(Command::RemoveEvent { at, index })
            }
            Command::RemoveEvent { at, index } => {
                score.measure_capacity(at.measure)?;
                let voice = score.voice_mut(at)?;
                if index >= voice.events.len() {
                    return Err(ModelError::EventOutOfRange(index));
                }
                let event = voice.events.remove(index);
                Ok(Command::InsertEvent { at, index, event })
            }
            Command::InsertVoice {
                staff,
                measure,
                index,
                voices,
            } => {
                check_measure_index(score, measure)?;
                if voices.len() != score.measures.len() - measure {
                    return Err(ModelError::Invalid(
                        "voice data must cover every affected measure",
                    ));
                }
                for (voice, info) in voices.iter().zip(&score.measures[measure..]) {
                    if voice.used_ticks() > info.capacity() {
                        return Err(ModelError::MeasureOverfull);
                    }
                    for event in &voice.events {
                        event.validate()?;
                    }
                }
                let target = score.staff_mut(staff)?;
                if target.measures[measure..]
                    .iter()
                    .any(|content| content.voices.len() < index)
                {
                    return Err(ModelError::Invalid(
                        "voice index is not available in every affected measure",
                    ));
                }
                for (content, voice) in target.measures[measure..].iter_mut().zip(voices) {
                    content.voices.insert(index, voice);
                }
                Ok(Command::RemoveVoice {
                    staff,
                    measure,
                    index,
                })
            }
            Command::RemoveVoice {
                staff,
                measure,
                index,
            } => {
                if index == 0 {
                    return Err(ModelError::Invalid("the original voice cannot be removed"));
                }
                check_measure_index(score, measure)?;
                let target = score.staff_mut(staff)?;
                if target.measures[measure..]
                    .iter()
                    .any(|content| content.voices.len() <= index)
                {
                    return Err(ModelError::Invalid(
                        "voice does not exist in every affected measure",
                    ));
                }
                let voices = target.measures[measure..]
                    .iter_mut()
                    .map(|content| content.voices.remove(index))
                    .collect();
                Ok(Command::InsertVoice {
                    staff,
                    measure,
                    index,
                    voices,
                })
            }
            Command::SetVoiceEnd { at, end_tick } => {
                let capacity = score.measure_capacity(at.measure)?;
                let voice = score.voice_mut(at)?;
                if end_tick.is_some_and(|end| end > capacity || end < voice.used_ticks()) {
                    return Err(ModelError::Invalid(
                        "voice end must follow its events within the measure",
                    ));
                }
                let old = mem::replace(&mut voice.end_tick, end_tick);
                Ok(Command::SetVoiceEnd { at, end_tick: old })
            }
            Command::ReplaceEvent { at, index, event } => {
                event.validate()?;
                let capacity = score.measure_capacity(at.measure)?;
                let voice = score.voice_mut(at)?;
                let old = voice
                    .events
                    .get(index)
                    .ok_or(ModelError::EventOutOfRange(index))?;
                let used = voice.used_ticks() - old.duration.ticks() + event.duration.ticks();
                if used > capacity || voice.end_tick.is_some_and(|end| used > end) {
                    return Err(ModelError::MeasureOverfull);
                }
                let old = mem::replace(&mut voice.events[index], event);
                Ok(Command::ReplaceEvent {
                    at,
                    index,
                    event: old,
                })
            }
            Command::InsertMeasure { index, data } => {
                if index > score.measures.len() {
                    return Err(ModelError::MeasureOutOfRange(index));
                }
                data.info.validate()?;
                if data.content.len() != score.staves().count() {
                    return Err(ModelError::Invalid(
                        "measure content must cover every staff",
                    ));
                }
                let mut seen = HashSet::new();
                let capacity = data.info.capacity();
                for (id, content) in &data.content {
                    score.staff(*id)?;
                    if !seen.insert(*id) {
                        return Err(ModelError::DuplicateId(id.0));
                    }
                    check_fits(content, capacity)?;
                }
                score.measures.insert(index, data.info);
                for mark in &mut score.tempo_directions {
                    if mark.measure >= index {
                        mark.measure += 1;
                    }
                }
                for (id, content) in data.content {
                    score.staff_mut(id)?.measures.insert(index, content);
                }
                Ok(Command::RemoveMeasure { index })
            }
            Command::RemoveMeasure { index } => {
                check_measure_index(score, index)?;
                if score.measures.len() == 1 {
                    return Err(ModelError::Invalid("a score needs at least one measure"));
                }
                let info = score.measures.remove(index);
                let removed_directions: Vec<_> = score
                    .tempo_directions
                    .iter()
                    .filter(|mark| mark.measure == index)
                    .copied()
                    .collect();
                score.tempo_directions.retain(|mark| mark.measure != index);
                for mark in &mut score.tempo_directions {
                    if mark.measure > index {
                        mark.measure -= 1;
                    }
                }
                let content = score
                    .staves_mut()
                    .map(|staff| (staff.id, staff.measures.remove(index)))
                    .collect();
                let mut commands = vec![Command::InsertMeasure {
                    index,
                    data: MeasureData { info, content },
                }];
                commands.extend(removed_directions.into_iter().map(|mark| {
                    Command::SetTempoDirection {
                        measure: mark.measure,
                        tick: mark.tick,
                        direction: Some(mark.direction),
                    }
                }));
                Ok(Command::Batch { commands })
            }
            Command::SetTimeSignature {
                measure,
                time_signature,
            } => {
                check_measure_index(score, measure)?;
                let old_info = score.measures[measure];
                let display = match old_info.time_signature_display {
                    TimeSignatureDisplay::Common if time_signature != TimeSignature::COMMON => {
                        TimeSignatureDisplay::Numeric
                    }
                    TimeSignatureDisplay::AllaBreve
                        if time_signature
                            != (TimeSignature {
                                numerator: 2,
                                denominator: 2,
                            }) =>
                    {
                        TimeSignatureDisplay::Numeric
                    }
                    current => current,
                };
                let candidate = MeasureInfo {
                    time_signature,
                    time_signature_display: display,
                    ..old_info
                };
                candidate.validate()?;
                let capacity = candidate.capacity();
                for staff in score.staves() {
                    check_fits(&staff.measures[measure], capacity)?;
                }
                let old = mem::replace(&mut score.measures[measure].time_signature, time_signature);
                let old_display =
                    mem::replace(&mut score.measures[measure].time_signature_display, display);
                Ok(Command::Batch {
                    commands: vec![
                        Command::SetTimeSignature {
                            measure,
                            time_signature: old,
                        },
                        Command::SetTimeSignatureDisplay {
                            measure,
                            display: old_display,
                        },
                    ],
                })
            }
            Command::SetTimeSignatureDisplay { measure, display } => {
                check_measure_index(score, measure)?;
                let candidate = MeasureInfo {
                    time_signature_display: display,
                    ..score.measures[measure]
                };
                candidate.validate()?;
                let old =
                    mem::replace(&mut score.measures[measure].time_signature_display, display);
                Ok(Command::SetTimeSignatureDisplay {
                    measure,
                    display: old,
                })
            }
            Command::SetMeasureLength { measure, length } => {
                check_measure_index(score, measure)?;
                let info = score.measures[measure];
                // A length equal to the time signature is the same as no override.
                let length = length.filter(|&l| l != info.time_signature.ticks_per_measure());
                let candidate = MeasureInfo { length, ..info };
                candidate.validate()?;
                let capacity = candidate.capacity();
                for staff in score.staves() {
                    check_fits(&staff.measures[measure], capacity)?;
                }
                let old = mem::replace(&mut score.measures[measure].length, length);
                Ok(Command::SetMeasureLength {
                    measure,
                    length: old,
                })
            }
            Command::SetKeySignature {
                measure,
                key_signature,
            } => {
                check_measure_index(score, measure)?;
                key_signature.validate()?;
                let old = mem::replace(&mut score.measures[measure].key_signature, key_signature);
                Ok(Command::SetKeySignature {
                    measure,
                    key_signature: old,
                })
            }
            Command::SetClef { staff, clef } => {
                let old = mem::replace(&mut score.staff_mut(staff)?.clef, clef);
                Ok(Command::SetClef { staff, clef: old })
            }
            Command::AddPart { index, part } => {
                if index > score.parts.len() {
                    return Err(ModelError::Invalid("part index is out of range"));
                }
                let mut used: HashSet<u64> = score.parts.iter().map(|p| p.id.0).collect();
                used.extend(score.staves().map(|s| s.id.0));
                for id in std::iter::once(part.id.0).chain(part.staves.iter().map(|s| s.id.0)) {
                    if !used.insert(id) {
                        return Err(ModelError::DuplicateId(id));
                    }
                }
                for staff in &part.staves {
                    if staff.measures.len() != score.measures.len() {
                        return Err(ModelError::Invalid("staff must have one entry per measure"));
                    }
                    for (content, info) in staff.measures.iter().zip(&score.measures) {
                        check_fits(content, info.capacity())?;
                    }
                }
                let id = part.id;
                score.parts.insert(index, part);
                Ok(Command::RemovePart { part: id })
            }
            Command::MovePart { part, index } => {
                let old_index = score
                    .parts
                    .iter()
                    .position(|candidate| candidate.id == part)
                    .ok_or(ModelError::PartNotFound(part))?;
                if index >= score.parts.len() {
                    return Err(ModelError::Invalid("part index is out of range"));
                }
                let moved = score.parts.remove(old_index);
                score.parts.insert(index, moved);
                Ok(Command::MovePart {
                    part,
                    index: old_index,
                })
            }
            Command::RemovePart { part } => {
                let index = score
                    .parts
                    .iter()
                    .position(|p| p.id == part)
                    .ok_or(ModelError::PartNotFound(part))?;
                let part = score.parts.remove(index);
                Ok(Command::AddPart { index, part })
            }
            Command::SetPartName { part, name } => {
                if name.trim().is_empty() {
                    return Err(ModelError::Invalid("part name cannot be empty"));
                }
                let target = score
                    .parts
                    .iter_mut()
                    .find(|candidate| candidate.id == part)
                    .ok_or(ModelError::PartNotFound(part))?;
                let old = mem::replace(&mut target.name, name);
                Ok(Command::SetPartName { part, name: old })
            }
            Command::SetPartShortName { part, short_name } => {
                let target = score
                    .parts
                    .iter_mut()
                    .find(|candidate| candidate.id == part)
                    .ok_or(ModelError::PartNotFound(part))?;
                let old = mem::replace(&mut target.short_name, short_name);
                Ok(Command::SetPartShortName {
                    part,
                    short_name: old,
                })
            }
            Command::SetPartStaves { part, staves } => {
                if staves.is_empty() {
                    return Err(ModelError::Invalid("a part needs at least one staff"));
                }
                let part_index = score
                    .parts
                    .iter()
                    .position(|candidate| candidate.id == part)
                    .ok_or(ModelError::PartNotFound(part))?;
                let old_staves = &score.parts[part_index].staves;
                let retained: HashSet<_> = staves.iter().map(|staff| staff.id).collect();
                if old_staves.iter().any(|staff| {
                    !retained.contains(&staff.id)
                        && staff.measures.iter().any(|measure| {
                            measure.voices.iter().any(|voice| !voice.events.is_empty())
                        })
                }) {
                    return Err(ModelError::Invalid(
                        "cannot remove a staff that contains notes or rests",
                    ));
                }

                let old_ids: HashSet<_> = old_staves.iter().map(|staff| staff.id).collect();
                let mut used: HashSet<u64> = score.parts.iter().map(|part| part.id.0).collect();
                let mut seen_staves = HashSet::new();
                used.extend(
                    score
                        .staves()
                        .filter(|staff| !old_ids.contains(&staff.id))
                        .map(|staff| staff.id.0),
                );
                for staff in &staves {
                    if !seen_staves.insert(staff.id) {
                        return Err(ModelError::DuplicateId(staff.id.0));
                    }
                    if !old_ids.contains(&staff.id) && !used.insert(staff.id.0) {
                        return Err(ModelError::DuplicateId(staff.id.0));
                    }
                    if staff.measures.len() != score.measures.len() {
                        return Err(ModelError::Invalid("staff must have one entry per measure"));
                    }
                    for (content, info) in staff.measures.iter().zip(&score.measures) {
                        check_fits(content, info.capacity())?;
                    }
                }
                let old = mem::replace(&mut score.parts[part_index].staves, staves);
                Ok(Command::SetPartStaves { part, staves: old })
            }
            Command::SetMetadata { title, composer } => {
                let title = mem::replace(&mut score.title, title);
                let composer = mem::replace(&mut score.composer, composer);
                Ok(Command::SetMetadata { title, composer })
            }
            Command::SetTempo { tempo } => {
                tempo.validate()?;
                let old = mem::replace(&mut score.tempo, tempo);
                Ok(Command::SetTempo { tempo: old })
            }
            Command::SetTempoDirection {
                measure,
                tick,
                direction,
            } => {
                let capacity = score.measure_capacity(measure)?;
                if tick > capacity {
                    return Err(ModelError::Invalid(
                        "tempo direction position is outside its measure",
                    ));
                }
                let existing = score
                    .tempo_directions
                    .iter()
                    .position(|mark| mark.measure == measure && mark.tick == tick);
                let old = existing.map(|index| score.tempo_directions.remove(index).direction);
                if let Some(direction) = direction {
                    score.tempo_directions.push(TempoDirectionMark {
                        measure,
                        tick,
                        direction,
                    });
                    score
                        .tempo_directions
                        .sort_by_key(|mark| (mark.measure, mark.tick));
                }
                Ok(Command::SetTempoDirection {
                    measure,
                    tick,
                    direction: old,
                })
            }
            Command::Batch { commands } => {
                let mut inverses = Vec::with_capacity(commands.len());
                for command in commands {
                    match command.apply(score) {
                        Ok(inverse) => inverses.push(inverse),
                        Err(error) => {
                            for inverse in inverses.into_iter().rev() {
                                let undone = inverse.apply(score);
                                debug_assert!(
                                    undone.is_ok(),
                                    "inverse of an applied command failed"
                                );
                            }
                            return Err(error);
                        }
                    }
                }
                inverses.reverse();
                Ok(Command::Batch { commands: inverses })
            }
        }
    }
}
