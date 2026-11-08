//! The order in which measures are played, following repeats and endings.

use std::collections::HashMap;

use crate::score::{Bars, Score};

/// Indices of the measures in the order they are played.
///
/// A repeat goes back to the nearest repeat start before it, or to where the previous repeat
/// and its endings finished, or to the beginning. Each pass through it counts from 1; a
/// measure with an ending is played only on the passes its numbers name, so an ending that
/// holds the repeat's end is left out on the last pass and the music continues after it.
pub fn play_order(score: &Score) -> Vec<usize> {
    let measures = &score.measures;
    let mut order = Vec::with_capacity(measures.len());
    // Where the current repeat starts, and which pass through it is being played.
    let mut section_start = 0;
    let mut pass: u32 = 1;
    // Jumps already taken back from each repeat end.
    let mut taken = HashMap::<usize, u8>::new();
    // Just jumped back: the repeat start there does not begin a new repeat.
    let mut returning = false;
    // A repeat has been played out; its endings may follow, and then a new section starts.
    let mut finished = false;
    // Guards against scores whose marks would never end.
    let limit = measures.len() * (usize::from(Bars::MAX_REPEAT_TIMES) + 1) + 1;
    let mut index = 0;
    while index < measures.len() && order.len() < limit {
        let bars = measures[index].bars;
        if bars.repeat_start && !returning {
            section_start = index;
            pass = 1;
            finished = false;
        }
        returning = false;
        match bars.ending {
            Some(ending) if !ending.numbers.contains(pass) => {
                if bars.repeat_end.is_some() {
                    finished = true;
                }
                index += 1;
                continue;
            }
            Some(_) => {}
            None if finished => {
                section_start = index;
                pass = 1;
                finished = false;
            }
            None => {}
        }
        order.push(index);
        if let Some(times) = bars.repeat_end {
            let jumps = taken.entry(index).or_insert(0);
            if *jumps + 1 < times {
                *jumps += 1;
                pass += 1;
                returning = true;
                index = section_start;
                continue;
            }
            *jumps = 0;
            finished = true;
        }
        index += 1;
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::{Ending, EndingNumbers};
    use crate::{Command, Document};

    fn score(bars: &[(usize, Bars)], count: usize) -> Score {
        let mut doc = Document::new();
        for index in 1..count {
            let data = doc.score().blank_measure(index);
            doc.execute(Command::InsertMeasure { index, data }).unwrap();
        }
        for &(measure, bars) in bars {
            doc.execute(Command::SetBars { measure, bars }).unwrap();
        }
        doc.into_score()
    }

    fn start() -> Bars {
        Bars {
            repeat_start: true,
            ..Bars::default()
        }
    }

    fn end(times: u8) -> Bars {
        Bars {
            repeat_end: Some(times),
            ..Bars::default()
        }
    }

    fn ending(numbers: &[u8], bars: Bars) -> Bars {
        Bars {
            ending: Some(Ending {
                numbers: EndingNumbers::new(numbers).unwrap(),
                open: false,
            }),
            ..bars
        }
    }

    #[test]
    fn without_repeats_measures_play_in_order() {
        assert_eq!(play_order(&score(&[], 3)), vec![0, 1, 2]);
    }

    #[test]
    fn a_repeat_goes_back_to_its_start_or_the_beginning() {
        assert_eq!(play_order(&score(&[(1, end(2))], 3)), vec![0, 1, 0, 1, 2]);
        assert_eq!(
            play_order(&score(&[(1, start()), (2, end(3))], 4)),
            vec![0, 1, 2, 1, 2, 1, 2, 3]
        );
        // A second repeat without a start goes back to where the first one finished.
        assert_eq!(
            play_order(&score(&[(0, end(2)), (2, end(2))], 3)),
            vec![0, 0, 1, 2, 1, 2]
        );
    }

    #[test]
    fn endings_play_on_their_passes() {
        // ||: 0 1 |1. 2 :|2. 3 | 4
        let bars = [
            (0, start()),
            (2, ending(&[1], end(2))),
            (3, ending(&[2], Bars::default())),
        ];
        assert_eq!(play_order(&score(&bars, 5)), vec![0, 1, 2, 0, 1, 3, 4]);

        // Three passes: |1., 2. over two measures :|3.
        let bars = [
            (1, ending(&[1, 2], Bars::default())),
            (2, ending(&[1, 2], end(3))),
            (3, ending(&[3], Bars::default())),
        ];
        assert_eq!(
            play_order(&score(&bars, 5)),
            vec![0, 1, 2, 0, 1, 2, 0, 3, 4]
        );

        // After the endings a new section begins, so a later repeat goes back to it.
        let bars = [
            (1, ending(&[1], end(2))),
            (2, ending(&[2], Bars::default())),
            (4, end(2)),
        ];
        assert_eq!(play_order(&score(&bars, 5)), vec![0, 1, 0, 2, 3, 4, 3, 4]);
    }
}
