//! Dynamics and hairpins, drawn on one line below each staff for the whole system.

use std::collections::HashMap;
use std::ops::Range;

use notula_model::edit::HairpinSpan;
use notula_model::{Dynamic, EventId, Hairpin, Staff};

use crate::render::Canvas;
use crate::{PlacedEvent, smufl};

const DYNAMIC_SIZE: f32 = 4.0;
/// Baseline of the dynamics line below the bottom staff line, when no notes reach lower.
const LINE_BELOW_STAFF: f32 = 2.6;
/// Baseline distance below the lowest notehead or stem end above it.
const LINE_BELOW_NOTES: f32 = 2.3;
/// Lowest baseline above the next staff, so the marks stay between the two staves.
const LINE_ABOVE_NEXT_STAFF: f32 = 0.8;
const NOTEHEAD_DEPTH: f32 = 0.5;
const STEM_LENGTH: f32 = 3.5;
/// Dynamics are centred under a notehead of this width.
const NOTEHEAD_CENTER: f32 = 0.59;
/// Hairpins sit at the height of a dynamic's lowercase letters.
const HAIRPIN_RAISE: f32 = 0.55;
const HAIRPIN_OPENING: f32 = 1.0;
const HAIRPIN_THICKNESS: f32 = 0.16;
/// Room between a hairpin and the column or dynamic after it.
const HAIRPIN_GAP: f32 = 0.6;
const DYNAMIC_GAP: f32 = 0.4;
const MIN_HAIRPIN_LENGTH: f32 = 1.0;
/// How far below the dynamics baseline its lowest glyph reaches.
pub(crate) const LINE_DEPTH: f32 = 0.7;

/// How far below a staff's top line its dynamics line reaches, when the staff's notes reach
/// down to `notes_bottom`.
pub(crate) fn line_bottom(notes_bottom: f32) -> f32 {
    (4.0 + LINE_BELOW_STAFF).max(notes_bottom + LINE_BELOW_NOTES) + LINE_DEPTH
}

/// Horizontal room a dynamic needs around its column: (left, right).
pub(crate) fn dynamic_extent(dynamic: Dynamic) -> (f32, f32) {
    let width = smufl::dynamic(dynamic).1;
    (width / 2.0 - NOTEHEAD_CENTER, width / 2.0 + NOTEHEAD_CENTER)
}

/// Where each score tick of a system lies horizontally.
#[derive(Default)]
pub(crate) struct Timeline {
    /// In drawing order; a measure's end and the next measure's start share a tick.
    points: Vec<(u64, f32)>,
}

impl Timeline {
    pub fn push(&mut self, tick: u64, x: f32) {
        self.points.push((tick, x));
    }

    fn range(&self) -> Range<u64> {
        let start = self.points.first().map_or(0, |point| point.0);
        let end = self.points.last().map_or(0, |point| point.0);
        start..end
    }

    /// Where `tick` lies; at a barline, `after` picks the following measure over the barline.
    fn x_at(&self, tick: u64, after: bool) -> Option<f32> {
        // Points are in drawing order, so the barline comes before the next measure's start.
        let exact = if after {
            self.points.iter().rfind(|point| point.0 == tick)
        } else {
            self.points.iter().find(|point| point.0 == tick)
        };
        if let Some(point) = exact {
            return Some(point.1);
        }
        let next = self.points.iter().position(|point| point.0 > tick)?;
        let (before, after) = (self.points.get(next.checked_sub(1)?)?, self.points[next]);
        let fraction = (tick - before.0) as f32 / (after.0 - before.0) as f32;
        Some(before.1 + (after.1 - before.1) * fraction)
    }

    fn start_x(&self) -> f32 {
        self.points.first().map_or(0.0, |point| point.1)
    }

    fn end_x(&self) -> f32 {
        self.points.last().map_or(0.0, |point| point.1)
    }
}

pub(crate) struct VolumeMarks<'a> {
    pub staves: &'a [&'a Staff],
    /// Top line of each staff in this system.
    pub staff_tops: Vec<f32>,
    pub measures: Range<usize>,
    pub placed: &'a [PlacedEvent],
    pub hairpins: &'a [HairpinSpan],
    pub timeline: &'a Timeline,
}

struct Dynamics {
    id: EventId,
    tick: u64,
    dynamic: Dynamic,
    x: f32,
}

impl VolumeMarks<'_> {
    /// Draws the marks and returns, for each staff that has any, how far down they reach.
    pub fn emit(&self, canvas: &mut Canvas) -> Vec<Option<f32>> {
        let mut bottoms = vec![None; self.staves.len()];
        let ticks = self.timeline.range();
        let dynamics_by_staff: Vec<Vec<Dynamics>> = self
            .staves
            .iter()
            .map(|staff| self.dynamics(staff))
            .collect();
        for (index, staff) in self.staves.iter().enumerate() {
            let dynamics = &dynamics_by_staff[index];
            let hairpins: Vec<_> = self
                .hairpins
                .iter()
                .filter(|span| {
                    span.staff == staff.id
                        && span.start_tick < ticks.end
                        && span.end_tick > ticks.start
                })
                .collect();
            if dynamics.is_empty() && hairpins.is_empty() {
                continue;
            }
            let baseline = self.baseline(index);
            for mark in dynamics {
                let (glyph, width) = smufl::dynamic(mark.dynamic);
                canvas.glyph(
                    Some(mark.id),
                    mark.x + NOTEHEAD_CENTER - width / 2.0,
                    baseline,
                    glyph,
                    DYNAMIC_SIZE,
                );
            }
            for span in hairpins {
                self.emit_hairpin(canvas, span, dynamics, baseline - HAIRPIN_RAISE);
            }
            bottoms[index] = Some(baseline + LINE_DEPTH);
        }
        bottoms
    }

    fn dynamics(&self, staff: &Staff) -> Vec<Dynamics> {
        let marks: HashMap<EventId, Dynamic> = staff.measures[self.measures.clone()]
            .iter()
            .flat_map(|measure| &measure.voices)
            .flat_map(|voice| &voice.events)
            .filter_map(|event| Some((event.id, event.markings.dynamic?)))
            .collect();
        self.placed
            .iter()
            .filter_map(|event| {
                let dynamic = *marks.get(&event.id)?;
                Some(Dynamics {
                    id: event.id,
                    tick: event.start_tick,
                    dynamic,
                    x: self.timeline.x_at(event.start_tick, true)?,
                })
            })
            .collect()
    }

    /// One line per staff and system, below the staff and every note on it, but above the
    /// next staff.
    fn baseline(&self, index: usize) -> f32 {
        let staff = self.staves[index].id;
        let below_staff = self.staff_tops[index] + 4.0 + LINE_BELOW_STAFF;
        let lowest = self
            .placed
            .iter()
            .filter(|event| event.staff == staff)
            .filter_map(|event| {
                let lowest = event
                    .notes
                    .iter()
                    .map(|note| note.y)
                    .max_by(f32::total_cmp)?;
                Some(if event.stem_up {
                    lowest + NOTEHEAD_DEPTH
                } else {
                    lowest + STEM_LENGTH
                })
            })
            .map(|bottom| bottom + LINE_BELOW_NOTES)
            .fold(below_staff, f32::max);
        match self.staff_tops.get(index + 1) {
            Some(next) => lowest.min(next - LINE_ABOVE_NEXT_STAFF).max(below_staff),
            None => lowest,
        }
    }

    fn emit_hairpin(
        &self,
        canvas: &mut Canvas,
        span: &HairpinSpan,
        dynamics: &[Dynamics],
        center: f32,
    ) {
        // Hairpins sharing an event meet in its middle.
        let previous = self
            .hairpins
            .iter()
            .find(|other| other.end == span.start && other.start != span.start);
        let next = self
            .hairpins
            .iter()
            .find(|other| other.start == span.end && other.end != span.end);
        let start = previous.map_or(span.start_tick, |other| {
            (span.start_tick + other.end_tick) / 2
        });
        let end = next.map_or(span.end_tick, |other| {
            (other.start_tick + span.end_tick) / 2
        });
        let ticks = self.timeline.range();

        let start_x = if start >= ticks.start {
            let Some(mut x) = self.timeline.x_at(start, true) else {
                return;
            };
            if previous.is_none()
                && let Some(mark) = dynamics.iter().find(|mark| mark.tick == span.start_tick)
            {
                x = x.max(mark.x + dynamic_extent(mark.dynamic).1 + DYNAMIC_GAP);
            }
            x
        } else {
            self.timeline.start_x() + HAIRPIN_GAP
        };
        let end_x = if end <= ticks.end {
            let Some(mut x) = self.timeline.x_at(end, false) else {
                return;
            };
            if next.is_none() {
                x -= HAIRPIN_GAP;
                if let Some(mark) = dynamics.iter().find(|mark| mark.tick == end) {
                    x = x.min(mark.x - dynamic_extent(mark.dynamic).0 - DYNAMIC_GAP);
                }
            }
            x
        } else {
            self.timeline.end_x() - HAIRPIN_GAP
        };
        let end_x = end_x.max(start_x + MIN_HAIRPIN_LENGTH);

        // A hairpin continued on another system keeps the opening it has there.
        let opening = |tick: u64| {
            let fraction = (tick.clamp(start, end) - start) as f32 / (end - start).max(1) as f32;
            let fraction = match span.kind {
                Hairpin::Crescendo => fraction,
                Hairpin::Diminuendo => 1.0 - fraction,
            };
            HAIRPIN_OPENING * fraction / 2.0
        };
        let first = opening(start.max(ticks.start));
        let last = opening(end.min(ticks.end));
        for side in [-1.0, 1.0] {
            canvas.line(
                Some(span.start),
                (start_x, center + side * first),
                (end_x, center + side * last),
                HAIRPIN_THICKNESS,
            );
        }
    }
}
