//! Rewriting a voice's measures from events placed in score time, shared by the time
//! operations (shift and paste).

use std::ops::RangeInclusive;

use crate::command::Command;
use crate::duration::{Duration, MAX_DOTS, NoteValue};
use crate::edit::rest_durations;
use crate::error::ModelError;
use crate::history::Document;
use crate::ids::EventId;
use crate::markings::Markings;
use crate::score::{Event, EventKind, Score, Staff, Voice, VoiceLocation};

/// Where each measure starts in ticks from the start of the score, plus the end of the score.
pub(crate) fn measure_starts(score: &Score) -> Vec<u64> {
    let mut starts = Vec::with_capacity(score.measures.len() + 1);
    let mut tick = 0;
    for info in &score.measures {
        starts.push(tick);
        tick += u64::from(info.capacity());
    }
    starts.push(tick);
    starts
}

/// The measure a score position lies in, given `measure_starts`.
pub(crate) fn measure_of(starts: &[u64], tick: u64) -> usize {
    starts.partition_point(|start| *start <= tick) - 1
}

/// Placeholder id for events created while planning an edit; `assign_ids` gives them real ids
/// before the commands run. Ids start at 1, so 0 never names an event.
pub(crate) const NEW_EVENT: EventId = EventId(0);

/// An event of one voice with its start in ticks from the start of the score.
#[derive(Debug, Clone)]
pub(crate) struct Placed {
    pub start: u64,
    pub event: Event,
}

impl Placed {
    pub fn end(&self) -> u64 {
        self.start + u64::from(self.event.duration.ticks())
    }
}

/// Every event of one voice, in time order.
pub(crate) fn voice_events(staff: &Staff, starts: &[u64], voice: usize) -> Vec<Placed> {
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
    placed
}

/// Where a voice's time ends in a measure. Time after an explicit voice end, or after the
/// events of a voice that stops early, belongs to no rest.
pub(crate) fn voice_limit(voice: &Voice, measure_start: u64, measure_end: u64) -> u64 {
    if voice.events.is_empty() {
        voice
            .end_tick
            .map_or(measure_end, |end| measure_start + u64::from(end))
    } else {
        measure_start + u64::from(voice.used_ticks())
    }
}

/// Note values that add up to `ticks`, longest first, as tied pieces of one note; dotted
/// values are used where they fit.
fn note_durations(mut ticks: u32) -> Result<Vec<Duration>, ModelError> {
    const VALUES: [NoteValue; 7] = [
        NoteValue::Whole,
        NoteValue::Half,
        NoteValue::Quarter,
        NoteValue::Eighth,
        NoteValue::Sixteenth,
        NoteValue::ThirtySecond,
        NoteValue::SixtyFourth,
    ];
    let total = ticks;
    let mut pieces = Vec::new();
    'outer: while ticks > 0 {
        for value in VALUES {
            for dots in (0..=MAX_DOTS).rev() {
                let duration = Duration::new(value, dots);
                if duration.ticks() <= ticks {
                    pieces.push(duration);
                    ticks -= duration.ticks();
                    continue 'outer;
                }
            }
        }
        // Dotted values can leave a remainder that plain values would have covered.
        return rest_durations(total);
    }
    Ok(pieces)
}

/// Places an event over `start..end`, split at barlines and into note values that fit; `end`
/// may differ from the event's own length to shorten it. Pieces of a note are tied; the first
/// keeps the event's id and the marks that start something, the last the marks that end
/// something and the event's own tie.
pub(crate) fn split_at_barlines(
    event: Event,
    start: u64,
    end: u64,
    starts: &[u64],
) -> Result<Vec<Placed>, ModelError> {
    let is_rest = matches!(event.kind, EventKind::Rest);
    let mut pieces = Vec::new();
    let mut tick = start;
    while tick < end {
        let measure_end = starts[measure_of(starts, tick) + 1].min(end);
        let length = (measure_end - tick) as u32;
        let durations = if is_rest {
            rest_durations(length)?
        } else {
            note_durations(length)?
        };
        for duration in durations {
            pieces.push((tick, duration));
            tick += u64::from(duration.ticks());
        }
    }
    if let [(_, duration)] = pieces[..]
        && duration == event.duration
    {
        return Ok(vec![Placed { start, event }]);
    }

    let last = pieces.len() - 1;
    let tied = matches!(event.kind, EventKind::Chord { tied: true, .. });
    Ok(pieces
        .into_iter()
        .enumerate()
        .map(|(index, (start, duration))| {
            let mut markings = Markings::default();
            if index == 0 {
                markings = Markings {
                    hairpin_stop: false,
                    slur_stop: false,
                    phrase_slur_stop: false,
                    ..event.markings.clone()
                };
            }
            if index == last {
                markings.hairpin_stop = event.markings.hairpin_stop;
                markings.slur_stop = event.markings.slur_stop;
                markings.phrase_slur_stop = event.markings.phrase_slur_stop;
            }
            let kind = match &event.kind {
                EventKind::Rest => EventKind::Rest,
                EventKind::Chord { pitches, .. } => EventKind::Chord {
                    pitches: pitches.clone(),
                    tied: index < last || tied,
                },
            };
            Placed {
                start,
                event: Event {
                    id: if index == 0 { event.id } else { NEW_EVENT },
                    duration,
                    kind,
                    markings,
                },
            }
        })
        .collect())
}

/// Commands that rewrite `measures` of one voice so they hold `items`, which are sorted by
/// start and each lie inside one measure. Gaps become rests with `NEW_EVENT` ids; a measure
/// that was empty and holds only rests again stays empty; unchanged measures are skipped.
pub(crate) fn rewrite_measures(
    staff: &Staff,
    starts: &[u64],
    voice: usize,
    measures: RangeInclusive<usize>,
    items: &[Placed],
) -> Result<Vec<Command>, ModelError> {
    let mut commands = Vec::new();
    for measure in measures {
        let (measure_start, measure_end) = (starts[measure], starts[measure + 1]);
        let Some(original) = staff.measures[measure].voices.get(voice) else {
            continue;
        };
        let at = VoiceLocation {
            staff: staff.id,
            measure,
            voice,
        };
        let first = items.partition_point(|item| item.start < measure_start);
        let count = items[first..].partition_point(|item| item.start < measure_end);
        let in_measure = &items[first..first + count];
        let limit = voice_limit(original, measure_start, measure_end)
            .max(in_measure.last().map_or(measure_start, Placed::end));

        let mut events = Vec::new();
        let mut tick = measure_start;
        for item in in_measure.iter().map(Some).chain([None]) {
            let until = item.map_or(limit, |item| item.start);
            if until > tick {
                for duration in rest_durations((until - tick) as u32)? {
                    events.push(Event::new(NEW_EVENT, duration, EventKind::Rest));
                }
            }
            if let Some(item) = item {
                events.push(item.event.clone());
                tick = item.end();
            }
        }
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
        // A voice that ended early but now holds more ends after its new events.
        if let Some(end) = original.end_tick {
            let used = (limit - measure_start) as u32;
            if used > end {
                commands.push(Command::SetVoiceEnd {
                    at,
                    end_tick: Some(used),
                });
            }
        }
        for (index, event) in events.into_iter().enumerate() {
            commands.push(Command::InsertEvent { at, index, event });
        }
    }
    Ok(commands)
}

/// Gives events created while planning their real ids, once the score is no longer read.
pub(crate) fn assign_ids(doc: &mut Document, commands: &mut [Command]) {
    for command in commands {
        if let Command::InsertEvent { event, .. } = command
            && event.id == NEW_EVENT
        {
            event.id = doc.new_event_id();
        }
    }
}
