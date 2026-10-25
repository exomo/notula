use serde::{Deserialize, Serialize};

use crate::duration::{
    Duration, NoteValue, TICKS_PER_QUARTER, TimeSignature, TimeSignatureDisplay,
};
use crate::error::ModelError;
use crate::ids::{EventId, PartId, StaffId};
use crate::markings::Markings;
use crate::pitch::{Pitch, Step};

/// Score-wide metronome setting, expressed in the displayed beat unit per minute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tempo {
    pub beat_unit: NoteValue,
    pub units_per_minute: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TempoDirection {
    Accelerando,
    Ritardando,
    Rallentando,
    ATempo,
    Rubato,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TempoDirectionMark {
    pub measure: usize,
    pub tick: u32,
    pub direction: TempoDirection,
}

impl Default for Tempo {
    fn default() -> Self {
        Self {
            beat_unit: NoteValue::Quarter,
            units_per_minute: 120,
        }
    }
}

impl Tempo {
    pub fn validate(self) -> Result<(), ModelError> {
        if !(1..=1000).contains(&self.units_per_minute) {
            return Err(ModelError::Invalid(
                "tempo must be 1..=1000 units per minute",
            ));
        }
        Ok(())
    }

    /// Equivalent quarter notes per minute, used to convert score ticks to seconds.
    pub fn quarter_notes_per_minute(self) -> f64 {
        f64::from(self.units_per_minute) * f64::from(self.beat_unit.ticks())
            / f64::from(TICKS_PER_QUARTER)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Clef {
    Treble,
    Bass,
    Alto,
    Tenor,
}

/// Number of sharps (positive) or flats (negative), -7..=7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct KeySignature {
    pub fifths: i8,
}

impl KeySignature {
    pub fn validate(self) -> Result<(), ModelError> {
        if !(-7..=7).contains(&self.fifths) {
            return Err(ModelError::Invalid("key signature must be within -7..=7"));
        }
        Ok(())
    }

    /// Alteration the key signature applies to a step: 1 sharp, -1 flat, 0 none.
    pub fn alter(self, step: Step) -> i8 {
        const SHARP_ORDER: [Step; 7] = [
            Step::F,
            Step::C,
            Step::G,
            Step::D,
            Step::A,
            Step::E,
            Step::B,
        ];
        const FLAT_ORDER: [Step; 7] = [
            Step::B,
            Step::E,
            Step::A,
            Step::D,
            Step::G,
            Step::C,
            Step::F,
        ];
        let count = usize::from(self.fifths.unsigned_abs()).min(7);
        if self.fifths > 0 && SHARP_ORDER[..count].contains(&step) {
            1
        } else if self.fifths < 0 && FLAT_ORDER[..count].contains(&step) {
            -1
        } else {
            0
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    Rest,
    /// `tied` joins this chord to the next event.
    Chord {
        pitches: Vec<Pitch>,
        tied: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub duration: Duration,
    pub kind: EventKind,
    #[serde(default, skip_serializing_if = "Markings::is_empty")]
    pub markings: Markings,
}

impl Event {
    pub fn new(id: EventId, duration: Duration, kind: EventKind) -> Self {
        Self {
            id,
            duration,
            kind,
            markings: Markings::default(),
        }
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        self.duration.validate()?;
        if let EventKind::Chord { pitches, .. } = &self.kind {
            if pitches.is_empty() {
                return Err(ModelError::Invalid("a chord needs at least one pitch"));
            }
            for pitch in pitches {
                pitch.validate()?;
            }
        }
        self.markings.validate(matches!(self.kind, EventKind::Rest))
    }
}

/// An empty voice stands for a whole-measure rest.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Voice {
    pub events: Vec<Event>,
    /// Explicit end within a measure, used when another voice continues alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_tick: Option<u32>,
}

impl Voice {
    pub fn used_ticks(&self) -> u32 {
        self.events.iter().map(|e| e.duration.ticks()).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaffMeasure {
    pub voices: Vec<Voice>,
}

impl Default for StaffMeasure {
    fn default() -> Self {
        Self {
            voices: vec![Voice::default()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Staff {
    pub id: StaffId,
    pub clef: Clef,
    /// Always the same length as `Score::measures`.
    pub measures: Vec<StaffMeasure>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    pub id: PartId,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub short_name: String,
    pub staves: Vec<Staff>,
}

/// Properties shared by all staves in a measure.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeasureInfo {
    pub time_signature: TimeSignature,
    #[serde(default)]
    pub time_signature_display: TimeSignatureDisplay,
    pub key_signature: KeySignature,
    /// Length in ticks when shorter than the time signature, as in a pickup measure (anacrusis).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<u32>,
}

impl MeasureInfo {
    /// How much music the measure holds.
    pub fn capacity(self) -> u32 {
        self.length
            .unwrap_or_else(|| self.time_signature.ticks_per_measure())
    }

    pub fn validate(self) -> Result<(), ModelError> {
        self.time_signature.validate()?;
        match self.time_signature_display {
            TimeSignatureDisplay::Numeric => {}
            TimeSignatureDisplay::Common if self.time_signature == TimeSignature::COMMON => {}
            TimeSignatureDisplay::AllaBreve
                if self.time_signature
                    == (TimeSignature {
                        numerator: 2,
                        denominator: 2,
                    }) => {}
            TimeSignatureDisplay::Common | TimeSignatureDisplay::AllaBreve => {
                return Err(ModelError::Invalid(
                    "symbolic time signature does not match its meter",
                ));
            }
        }
        self.key_signature.validate()?;
        if let Some(length) = self.length {
            let grain = NoteValue::SixtyFourth.ticks();
            if length == 0 || length % grain != 0 {
                return Err(ModelError::Invalid(
                    "measure length must be a multiple of a 64th note",
                ));
            }
            if length > self.time_signature.ticks_per_measure() {
                return Err(ModelError::Invalid(
                    "measure length exceeds the time signature",
                ));
            }
        }
        Ok(())
    }
}

/// Everything needed to insert a measure, or to restore a removed one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeasureData {
    pub info: MeasureInfo,
    pub content: Vec<(StaffId, StaffMeasure)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceLocation {
    pub staff: StaffId,
    pub measure: usize,
    pub voice: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Score {
    pub title: String,
    pub composer: String,
    #[serde(default)]
    pub tempo: Tempo,
    #[serde(default)]
    pub tempo_directions: Vec<TempoDirectionMark>,
    pub parts: Vec<Part>,
    /// Never empty.
    pub measures: Vec<MeasureInfo>,
    next_id: IdCounter,
}

/// Id allocation is not undoable, so the counter must not affect score equality.
#[derive(Debug, Clone, Copy, Eq, Serialize, Deserialize)]
#[serde(transparent)]
struct IdCounter(u64);

impl PartialEq for IdCounter {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Default for Score {
    fn default() -> Self {
        Self::new()
    }
}

impl Score {
    pub fn new() -> Self {
        Self {
            title: String::new(),
            composer: String::new(),
            tempo: Tempo::default(),
            tempo_directions: Vec::new(),
            parts: Vec::new(),
            measures: vec![MeasureInfo::default()],
            next_id: IdCounter(1),
        }
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id.0;
        self.next_id.0 += 1;
        id
    }

    pub fn new_event_id(&mut self) -> EventId {
        EventId(self.alloc_id())
    }

    /// Builds a part that fits the current measures; add it with `Command::AddPart`.
    pub fn new_part(&mut self, name: &str, clefs: &[Clef]) -> Part {
        let measure_count = self.measures.len();
        let id = PartId(self.alloc_id());
        let staves = clefs
            .iter()
            .map(|&clef| Staff {
                id: StaffId(self.alloc_id()),
                clef,
                measures: vec![StaffMeasure::default(); measure_count],
            })
            .collect();
        Part {
            id,
            name: name.to_owned(),
            short_name: String::new(),
            staves,
        }
    }

    /// Empty content for a new measure at `index`, inheriting signatures from its predecessor.
    pub fn blank_measure(&self, index: usize) -> MeasureData {
        let inherit = index.saturating_sub(1).min(self.measures.len() - 1);
        MeasureData {
            info: MeasureInfo {
                length: None,
                ..self.measures[inherit]
            },
            content: self
                .staves()
                .map(|staff| {
                    let voice_source = staff
                        .measures
                        .get(index)
                        .unwrap_or(&staff.measures[inherit]);
                    let voice_count = voice_source.voices.len();
                    (
                        staff.id,
                        StaffMeasure {
                            voices: vec![Voice::default(); voice_count],
                        },
                    )
                })
                .collect(),
        }
    }

    pub fn staves(&self) -> impl Iterator<Item = &Staff> {
        self.parts.iter().flat_map(|part| part.staves.iter())
    }

    pub(crate) fn staves_mut(&mut self) -> impl Iterator<Item = &mut Staff> {
        self.parts
            .iter_mut()
            .flat_map(|part| part.staves.iter_mut())
    }

    pub fn staff(&self, id: StaffId) -> Result<&Staff, ModelError> {
        self.staves()
            .find(|staff| staff.id == id)
            .ok_or(ModelError::StaffNotFound(id))
    }

    pub(crate) fn staff_mut(&mut self, id: StaffId) -> Result<&mut Staff, ModelError> {
        self.staves_mut()
            .find(|staff| staff.id == id)
            .ok_or(ModelError::StaffNotFound(id))
    }

    pub fn measure_capacity(&self, measure: usize) -> Result<u32, ModelError> {
        self.measures
            .get(measure)
            .map(|info| info.capacity())
            .ok_or(ModelError::MeasureOutOfRange(measure))
    }

    pub fn voice(&self, at: VoiceLocation) -> Result<&Voice, ModelError> {
        self.staff(at.staff)?
            .measures
            .get(at.measure)
            .ok_or(ModelError::MeasureOutOfRange(at.measure))?
            .voices
            .get(at.voice)
            .ok_or(ModelError::VoiceOutOfRange(at.voice))
    }

    pub(crate) fn voice_mut(&mut self, at: VoiceLocation) -> Result<&mut Voice, ModelError> {
        self.staff_mut(at.staff)?
            .measures
            .get_mut(at.measure)
            .ok_or(ModelError::MeasureOutOfRange(at.measure))?
            .voices
            .get_mut(at.voice)
            .ok_or(ModelError::VoiceOutOfRange(at.voice))
    }

    /// Checks the invariants that commands maintain, for scores that did not come from them.
    pub fn validate(&self) -> Result<(), ModelError> {
        self.tempo.validate()?;
        let mut direction_positions = std::collections::HashSet::new();
        for mark in &self.tempo_directions {
            let capacity = self.measure_capacity(mark.measure)?;
            if mark.tick > capacity {
                return Err(ModelError::Invalid(
                    "tempo direction position is outside its measure",
                ));
            }
            if !direction_positions.insert((mark.measure, mark.tick)) {
                return Err(ModelError::Invalid(
                    "only one tempo direction is allowed at each position",
                ));
            }
        }
        if self.measures.is_empty() {
            return Err(ModelError::Invalid("a score needs at least one measure"));
        }
        for info in &self.measures {
            info.validate()?;
        }

        let mut seen = std::collections::HashSet::new();
        let mut claim = |id: u64| {
            if id >= self.next_id.0 {
                Err(ModelError::Invalid("id is beyond the allocation counter"))
            } else if !seen.insert(id) {
                Err(ModelError::DuplicateId(id))
            } else {
                Ok(())
            }
        };
        for part in &self.parts {
            claim(part.id.0)?;
            for staff in &part.staves {
                claim(staff.id.0)?;
                if staff.measures.len() != self.measures.len() {
                    return Err(ModelError::Invalid("staff must have one entry per measure"));
                }
                for (content, info) in staff.measures.iter().zip(&self.measures) {
                    if content.voices.is_empty() {
                        return Err(ModelError::Invalid("a measure needs at least one voice"));
                    }
                    let capacity = info.capacity();
                    for voice in &content.voices {
                        if voice.used_ticks() > capacity {
                            return Err(ModelError::MeasureOverfull);
                        }
                        if voice
                            .end_tick
                            .is_some_and(|end| end > capacity || end < voice.used_ticks())
                        {
                            return Err(ModelError::Invalid(
                                "voice end must follow its events within the measure",
                            ));
                        }
                        for event in &voice.events {
                            event.validate()?;
                            claim(event.id.0)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
