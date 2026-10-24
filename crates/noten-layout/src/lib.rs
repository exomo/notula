//! Engraving layout: turns a score into a list of drawing primitives.

mod emit;
mod engrave;
mod measure;
mod prefix;
mod render;
mod smufl;
mod staff;

use std::ops::Range;

use noten_model::{NoteValue, Pitch, Score, Staff, StaffId};

use measure::{MeasureMetrics, measure_metrics};
use prefix::Prefix;
use render::Canvas;
pub use render::{Anchor, Item, MeasureArea, RenderList, Shape};

pub struct LayoutOptions {
    /// Available width in staff spaces; systems are broken to fit.
    pub width: f32,
}

const MARGIN: f32 = 2.0;
const VERTICAL_MARGIN: f32 = 5.0;
const STAFF_HEIGHT: f32 = 4.0;
const STAFF_PITCH_IN_PART: f32 = 9.0;
const STAFF_PITCH_BETWEEN_PARTS: f32 = 10.0;
const SYSTEM_GAP: f32 = 8.0;
const NAME_SIZE: f32 = 2.0;
const NAME_CHAR_WIDTH: f32 = 1.1;
const NAME_GAP: f32 = 0.6;
const BRACE_ROOM: f32 = 1.8;
const BRACE_GAP: f32 = 0.4;

/// The longest note value that fits, used to rest through a shortened measure.
fn rest_value_for(ticks: u32) -> NoteValue {
    [
        NoteValue::Whole,
        NoteValue::Half,
        NoteValue::Quarter,
        NoteValue::Eighth,
        NoteValue::Sixteenth,
        NoteValue::ThirtySecond,
    ]
    .into_iter()
    .find(|value| value.ticks() <= ticks)
    .unwrap_or(NoteValue::SixtyFourth)
}

/// Vertical arrangement of the staves within a system.
struct Rows {
    /// Top line of each staff relative to the top of the system.
    y: Vec<f32>,
    /// Rows belonging to each part.
    parts: Vec<Range<usize>>,
    height: f32,
}

impl Rows {
    fn new(score: &Score) -> Self {
        let mut y = Vec::new();
        let mut parts = Vec::new();
        let mut cursor = 0.0;
        for part in &score.parts {
            let start = y.len();
            for staff_index in 0..part.staves.len() {
                if !y.is_empty() {
                    cursor += if staff_index == 0 {
                        STAFF_PITCH_BETWEEN_PARTS
                    } else {
                        STAFF_PITCH_IN_PART
                    };
                }
                y.push(cursor);
            }
            parts.push(start..y.len());
        }
        let height = y.last().map_or(0.0, |last| last + STAFF_HEIGHT);
        Self { y, parts, height }
    }

    fn has_braces(&self) -> bool {
        self.parts.iter().any(|r| r.len() > 1)
    }
}

fn prefix_for(score: &Score, measure: usize, first_in_system: bool) -> Prefix {
    let info = score.measures[measure];
    let previous = measure.checked_sub(1).map(|p| score.measures[p]);
    let key_changed = previous.is_some_and(|p| p.key_signature != info.key_signature);
    let time_changed = previous.is_none_or(|p| p.time_signature != info.time_signature);
    Prefix {
        clef: first_in_system,
        key: info.key_signature.fifths != 0 && (first_in_system || key_changed),
        time: time_changed,
    }
}

struct System {
    measures: Range<usize>,
    /// Natural width including clef and signatures.
    width: f32,
}

/// Fills each system with as many measures as fit, but at least one.
fn break_systems(
    score: &Score,
    metrics: &[MeasureMetrics],
    first_width: f32,
    other_width: f32,
) -> Vec<System> {
    let measure_width = |m: usize, first: bool| {
        prefix_for(score, m, first).width(score.measures[m]) + metrics[m].body_width
    };
    let mut systems = Vec::new();
    let mut start = 0;
    while start < metrics.len() {
        let available = if systems.is_empty() {
            first_width
        } else {
            other_width
        };
        let mut width = measure_width(start, true);
        let mut end = start + 1;
        while end < metrics.len() {
            let next = measure_width(end, false);
            if width + next > available {
                break;
            }
            width += next;
            end += 1;
        }
        systems.push(System {
            measures: start..end,
            width,
        });
        start = end;
    }
    systems
}

struct Layouter<'a> {
    score: &'a Score,
    staves: Vec<&'a Staff>,
    metrics: Vec<MeasureMetrics>,
    rows: Rows,
}

pub fn layout(score: &Score, options: &LayoutOptions) -> RenderList {
    let staves: Vec<&Staff> = score.staves().collect();
    if staves.is_empty() {
        return RenderList {
            width: options.width,
            height: 2.0 * VERTICAL_MARGIN,
            items: Vec::new(),
            areas: Vec::new(),
        };
    }
    let metrics = (0..score.measures.len())
        .map(|m| measure_metrics(score, &staves, m))
        .collect();
    let layouter = Layouter {
        score,
        staves,
        metrics,
        rows: Rows::new(score),
    };
    layouter.run(options.width)
}

/// What lies under a point on the page, in terms of the score.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub staff: StaffId,
    pub measure: usize,
    pub next_measure: Option<usize>,
    /// Onset of the nearest point in time, or `None` past the last one (the end of the measure).
    pub tick: Option<u32>,
    /// The pitch at this height on the staff, spelled according to the key signature.
    pub pitch: Pitch,
}

/// Staff spaces beyond the last column that still count as that column.
const END_OF_MEASURE_REACH: f32 = 2.5;
/// How far from the middle of a staff a click still belongs to it.
const STAFF_REACH: f32 = 4.5;

pub fn hit_test(score: &Score, list: &RenderList, x: f32, y: f32) -> Option<Hit> {
    let area = list
        .areas
        .iter()
        .filter(|a| x >= a.x_start - 1.0 && x <= a.x_end)
        .map(|a| {
            let horizontal_distance = if x < a.x_start {
                a.x_start - x
            } else if x > a.x_end {
                x - a.x_end
            } else {
                0.0
            };
            (a, (y - (a.y_top + 2.0)).abs(), horizontal_distance)
        })
        .filter(|(_, vertical_distance, _)| *vertical_distance <= STAFF_REACH)
        .min_by(|a, b| {
            a.1.total_cmp(&b.1)
                .then_with(|| a.2.total_cmp(&b.2))
                .then_with(|| b.0.x_start.total_cmp(&a.0.x_start))
        })?
        .0;

    let staff = score.staff(area.staff).ok()?;
    let key = score.measures.get(area.measure)?.key_signature;
    let near_measure_end = x >= area.x_end - END_OF_MEASURE_REACH;
    let tick = match area.columns.last() {
        Some(&(_, last_x)) if !near_measure_end && x <= last_x + END_OF_MEASURE_REACH => area
            .columns
            .iter()
            .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
            .map(|&(tick, _)| tick),
        _ => None,
    };
    Some(Hit {
        staff: area.staff,
        measure: area.measure,
        next_measure: area.next_measure,
        tick,
        pitch: staff::pitch_at(y - area.y_top, staff.clef, key),
    })
}

impl Layouter<'_> {
    fn run(&self, width: f32) -> RenderList {
        let score = self.score;
        let name_chars = score
            .parts
            .iter()
            .map(|p| p.name.chars().count())
            .max()
            .unwrap_or(0);
        let names_width = if name_chars > 0 {
            name_chars as f32 * NAME_CHAR_WIDTH + NAME_GAP
        } else {
            0.0
        };
        let first_x = MARGIN + names_width + self.brace_room();
        let other_x = MARGIN + self.brace_room();
        let right = width - MARGIN;

        let systems = break_systems(score, &self.metrics, right - first_x, right - other_x);

        let mut canvas = Canvas::default();
        let mut y = VERTICAL_MARGIN;
        for (index, system) in systems.iter().enumerate() {
            let x_start = if index == 0 { first_x } else { other_x };
            let is_last = index + 1 == systems.len();
            let body: f32 = system
                .measures
                .clone()
                .map(|m| self.metrics[m].body_width)
                .sum();
            let fixed = system.width - body;
            let available = right - x_start;
            // Only full systems are justified; the last one keeps its natural spacing.
            let stretch = if !is_last && body > 0.0 && available > system.width {
                (available - fixed) / body
            } else {
                1.0
            };
            let x_end = x_start + fixed + body * stretch;
            self.emit_system(
                &mut canvas,
                system,
                (x_start, x_end),
                y,
                stretch,
                index == 0,
            );
            y += self.rows.height + SYSTEM_GAP;
        }

        RenderList {
            width,
            height: y - SYSTEM_GAP + VERTICAL_MARGIN,
            items: canvas.items,
            areas: canvas.areas,
        }
    }

    fn brace_room(&self) -> f32 {
        if self.rows.has_braces() {
            BRACE_ROOM
        } else {
            0.0
        }
    }

    fn emit_system(
        &self,
        canvas: &mut Canvas,
        system: &System,
        (x_start, x_end): (f32, f32),
        y_top: f32,
        stretch: f32,
        first_system: bool,
    ) {
        let rows = &self.rows;
        for &row in &rows.y {
            for line in 0..5 {
                let y = y_top + row + line as f32;
                canvas.line(None, (x_start, y), (x_end, y), smufl::STAFF_LINE_THICKNESS);
            }
        }

        if rows.y.len() > 1 {
            let bottom = y_top + rows.height;
            canvas.line(
                None,
                (x_start, y_top),
                (x_start, bottom),
                smufl::THIN_BARLINE,
            );
        }
        for (part, range) in self.score.parts.iter().zip(&rows.parts) {
            if range.is_empty() {
                continue;
            }
            let top = y_top + rows.y[range.start];
            let bottom = y_top + rows.y[range.end - 1] + STAFF_HEIGHT;
            if range.len() > 1 {
                let height = bottom - top;
                let brace_width = smufl::BRACE_WIDTH_PER_HEIGHT * height;
                canvas.glyph(
                    None,
                    x_start - BRACE_GAP - brace_width,
                    bottom,
                    smufl::BRACE,
                    height,
                );
            }
            if first_system && !part.name.is_empty() {
                canvas.text(
                    x_start - self.brace_room() - NAME_GAP,
                    (top + bottom) / 2.0 + NAME_SIZE * 0.35,
                    &part.name,
                    NAME_SIZE,
                    Anchor::End,
                );
            }
        }

        let mut cursor = x_start;
        for (offset, measure) in system.measures.clone().enumerate() {
            let info = self.score.measures[measure];
            let metrics = &self.metrics[measure];
            let prefix = prefix_for(self.score, measure, offset == 0);

            for (staff, &row) in self.staves.iter().zip(&rows.y) {
                prefix.draw(canvas, cursor, y_top + row, staff.clef, info);
            }
            let body_start = cursor + prefix.width(info);
            let body_end = body_start + metrics.body_width * stretch;
            let x_of = |onset: u32| body_start + metrics.offset_of(onset) * stretch;

            for (staff, &row) in self.staves.iter().zip(&rows.y) {
                canvas.areas.push(MeasureArea {
                    staff: staff.id,
                    measure,
                    next_measure: system.measures.clone().nth(offset + 1),
                    time_signature: info.time_signature,
                    x_start: body_start,
                    x_end: body_end,
                    y_top: y_top + row,
                    columns: metrics
                        .columns
                        .iter()
                        .map(|&tick| (tick, x_of(tick)))
                        .collect(),
                });
            }

            for (plan, &row) in metrics.plans.iter().zip(&rows.y) {
                let y0 = y_top + row;
                if plan.is_empty() {
                    let value = if info.length.is_some() {
                        rest_value_for(info.capacity())
                    } else {
                        NoteValue::Whole
                    };
                    let (glyph, width, rest_y) = smufl::rest(value);
                    let x = (body_start + body_end - width) / 2.0;
                    canvas.glyph(None, x, y0 + rest_y, glyph, 4.0);
                }
                for voice in &plan.voices {
                    emit::emit_voice(canvas, voice, y0, &x_of);
                }
            }

            let is_final = measure + 1 == self.score.measures.len();
            for range in rows.parts.iter().filter(|r| !r.is_empty()) {
                let top = y_top + rows.y[range.start];
                let bottom = y_top + rows.y[range.end - 1] + STAFF_HEIGHT;
                if is_final {
                    let thick = body_end - smufl::THICK_BARLINE / 2.0;
                    let thin = thick - smufl::THICK_BARLINE / 2.0 - 0.4 - smufl::THIN_BARLINE / 2.0;
                    canvas.line(None, (thin, top), (thin, bottom), smufl::THIN_BARLINE);
                    canvas.line(None, (thick, top), (thick, bottom), smufl::THICK_BARLINE);
                } else {
                    canvas.line(
                        None,
                        (body_end, top),
                        (body_end, bottom),
                        smufl::THIN_BARLINE,
                    );
                }
            }
            cursor = body_end;
        }
    }
}
