//! Moving a selection earlier or later in time, through rests only.
//!
//! In each selected staff and voice, the selected events move as one block. The time they move
//! into must be rest in that voice, and the time they leave becomes rest, so notes never pass
//! each other and slurs, hairpins, and ties keep their order. If any voice cannot move, nothing
//! does.

use std::collections::BTreeSet;

use crate::command::Command;
use crate::edit::rest_durations;
use crate::error::ModelError;
use crate::history::Document;
use crate::ids::StaffId;
use crate::score::{Event, EventKind, Score, VoiceLocation};
use crate::selection::Selection;

fn measure_starts(score: &Score) -> Vec<u64> {
    let mut starts = Vec::with_capacity(score.measures.len() + 1);
    let mut tick = 0;
    for info in &score.measures {
        starts.push(tick);
        tick += u64::from(info.capacity());
    }
    starts.push(tick);
    starts
}

/// An event of one voice with its start in ticks from the start of the score.
struct Placed {
    start: u64,
    event: Event,
}

impl Placed {
    fn end(&self) -> u64 {
        self.start + u64::from(self.event.duration.ticks())
    }
}

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
    for command in &mut commands {
        if let Command::InsertEvent { event, .. } = command
            && event.id == NEW_REST
        {
            event.id = doc.new_event_id();
        }
    }
    doc.execute(Command::Batch { commands })?;

    let moved = |tick: u64| if forward { tick + step } else { tick - step };
    Ok(Selection {
        anchor: moved(selection.anchor),
        focus: moved(selection.focus),
        ..*selection
    })
}

/// Placeholder id for rests created while planning a shift; real ids are assigned before the
/// commands run. Ids start at 1, so 0 never names an event.
const NEW_REST: crate::ids::EventId = crate::ids::EventId(0);

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
        self.starts.partition_point(|start| *start <= tick) - 1
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
        // Every event of this voice, in time order.
        let mut placed = Vec::new();
        for (measure, content) in staff.measures.iter().enumerate() {
            let mut tick = starts[measure];
            for event in content
                .voices
                .get(voice)
                .map_or(&[][..], |voice| &voice.events)
            {
                placed.push(Placed {
                    start: tick,
                    event: event.clone(),
                });
                tick += u64::from(event.duration.ticks());
            }
        }
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
            // Time after an explicit voice end, or after the events of a voice that stops early,
            // belongs to no rest.
            let limit = if voice_content.events.is_empty() {
                voice_content
                    .end_tick
                    .map_or(starts[measure + 1], |end| starts[measure] + u64::from(end))
            } else {
                starts[measure] + u64::from(voice_content.used_ticks())
            };
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

        let mut commands = Vec::new();
        for measure in first_measure..=measure_of(affected_end - 1) {
            let (measure_start, measure_end) = (starts[measure], starts[measure + 1]);
            let content = &staff.measures[measure];
            let Some(original) = content.voices.get(voice) else {
                continue;
            };
            let at = VoiceLocation {
                staff: staff_id,
                measure,
                voice,
            };
            let mut items: Vec<&Placed> = kept
                .iter()
                .chain(moved.iter())
                .filter(|item| measure_start <= item.start && item.start < measure_end)
                .collect();
            items.sort_by_key(|item| item.start);
            let limit = if original.events.is_empty() {
                original
                    .end_tick
                    .map_or(measure_end, |end| measure_start + u64::from(end))
            } else {
                measure_start + u64::from(original.used_ticks())
            }
            .max(items.last().map_or(measure_start, |item| item.end()));

            let mut events = Vec::new();
            let mut tick = measure_start;
            for item in items.iter().copied().map(Some).chain([None]) {
                let until = item.map_or(limit, |item| item.start);
                if until > tick {
                    for duration in rest_durations((until - tick) as u32)? {
                        events.push(Event::new(NEW_REST, duration, EventKind::Rest));
                    }
                }
                if let Some(item) = item {
                    events.push(item.event.clone());
                    tick = item.end();
                }
            }
            // A measure that was empty and holds only rests again stays empty.
            if original.events.is_empty()
                && events
                    .iter()
                    .all(|event| matches!(event.kind, EventKind::Rest) && event.markings.is_empty())
            {
                events.clear();
            }
            if events == original.events {
                continue;
            }
            for index in (0..original.events.len()).rev() {
                commands.push(Command::RemoveEvent { at, index });
            }
            for (index, event) in events.into_iter().enumerate() {
                commands.push(Command::InsertEvent { at, index, event });
            }
        }
        Ok(commands)
    }
}
