use serde::{Deserialize, Serialize};

use crate::error::ModelError;

/// Tick resolution; divisible so that a 64th note with up to two dots is an integer.
pub const TICKS_PER_QUARTER: u32 = 960;

pub const MAX_DOTS: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NoteValue {
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    SixtyFourth,
}

impl NoteValue {
    pub fn ticks(self) -> u32 {
        (4 * TICKS_PER_QUARTER) >> (self as u32)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Duration {
    pub value: NoteValue,
    pub dots: u8,
}

impl Duration {
    pub fn new(value: NoteValue, dots: u8) -> Self {
        Self { value, dots }
    }

    pub fn ticks(self) -> u32 {
        let dots = u32::from(self.dots.min(MAX_DOTS));
        (self.value.ticks() * ((2 << dots) - 1)) >> dots
    }

    pub fn validate(self) -> Result<(), ModelError> {
        if self.dots > MAX_DOTS {
            return Err(ModelError::Invalid("at most two dots are supported"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeSignatureDisplay {
    #[default]
    Numeric,
    Common,
    AllaBreve,
}

impl TimeSignature {
    pub const COMMON: TimeSignature = TimeSignature {
        numerator: 4,
        denominator: 4,
    };

    pub fn ticks_per_measure(self) -> u32 {
        u32::from(self.numerator) * (4 * TICKS_PER_QUARTER / u32::from(self.denominator))
    }

    pub fn validate(self) -> Result<(), ModelError> {
        if !(1..=32).contains(&self.numerator) {
            return Err(ModelError::Invalid(
                "time signature numerator must be 1..=32",
            ));
        }
        if !matches!(self.denominator, 1 | 2 | 4 | 8 | 16 | 32) {
            return Err(ModelError::Invalid(
                "time signature denominator must be a power of two up to 32",
            ));
        }
        Ok(())
    }
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self::COMMON
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_value_ticks() {
        assert_eq!(NoteValue::Whole.ticks(), 3840);
        assert_eq!(NoteValue::Quarter.ticks(), TICKS_PER_QUARTER);
        assert_eq!(NoteValue::SixtyFourth.ticks(), 60);
    }

    #[test]
    fn dotted_durations() {
        assert_eq!(Duration::new(NoteValue::Quarter, 1).ticks(), 1440);
        assert_eq!(Duration::new(NoteValue::Quarter, 2).ticks(), 1680);
        assert_eq!(Duration::new(NoteValue::SixtyFourth, 2).ticks(), 105);
        assert!(Duration::new(NoteValue::Quarter, 3).validate().is_err());
    }

    #[test]
    fn measure_lengths() {
        assert_eq!(TimeSignature::COMMON.ticks_per_measure(), 3840);
        let six_eight = TimeSignature {
            numerator: 6,
            denominator: 8,
        };
        assert_eq!(six_eight.ticks_per_measure(), 2880);
        assert!(
            TimeSignature {
                numerator: 4,
                denominator: 3
            }
            .validate()
            .is_err()
        );
    }
}
