//! Moving a selection earlier or later in time, through rests only.
//!
//! In each selected staff and voice, the selected events move as one block. The time they move
//! into must be rest in that voice, and the time they leave becomes rest, so notes never pass
//! each other and slurs, hairpins, and ties keep their order. If any voice cannot move, nothing
//! does.

use std::collections::BTreeSet;

use crate::command::Command;
use crate::error::ModelError;
use crate::history::Document;
use crate::ids::StaffId;
use crate::rewrite::{
    Placed, assign_ids, measure_of, measure_starts, rewrite_measures, voice_events, voice_limit,
};
use crate::score::{EventKind, Score};
use crate::selection::Selection;

/// Moves the selected events by `step` ticks, later when `forward`, and returns the selection
/// at its new place. Fails, changing nothing, when any selected voice cannot move.
pub fn shift_selection(
    doc: &mut Document,
    selection: &Selection,
    forward: bool,
    step: u32,
) -> Result<Selection, ModelError> {
    if step == 0 {
        return Err(ModelError::Invalid("choose a note value to move by"));
    }
    let score = doc.score();
    let resolved = selection.resolve(score).ok_or(ModelError::Invalid(
        "the selection no longer fits the score",
    ))?;
    if resolved.events.is_empty() {
        return Err(ModelError::Invalid("select notes to move them"));
    }
    let starts = measure_starts(score);
    let step = u64::from(step);
    let context = Shift {
        score,
        total: *starts.last().unwrap_or(&0),
        starts,
        span_start: resolved.span.start,
        span_end: resolved.span.end,
        forward,
        step,
    };

    let mut commands = Vec::new();
    for &staff_id in &resolved.staves {
        let staff = score.staff(staff_id)?;
        let voice_count = staff
            .measures
            .iter()
            .map(|measure| measure.voices.len())
            .max()
            .unwrap_or(0);
        for voice in 0..voice_count {
            if selection.voice.is_some_and(|only| only != voice) {
                continue;
            }
            commands.extend(context.voice(staff_id, voice)?);
        }
    }
    if commands.is_empty() {
        return Err(ModelError::Invalid("select notes to move them"));
    }
    // Rests filling the left-behind time get their ids now that the score is no longer read.
    assign_ids(doc, &mut commands);
    doc.execute(Command::Batch { commands })?;

    let moved = |tick: u64| if forward { tick + step } else { tick - step };
    Ok(Selection {
        anchor: moved(selection.anchor),
        focus: moved(selection.focus),
        ..*selection
    })
}

/// One shift of a selection's grown span.
struct Shift<'a> {
    score: &'a Score,
    /// Where each measure starts, plus the end of the score.
    starts: Vec<u64>,
    total: u64,
    span_start: u64,
    span_end: u64,
    forward: bool,
    step: u64,
}

impl Shift<'_> {
    fn measure_of(&self, tick: u64) -> usize {
        measure_of(&self.starts, tick)
    }

    /// Commands that move one voice's selected block, or none when it has nothing selected.
    fn voice(&self, staff_id: StaffId, voice: usize) -> Result<Vec<Command>, ModelError> {
        let (score, starts, total, forward, step) = (
            self.score,
            &self.starts,
            self.total,
            self.forward,
            self.step,
        );
        let (span_start, span_end) = (self.span_start, self.span_end);
        let measure_of = |tick: u64| self.measure_of(tick);
        let staff = score.staff(staff_id)?;
        let placed = voice_events(staff, starts, voice);
        let block: Vec<usize> = (0..placed.len())
            .filter(|&index| span_start <= placed[index].start && placed[index].start < span_end)
            .collect();
        let (Some(&first), Some(&last)) = (block.first(), block.last()) else {
            return Ok(Vec::new());
        };
        let (block_start, block_end) = (placed[first].start, placed[last].end());

        // The time the block moves into, which must be unmarked rest in this voice.
        let (target_start, target_end) = if forward {
            (block_end, block_end + step)
        } else {
            let start = block_start.checked_sub(step).ok_or(ModelError::Invalid(
                "the notes are already at the start of the score",
            ))?;
            (start, block_start)
        };
        if target_end > total {
            return Err(ModelError::Invalid(
                "the notes would move past the end of the score",
            ));
        }
        for measure in measure_of(target_start)..=measure_of(target_end - 1) {
            let content = &staff.measures[measure];
            let Some(voice_content) = content.voices.get(voice) else {
                return Err(ModelError::Invalid("the voice does not continue there"));
            };
            let limit = voice_limit(voice_content, starts[measure], starts[measure + 1]);
            if target_start.max(starts[measure]) < starts[measure + 1].min(target_end)
                && limit < starts[measure + 1].min(target_end)
            {
                return Err(ModelError::Invalid("the voice does not continue there"));
            }
        }
        for item in &placed {
            if item.start < target_end && target_start < item.end() {
                let is_rest = matches!(item.event.kind, EventKind::Rest);
                if !is_rest {
                    return Err(ModelError::Invalid("another note is in the way"));
                }
                if !item.event.markings.is_empty() {
                    return Err(ModelError::Invalid("a rest with marks is in the way"));
                }
            }
        }

        // New positions for the block; none may straddle a barline.
        let offset = |tick: u64| if forward { tick + step } else { tick - step };
        let mut moved: Vec<Placed> = block
            .iter()
            .map(|&index| Placed {
                start: offset(placed[index].start),
                event: placed[index].event.clone(),
            })
            .collect();
        for item in &moved {
            if measure_of(item.start) != measure_of(item.end() - 1) {
                return Err(ModelError::Invalid("a note would cross a barline"));
            }
        }
        // Moving apart breaks ties at the block's edges.
        if let Some(item) = moved.last_mut()
            && let EventKind::Chord { tied, .. } = &mut item.event.kind
        {
            *tied = false;
        }

        // Rebuild every measure the move touches.
        let affected_start = block_start.min(target_start);
        let affected_end = block_end.max(target_end);
        let block_ids: BTreeSet<_> = block.iter().map(|&index| placed[index].event.id).collect();
        let mut kept: Vec<Placed> = placed
            .into_iter()
            .filter(|item| !block_ids.contains(&item.event.id))
            .filter(|item| {
                // Rests overlapping the affected time are rewritten from the gaps.
                item.end() <= affected_start || affected_end <= item.start
            })
            .collect();
        // The event just before the affected time loses its tie too; its measure is rewritten.
        let mut first_measure = measure_of(affected_start);
        if let Some(before) = kept
            .iter_mut()
            .rev()
            .find(|item| item.end() <= affected_start)
            && before.end() == affected_start
            && let EventKind::Chord {
                tied: tied @ true, ..
            } = &mut before.event.kind
        {
            *tied = false;
            first_measure = first_measure.min(measure_of(before.start));
        }

        let mut items: Vec<Placed> = kept.into_iter().chain(moved).collect();
        items.sort_by_key(|item| item.start);
        rewrite_measures(
            staff,
            starts,
            voice,
            first_measure..=measure_of(affected_end - 1),
            &items,
        )
    }
}
