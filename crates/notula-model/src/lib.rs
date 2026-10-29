//! Score data model and edit commands.

mod command;
mod duration;
pub mod edit;
mod error;
mod history;
mod ids;
mod markings;
mod pitch;
mod score;
pub mod selection;
pub mod shift;

pub use command::Command;
pub use duration::{
    Duration, MAX_DOTS, NoteValue, TICKS_PER_QUARTER, TimeSignature, TimeSignatureDisplay,
};
pub use error::ModelError;
pub use history::Document;
pub use ids::{EventId, PartId, StaffId};
pub use markings::{Articulation, Dynamic, Hairpin, Markings, SlurPlacement};
pub use pitch::{Pitch, Step};
pub use score::{
    Clef, Event, EventKind, KeySignature, MeasureData, MeasureInfo, Part, Score, Staff,
    StaffMeasure, Tempo, TempoChangeMark, TempoDirection, TempoDirectionMark, Voice, VoiceLocation,
};
