use serde::{Deserialize, Serialize};

use crate::duration::{
    Duration, NoteValue, TICKS_PER_QUARTER, TimeSignature, TimeSignatureDisplay,
};
use crate::error::ModelError;
use crate::ids::{EventId, PartId, StaffId};
use crate::markings::Markings;
use crate::pitch::{Pitch, Step};

/// Metronome setting, expressed in the displayed beat unit per minute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tempo {
    pub beat_unit: NoteValue,
    /// Dots on the beat unit, as in a dotted quarter beat for 6/8.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub beat_unit_dots: u8,
    pub units_per_minute: u16,
}

fn is_zero(value: &u8) -> bool {
    *value == 0
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

/// Tempo text such as "Allegro" or "Andante con moto", printed above the score like a
/// metronome mark and together with one at the same position. Playback ignores it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TempoTextMark {
    pub measure: usize,
    pub tick: u32,
    pub text: String,
}

/// A metronome mark that sets the tempo from its position onward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TempoChangeMark {
    pub measure: usize,
    pub tick: u32,
    pub tempo: Tempo,
    /// Affects playback without being printed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}

impl Default for Tempo {
    fn default() -> Self {
        Self {
            beat_unit: NoteValue::Quarter,
            beat_unit_dots: 0,
            units_per_minute: 120,
        }
    }
}

/// Tempo text must show something and stay on one line.
pub fn validate_tempo_text(text: &str) -> Result<(), ModelError> {
    if text.trim().is_empty() {
        return Err(ModelError::Invalid("tempo text cannot be empty"));
    }
    if text.contains(['\n', '\r']) {
        return Err(ModelError::Invalid("tempo text must be a single line"));
    }
    Ok(())
}

impl Tempo {
    pub fn validate(self) -> Result<(), ModelError> {
        self.beat().validate()?;
        if !(1..=1000).contains(&self.units_per_minute) {
            return Err(ModelError::Invalid(
                "tempo must be 1..=1000 units per minute",
            ));
        }
        Ok(())
    }

    pub fn beat(self) -> Duration {
        Duration::new(self.beat_unit, self.beat_unit_dots)
    }

    /// Equivalent quarter notes per minute, used to convert score ticks to seconds.
    pub fn quarter_notes_per_minute(self) -> f64 {
        f64::from(self.units_per_minute) * f64::from(self.beat().ticks())
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
    /// The name and short name are kept but not printed beside the staves.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hide_name: bool,
    pub staves: Vec<Staff>,
}

/// The line at a measure's end, unless a repeat ends there. The last measure of the score
/// always ends with a final barline.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Barline {
    #[default]
    Regular,
    /// Two thin lines, as at the end of a section.
    Double,
    /// A thin and a thick line, as at the end of a piece.
    Final,
}

impl Barline {
    fn is_regular(&self) -> bool {
        *self == Self::Regular
    }
}

/// The passes through a repeat that play an ending, numbered from 1 to 16.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(into = "Vec<u8>", try_from = "Vec<u8>")]
pub struct EndingNumbers(u16);

impl EndingNumbers {
    pub const MAX: u8 = 16;

    /// The numbers, or `None` when the list is empty or holds a number outside 1..=16.
    pub fn new(numbers: &[u8]) -> Option<Self> {
        let mut mask = 0_u16;
        for &number in numbers {
            if !(1..=Self::MAX).contains(&number) {
                return None;
            }
            mask |= 1 << (number - 1);
        }
        (mask != 0).then_some(Self(mask))
    }

    pub fn contains(self, pass: u32) -> bool {
        (1..=u32::from(Self::MAX)).contains(&pass) && self.0 & (1 << (pass - 1)) != 0
    }

    /// Ascending.
    pub fn numbers(self) -> Vec<u8> {
        (1..=Self::MAX)
            .filter(|number| self.0 & (1 << (number - 1)) != 0)
            .collect()
    }

    /// As printed at the start of the ending, such as "1." or "1., 2.".
    pub fn label(self) -> String {
        self.numbers()
            .iter()
            .map(|number| format!("{number}."))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl From<EndingNumbers> for Vec<u8> {
    fn from(numbers: EndingNumbers) -> Self {
        numbers.numbers()
    }
}

impl TryFrom<Vec<u8>> for EndingNumbers {
    type Error = &'static str;

    fn try_from(numbers: Vec<u8>) -> Result<Self, Self::Error> {
        Self::new(&numbers).ok_or("ending numbers must be 1 to 16")
    }
}

/// A volta: the measure is played only on the passes through the repeat it names. Neighbouring
/// measures with the same numbers form one ending, drawn as one bracket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Ending {
    pub numbers: EndingNumbers,
    /// The bracket has no hook at its end, as usual for the last ending. It counts where the
    /// ending's last measure has it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub open: bool,
}

/// Repeat signs, the barline, and the ending of one measure, shared by all staves.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Bars {
    /// A repeat begins at the measure's start.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub repeat_start: bool,
    /// A repeat ends at the measure's end; the music from the repeat start is played this
    /// many times in all (2 to 10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat_end: Option<u8>,
    #[serde(default, skip_serializing_if = "Barline::is_regular")]
    pub barline: Barline,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ending: Option<Ending>,
}

impl Bars {
    pub const MAX_REPEAT_TIMES: u8 = 10;

    pub fn is_plain(&self) -> bool {
        *self == Self::default()
    }

    pub fn validate(self) -> Result<(), ModelError> {
        if self
            .repeat_end
            .is_some_and(|times| !(2..=Self::MAX_REPEAT_TIMES).contains(&times))
        {
            return Err(ModelError::Invalid(
                "a repeat is played 2 to 10 times in all",
            ));
        }
        Ok(())
    }
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
    /// Repeats, barline, and ending.
    #[serde(default, skip_serializing_if = "Bars::is_plain")]
    pub bars: Bars,
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
        self.bars.validate()?;
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
#[serde(from = "StoredScore")]
pub struct Score {
    pub title: String,
    pub composer: String,
    pub tempo_directions: Vec<TempoDirectionMark>,
    /// Sorted by position; a mark at the very start sets the initial tempo.
    pub tempo_changes: Vec<TempoChangeMark>,
    /// Sorted by position.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tempo_texts: Vec<TempoTextMark>,
    pub parts: Vec<Part>,
    /// Never empty.
    pub measures: Vec<MeasureInfo>,
    next_id: IdCounter,
}

/// A score as stored in files, including fields that older versions wrote.
#[derive(Deserialize)]
struct StoredScore {
    title: String,
    composer: String,
    /// The default tempo before it became a mark at the start of the score.
    #[serde(default)]
    tempo: Option<Tempo>,
    #[serde(default)]
    tempo_directions: Vec<TempoDirectionMark>,
    #[serde(default)]
    tempo_changes: Vec<TempoChangeMark>,
    #[serde(default)]
    tempo_texts: Vec<TempoTextMark>,
    parts: Vec<Part>,
    measures: Vec<MeasureInfo>,
    next_id: IdCounter,
}

impl From<StoredScore> for Score {
    fn from(stored: StoredScore) -> Self {
        let mut tempo_changes = stored.tempo_changes;
        if let Some(tempo) = stored.tempo
            && !tempo_changes
                .iter()
                .any(|mark| (mark.measure, mark.tick) == (0, 0))
        {
            tempo_changes.insert(
                0,
                TempoChangeMark {
                    measure: 0,
                    tick: 0,
                    tempo,
                    hidden: false,
                },
            );
        }
        Self {
            title: stored.title,
            composer: stored.composer,
            tempo_directions: stored.tempo_directions,
            tempo_changes,
            tempo_texts: stored.tempo_texts,
            parts: stored.parts,
            measures: stored.measures,
            next_id: stored.next_id,
        }
    }
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
            tempo_directions: Vec::new(),
            tempo_changes: Vec::new(),
            tempo_texts: Vec::new(),
            parts: Vec::new(),
            measures: vec![MeasureInfo::default()],
            next_id: IdCounter(1),
        }
    }

    /// The tempo at the start: the mark there, or the default when the score has none.
    pub fn initial_tempo(&self) -> Tempo {
        self.tempo_change_at(0, 0)
            .map_or_else(Tempo::default, |mark| mark.tempo)
    }

    pub fn tempo_change_at(&self, measure: usize, tick: u32) -> Option<&TempoChangeMark> {
        self.tempo_changes
            .iter()
            .find(|mark| mark.measure == measure && mark.tick == tick)
    }

    pub fn tempo_text_at(&self, measure: usize, tick: u32) -> Option<&TempoTextMark> {
        self.tempo_texts
            .iter()
            .find(|mark| mark.measure == measure && mark.tick == tick)
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
            hide_name: false,
            staves,
        }
    }

    /// Empty content for a new measure at `index`, inheriting signatures from its predecessor.
    pub fn blank_measure(&self, index: usize) -> MeasureData {
        let inherit = index.saturating_sub(1).min(self.measures.len() - 1);
        MeasureData {
            info: MeasureInfo {
                length: None,
                bars: Bars::default(),
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
        let mut change_positions = std::collections::HashSet::new();
        for mark in &self.tempo_changes {
            mark.tempo.validate()?;
            let capacity = self.measure_capacity(mark.measure)?;
            if mark.tick > capacity {
                return Err(ModelError::Invalid(
                    "tempo change position is outside its measure",
                ));
            }
            if !change_positions.insert((mark.measure, mark.tick)) {
                return Err(ModelError::Invalid(
                    "only one tempo change is allowed at each position",
                ));
            }
        }
        let mut text_positions = std::collections::HashSet::new();
        for mark in &self.tempo_texts {
            validate_tempo_text(&mark.text)?;
            let capacity = self.measure_capacity(mark.measure)?;
            if mark.tick > capacity {
                return Err(ModelError::Invalid(
                    "tempo text position is outside its measure",
                ));
            }
            if !text_positions.insert((mark.measure, mark.tick)) {
                return Err(ModelError::Invalid(
                    "only one tempo text is allowed at each position",
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
