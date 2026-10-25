//! Mapping between pitches and positions on a five-line staff.

use noten_model::{Clef, KeySignature, Pitch, Step};

/// Distance of a staff position from the top line, in staff spaces (lines are at 0..=4).
pub fn pitch_y(pitch: Pitch, clef: Clef) -> f32 {
    4.0 - f32::from(pitch.diatonic_index() - bottom_line(clef)) * 0.5
}

/// Inverse of `pitch_y`: the pitch at a distance from the top line, with the key's alteration.
pub fn pitch_at(y: f32, clef: Clef, key: KeySignature) -> Pitch {
    let steps = ((4.0 - y) * 2.0).round() as i16;
    let diatonic = bottom_line(clef) + steps;
    let step = Step::from_index(diatonic.rem_euclid(7) as u8).unwrap_or(Step::C);
    let octave = i8::try_from(diatonic.div_euclid(7)).unwrap_or(4);
    Pitch::new(step, key.alter(step), octave)
}

/// Diatonic index of the pitch on the bottom line.
fn bottom_line(clef: Clef) -> i16 {
    match clef {
        Clef::Treble => 30, // E4
        Clef::Bass => 18,   // G2
        Clef::Alto => 24,   // F3
        Clef::Tenor => 22,  // D3
    }
}

/// Alteration a key signature applies to a step.
pub fn key_alter(fifths: i8, step: Step) -> i8 {
    KeySignature { fifths }.alter(step)
}

/// Vertical positions of the key signature accidentals, in drawing order.
#[cfg(test)]
pub fn key_signature_ys(fifths: i8, clef: Clef) -> Vec<f32> {
    key_signature_positions(fifths, clef)
        .into_iter()
        .map(|(_, y)| y)
        .collect()
}

/// Pitch and vertical position for each accidental in a key signature.
pub fn key_signature_positions(fifths: i8, clef: Clef) -> Vec<(Step, f32)> {
    let count = usize::from(fifths.unsigned_abs()).min(7);
    let bottom = bottom_line(clef);
    let to_y = |index: i16| 4.0 - f32::from(index - bottom) * 0.5;

    // Each accidental steps by a fourth or fifth to stay on the staff.
    let (start, steps, pitches): (i16, [i16; 6], &[Step]) = if fifths > 0 {
        match clef {
            Clef::Treble => (38, [-3, 4, -3, -3, 4, -3], &SHARP_ORDER),
            Clef::Alto => (31, [-3, 4, -3, -3, 4, -3], &SHARP_ORDER),
            Clef::Bass => (24, [-3, 4, -3, -3, 4, -3], &SHARP_ORDER),
            Clef::Tenor => (24, [4, -3, 4, -3, 4, -3], &SHARP_ORDER),
        }
    } else {
        let start = match clef {
            Clef::Treble => 34,
            Clef::Alto | Clef::Tenor => 27,
            Clef::Bass => 20,
        };
        (start, [3, -4, 3, -4, 3, -4], &FLAT_ORDER)
    };

    let mut index = start;
    let mut positions = Vec::with_capacity(count);
    for i in 0..count {
        positions.push((pitches[i], to_y(index)));
        if let Some(step) = steps.get(i) {
            index += step;
        }
    }
    positions
}

const SHARP_ORDER: [Step; 7] = [
    Step::F,
    Step::C,
    Step::G,
    Step::D,
    Step::A,
    Step::E,
    Step::B,
];
const FLAT_ORDER: [Step; 7] = [
    Step::B,
    Step::E,
    Step::A,
    Step::D,
    Step::G,
    Step::C,
    Step::F,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pitch_positions() {
        // Treble: E4 on the bottom line, F5 on the top line, middle C on the first ledger line.
        assert_eq!(pitch_y(Pitch::new(Step::E, 0, 4), Clef::Treble), 4.0);
        assert_eq!(pitch_y(Pitch::new(Step::F, 0, 5), Clef::Treble), 0.0);
        assert_eq!(pitch_y(Pitch::new(Step::C, 0, 4), Clef::Treble), 5.0);
        // Bass: G2 on the bottom line, A3 on the top line, middle C on the first ledger line above.
        assert_eq!(pitch_y(Pitch::new(Step::G, 0, 2), Clef::Bass), 4.0);
        assert_eq!(pitch_y(Pitch::new(Step::A, 0, 3), Clef::Bass), 0.0);
        assert_eq!(pitch_y(Pitch::new(Step::C, 0, 4), Clef::Bass), -1.0);
        // Alto: middle C on the middle line.
        assert_eq!(pitch_y(Pitch::new(Step::C, 0, 4), Clef::Alto), 2.0);
        // Tenor: middle C on the second line from the top.
        assert_eq!(pitch_y(Pitch::new(Step::C, 0, 4), Clef::Tenor), 1.0);
    }

    #[test]
    fn pitch_at_inverts_pitch_y() {
        for clef in [Clef::Treble, Clef::Bass, Clef::Alto, Clef::Tenor] {
            for octave in 1..=6 {
                for index in 0..7 {
                    let step = Step::from_index(index).unwrap();
                    let pitch = Pitch::new(step, 0, octave);
                    let y = pitch_y(pitch, clef);
                    assert_eq!(pitch_at(y, clef, KeySignature::default()), pitch);
                }
            }
        }
    }

    #[test]
    fn pitch_at_applies_the_key_signature() {
        let d_major = KeySignature { fifths: 2 };
        // The top line of the treble staff is F5.
        assert_eq!(
            pitch_at(0.0, Clef::Treble, d_major),
            Pitch::new(Step::F, 1, 5)
        );
    }

    #[test]
    fn key_alterations() {
        assert_eq!(key_alter(2, Step::F), 1);
        assert_eq!(key_alter(2, Step::C), 1);
        assert_eq!(key_alter(2, Step::G), 0);
        assert_eq!(key_alter(-1, Step::B), -1);
        assert_eq!(key_alter(-1, Step::E), 0);
        assert_eq!(key_alter(0, Step::F), 0);
    }

    #[test]
    fn treble_key_signatures_match_convention() {
        // F# on the top line, C# in the third space.
        assert_eq!(key_signature_ys(2, Clef::Treble), vec![0.0, 1.5]);
        // Bb on the middle line, Eb in the fourth space.
        assert_eq!(key_signature_ys(-2, Clef::Treble), vec![2.0, 0.5]);
    }

    #[test]
    fn all_key_signatures_stay_near_the_staff() {
        for clef in [Clef::Treble, Clef::Bass, Clef::Alto, Clef::Tenor] {
            for fifths in -7..=7 {
                for y in key_signature_ys(fifths, clef) {
                    assert!((-1.0..=5.0).contains(&y), "{clef:?} {fifths}: {y}");
                }
            }
        }
    }
}
