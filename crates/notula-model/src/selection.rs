//! Range selections: a time span across a run of staves, optionally limited to one voice.
//!
//! An anchor (where the selection started) stays fixed while keyboard steps move the focus.
//! In each selected staff and voice, an event belongs to the selection when it starts inside
//! the span; the span then grows until every selected event fits in it completely.

use serde::{Deserialize, Serialize};

use crate::ids::{EventId, StaffId};
use crate::score::Score;

/// A stretch of score time in ticks from the start of the score, end exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    /// Staff and voice whose notes keyboard steps follow.
    pub anchor_staff: StaffId,
    pub anchor_voice: usize,
    pub anchor: Span,
    /// The staff at the moving edge; staves between it and the anchor staff are selected.
    pub focus_staff: StaffId,
    pub focus: Span,
    /// Limits the selection to one voice index; `None` includes all voices.
    #[serde(default)]
    pub voice: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Next,
    Previous,
    NextMeasure,
    PreviousMeasure,
    StaffUp,
    StaffDown,
}

/// The part of one measure a selection covers, in ticks within that measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeasureSpan {
    pub measure: usize,
    pub start: u32,
    pub end: u32,
    /// The span runs to the end of the measure.
    pub to_end: bool,
}

/// What a selection covers in the current score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedSelection {
    /// Selected staves in score order.
    pub staves: Vec<StaffId>,
    /// The span after growing to whole events.
    pub span: Span,
    pub measures: Vec<MeasureSpan>,
    pub events: Vec<EventId>,
}

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

fn staff_index(score: &Score, staff: StaffId) -> Option<usize> {
    score.staves().position(|candidate| candidate.id == staff)
}

/// The steps keyboard extension moves through in one voice: each event, and each measure
/// part the voice leaves empty.
fn voice_units(score: &Score, staff: StaffId, voice: usize) -> Vec<Span> {
    let Ok(staff) = score.staff(staff) else {
        return Vec::new();
    };
    let starts = measure_starts(score);
    let mut units = Vec::new();
    for (measure, content) in staff.measures.iter().enumerate() {
        let (start, end) = (starts[measure], starts[measure + 1]);
        let mut tick = start;
        for event in content
            .voices
            .get(voice)
            .map_or(&[][..], |voice| &voice.events)
        {
            let next = tick + u64::from(event.duration.ticks());
            units.push(Span {
                start: tick,
                end: next,
            });
            tick = next;
        }
        if tick < end {
            units.push(Span { start: tick, end });
        }
    }
    units
}

impl Selection {
    /// A selection of the event or empty measure part at a position in one voice.
    pub fn at(
        score: &Score,
        staff: StaffId,
        voice: usize,
        measure: usize,
        tick: u32,
    ) -> Option<Self> {
        let starts = measure_starts(score);
        let position = starts.get(measure)? + u64::from(tick);
        let unit = voice_units(score, staff, voice)
            .into_iter()
            .find(|unit| unit.start <= position && position < unit.end)?;
        Some(Self {
            anchor_staff: staff,
            anchor_voice: voice,
            anchor: unit,
            focus_staff: staff,
            focus: unit,
            voice: None,
        })
    }

    /// A selection of one event.
    pub fn for_event(score: &Score, event: EventId) -> Option<Self> {
        let located = crate::edit::locate(score, event)?;
        Self::at(
            score,
            located.cursor.staff,
            located.cursor.voice,
            located.cursor.measure,
            located.cursor.tick,
        )
    }

    /// Every staff and the whole length of the score.
    pub fn all(score: &Score) -> Option<Self> {
        let first = score.staves().next()?.id;
        let last = score.staves().last()?.id;
        let end = *measure_starts(score).last()?;
        Some(Self {
            anchor_staff: first,
            anchor_voice: 0,
            anchor: Span { start: 0, end },
            focus_staff: last,
            focus: Span { start: 0, end },
            voice: None,
        })
    }

    /// A run of staves and a span of time, as drawn with the mouse.
    pub fn range(first_staff: StaffId, last_staff: StaffId, span: Span) -> Self {
        Self {
            anchor_staff: first_staff,
            anchor_voice: 0,
            anchor: span,
            focus_staff: last_staff,
            focus: span,
            voice: None,
        }
    }

    pub fn span(&self) -> Span {
        Span {
            start: self.anchor.start.min(self.focus.start),
            end: self.anchor.end.max(self.focus.end),
        }
    }

    /// Moves the focus by a note, a measure, or a staff.
    pub fn extend(&self, score: &Score, step: Step) -> Self {
        let mut next = *self;
        match step {
            Step::Next | Step::Previous => {
                let units = voice_units(score, self.anchor_staff, self.anchor_voice);
                let current = units
                    .iter()
                    .position(|unit| unit.start <= self.focus.start && self.focus.start < unit.end);
                let target = match (current, step) {
                    (Some(index), Step::Next) => units.get(index + 1),
                    (Some(index), _) => index.checked_sub(1).and_then(|index| units.get(index)),
                    (None, _) => None,
                };
                if let Some(unit) = target {
                    next.focus = *unit;
                }
            }
            Step::NextMeasure | Step::PreviousMeasure => {
                // The moving edge jumps to a barline, like word steps in a text editor; it
                // crosses the anchor when it has to.
                let boundaries = measure_starts(score);
                let units = voice_units(score, self.anchor_staff, self.anchor_voice);
                let ending_at = |boundary: u64| units.iter().find(|unit| unit.end == boundary);
                let starting_at = |boundary: u64| units.iter().find(|unit| unit.start == boundary);
                let after = |tick: u64| boundaries.iter().copied().find(|b| *b > tick);
                let before = |tick: u64| boundaries.iter().copied().rev().find(|b| *b < tick);
                let right_of_anchor = self.focus.start >= self.anchor.start;
                let forward = step == Step::NextMeasure;
                let target = match (forward, right_of_anchor) {
                    (true, true) => after(self.focus.end).and_then(ending_at),
                    (false, false) => before(self.focus.start).and_then(starting_at),
                    (false, true) => before(self.focus.end).and_then(|b| {
                        if b >= self.anchor.end {
                            ending_at(b)
                        } else {
                            // Back past the anchor: select from its measure start instead.
                            let start = boundaries
                                .iter()
                                .copied()
                                .rev()
                                .find(|b| *b <= self.anchor.start)?;
                            starting_at(start)
                        }
                    }),
                    (true, false) => after(self.focus.start).and_then(|b| {
                        if b <= self.anchor.start {
                            starting_at(b)
                        } else {
                            let end = boundaries.iter().copied().find(|b| *b >= self.anchor.end)?;
                            ending_at(end)
                        }
                    }),
                };
                if let Some(unit) = target {
                    next.focus = *unit;
                }
            }
            Step::StaffUp | Step::StaffDown => {
                let staves: Vec<_> = score.staves().map(|staff| staff.id).collect();
                if let Some(index) = staves.iter().position(|id| *id == self.focus_staff) {
                    let target = if step == Step::StaffDown {
                        staves.get(index + 1)
                    } else {
                        index.checked_sub(1).and_then(|index| staves.get(index))
                    };
                    if let Some(staff) = target {
                        next.focus_staff = *staff;
                    }
                }
            }
        }
        next
    }

    /// Moves the focus to an event, possibly in another staff, as with Shift+click.
    pub fn extend_to_event(&self, score: &Score, event: EventId) -> Option<Self> {
        let target = Self::for_event(score, event)?;
        Some(Self {
            focus_staff: target.anchor_staff,
            focus: target.anchor,
            ..*self
        })
    }

    /// The staves, events, and grown span this selection covers in `score`.
    pub fn resolve(&self, score: &Score) -> Option<ResolvedSelection> {
        let first = staff_index(score, self.anchor_staff)?;
        let last = staff_index(score, self.focus_staff)?;
        let (first, last) = (first.min(last), first.max(last));
        let staves: Vec<_> = score.staves().skip(first).take(last - first + 1).collect();
        let starts = measure_starts(score);

        // Every selected event with its span, in score order.
        let mut candidates = Vec::new();
        for staff in &staves {
            for (measure, content) in staff.measures.iter().enumerate() {
                for (voice_index, voice) in content.voices.iter().enumerate() {
                    if self.voice.is_some_and(|only| only != voice_index) {
                        continue;
                    }
                    let mut tick = starts[measure];
                    for event in &voice.events {
                        let end = tick + u64::from(event.duration.ticks());
                        candidates.push((event.id, tick, end));
                        tick = end;
                    }
                }
            }
        }

        // Grow until every event starting inside also ends inside.
        let mut span = self.span();
        loop {
            let end = candidates
                .iter()
                .filter(|(_, start, _)| span.start <= *start && *start < span.end)
                .map(|(_, _, end)| *end)
                .fold(span.end, u64::max);
            if end == span.end {
                break;
            }
            span.end = end;
        }
        let events = candidates
            .iter()
            .filter(|(_, start, _)| span.start <= *start && *start < span.end)
            .map(|(id, _, _)| *id)
            .collect();

        let measures = (0..score.measures.len())
            .filter(|&measure| starts[measure] < span.end && span.start < starts[measure + 1])
            .map(|measure| MeasureSpan {
                measure,
                start: (span.start.max(starts[measure]) - starts[measure]) as u32,
                end: (span.end.min(starts[measure + 1]) - starts[measure]) as u32,
                to_end: span.end >= starts[measure + 1],
            })
            .collect();
        Some(ResolvedSelection {
            staves: staves.iter().map(|staff| staff.id).collect(),
            span,
            measures,
            events,
        })
    }
}
