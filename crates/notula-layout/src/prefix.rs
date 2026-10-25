//! Clef, key signature and time signature at the start of a system or after a change.

use notula_model::{Clef, KeySignature, MeasureInfo, Step, TimeSignatureDisplay};

use crate::render::Canvas;
use crate::smufl;
use crate::staff::key_signature_positions;

const LEAD_PAD: f32 = 1.0;
const CLEF_WIDTH: f32 = 2.8;
const CLEF_GAP: f32 = 1.0;
const MID_SYSTEM_PAD: f32 = 0.8;
const KEY_ADVANCE_GAP: f32 = 0.25;
const KEY_TRAIL: f32 = 0.75;
const KEY_CANCEL_TRAIL: f32 = 0.5;
const TIME_TRAIL: f32 = 1.0;
const TIME_SYMBOL_WIDTH: f32 = 2.4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Prefix {
    pub clef: bool,
    pub key: bool,
    pub cancel_key: Option<i8>,
    pub time: bool,
}

fn digits(mut n: u8) -> usize {
    let mut count = 1;
    while n >= 10 {
        n /= 10;
        count += 1;
    }
    count
}

fn key_block(fifths: i8) -> f32 {
    let (_, width) = smufl::accidental(fifths.signum());
    f32::from(fifths.unsigned_abs()) * (width + KEY_ADVANCE_GAP) + KEY_TRAIL
}

fn cancelled_pitches(previous: i8, current: i8) -> Vec<Step> {
    key_signature_positions(previous, Clef::Treble)
        .into_iter()
        .map(|(step, _)| step)
        .filter(|step| {
            (KeySignature { fifths: previous }).alter(*step)
                != (KeySignature { fifths: current }).alter(*step)
        })
        .collect()
}

fn time_digits_width(info: MeasureInfo) -> f32 {
    let time = info.time_signature;
    digits(time.numerator).max(digits(time.denominator)) as f32 * smufl::TIME_DIGIT_ADVANCE
}

fn time_width(info: MeasureInfo) -> f32 {
    match info.time_signature_display {
        TimeSignatureDisplay::Numeric => time_digits_width(info),
        TimeSignatureDisplay::Common | TimeSignatureDisplay::AllaBreve => TIME_SYMBOL_WIDTH,
    }
}

impl Prefix {
    pub fn width(self, info: MeasureInfo) -> f32 {
        let mut width = 0.0;
        if self.clef {
            width += LEAD_PAD + CLEF_WIDTH + CLEF_GAP;
        } else if self.key || self.time {
            width += MID_SYSTEM_PAD;
        }
        if self.key {
            if let Some(previous) = self.cancel_key {
                let count = cancelled_pitches(previous, info.key_signature.fifths).len();
                if count > 0 {
                    let (_, natural_width) = smufl::accidental(0);
                    width += count as f32 * (natural_width + KEY_ADVANCE_GAP) + KEY_CANCEL_TRAIL;
                }
            }
            if info.key_signature.fifths != 0 {
                width += key_block(info.key_signature.fifths);
            }
        }
        if self.time {
            width += time_width(info) + TIME_TRAIL;
        }
        width
    }

    pub fn draw(self, canvas: &mut Canvas, x: f32, y0: f32, clef: Clef, info: MeasureInfo) {
        let mut x = x;
        if self.clef {
            let glyph = smufl::clef_glyph(clef);
            canvas.glyph(None, x + LEAD_PAD, y0 + glyph.y, glyph.glyph, 4.0);
            x += LEAD_PAD + CLEF_WIDTH + CLEF_GAP;
        } else if self.key || self.time {
            x += MID_SYSTEM_PAD;
        }
        if self.key {
            let fifths = info.key_signature.fifths;
            if let Some(previous) = self.cancel_key {
                let (natural, width) = smufl::accidental(0);
                for (step, y) in key_signature_positions(previous, clef) {
                    if (KeySignature { fifths: previous }).alter(step)
                        != (KeySignature { fifths }).alter(step)
                    {
                        canvas.glyph(None, x, y0 + y, natural, 4.0);
                        x += width + KEY_ADVANCE_GAP;
                    }
                }
                if cancelled_pitches(previous, fifths).len() > 0 {
                    x += KEY_CANCEL_TRAIL;
                }
            }
            if fifths != 0 {
                let (glyph, width) = smufl::accidental(fifths.signum());
                for y in key_signature_positions(fifths, clef)
                    .into_iter()
                    .map(|(_, y)| y)
                {
                    canvas.glyph(None, x, y0 + y, glyph, 4.0);
                    x += width + KEY_ADVANCE_GAP;
                }
                x += KEY_TRAIL;
            }
        }
        if self.time {
            match info.time_signature_display {
                TimeSignatureDisplay::Common | TimeSignatureDisplay::AllaBreve => {
                    let glyph = if info.time_signature_display == TimeSignatureDisplay::Common {
                        smufl::TIME_COMMON
                    } else {
                        smufl::TIME_CUT_COMMON
                    };
                    canvas.glyph(None, x, y0 + 2.5, glyph, 4.0);
                }
                TimeSignatureDisplay::Numeric => {
                    let time = info.time_signature;
                    let block = time_digits_width(info);
                    for (value, baseline) in [(time.numerator, 1.0), (time.denominator, 3.0)] {
                        let text = value.to_string();
                        let start =
                            x + (block - text.len() as f32 * smufl::TIME_DIGIT_ADVANCE) / 2.0;
                        for (i, digit) in text.chars().filter_map(|c| c.to_digit(10)).enumerate() {
                            canvas.glyph(
                                None,
                                start + i as f32 * smufl::TIME_DIGIT_ADVANCE,
                                y0 + baseline,
                                smufl::time_digit(digit),
                                4.0,
                            );
                        }
                    }
                }
            }
        }
    }
}
