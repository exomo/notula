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

/// How a sustain pedal mark is printed. All styles mean the same to playback.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PedalStyle {
    /// "Ped." where the pedal goes down and "✱" where it comes up.
    #[default]
    Text,
    /// A bracket line with hooks at both ends and a notch where the pedal is changed.
    Line,
    /// "Ped." followed by a line that ends in a hook.
    TextLine,
}

/// Which side of the staff a text mark goes on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextPlacement {
    /// Above the staff, as for playing directions such as "div." or "pizz.".
    #[default]
    Above,
    /// On the dynamics line below the staff, as for expressions such as "dolce".
    Below,
}

impl TextPlacement {
    fn is_above(&self) -> bool {
        *self == Self::Above
    }
}

/// Free text at an event, such as "div.", "pizz.", or "dolce". Playback ignores it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StaffText {
    pub text: String,
    #[serde(default, skip_serializing_if = "TextPlacement::is_above")]
    pub placement: TextPlacement,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
}

impl StaffText {
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.text.trim().is_empty() {
            return Err(ModelError::Invalid("text cannot be empty"));
        }
        if self.text.contains(['\n', '\r']) {
            return Err(ModelError::Invalid("text must be a single line"));
        }
        Ok(())
    }
}

/// How a bracket's line is drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BracketLine {
    #[default]
    Solid,
    Dashed,
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

/// A horizontal bracket over or under a group of notes, as early-music editions use to
/// mark the ligatures of their source. Its hooks point towards the notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Bracket {
    #[serde(default, skip_serializing_if = "TextPlacement::is_above")]
    pub placement: TextPlacement,
    #[serde(default, skip_serializing_if = "BracketLine::is_solid")]
    pub line: BracketLine,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub start_hook: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub end_hook: bool,
}

impl Default for Bracket {
    fn default() -> Self {
        Self {
            placement: TextPlacement::Above,
            line: BracketLine::Solid,
            start_hook: true,
            end_hook: true,
        }
    }
}

impl BracketLine {
    fn is_solid(&self) -> bool {
        *self == Self::Solid
    }
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
    /// The sustain pedal goes down at the start of this event and comes up at the end of the
    /// event with the matching `pedal_stop` in the same voice. A start while the pedal is
    /// down changes it: the held pedal ends at the previous event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pedal_start: Option<PedalStyle>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pedal_stop: bool,
    /// The pedal ending here comes up without a printed release, as a "Ped." whose end is
    /// left to the player. Only set together with `pedal_stop`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pedal_release_hidden: bool,
    /// A bracket starts at this event and runs through the event with the matching
    /// `bracket_stop` in the same voice; one event can carry both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bracket_start: Option<Bracket>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bracket_stop: bool,
    /// Free text printed at this event: at most one above and one below the staff.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<StaffText>,
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
    #[serde(default)]
    pedal_start: Option<PedalStyle>,
    #[serde(default)]
    pedal_stop: bool,
    #[serde(default)]
    pedal_release_hidden: bool,
    #[serde(default)]
    bracket_start: Option<Bracket>,
    #[serde(default)]
    bracket_stop: bool,
    #[serde(default)]
    texts: Vec<StaffText>,
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
            pedal_start: stored.pedal_start,
            pedal_stop: stored.pedal_stop,
            pedal_release_hidden: stored.pedal_release_hidden,
            bracket_start: stored.bracket_start,
            bracket_stop: stored.bracket_stop,
            texts: stored.texts,
        }
    }
}

impl SlurPlacement {
    fn is_auto(&self) -> bool {
        *self == Self::Auto
    }
}

impl Markings {
    /// The text on one side of the staff, if any.
    pub fn text(&self, placement: TextPlacement) -> Option<&StaffText> {
        self.texts.iter().find(|text| text.placement == placement)
    }

    /// Sets or, with `None`, removes the text on one side of the staff, keeping the text above
    /// before the text below.
    pub fn set_text(&mut self, placement: TextPlacement, text: Option<StaffText>) {
        self.texts.retain(|other| other.placement != placement);
        if let Some(text) = text {
            self.texts.push(StaffText { placement, ..text });
            self.texts
                .sort_by_key(|text| text.placement == TextPlacement::Below);
        }
    }

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
            && self.pedal_start.is_none()
            && !self.pedal_stop
            && !self.pedal_release_hidden
            && self.bracket_start.is_none()
            && !self.bracket_stop
            && self.texts.is_empty()
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
        for (index, text) in self.texts.iter().enumerate() {
            text.validate()?;
            if self.texts[..index]
                .iter()
                .any(|other| other.placement == text.placement)
            {
                return Err(ModelError::Invalid(
                    "an event has at most one text above and one below the staff",
                ));
            }
        }
        if self.pedal_release_hidden && !self.pedal_stop {
            return Err(ModelError::Invalid(
                "only a pedal's end can hide its release",
            ));
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
            pedal_start: Some(PedalStyle::Line),
            pedal_stop: true,
            pedal_release_hidden: true,
            bracket_start: Some(Bracket::default()),
            bracket_stop: true,
            texts: vec![StaffText {
                text: "dolce".to_owned(),
                placement: TextPlacement::Below,
                italic: true,
            }],
        };
        let rest = markings.for_rest();
        assert_eq!(rest.texts.len(), 1, "a rest keeps its text");
        assert!(
            rest.bracket_start.is_some() && rest.bracket_stop,
            "and its bracket"
        );
        assert_eq!(rest.dynamic, Some(Dynamic::F));
        assert!(rest.register.is_some(), "a rest keeps the register");
        assert_eq!(
            rest.pedal_start,
            Some(PedalStyle::Line),
            "a rest keeps the pedal"
        );
        assert!(rest.pedal_stop && rest.pedal_release_hidden);
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

    #[test]
    fn staff_text_stores_only_what_differs_from_plain_text_above() {
        let mut markings = Markings::default();
        let text = |text: &str, italic| StaffText {
            text: text.to_owned(),
            placement: TextPlacement::Above,
            italic,
        };
        markings.set_text(TextPlacement::Below, Some(text("dolce", true)));
        markings.set_text(TextPlacement::Above, Some(text("div.", false)));
        let json = serde_json::to_string(&markings).unwrap();
        assert_eq!(
            json,
            r#"{"texts":[{"text":"div."},{"text":"dolce","placement":"below","italic":true}]}"#
        );
        assert_eq!(serde_json::from_str::<Markings>(&json).unwrap(), markings);
        assert!(markings.validate(false).is_ok());
        assert_eq!(markings.text(TextPlacement::Below).unwrap().text, "dolce");

        markings.set_text(TextPlacement::Above, None);
        assert_eq!(markings.texts.len(), 1);
        markings.set_text(TextPlacement::Below, None);
        assert!(markings.is_empty());

        let twice = Markings {
            texts: vec![text("div.", false), text("pizz.", false)],
            ..Markings::default()
        };
        assert!(twice.validate(false).is_err());
        let blank = Markings {
            texts: vec![text(" ", false)],
            ..Markings::default()
        };
        assert!(blank.validate(true).is_err());
    }

    #[test]
    fn brackets_store_only_what_differs_from_a_solid_bracket_above() {
        let plain = Markings {
            bracket_start: Some(Bracket::default()),
            bracket_stop: true,
            ..Markings::default()
        };
        let json = serde_json::to_string(&plain).unwrap();
        assert_eq!(json, r#"{"bracket_start":{},"bracket_stop":true}"#);
        assert_eq!(serde_json::from_str::<Markings>(&json).unwrap(), plain);
        let open = Markings {
            bracket_start: Some(Bracket {
                placement: TextPlacement::Below,
                line: BracketLine::Dashed,
                start_hook: true,
                end_hook: false,
            }),
            ..Markings::default()
        };
        let json = serde_json::to_string(&open).unwrap();
        assert_eq!(
            json,
            r#"{"bracket_start":{"placement":"below","line":"dashed","end_hook":false}}"#
        );
        assert_eq!(serde_json::from_str::<Markings>(&json).unwrap(), open);
    }

    #[test]
    fn pedals_store_their_style_and_stop() {
        let pedal = Markings {
            pedal_start: Some(PedalStyle::TextLine),
            pedal_stop: true,
            ..Markings::default()
        };
        let json = serde_json::to_string(&pedal).unwrap();
        assert_eq!(json, r#"{"pedal_start":"text_line","pedal_stop":true}"#);
        assert_eq!(serde_json::from_str::<Markings>(&json).unwrap(), pedal);
        assert!(!pedal.is_empty());
        assert!(serde_json::from_str::<Markings>("{}").unwrap().is_empty());

        let hidden = Markings {
            pedal_release_hidden: true,
            ..Markings::default()
        };
        assert!(
            hidden.validate(false).is_err(),
            "a hidden release needs a stop"
        );
    }
}
