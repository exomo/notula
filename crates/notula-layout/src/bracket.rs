//! Horizontal brackets over or under a group of notes, with hooks towards the notes.

use notula_model::edit::BracketSpan;
use notula_model::{BracketLine, EventId, StaffId, TextPlacement};

use crate::PlacedEvent;
use crate::render::Canvas;
use crate::volume::Timeline;

/// Room between a bracket and the notes or staff it spans.
const GAP: f32 = 1.0;
const HOOK: f32 = 0.8;
const THICKNESS: f32 = 0.12;
const DASH: f32 = 0.7;
const DASH_GAP: f32 = 0.5;
/// Room between the ends of a bracket and its end notes.
const END_ROOM: f32 = 0.2;
/// How much further a staff's content reaches with a bracket beside it.
pub(crate) const ROOM: f32 = GAP + HOOK;

/// A bracket placed in one system, before it is drawn.
pub(crate) struct Placed {
    pub event: EventId,
    pub staff: StaffId,
    pub above: bool,
    pub line: BracketLine,
    pub x0: f32,
    pub x1: f32,
    /// Height of the line.
    pub y: f32,
    pub start_hook: bool,
    pub end_hook: bool,
}

impl Placed {
    /// How far the bracket reaches away from the staff: its top above, its bottom below.
    pub fn outer(&self) -> f32 {
        if self.above {
            self.y - THICKNESS / 2.0
        } else {
            self.y + THICKNESS / 2.0
        }
    }

    pub fn overlaps(&self, x0: f32, x1: f32) -> bool {
        self.x0 <= x1 && x0 <= self.x1
    }

    pub fn emit(&self, canvas: &mut Canvas) {
        let (x0, x1, y) = (self.x0, self.x1, self.y);
        match self.line {
            BracketLine::Solid => canvas.line(Some(self.event), (x0, y), (x1, y), THICKNESS),
            BracketLine::Dashed => {
                let mut x = x0;
                while x < x1 {
                    let end = (x + DASH).min(x1);
                    canvas.line(Some(self.event), (x, y), (end, y), THICKNESS);
                    x = end + DASH_GAP;
                }
            }
        }
        let hook = if self.above { HOOK } else { -HOOK };
        let half = THICKNESS / 2.0;
        let edge = if self.above { -half } else { half };
        if self.start_hook {
            let x = x0 + half;
            canvas.line(Some(self.event), (x, y + edge), (x, y + hook), THICKNESS);
        }
        if self.end_hook {
            let x = x1 - half;
            canvas.line(Some(self.event), (x, y + edge), (x, y + hook), THICKNESS);
        }
    }
}

/// Places the brackets of one system beside the events they span: above them, or below
/// them, clear of the staff. A bracket continued from or onto another system runs to the
/// system's edge there, without a hook.
pub(crate) fn place(
    spans: &[BracketSpan],
    placed: &[PlacedEvent],
    staff_tops: &[(StaffId, f32)],
    timeline: &Timeline,
) -> Vec<Placed> {
    let ticks = timeline.range();
    spans
        .iter()
        .filter(|span| span.start_tick < ticks.end && span.end_tick > ticks.start)
        .filter_map(|span| {
            let &(staff, staff_top) = staff_tops.iter().find(|(id, _)| *id == span.staff)?;
            let start = placed.iter().find(|event| event.id == span.start);
            let end = placed.iter().find(|event| event.id == span.end);
            let x0 = start.map_or(timeline.start_x() + END_ROOM, |event| event.left - END_ROOM);
            let x1 = end.map_or(timeline.end_x() - END_ROOM, |event| event.right + END_ROOM);
            let covered = placed.iter().filter(|event| {
                event.staff == staff
                    && event.start_tick >= span.start_tick
                    && event.start_tick < span.end_tick
            });
            let above = span.bracket.placement == TextPlacement::Above;
            let y = if above {
                covered.map(|event| event.top).fold(staff_top, f32::min) - GAP
            } else {
                covered
                    .map(|event| event.bottom)
                    .fold(staff_top + 4.0, f32::max)
                    + GAP
            };
            Some(Placed {
                event: span.start,
                staff,
                above,
                line: span.bracket.line,
                x0,
                x1: x1.max(x0 + 1.0),
                y,
                start_hook: span.bracket.start_hook && start.is_some(),
                end_hook: span.bracket.end_hook && end.is_some(),
            })
        })
        .collect()
}
