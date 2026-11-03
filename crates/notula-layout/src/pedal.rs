//! Sustain pedal marks, drawn on one line below each staff for the whole system, under its
//! dynamics.

use notula_model::PedalStyle;
use notula_model::edit::PedalSpan;

use crate::PlacedEvent;
use crate::render::Canvas;
use crate::volume::Timeline;

pub(crate) const PEDAL_SIZE: f32 = 3.2;
pub(crate) const PED: char = '\u{E650}';
pub(crate) const PEDAL_UP: char = '\u{E655}';
/// Bravura's "Ped." and "✱" at `PEDAL_SIZE`.
pub(crate) const PED_WIDTH: f32 = 4.076 * PEDAL_SIZE / 4.0;
const PED_HEIGHT: f32 = 2.22 * PEDAL_SIZE / 4.0;
pub(crate) const UP_WIDTH: f32 = 1.8 * PEDAL_SIZE / 4.0;
/// Top of the pedal line below the bottom staff line, when nothing reaches lower.
const BELOW_STAFF: f32 = 1.6;
/// Room between notes or dynamics and the pedal line below them.
const BELOW_MARKS: f32 = 0.8;
/// How far the lowest pedal mark reaches below its baseline.
const DEPTH: f32 = 0.15;
/// Height of a pedal bracket's hooks and of the notch where the pedal is changed.
const HOOK: f32 = 1.1;
/// Half the width of the notch where a bracket's pedal is changed.
const NOTCH: f32 = 0.6;
const LINE_THICKNESS: f32 = 0.14;
/// Room between a release mark or hook and the column after it.
const RELEASE_GAP: f32 = 0.3;
/// Room between "Ped." and the line after it.
const TEXT_GAP: f32 = 0.3;
const MIN_LINE: f32 = 0.8;
/// Marks are aligned with the middle of a notehead of this width.
const NOTEHEAD_CENTER: f32 = 0.59;

/// How far below a staff's top line its pedal line reaches, when its notes and dynamics reach
/// down to `marks_bottom`.
pub(crate) fn line_bottom(marks_bottom: f32) -> f32 {
    (4.0 + BELOW_STAFF).max(marks_bottom + BELOW_MARKS) + PED_HEIGHT + DEPTH
}

/// Horizontal room a pedal start in this style needs after its column.
pub(crate) fn start_extent(style: PedalStyle) -> f32 {
    match style {
        PedalStyle::Text | PedalStyle::TextLine => PED_WIDTH,
        PedalStyle::Line => 0.0,
    }
}

/// Horizontal room a release needs before the column after it.
pub(crate) fn release_extent() -> f32 {
    UP_WIDTH + RELEASE_GAP
}

pub(crate) struct PedalMarks<'a> {
    pub staff: notula_model::StaffId,
    pub staff_top: f32,
    /// Bottom of the staff's dynamics line, if it has one in this system.
    pub dynamics_bottom: Option<f32>,
    /// How far the staff's notes, articulations, and fermatas reach down in this system.
    pub marks_bottom: f32,
    pub placed: &'a [PlacedEvent],
    pub pedals: &'a [PedalSpan],
    pub timeline: &'a Timeline,
}

impl PedalMarks<'_> {
    /// Draws the staff's pedals in this system and returns how far down they reach.
    pub fn emit(&self, canvas: &mut Canvas) -> Option<f32> {
        let ticks = self.timeline.range();
        let spans: Vec<&PedalSpan> = self
            .pedals
            .iter()
            .filter(|span| {
                span.staff == self.staff
                    && span.start_tick < ticks.end
                    && span.end_tick > ticks.start
            })
            .collect();
        if spans.is_empty() {
            return None;
        }
        let notes_bottom = self
            .placed
            .iter()
            .filter(|event| event.staff == self.staff)
            .map(|event| event.bottom)
            .fold(self.marks_bottom, f32::max);
        let marks_bottom = self
            .dynamics_bottom
            .map_or(notes_bottom, |line| line.max(notes_bottom));
        let baseline =
            (self.staff_top + 4.0 + BELOW_STAFF).max(marks_bottom + BELOW_MARKS) + PED_HEIGHT;
        for span in spans {
            self.emit_span(canvas, span, baseline);
        }
        Some(baseline + DEPTH)
    }

    fn emit_span(&self, canvas: &mut Canvas, span: &PedalSpan, baseline: f32) {
        let ticks = self.timeline.range();
        let id = Some(span.start);
        // A change: the previous pedal in this voice comes up where this one goes down.
        // A text pedal next to a bracket is drawn as a release and a new pedal, and a pedal
        // whose release is hidden is not changed into the next.
        let text_only = |style| style == PedalStyle::Text;
        let adjacent = |other: &PedalSpan| {
            other.staff == span.staff
                && other.voice == span.voice
                && text_only(other.style) == text_only(span.style)
        };
        let changed_from = span.start_tick > ticks.start
            && self.pedals.iter().any(|other| {
                adjacent(other) && !other.release_hidden && other.end_tick == span.start_tick
            });
        let changed_into = span.end_tick < ticks.end
            && !span.release_hidden
            && self
                .pedals
                .iter()
                .any(|other| adjacent(other) && other.start_tick == span.end_tick);
        let starts_here = span.start_tick >= ticks.start;
        let ends_here = span.end_tick <= ticks.end;

        let Some(start_x) = (if starts_here {
            self.timeline.x_at(span.start_tick, true)
        } else {
            Some(self.timeline.start_x())
        }) else {
            return;
        };
        // A release goes before the next column, ahead of a barline there; the notch of a
        // change peaks over the note it changes at, after a barline.
        let Some(release_x) = (if ends_here {
            self.timeline
                .x_at(
                    span.end_tick,
                    changed_into && span.style != PedalStyle::Text,
                )
                .map(|x| if changed_into { x } else { x - RELEASE_GAP })
        } else {
            Some(self.timeline.end_x())
        }) else {
            return;
        };

        // "Ped." starts each pedal of the text styles; with a line, a change is only a notch.
        let text = starts_here
            && match span.style {
                PedalStyle::Text => true,
                PedalStyle::TextLine => !changed_from,
                PedalStyle::Line => false,
            };
        if text {
            canvas.glyph(id, start_x, baseline, PED, PEDAL_SIZE);
        }
        let released = ends_here && !span.release_hidden;
        if span.style == PedalStyle::Text {
            if released {
                let column = if changed_into {
                    release_x - RELEASE_GAP
                } else {
                    release_x
                };
                let x = (column - UP_WIDTH).max(start_x + PED_WIDTH);
                canvas.glyph(id, x, baseline, PEDAL_UP, PEDAL_SIZE);
            }
            return;
        }

        let top = baseline - HOOK;
        let line_start = if text {
            start_x + PED_WIDTH + TEXT_GAP
        } else if starts_here && changed_from {
            // The notch's falling half, from its peak over the changing note.
            let peak = start_x + NOTEHEAD_CENTER;
            canvas.line(id, (peak, top), (peak + NOTCH, baseline), LINE_THICKNESS);
            peak + NOTCH
        } else if starts_here {
            canvas.line(id, (start_x, top), (start_x, baseline), LINE_THICKNESS);
            start_x
        } else {
            start_x
        };
        let line_end = if ends_here && changed_into {
            let peak = release_x + NOTEHEAD_CENTER;
            canvas.line(id, (peak - NOTCH, baseline), (peak, top), LINE_THICKNESS);
            peak - NOTCH
        } else {
            release_x
        };
        let line_end = line_end.max(line_start + MIN_LINE);
        canvas.line(
            id,
            (line_start, baseline),
            (line_end, baseline),
            LINE_THICKNESS,
        );
        if released && !changed_into {
            canvas.line(id, (line_end, baseline), (line_end, top), LINE_THICKNESS);
        }
    }
}
