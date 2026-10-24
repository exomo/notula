use serde::{Deserialize, Serialize};

use crate::error::ModelError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Step {
    C,
    D,
    E,
    F,
    G,
    A,
    B,
}

impl Step {
    /// Position within the octave on the staff, C = 0 .. B = 6.
    pub fn index(self) -> u8 {
        self as u8
    }

    pub fn from_index(index: u8) -> Option<Step> {
        Some(match index {
            0 => Step::C,
            1 => Step::D,
            2 => Step::E,
            3 => Step::F,
            4 => Step::G,
            5 => Step::A,
            6 => Step::B,
            _ => return None,
        })
    }

    pub fn semitone(self) -> i8 {
        match self {
            Step::C => 0,
            Step::D => 2,
            Step::E => 4,
            Step::F => 5,
            Step::G => 7,
            Step::A => 9,
            Step::B => 11,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pitch {
    pub step: Step,
    /// Semitone offset from the natural note: -1 flat, 1 sharp, up to double accidentals.
    pub alter: i8,
    /// Scientific pitch notation, middle C is C4.
    pub octave: i8,
}

impl Pitch {
    pub fn new(step: Step, alter: i8, octave: i8) -> Self {
        Self {
            step,
            alter,
            octave,
        }
    }

    pub fn midi(self) -> i16 {
        12 * (i16::from(self.octave) + 1) + i16::from(self.step.semitone()) + i16::from(self.alter)
    }

    /// Staff position counted in steps, so enharmonic pitches differ.
    pub fn diatonic_index(self) -> i16 {
        i16::from(self.octave) * 7 + i16::from(self.step.index())
    }

    /// Spells a MIDI note with sharps or flats, `None` outside the MIDI range.
    pub fn from_midi(midi: i16, sharps: bool) -> Option<Pitch> {
        use Step::{A, B, C, D, E, F, G};
        const SHARPS: [(Step, i8); 12] = [
            (C, 0),
            (C, 1),
            (D, 0),
            (D, 1),
            (E, 0),
            (F, 0),
            (F, 1),
            (G, 0),
            (G, 1),
            (A, 0),
            (A, 1),
            (B, 0),
        ];
        const FLATS: [(Step, i8); 12] = [
            (C, 0),
            (D, -1),
            (D, 0),
            (E, -1),
            (E, 0),
            (F, 0),
            (G, -1),
            (G, 0),
            (A, -1),
            (A, 0),
            (B, -1),
            (B, 0),
        ];
        if !(0..=127).contains(&midi) {
            return None;
        }
        let table = if sharps { &SHARPS } else { &FLATS };
        let (step, alter) = table[midi.rem_euclid(12) as usize];
        Some(Pitch::new(step, alter, (midi.div_euclid(12) - 1) as i8))
    }

    pub fn validate(self) -> Result<(), ModelError> {
        if !(-2..=2).contains(&self.alter) {
            return Err(ModelError::Invalid(
                "accidental must be within double flat..double sharp",
            ));
        }
        if !(0..=127).contains(&self.midi()) {
            return Err(ModelError::Invalid("pitch is outside the MIDI range"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_numbers() {
        assert_eq!(Pitch::new(Step::C, 0, 4).midi(), 60);
        assert_eq!(Pitch::new(Step::A, 0, 4).midi(), 69);
        assert_eq!(Pitch::new(Step::B, -1, 3).midi(), 58);
    }

    #[test]
    fn enharmonics_differ_on_staff() {
        let c_sharp = Pitch::new(Step::C, 1, 4);
        let d_flat = Pitch::new(Step::D, -1, 4);
        assert_eq!(c_sharp.midi(), d_flat.midi());
        assert_ne!(c_sharp.diatonic_index(), d_flat.diatonic_index());
    }

    #[test]
    fn rejects_out_of_range() {
        assert!(Pitch::new(Step::C, 3, 4).validate().is_err());
        assert!(Pitch::new(Step::C, 0, 10).validate().is_err());
        assert!(Pitch::new(Step::C, 0, 4).validate().is_ok());
    }
}
