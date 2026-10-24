use std::collections::HashSet;
use std::mem;

use serde::{Deserialize, Serialize};

use crate::duration::TimeSignature;
use crate::error::ModelError;
use crate::ids::{PartId, StaffId};
use crate::score::{
    Clef, Event, KeySignature, MeasureData, MeasureInfo, Part, Score, StaffMeasure, VoiceLocation,
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
    RemovePart {
        part: PartId,
    },
    SetMetadata {
        title: String,
        composer: String,
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
    if content
        .voices
        .iter()
        .any(|voice| voice.used_ticks() > capacity)
    {
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
                if voice.used_ticks() + event.duration.ticks() > capacity {
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
            Command::ReplaceEvent { at, index, event } => {
                event.validate()?;
                let capacity = score.measure_capacity(at.measure)?;
                let voice = score.voice_mut(at)?;
                let old = voice
                    .events
                    .get(index)
                    .ok_or(ModelError::EventOutOfRange(index))?;
                let used = voice.used_ticks() - old.duration.ticks() + event.duration.ticks();
                if used > capacity {
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
                let content = score
                    .staves_mut()
                    .map(|staff| (staff.id, staff.measures.remove(index)))
                    .collect();
                Ok(Command::InsertMeasure {
                    index,
                    data: MeasureData { info, content },
                })
            }
            Command::SetTimeSignature {
                measure,
                time_signature,
            } => {
                check_measure_index(score, measure)?;
                let candidate = MeasureInfo {
                    time_signature,
                    ..score.measures[measure]
                };
                candidate.validate()?;
                let capacity = candidate.capacity();
                for staff in score.staves() {
                    check_fits(&staff.measures[measure], capacity)?;
                }
                let old = mem::replace(&mut score.measures[measure].time_signature, time_signature);
                Ok(Command::SetTimeSignature {
                    measure,
                    time_signature: old,
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
            Command::RemovePart { part } => {
                let index = score
                    .parts
                    .iter()
                    .position(|p| p.id == part)
                    .ok_or(ModelError::PartNotFound(part))?;
                let part = score.parts.remove(index);
                Ok(Command::AddPart { index, part })
            }
            Command::SetMetadata { title, composer } => {
                let title = mem::replace(&mut score.title, title);
                let composer = mem::replace(&mut score.composer, composer);
                Ok(Command::SetMetadata { title, composer })
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
