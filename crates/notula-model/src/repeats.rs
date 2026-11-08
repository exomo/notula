//! The order in which measures are played, following repeats, endings, and jumps.

use std::collections::{HashMap, HashSet};

use crate::score::{Bars, JumpTarget, JumpUntil, Score};

/// For each measure, how often the repeat it belongs to is played (1 outside repeats): the
/// pass that is played after a jump, when repeats are not taken again. Endings after a repeat's
/// end belong to it.
fn final_passes(score: &Score) -> Vec<u32> {
    let measures = &score.measures;
    let mut passes = vec![1_u32; measures.len()];
    let mut section_start = 0;
    let mut finished = false;
    for index in 0..measures.len() {
        let bars = measures[index].bars;
        if bars.repeat_start || (finished && bars.ending.is_none()) {
            section_start = index;
            finished = false;
        }
        if let Some(times) = bars.repeat_end {
            let times = u32::from(times);
            passes[section_start..=index].fill(times);
            for (offset, info) in measures[index + 1..].iter().enumerate() {
                if info.bars.ending.is_none() || info.bars.repeat_start {
                    break;
                }
                passes[index + 1 + offset] = times;
            }
            finished = true;
        }
    }
    passes
}

/// Indices of the measures in the order they are played.
///
/// A repeat goes back to the nearest repeat start before it, or to where the previous repeat
/// and its endings finished, or to the beginning. Each pass through it counts from 1; a
/// measure with an ending is played only on the passes its numbers name, so an ending that
/// holds the repeat's end is left out on the last pass and the music continues after it.
///
/// A jump ("D.C.", "D.S.") is taken once, on the last pass through a repeat it ends. It goes
/// back to the beginning or to the nearest segno before it (or the first one); from there
/// repeats are not taken again and endings play their last pass. After a jump al Fine the
/// music ends at "Fine"; after a jump al Coda, "To Coda" goes on at the next coda sign.
pub fn play_order(score: &Score) -> Vec<usize> {
    let measures = &score.measures;
    let final_pass = final_passes(score);
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
    // After a jump: how far to play; repeats are no longer taken.
    let mut jumped: Option<JumpUntil> = None;
    let mut jumps_taken = HashSet::new();
    // Guards against scores whose marks would never end.
    let limit = 2 * measures.len() * (usize::from(Bars::MAX_REPEAT_TIMES) + 1) + 1;
    let mut index = 0;
    while index < measures.len() && order.len() < limit {
        let bars = measures[index].bars;
        if jumped.is_none() && bars.repeat_start && !returning {
            section_start = index;
            pass = 1;
            finished = false;
        }
        returning = false;
        let current = if jumped.is_some() {
            final_pass[index]
        } else {
            pass
        };
        match bars.ending {
            Some(ending) if !ending.numbers.contains(current) => {
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
        if jumped.is_none()
            && let Some(times) = bars.repeat_end
        {
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
        match jumped {
            Some(JumpUntil::Fine) if bars.fine => break,
            Some(JumpUntil::Coda) if bars.to_coda => {
                let coda = (index + 1..measures.len())
                    .find(|&measure| measures[measure].bars.coda)
                    .or_else(|| (0..measures.len()).find(|&measure| measures[measure].bars.coda));
                if let Some(coda) = coda {
                    jumped = Some(JumpUntil::End);
                    index = coda;
                    continue;
                }
            }
            _ => {}
        }
        if let Some(jump) = bars.jump
            && jumps_taken.insert(index)
        {
            let target = match jump.target {
                JumpTarget::DaCapo => 0,
                JumpTarget::DalSegno => (0..=index)
                    .rev()
                    .find(|&measure| measures[measure].bars.segno)
                    .or_else(|| (0..measures.len()).find(|&measure| measures[measure].bars.segno))
                    .unwrap_or(0),
            };
            jumped = Some(jump.until);
            finished = false;
            index = target;
            continue;
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

    fn jump(target: JumpTarget, until: JumpUntil, bars: Bars) -> Bars {
        Bars {
            jump: Some(crate::score::Jump { target, until }),
            ..bars
        }
    }

    #[test]
    fn da_capo_goes_back_once_without_repeats() {
        use JumpTarget::DaCapo;
        // 0 ||: 1 :|| 2 D.C.: the repeat is not taken after the jump.
        let bars = [
            (
                1,
                Bars {
                    repeat_start: true,
                    ..end(2)
                },
            ),
            (2, jump(DaCapo, JumpUntil::End, Bars::default())),
        ];
        assert_eq!(play_order(&score(&bars, 3)), vec![0, 1, 1, 2, 0, 1, 2]);

        // ||: 0 |1. 1 :|2. 2 Fine | 3 D.C. al Fine: after the jump the second ending plays.
        let bars = [
            (1, ending(&[1], end(2))),
            (
                2,
                Bars {
                    fine: true,
                    ..ending(&[2], Bars::default())
                },
            ),
            (3, jump(DaCapo, JumpUntil::Fine, Bars::default())),
        ];
        assert_eq!(play_order(&score(&bars, 5)), vec![0, 1, 0, 2, 3, 0, 2]);
    }

    #[test]
    fn dal_segno_al_coda_skips_to_the_coda() {
        use JumpTarget::DalSegno;
        // 0 | segno 1 | 2 To Coda | 3 D.S. al Coda | coda 4 | 5
        let bars = [
            (
                1,
                Bars {
                    segno: true,
                    ..Bars::default()
                },
            ),
            (
                2,
                Bars {
                    to_coda: true,
                    ..Bars::default()
                },
            ),
            (3, jump(DalSegno, JumpUntil::Coda, Bars::default())),
            (
                4,
                Bars {
                    coda: true,
                    ..Bars::default()
                },
            ),
        ];
        assert_eq!(play_order(&score(&bars, 6)), vec![0, 1, 2, 3, 1, 2, 4, 5]);

        // Without a segno, D.S. goes back to the beginning.
        let bars = [(1, jump(DalSegno, JumpUntil::End, Bars::default()))];
        assert_eq!(play_order(&score(&bars, 3)), vec![0, 1, 0, 1, 2]);
    }
}
