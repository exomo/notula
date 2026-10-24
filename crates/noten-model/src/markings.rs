//! Performance annotations that can be attached to a note, chord or rest.

use serde::{Deserialize, Serialize};

use crate::error::ModelError;

/// A volume mark written below the staff, from very soft to very loud.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Dynamic {
    Ppp,
    Pp,
    P,
    Mp,
    Mf,
    F,
    Ff,
    Fff,
    Fp,
    Sf,
    Sfz,
    Rfz,
}

/// A gradual volume change. `Crescendo` and `Diminuendo` start one, and it lasts until the
/// next `End` on the same staff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Hairpin {
    Crescendo,
    Diminuendo,
    End,
}

/// How a single note is attacked or held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Articulation {
    Staccato,
    Staccatissimo,
    Tenuto,
    Accent,
    Marcato,
    Fermata,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Markings {
    /// Takes effect at this event and stays until the next dynamic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynamic: Option<Dynamic>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hairpin: Option<Hairpin>,
    /// Each articulation at most once.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub articulations: Vec<Articulation>,
}

impl Markings {
    pub fn is_empty(&self) -> bool {
        self.dynamic.is_none() && self.hairpin.is_none() && self.articulations.is_empty()
    }

    pub fn validate(&self, on_rest: bool) -> Result<(), ModelError> {
        for (index, articulation) in self.articulations.iter().enumerate() {
            if self.articulations[..index].contains(articulation) {
                return Err(ModelError::Invalid("an articulation can only be used once"));
            }
            if on_rest && *articulation != Articulation::Fermata {
                return Err(ModelError::Invalid(
                    "only a fermata can be placed on a rest",
                ));
            }
        }
        Ok(())
    }

    /// What remains when a note is replaced by a rest.
    pub fn for_rest(mut self) -> Self {
        self.articulations.retain(|a| *a == Articulation::Fermata);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rests_keep_only_a_fermata() {
        let markings = Markings {
            dynamic: Some(Dynamic::F),
            hairpin: None,
            articulations: vec![Articulation::Staccato, Articulation::Fermata],
        };
        let rest = markings.for_rest();
        assert_eq!(rest.dynamic, Some(Dynamic::F));
        assert_eq!(rest.articulations, vec![Articulation::Fermata]);
        assert!(rest.validate(true).is_ok());
    }

    #[test]
    fn validation_rejects_duplicates_and_rest_articulations() {
        let twice = Markings {
            articulations: vec![Articulation::Accent, Articulation::Accent],
            ..Markings::default()
        };
        assert!(twice.validate(false).is_err());

        let accent = Markings {
            articulations: vec![Articulation::Accent],
            ..Markings::default()
        };
        assert!(accent.validate(false).is_ok());
        assert!(accent.validate(true).is_err());
    }
}
