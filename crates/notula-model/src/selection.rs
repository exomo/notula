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

/// Like a text selection: everything between two positions in score time, across the staves
/// from the anchor staff to the focus staff. With both positions equal it selects no notes and
/// is a cursor spanning those staves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    /// Staff and voice whose notes keyboard steps follow; where the selection started.
    pub anchor_staff: StaffId,
    pub anchor_voice: usize,
    /// The fixed position, in ticks from the start of the score.
    pub anchor: u64,
    /// The staff at the moving edge; staves between it and the anchor staff are selected.
    pub focus_staff: StaffId,
    /// The moving position, where the cursor goes.
    pub focus: u64,
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
    /// The span after growing to whole events; empty for a cursor spanning staves.
    pub span: Span,
    pub measures: Vec<MeasureSpan>,
    pub events: Vec<EventId>,
    /// Where the cursor goes: the moving edge, after growing.
    pub caret: u64,
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

/// Measure and tick of a score position. A position on a barline belongs to the following
/// measure, except at the end of the score.
pub fn position(score: &Score, tick: u64) -> (usize, u32) {
    let starts = measure_starts(score);
    let last = score.measures.len() - 1;
    let measure = starts[..=last].partition_point(|start| *start <= tick) - 1;
    (
        measure,
        (tick - starts[measure]).min(u64::from(u32::MAX)) as u32,
    )
}

/// Ticks from the start of the score to a measure and tick.
pub fn absolute(score: &Score, measure: usize, tick: u32) -> Option<u64> {
    Some(measure_starts(score).get(measure)? + u64::from(tick))
}

fn staff_index(score: &Score, staff: StaffId) -> Option<usize> {
    score.staves().position(|candidate| candidate.id == staff)
}

/// Positions keyboard steps stop at in one voice: the start and end of each event, and the
/// barlines, so a measure part the voice leaves empty is one step.
fn voice_boundaries(score: &Score, staff: StaffId, voice: usize) -> Vec<u64> {
    let starts = measure_starts(score);
    let mut boundaries = starts.clone();
    if let Ok(staff) = score.staff(staff) {
        for (measure, content) in staff.measures.iter().enumerate() {
            let mut tick = starts[measure];
            for event in content
                .voices
                .get(voice)
                .map_or(&[][..], |voice| &voice.events)
            {
                tick += u64::from(event.duration.ticks());
                boundaries.push(tick);
            }
        }
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    boundaries
}

impl Selection {
    /// A cursor at a position, covering one staff and no notes.
    pub fn caret(
        score: &Score,
        staff: StaffId,
        voice: usize,
        measure: usize,
        tick: u32,
    ) -> Option<Self> {
        score.staff(staff).ok()?;
        let position = absolute(score, measure, tick)?;
        Some(Self {
            anchor_staff: staff,
            anchor_voice: voice,
            anchor: position,
            focus_staff: staff,
            focus: position,
            voice: None,
        })
    }

    /// A selection of exactly one event, with the cursor after it.
    pub fn for_event(score: &Score, event: EventId) -> Option<Self> {
        let located = crate::edit::locate(score, event)?;
        let staff = score.staff(located.cursor.staff).ok()?;
        let duration = staff.measures[located.cursor.measure].voices[located.cursor.voice].events
            [located.index]
            .duration
            .ticks();
        let start = absolute(score, located.cursor.measure, located.cursor.tick)?;
        Some(Self {
            anchor_staff: located.cursor.staff,
            anchor_voice: located.cursor.voice,
            anchor: start,
            focus_staff: located.cursor.staff,
            focus: start + u64::from(duration),
            voice: None,
        })
    }

    /// Every staff and the whole length of the score.
    pub fn all(score: &Score) -> Option<Self> {
        let first = score.staves().next()?.id;
        let last = score.staves().last()?.id;
        let end = *measure_starts(score).last()?;
        Some(Self {
            anchor_staff: first,
            anchor_voice: 0,
            anchor: 0,
            focus_staff: last,
            focus: end,
            voice: None,
        })
    }

    /// A run of staves and a span of time, as drawn with the mouse; the cursor goes to its end.
    pub fn range(first_staff: StaffId, last_staff: StaffId, span: Span) -> Self {
        Self {
            anchor_staff: first_staff,
            anchor_voice: 0,
            anchor: span.start,
            focus_staff: last_staff,
            focus: span.end,
            voice: None,
        }
    }

    /// The same range with the moving edge on the other side, as when drawn right to left.
    pub fn reversed(&self) -> Self {
        Self {
            anchor: self.focus,
            focus: self.anchor,
            ..*self
        }
    }

    pub fn span(&self) -> Span {
        Span {
            start: self.anchor.min(self.focus),
            end: self.anchor.max(self.focus),
        }
    }

    /// No notes are selected: a cursor, possibly spanning several staves.
    pub fn is_caret(&self) -> bool {
        self.anchor == self.focus
    }

    /// The same staves and voice, collapsed to a cursor at `position`.
    pub fn collapsed_to(&self, position: u64) -> Self {
        Self {
            anchor: position,
            focus: position,
            ..*self
        }
    }

    /// Moves the focus by a note, to a barline, or by a staff.
    pub fn extend(&self, score: &Score, step: Step) -> Self {
        let mut next = *self;
        let stops = match step {
            Step::Next | Step::Previous => {
                voice_boundaries(score, self.anchor_staff, self.anchor_voice)
            }
            Step::NextMeasure | Step::PreviousMeasure => measure_starts(score),
            Step::StaffUp | Step::StaffDown => Vec::new(),
        };
        match step {
            Step::Next | Step::NextMeasure => {
                if let Some(stop) = stops.iter().copied().find(|stop| *stop > self.focus) {
                    next.focus = stop;
                }
            }
            Step::Previous | Step::PreviousMeasure => {
                if let Some(stop) = stops.iter().copied().rev().find(|stop| *stop < self.focus) {
                    next.focus = stop;
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

    /// Moves the focus to take in an event, possibly in another staff, as with Shift+click: to
    /// its end when it lies after the anchor, otherwise to its start.
    pub fn extend_to_event(&self, score: &Score, event: EventId) -> Option<Self> {
        let target = Self::for_event(score, event)?;
        Some(Self {
            focus_staff: target.anchor_staff,
            focus: if target.anchor >= self.anchor {
                target.focus
            } else {
                target.anchor
            },
            ..*self
        })
    }

    /// The staves, events, grown span, and cursor position this selection has in `score`.
    pub fn resolve(&self, score: &Score) -> Option<ResolvedSelection> {
        let first = staff_index(score, self.anchor_staff)?;
        let last = staff_index(score, self.focus_staff)?;
        let (first, last) = (first.min(last), first.max(last));
        let staves: Vec<_> = score.staves().skip(first).take(last - first + 1).collect();
        let starts = measure_starts(score);
        let mut span = self.span();
        if self.is_caret() {
            return Some(ResolvedSelection {
                staves: staves.iter().map(|staff| staff.id).collect(),
                span,
                measures: Vec::new(),
                events: Vec::new(),
                caret: self.focus,
            });
        }

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
            caret: if self.focus >= self.anchor {
                span.end
            } else {
                span.start
            },
        })
    }
}
