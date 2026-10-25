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

/// Preferred side for a slur. `Auto` follows the stem direction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlurPlacement {
    #[default]
    Auto,
    Above,
    Below,
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
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub slur_start: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub slur_stop: bool,
    #[serde(default, skip_serializing_if = "SlurPlacement::is_auto")]
    pub slur_placement: SlurPlacement,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub phrase_slur_start: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub phrase_slur_stop: bool,
    #[serde(default, skip_serializing_if = "SlurPlacement::is_auto")]
    pub phrase_slur_placement: SlurPlacement,
}

impl SlurPlacement {
    fn is_auto(&self) -> bool {
        *self == Self::Auto
    }
}

impl Markings {
    pub fn is_empty(&self) -> bool {
        self.dynamic.is_none()
            && self.hairpin.is_none()
            && self.articulations.is_empty()
            && !self.slur_start
            && !self.slur_stop
            && self.slur_placement == SlurPlacement::Auto
            && !self.phrase_slur_start
            && !self.phrase_slur_stop
            && self.phrase_slur_placement == SlurPlacement::Auto
    }

    pub fn validate(&self, on_rest: bool) -> Result<(), ModelError> {
        if on_rest
            && (self.slur_start
                || self.slur_stop
                || self.phrase_slur_start
                || self.phrase_slur_stop)
        {
            return Err(ModelError::Invalid("slurs can only be placed on notes"));
        }
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
        self.slur_start = false;
        self.slur_stop = false;
        self.slur_placement = SlurPlacement::Auto;
        self.phrase_slur_start = false;
        self.phrase_slur_stop = false;
        self.phrase_slur_placement = SlurPlacement::Auto;
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
            slur_start: false,
            slur_stop: false,
            slur_placement: SlurPlacement::Auto,
            phrase_slur_start: false,
            phrase_slur_stop: false,
            phrase_slur_placement: SlurPlacement::Auto,
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
