//! Creating new scores from the setup dialog.

use noten_model::{Clef, Command, Document, KeySignature, ModelError, Score, TimeSignature};
use serde::{Deserialize, Serialize};

const MAX_MEASURES: usize = 999;
const MAX_INSTRUMENTS: usize = 64;

struct Preset {
    id: &'static str,
    name: &'static str,
    clefs: &'static [Clef],
}

// Instruments sound as written; transposition is not supported yet.
const PRESETS: &[Preset] = &[
    Preset {
        id: "piano",
        name: "Piano",
        clefs: &[Clef::Treble, Clef::Bass],
    },
    Preset {
        id: "organ",
        name: "Organ",
        clefs: &[Clef::Treble, Clef::Bass],
    },
    Preset {
        id: "harp",
        name: "Harp",
        clefs: &[Clef::Treble, Clef::Bass],
    },
    Preset {
        id: "flute",
        name: "Flute",
        clefs: &[Clef::Treble],
    },
    Preset {
        id: "oboe",
        name: "Oboe",
        clefs: &[Clef::Treble],
    },
    Preset {
        id: "clarinet",
        name: "Clarinet",
        clefs: &[Clef::Treble],
    },
    Preset {
        id: "bassoon",
        name: "Bassoon",
        clefs: &[Clef::Bass],
    },
    Preset {
        id: "horn",
        name: "Horn",
        clefs: &[Clef::Treble],
    },
    Preset {
        id: "trumpet",
        name: "Trumpet",
        clefs: &[Clef::Treble],
    },
    Preset {
        id: "trombone",
        name: "Trombone",
        clefs: &[Clef::Bass],
    },
    Preset {
        id: "violin",
        name: "Violin",
        clefs: &[Clef::Treble],
    },
    Preset {
        id: "viola",
        name: "Viola",
        clefs: &[Clef::Alto],
    },
    Preset {
        id: "cello",
        name: "Cello",
        clefs: &[Clef::Bass],
    },
    Preset {
        id: "double-bass",
        name: "Double Bass",
        clefs: &[Clef::Bass],
    },
    Preset {
        id: "guitar",
        name: "Guitar",
        clefs: &[Clef::Treble],
    },
    Preset {
        id: "voice",
        name: "Voice",
        clefs: &[Clef::Treble],
    },
];

#[derive(Debug, Serialize)]
pub struct InstrumentInfo {
    id: &'static str,
    name: &'static str,
    staves: usize,
}

pub fn instruments() -> Vec<InstrumentInfo> {
    PRESETS
        .iter()
        .map(|p| InstrumentInfo {
            id: p.id,
            name: p.name,
            staves: p.clefs.len(),
        })
        .collect()
}

#[derive(Debug, Deserialize)]
pub struct ScoreSetup {
    pub title: String,
    pub composer: String,
    /// Preset ids, top to bottom.
    pub instruments: Vec<String>,
    pub time_numerator: u8,
    pub time_denominator: u8,
    pub key_fifths: i8,
    pub measures: usize,
    /// Length in ticks of a shortened first measure (anacrusis).
    #[serde(default)]
    pub pickup: Option<u32>,
}

pub fn build(setup: &ScoreSetup) -> Result<Document, ModelError> {
    if setup.instruments.is_empty() || setup.instruments.len() > MAX_INSTRUMENTS {
        return Err(ModelError::Invalid("choose between 1 and 64 instruments"));
    }
    if !(1..=MAX_MEASURES).contains(&setup.measures) {
        return Err(ModelError::Invalid("a score can have 1 to 999 measures"));
    }
    let time_signature = TimeSignature {
        numerator: setup.time_numerator,
        denominator: setup.time_denominator,
    };
    let key_signature = KeySignature {
        fifths: setup.key_fifths,
    };
    time_signature.validate()?;
    key_signature.validate()?;

    let mut doc = Document::new();
    for (index, id) in setup.instruments.iter().enumerate() {
        let preset = PRESETS
            .iter()
            .find(|p| p.id == id)
            .ok_or(ModelError::Invalid("unknown instrument"))?;
        let part = doc.new_part(preset.name, preset.clefs);
        doc.execute(Command::AddPart { index, part })?;
    }
    doc.execute(Command::SetMetadata {
        title: setup.title.trim().to_owned(),
        composer: setup.composer.trim().to_owned(),
    })?;
    doc.execute(Command::SetTimeSignature {
        measure: 0,
        time_signature,
    })?;
    doc.execute(Command::SetKeySignature {
        measure: 0,
        key_signature,
    })?;
    if let Some(length) = setup.pickup {
        doc.execute(Command::SetMeasureLength {
            measure: 0,
            length: Some(length),
        })?;
    }
    for index in 1..setup.measures {
        let data = doc.score().blank_measure(index);
        doc.execute(Command::InsertMeasure { index, data })?;
    }
    // A fresh score has no history to undo.
    Ok(Document::from_score(doc.into_score()))
}

pub fn default_score() -> Score {
    let setup = ScoreSetup {
        title: String::new(),
        composer: String::new(),
        instruments: vec!["piano".to_owned()],
        time_numerator: 4,
        time_denominator: 4,
        key_fifths: 0,
        measures: 8,
        pickup: None,
    };
    build(&setup)
        .expect("the default setup is valid")
        .into_score()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> ScoreSetup {
        ScoreSetup {
            title: " Suite ".into(),
            composer: "Anon".into(),
            instruments: vec!["flute".into(), "piano".into()],
            time_numerator: 3,
            time_denominator: 4,
            key_fifths: -2,
            measures: 5,
            pickup: None,
        }
    }

    #[test]
    fn builds_a_valid_score_from_the_setup() {
        let doc = build(&setup()).unwrap();
        let score = doc.score();
        score.validate().unwrap();
        assert_eq!(score.title, "Suite");
        assert_eq!(score.parts.len(), 2);
        assert_eq!(score.staves().count(), 3);
        assert_eq!(score.measures.len(), 5);
        assert!(score.measures.iter().all(|m| m.key_signature.fifths == -2));
        assert!(
            score
                .measures
                .iter()
                .all(|m| m.time_signature.numerator == 3)
        );
        assert!(!doc.can_undo());
    }

    #[test]
    fn rejects_invalid_setups() {
        let mut empty = setup();
        empty.instruments.clear();
        assert!(build(&empty).is_err());

        let mut unknown = setup();
        unknown.instruments = vec!["kazoo".into()];
        assert!(build(&unknown).is_err());

        let mut time = setup();
        time.time_denominator = 3;
        assert!(build(&time).is_err());

        let mut key = setup();
        key.key_fifths = 8;
        assert!(build(&key).is_err());

        for measures in [0, 1000] {
            let mut count = setup();
            count.measures = measures;
            assert!(build(&count).is_err());
        }
    }

    #[test]
    fn a_pickup_only_shortens_the_first_measure() {
        let mut s = setup();
        s.pickup = Some(960);
        let doc = build(&s).unwrap();
        let measures = &doc.score().measures;
        assert_eq!(measures[0].length, Some(960));
        assert!(measures[1..].iter().all(|m| m.length.is_none()));
        doc.score().validate().unwrap();

        // 3/4 holds at most 2880 ticks, and lengths must be whole 64th notes.
        for invalid in [2880 + 960, 100, 0] {
            let mut s = setup();
            s.pickup = Some(invalid);
            assert!(build(&s).is_err(), "{invalid}");
        }
    }

    #[test]
    fn every_preset_can_be_built() {
        for preset in PRESETS {
            let mut s = setup();
            s.instruments = vec![preset.id.to_owned()];
            assert!(build(&s).is_ok(), "{}", preset.id);
        }
    }
}
