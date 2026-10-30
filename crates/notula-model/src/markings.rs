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

/// A gradual volume change, drawn below the staff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Hairpin {
    Crescendo,
    Diminuendo,
}

/// How older files stored hairpins: a start, or the event where one ended.
#[derive(Deserialize)]
enum StoredHairpin {
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

/// An accordion register symbol: which reeds sound from this event on. The dots of the
/// standard symbols are kept as a set, so presets and custom combinations are alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "side", rename_all = "snake_case")]
pub enum Register {
    /// Treble side with three reed ranks: high (4′) at the top, low (16′) at the bottom,
    /// and in the middle row the 8′ reed with a tremolo reed tuned above (right) or below
    /// (left) it.
    Treble {
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        high: bool,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        lower_tremolo: bool,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        middle: bool,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        upper_tremolo: bool,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        low: bool,
    },
    /// Bass side with two reed ranks: upper and lower half, and the master dot on the line.
    Bass {
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        upper: bool,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        lower: bool,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        master: bool,
    },
}

/// Preferred side for a slur. `Auto` follows the stem direction; `Double` draws one slur
/// above the chords' upper notes and one below their lower notes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlurPlacement {
    #[default]
    Auto,
    Above,
    Below,
    Double,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "StoredMarkings")]
pub struct Markings {
    /// Takes effect at this event and stays until the next dynamic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynamic: Option<Dynamic>,
    /// A hairpin starts at this event and runs through the event with the matching
    /// `hairpin_stop` in the same voice. One event can carry both, for a hairpin over a
    /// single note or one that ends where the next starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hairpin_start: Option<Hairpin>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hairpin_stop: bool,
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
    /// Takes effect at this event and stays until the next register on the staff.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub register: Option<Register>,
}

/// Markings as stored in files, including the older single `hairpin` field.
#[derive(Deserialize)]
struct StoredMarkings {
    #[serde(default)]
    dynamic: Option<Dynamic>,
    #[serde(default)]
    hairpin: Option<StoredHairpin>,
    #[serde(default)]
    hairpin_start: Option<Hairpin>,
    #[serde(default)]
    hairpin_stop: bool,
    #[serde(default)]
    articulations: Vec<Articulation>,
    #[serde(default)]
    slur_start: bool,
    #[serde(default)]
    slur_stop: bool,
    #[serde(default)]
    slur_placement: SlurPlacement,
    #[serde(default)]
    phrase_slur_start: bool,
    #[serde(default)]
    phrase_slur_stop: bool,
    #[serde(default)]
    phrase_slur_placement: SlurPlacement,
    #[serde(default)]
    register: Option<Register>,
}

impl From<StoredMarkings> for Markings {
    fn from(stored: StoredMarkings) -> Self {
        let (legacy_start, legacy_stop) = match stored.hairpin {
            Some(StoredHairpin::Crescendo) => (Some(Hairpin::Crescendo), false),
            Some(StoredHairpin::Diminuendo) => (Some(Hairpin::Diminuendo), false),
            Some(StoredHairpin::End) => (None, true),
            None => (None, false),
        };
        Self {
            dynamic: stored.dynamic,
            hairpin_start: stored.hairpin_start.or(legacy_start),
            hairpin_stop: stored.hairpin_stop || legacy_stop,
            articulations: stored.articulations,
            slur_start: stored.slur_start,
            slur_stop: stored.slur_stop,
            slur_placement: stored.slur_placement,
            phrase_slur_start: stored.phrase_slur_start,
            phrase_slur_stop: stored.phrase_slur_stop,
            phrase_slur_placement: stored.phrase_slur_placement,
            register: stored.register,
        }
    }
}

impl SlurPlacement {
    fn is_auto(&self) -> bool {
        *self == Self::Auto
    }
}

impl Markings {
    pub fn is_empty(&self) -> bool {
        self.dynamic.is_none()
            && self.hairpin_start.is_none()
            && !self.hairpin_stop
            && self.articulations.is_empty()
            && !self.slur_start
            && !self.slur_stop
            && self.slur_placement == SlurPlacement::Auto
            && !self.phrase_slur_start
            && !self.phrase_slur_stop
            && self.phrase_slur_placement == SlurPlacement::Auto
            && self.register.is_none()
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
            hairpin_start: None,
            hairpin_stop: false,
            articulations: vec![Articulation::Staccato, Articulation::Fermata],
            slur_start: false,
            slur_stop: false,
            slur_placement: SlurPlacement::Auto,
            phrase_slur_start: false,
            phrase_slur_stop: false,
            phrase_slur_placement: SlurPlacement::Auto,
            register: Some(Register::Bass {
                upper: true,
                lower: false,
                master: false,
            }),
        };
        let rest = markings.for_rest();
        assert_eq!(rest.dynamic, Some(Dynamic::F));
        assert!(rest.register.is_some(), "a rest keeps the register");
        assert_eq!(rest.articulations, vec![Articulation::Fermata]);
        assert!(rest.validate(true).is_ok());
    }

    #[test]
    fn older_hairpin_fields_load_as_start_and_stop() {
        let start: Markings = serde_json::from_str(r#"{"hairpin":"Diminuendo"}"#).unwrap();
        assert_eq!(start.hairpin_start, Some(Hairpin::Diminuendo));
        assert!(!start.hairpin_stop);
        let end: Markings = serde_json::from_str(r#"{"hairpin":"End"}"#).unwrap();
        assert_eq!(end.hairpin_start, None);
        assert!(end.hairpin_stop);

        let both = Markings {
            hairpin_start: Some(Hairpin::Crescendo),
            hairpin_stop: true,
            ..Markings::default()
        };
        let json = serde_json::to_string(&both).unwrap();
        assert_eq!(serde_json::from_str::<Markings>(&json).unwrap(), both);
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

    #[test]
    fn registers_store_only_their_dots() {
        let clarinet = Markings {
            register: Some(Register::Treble {
                high: false,
                lower_tremolo: false,
                middle: true,
                upper_tremolo: false,
                low: false,
            }),
            ..Markings::default()
        };
        let json = serde_json::to_string(&clarinet).unwrap();
        assert_eq!(json, r#"{"register":{"side":"treble","middle":true}}"#);
        assert_eq!(serde_json::from_str::<Markings>(&json).unwrap(), clarinet);
        assert!(!clarinet.is_empty());
    }
}
