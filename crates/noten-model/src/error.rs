use crate::ids::{EventId, PartId, StaffId};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModelError {
    #[error("part {0:?} not found")]
    PartNotFound(PartId),
    #[error("staff {0:?} not found")]
    StaffNotFound(StaffId),
    #[error("event {0:?} not found")]
    EventNotFound(EventId),
    #[error("id {0} is already in use")]
    DuplicateId(u64),
    #[error("measure index {0} is out of range")]
    MeasureOutOfRange(usize),
    #[error("voice index {0} is out of range")]
    VoiceOutOfRange(usize),
    #[error("event index {0} is out of range")]
    EventOutOfRange(usize),
    #[error("the change would exceed the length of the measure")]
    MeasureOverfull,
    #[error("invalid value: {0}")]
    Invalid(&'static str),
}
