//! Codepoints and metrics from the SMuFL Bravura font, in staff spaces at the standard size.

use notula_model::{Articulation, Clef, NoteValue};

pub const STAFF_LINE_THICKNESS: f32 = 0.13;
pub const STEM_THICKNESS: f32 = 0.12;
pub const THIN_BARLINE: f32 = 0.16;
pub const THICK_BARLINE: f32 = 0.5;
pub const LEDGER_THICKNESS: f32 = 0.16;
pub const LEDGER_EXTENSION: f32 = 0.4;
pub const BEAM_THICKNESS: f32 = 0.5;
pub const BEAM_SPACING: f32 = 0.25;

pub const DOT_WIDTH: f32 = 0.4;
pub const TIME_DIGIT_ADVANCE: f32 = 1.88;
pub const STEM_ANCHOR_OFFSET: f32 = 0.168;

pub const BRACE: char = '\u{E000}';
pub const BRACE_WIDTH_PER_HEIGHT: f32 = 0.277 / 4.0;
pub const AUGMENTATION_DOT: char = '\u{E1E7}';
pub const ACCENT_ABOVE: char = '\u{E4A0}';
pub const ACCENT_BELOW: char = '\u{E4A1}';
pub const STACCATO_ABOVE: char = '\u{E4A2}';
pub const STACCATO_BELOW: char = '\u{E4A3}';
pub const TENUTO_ABOVE: char = '\u{E4A4}';
pub const TENUTO_BELOW: char = '\u{E4A5}';
pub const STACCATISSIMO_ABOVE: char = '\u{E4A6}';
pub const STACCATISSIMO_BELOW: char = '\u{E4A7}';
pub const MARCATO_ABOVE: char = '\u{E4AC}';
pub const MARCATO_BELOW: char = '\u{E4AD}';
pub const FERMATA_ABOVE: char = '\u{E4C0}';
pub const FERMATA_BELOW: char = '\u{E4C1}';
pub const METRONOME_DOT: char = '\u{ECB7}';
/// Advance of a metronome note at the standard size; all stem-up notes share it.
pub const METRONOME_NOTE_WIDTH: f32 = 1.18;
/// How far a metronome notehead reaches below its origin.
pub const METRONOME_NOTE_DEPTH: f32 = 0.56;
pub const TIME_COMMON: char = '\u{E08A}';
pub const TIME_CUT_COMMON: char = '\u{E08B}';

pub struct ClefGlyph {
    pub glyph: char,
    /// Staff line the clef is anchored to, as distance from the top line.
    pub y: f32,
}

pub fn articulation(articulation: Articulation, below: bool) -> char {
    match (articulation, below) {
        (Articulation::Accent, false) => ACCENT_ABOVE,
        (Articulation::Accent, true) => ACCENT_BELOW,
        (Articulation::Staccato, false) => STACCATO_ABOVE,
        (Articulation::Staccato, true) => STACCATO_BELOW,
        (Articulation::Tenuto, false) => TENUTO_ABOVE,
        (Articulation::Tenuto, true) => TENUTO_BELOW,
        (Articulation::Staccatissimo, false) => STACCATISSIMO_ABOVE,
        (Articulation::Staccatissimo, true) => STACCATISSIMO_BELOW,
        (Articulation::Marcato, false) => MARCATO_ABOVE,
        (Articulation::Marcato, true) => MARCATO_BELOW,
        (Articulation::Fermata, false) => FERMATA_ABOVE,
        (Articulation::Fermata, true) => FERMATA_BELOW,
    }
}

pub fn clef_glyph(clef: Clef) -> ClefGlyph {
    match clef {
        Clef::Treble => ClefGlyph {
            glyph: '\u{E050}',
            y: 3.0,
        },
        Clef::Bass => ClefGlyph {
            glyph: '\u{E062}',
            y: 1.0,
        },
        Clef::Alto => ClefGlyph {
            glyph: '\u{E05C}',
            y: 2.0,
        },
        Clef::Tenor => ClefGlyph {
            glyph: '\u{E05C}',
            y: 1.0,
        },
    }
}

pub fn notehead(value: NoteValue) -> (char, f32) {
    match value {
        NoteValue::Whole => ('\u{E0A2}', 1.688),
        NoteValue::Half => ('\u{E0A3}', 1.18),
        _ => ('\u{E0A4}', 1.18),
    }
}

/// Stem-up note for a metronome mark, drawn with a flag instead of beams.
pub fn metronome_note(value: NoteValue) -> char {
    match value {
        NoteValue::Whole => '\u{ECA2}',
        NoteValue::Half => '\u{ECA3}',
        NoteValue::Quarter => '\u{ECA5}',
        NoteValue::Eighth => '\u{ECA7}',
        NoteValue::Sixteenth => '\u{ECA9}',
        NoteValue::ThirtySecond => '\u{ECAB}',
        NoteValue::SixtyFourth => '\u{ECAD}',
    }
}

/// Glyph, advance width and vertical origin (distance from the top staff line).
pub fn rest(value: NoteValue) -> (char, f32, f32) {
    match value {
        NoteValue::Whole => ('\u{E4E3}', 1.132, 1.0),
        NoteValue::Half => ('\u{E4E4}', 1.132, 2.0),
        NoteValue::Quarter => ('\u{E4E5}', 1.08, 2.0),
        NoteValue::Eighth => ('\u{E4E6}', 1.0, 2.0),
        NoteValue::Sixteenth => ('\u{E4E7}', 1.28, 2.0),
        NoteValue::ThirtySecond => ('\u{E4E8}', 1.452, 2.0),
        NoteValue::SixtyFourth => ('\u{E4E9}', 1.696, 2.0),
    }
}

/// Number of beams or flags a note value carries.
pub fn flag_count(value: NoteValue) -> usize {
    match value {
        NoteValue::Whole | NoteValue::Half | NoteValue::Quarter => 0,
        NoteValue::Eighth => 1,
        NoteValue::Sixteenth => 2,
        NoteValue::ThirtySecond => 3,
        NoteValue::SixtyFourth => 4,
    }
}

/// Flag glyph and the y offset that aligns its stem anchor with the stem end.
pub fn flag(value: NoteValue, stem_up: bool) -> Option<(char, f32)> {
    let flag = match (value, stem_up) {
        (NoteValue::Eighth, true) => ('\u{E240}', -0.04),
        (NoteValue::Eighth, false) => ('\u{E241}', 0.132),
        (NoteValue::Sixteenth, true) => ('\u{E242}', -0.088),
        (NoteValue::Sixteenth, false) => ('\u{E243}', 0.128),
        (NoteValue::ThirtySecond, true) => ('\u{E244}', 0.376),
        (NoteValue::ThirtySecond, false) => ('\u{E245}', -0.448),
        (NoteValue::SixtyFourth, true) => ('\u{E246}', 1.172),
        (NoteValue::SixtyFourth, false) => ('\u{E247}', -1.244),
        _ => return None,
    };
    Some(flag)
}

/// Glyph and advance width for an accidental, `alter` in -2..=2.
pub fn accidental(alter: i8) -> (char, f32) {
    match alter {
        i8::MIN..=-2 => ('\u{E264}', 1.652),
        -1 => ('\u{E260}', 0.904),
        0 => ('\u{E261}', 0.672),
        1 => ('\u{E262}', 0.996),
        2..=i8::MAX => ('\u{E263}', 1.0),
    }
}

pub fn time_digit(digit: u32) -> char {
    char::from_u32(0xE080 + digit).unwrap_or('\u{E080}')
}
