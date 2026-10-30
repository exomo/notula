//! Engraving layout: turns a score into a list of drawing primitives.

mod emit;
mod engrave;
mod measure;
mod prefix;
mod render;
mod smufl;
mod staff;
mod volume;

use std::ops::Range;

use notula_model::{EventKind, NoteValue, Pitch, Score, Staff, StaffId};

use measure::{MeasureMetrics, measure_metrics};
use prefix::Prefix;
use render::Canvas;
pub use render::{Anchor, Item, MeasureArea, RenderList, Shape};
use smufl::notehead;

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
/// Least room between what reaches out of neighbouring staves or systems.
const STAFF_CLEARANCE: f32 = 1.5;
/// Shortest tie leaving or entering a system.
const MIN_OPEN_TIE: f32 = 1.5;
/// How far tempo text below a part reaches under the line it hangs from.
const TEMPO_TEXT_DEPTH: f32 = 2.4;
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
    /// How far content reaches above the first staff and below the last one.
    above: f32,
    below: f32,
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
        Self {
            y,
            parts,
            height,
            above: 0.0,
            below: 0.0,
        }
    }

    /// The rows of one system, moved apart where content reaching out of a staff, given as
    /// (top, bottom) relative to its top line, would come closer to the next staff than
    /// `STAFF_CLEARANCE`. Staves never come closer than the default distances.
    fn spaced(&self, reaches: &[(f32, f32)]) -> Self {
        let mut y: Vec<f32> = Vec::with_capacity(self.y.len());
        for (index, reach) in reaches.iter().enumerate() {
            let row = match index.checked_sub(1) {
                None => 0.0,
                Some(previous) => {
                    let pitch = self.y[index] - self.y[previous];
                    let needed = reaches[previous].1 - reach.0 + STAFF_CLEARANCE;
                    y[previous] + pitch.max(needed)
                }
            };
            y.push(row);
        }
        let height = y.last().map_or(0.0, |last| last + STAFF_HEIGHT);
        Self {
            above: reaches.first().map_or(0.0, |reach| -reach.0.min(0.0)),
            below: reaches
                .last()
                .map_or(0.0, |reach| (reach.1 - STAFF_HEIGHT).max(0.0)),
            y,
            parts: self.parts.clone(),
            height,
        }
    }

    fn has_braces(&self) -> bool {
        self.parts.iter().any(|r| r.len() > 1)
    }
}

fn prefix_for(score: &Score, measure: usize, first_in_system: bool) -> Prefix {
    let info = score.measures[measure];
    let previous = measure.checked_sub(1).map(|p| score.measures[p]);
    let key_changed = previous.is_some_and(|p| p.key_signature != info.key_signature);
    let time_changed = previous.is_none_or(|p| {
        p.time_signature != info.time_signature
            || p.time_signature_display != info.time_signature_display
    });
    Prefix {
        clef: first_in_system,
        key: info.key_signature.fifths != 0 && first_in_system || key_changed,
        cancel_key: if key_changed {
            previous.map(|previous| previous.key_signature.fifths)
        } else {
            None
        },
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
    hairpins: Vec<notula_model::edit::HairpinSpan>,
}

struct PlacedNote {
    pitch: Pitch,
    x: f32,
    y: f32,
    width: f32,
}

struct PlacedEvent {
    id: notula_model::EventId,
    tied: bool,
    slur_start: bool,
    slur_stop: bool,
    slur_placement: notula_model::SlurPlacement,
    phrase_slur_start: bool,
    phrase_slur_stop: bool,
    phrase_slur_placement: notula_model::SlurPlacement,
    start_tick: u64,
    end_tick: u64,
    staff: StaffId,
    staff_top: f32,
    voice: usize,
    stem_up: bool,
    /// Ties curve below: on the notehead side, or with another voice, away from it.
    ties_below: bool,
    notes: Vec<PlacedNote>,
}

fn event_details(
    score: &Score,
    id: notula_model::EventId,
) -> Option<(StaffId, usize, u64, notula_model::Event)> {
    let located = notula_model::edit::locate(score, id)?;
    let staff = score.staff(located.cursor.staff).ok()?;
    let event = staff.measures[located.cursor.measure].voices[located.cursor.voice]
        .events
        .get(located.index)?
        .clone();
    let measure_start: u64 = score.measures[..located.cursor.measure]
        .iter()
        .map(|measure| u64::from(measure.capacity()))
        .sum();
    Some((
        located.cursor.staff,
        located.cursor.voice,
        measure_start + u64::from(located.cursor.tick),
        event,
    ))
}

fn slur_endpoint(
    score: &Score,
    event: notula_model::EventId,
    forward: bool,
    phrase: bool,
) -> Option<notula_model::EventId> {
    let (staff, voice, _, current) = event_details(score, event)?;
    let boundary = if phrase {
        if forward {
            current.markings.phrase_slur_start
        } else {
            current.markings.phrase_slur_stop
        }
    } else if forward {
        current.markings.slur_start
    } else {
        current.markings.slur_stop
    };
    if !boundary {
        return None;
    }
    let mut candidate = notula_model::edit::adjacent_event(score, event, forward)?;
    loop {
        let (candidate_staff, candidate_voice, _, candidate_event) =
            event_details(score, candidate)?;
        if candidate_staff != staff || candidate_voice != voice {
            return None;
        }
        if if phrase {
            if forward {
                candidate_event.markings.phrase_slur_stop
            } else {
                candidate_event.markings.phrase_slur_start
            }
        } else if forward {
            candidate_event.markings.slur_stop
        } else {
            candidate_event.markings.slur_start
        } {
            return Some(candidate);
        }
        candidate = notula_model::edit::adjacent_event(score, candidate, forward)?;
    }
}

fn slur_is_above(event: &PlacedEvent) -> bool {
    match event.slur_placement {
        notula_model::SlurPlacement::Auto => event.stem_up,
        notula_model::SlurPlacement::Above => true,
        notula_model::SlurPlacement::Below => false,
    }
}

fn slur_anchor(event: &PlacedEvent, above: bool) -> Option<&PlacedNote> {
    if above {
        event
            .notes
            .iter()
            .min_by(|left, right| left.y.total_cmp(&right.y))
    } else {
        event
            .notes
            .iter()
            .max_by(|left, right| left.y.total_cmp(&right.y))
    }
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
        hairpins: notula_model::edit::hairpins(score).0,
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

/// Staff spaces before the barline that count as the end of the measure.
const END_OF_MEASURE_REACH: f32 = 2.5;
/// A click this far right of a column still lands on that column's notehead.
const NOTEHEAD_REACH: f32 = 1.2;
/// How far from the middle of a staff a click still belongs to it.
const STAFF_REACH: f32 = 4.5;

/// The staves and time a box dragged from `(x0, y0)` to `(x1, y1)` covers, as a selection:
/// staves whose lines it touches, and the note columns inside it. An empty measure it touches
/// counts in full.
pub fn box_selection(
    score: &Score,
    list: &RenderList,
    (x0, y0): (f32, f32),
    (x1, y1): (f32, f32),
) -> Option<notula_model::selection::Selection> {
    let (left, right) = (x0.min(x1), x0.max(x1));
    let (top, bottom) = (y0.min(y1), y0.max(y1));
    let mut measure_starts = Vec::with_capacity(score.measures.len() + 1);
    let mut tick = 0_u64;
    for info in &score.measures {
        measure_starts.push(tick);
        tick += u64::from(info.capacity());
    }
    measure_starts.push(tick);

    let mut staves = Vec::new();
    let (mut start, mut end) = (u64::MAX, 0_u64);
    for area in &list.areas {
        let touches_staff = area.y_top <= bottom && top <= area.y_top + STAFF_HEIGHT;
        let touches_measure = area.x_start <= right && left <= area.x_end;
        if !touches_staff || !touches_measure {
            continue;
        }
        let measure_start = measure_starts[area.measure];
        let ticks: Vec<u64> = area
            .columns
            .iter()
            .filter(|(_, x)| left <= *x && *x <= right)
            .map(|(column, _)| measure_start + u64::from(*column))
            .collect();
        if area.columns.is_empty() {
            start = start.min(measure_start);
            end = end.max(measure_starts[area.measure + 1]);
        } else if let (Some(first), Some(last)) = (ticks.iter().min(), ticks.iter().max()) {
            // The last column's notes are completed when the selection is resolved.
            start = start.min(*first);
            end = end.max(last + 1);
        } else {
            continue;
        }
        if !staves.contains(&area.staff) {
            staves.push(area.staff);
        }
    }
    if staves.is_empty() || start >= end {
        return None;
    }
    let order: Vec<StaffId> = score.staves().map(|staff| staff.id).collect();
    staves.sort_by_key(|staff| order.iter().position(|id| id == staff));
    let selection = notula_model::selection::Selection::range(
        staves[0],
        *staves.last()?,
        notula_model::selection::Span { start, end },
    );
    // The cursor goes where the drag ended: the left edge when drawn right to left.
    Some(if x1 < x0 {
        selection.reversed()
    } else {
        selection
    })
}

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
    // A click belongs to the notehead it lands on, or else to the next column to its right;
    // with none to the right, it is at the end of the measure.
    let tick = if near_measure_end {
        None
    } else {
        area.columns
            .iter()
            .find(|&&(_, column_x)| x <= column_x + NOTEHEAD_REACH)
            .map(|&(tick, _)| tick)
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
        let first_name_chars = score
            .parts
            .iter()
            .filter(|part| !part.hide_name)
            .map(|p| p.name.chars().count())
            .max()
            .unwrap_or(0);
        let short_name_chars = score
            .parts
            .iter()
            .filter(|part| !part.hide_name)
            .map(|part| short_name(part).chars().count())
            .max()
            .unwrap_or(0);
        let first_names_width = if first_name_chars > 0 {
            first_name_chars as f32 * NAME_CHAR_WIDTH + NAME_GAP
        } else {
            0.0
        };
        let short_names_width = if short_name_chars > 0 {
            short_name_chars as f32 * NAME_CHAR_WIDTH + NAME_GAP
        } else {
            0.0
        };
        let first_x = MARGIN + first_names_width + self.brace_room();
        let other_x = MARGIN + short_names_width + self.brace_room();
        let right = width - MARGIN;

        let systems = break_systems(score, &self.metrics, right - first_x, right - other_x);

        let mut canvas = Canvas::default();
        let mut y = 0.0;
        let mut previous: Option<Rows> = None;
        for (index, system) in systems.iter().enumerate() {
            let rows = self.rows.spaced(&self.reaches(system));
            y += match &previous {
                None => VERTICAL_MARGIN.max(rows.above + STAFF_CLEARANCE),
                Some(previous) => {
                    previous.height + SYSTEM_GAP.max(previous.below + rows.above + STAFF_CLEARANCE)
                }
            };
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
                &rows,
                (x_start, x_end),
                y,
                stretch,
                index == 0,
            );
            previous = Some(rows);
        }
        let height = previous.map_or(2.0 * VERTICAL_MARGIN, |last| {
            y + last.height + VERTICAL_MARGIN.max(last.below + STAFF_CLEARANCE)
        });

        let event_voices: std::collections::HashMap<_, _> = self
            .staves
            .iter()
            .flat_map(|staff| {
                staff.measures.iter().flat_map(move |measure| {
                    measure
                        .voices
                        .iter()
                        .enumerate()
                        .flat_map(move |(voice, events)| {
                            events
                                .events
                                .iter()
                                .map(move |event| (event.id, (staff.id, voice)))
                        })
                })
            })
            .collect();
        for item in &mut canvas.items {
            if let Some((staff, voice)) = item.event.and_then(|event| event_voices.get(&event)) {
                item.staff = Some(*staff);
                item.voice = Some(*voice);
            }
        }

        RenderList {
            width,
            height,
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

    /// How far each staff's content reaches above and below its top line in a system,
    /// including the dynamics line and tempo marks of its part.
    fn reaches(&self, system: &System) -> Vec<(f32, f32)> {
        let mut reaches = vec![(0.0_f32, STAFF_HEIGHT); self.staves.len()];
        for measure in system.measures.clone() {
            for (reach, plan) in reaches.iter_mut().zip(&self.metrics[measure].plans) {
                let (top, bottom) = emit::staff_reach(&plan.voices);
                *reach = (reach.0.min(top), reach.1.max(bottom));
            }
        }

        let first_tick: u64 = self.score.measures[..system.measures.start]
            .iter()
            .map(|measure| u64::from(measure.capacity()))
            .sum();
        let last_tick = first_tick
            + self.score.measures[system.measures.clone()]
                .iter()
                .map(|measure| u64::from(measure.capacity()))
                .sum::<u64>();
        for (reach, staff) in reaches.iter_mut().zip(&self.staves) {
            let marked = staff.measures[system.measures.clone()]
                .iter()
                .flat_map(|measure| &measure.voices)
                .flat_map(|voice| &voice.events)
                .any(|event| {
                    let markings = &event.markings;
                    markings.dynamic.is_some()
                        || markings.hairpin_start.is_some()
                        || markings.hairpin_stop
                })
                || self.hairpins.iter().any(|span| {
                    span.staff == staff.id
                        && span.start_tick < last_tick
                        && span.end_tick > first_tick
                });
            if marked {
                reach.1 = reach.1.max(volume::line_bottom(reach.1));
            }
        }

        let in_system = |measure: usize| system.measures.contains(&measure);
        let tempo_marks = self
            .score
            .tempo_changes
            .iter()
            .any(|mark| !mark.hidden && in_system(mark.measure));
        let tempo_texts = self
            .score
            .tempo_directions
            .iter()
            .any(|mark| in_system(mark.measure));
        for part in self.rows.parts.iter().filter(|part| !part.is_empty()) {
            if tempo_marks {
                let top = &mut reaches[part.start].0;
                *top = emit::metronome_top(*top);
            }
            if tempo_texts {
                reaches[part.end - 1].1 += TEMPO_TEXT_DEPTH;
            }
        }
        reaches
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_system(
        &self,
        canvas: &mut Canvas,
        system: &System,
        rows: &Rows,
        (x_start, x_end): (f32, f32),
        y_top: f32,
        stretch: f32,
        first_system: bool,
    ) {
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
            let label = if part.hide_name {
                ""
            } else if first_system {
                &part.name
            } else {
                short_name(part)
            };
            if !label.is_empty() {
                canvas.text(
                    x_start - self.brace_room() - NAME_GAP,
                    (top + bottom) / 2.0 + NAME_SIZE * 0.35,
                    label,
                    NAME_SIZE,
                    Anchor::End,
                );
            }
        }

        let mut cursor = x_start;
        let mut timeline = volume::Timeline::default();
        let mut tempo_texts = Vec::new();
        let mut foreground_fermatas = Vec::new();
        let mut placed_events = Vec::new();
        let mut measure_tick: u64 = self.score.measures[..system.measures.start]
            .iter()
            .map(|measure| u64::from(measure.capacity()))
            .sum();
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
            timeline.push(measure_tick, body_start);
            for &column in &metrics.columns {
                timeline.push(measure_tick + u64::from(column), x_of(column));
            }

            for (staff, &row) in self.staves.iter().zip(&rows.y) {
                canvas.areas.push(MeasureArea {
                    staff: staff.id,
                    measure,
                    voice_count: staff.measures[measure].voices.len(),
                    next_measure: system.measures.clone().nth(offset + 1),
                    time_signature: info.time_signature,
                    time_signature_display: info.time_signature_display,
                    key_fifths: info.key_signature.fifths,
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

            for (staff_index, (plan, &row)) in metrics.plans.iter().zip(&rows.y).enumerate() {
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
                // A rest filling the measure sits in its middle, like the placeholder rest.
                let event_x = |event: &engrave::EventPlan| {
                    if event.fills_measure {
                        (body_start + body_end - event.right) / 2.0
                    } else {
                        x_of(event.onset)
                    }
                };
                for (voice_index, voice) in plan.voices.iter().enumerate() {
                    let centered = voice.events.first().filter(|event| event.fills_measure);
                    let voice_x = |onset: u32| match centered {
                        Some(rest) => event_x(rest),
                        None => x_of(onset),
                    };
                    emit::emit_voice(canvas, voice, y0, &voice_x);
                    for event in &voice.events {
                        let (tied, stem_up) = (event.tied, event.stem_up);
                        let notes = match &event.body {
                            engrave::Body::Rest { .. } => Vec::new(),
                            engrave::Body::Chord { notes } => {
                                let x = x_of(event.onset);
                                let width = notehead(event.value).1;
                                notes
                                    .iter()
                                    .map(|note| PlacedNote {
                                        pitch: note.pitch,
                                        x: emit::head_x(note, event, x, width),
                                        y: y0 + note.y,
                                        width,
                                    })
                                    .collect()
                            }
                        };
                        let start_tick = measure_tick + u64::from(event.onset);
                        placed_events.push(PlacedEvent {
                            id: event.id,
                            tied,
                            slur_start: event.slur_start,
                            slur_stop: event.slur_stop,
                            slur_placement: event.slur_placement,
                            phrase_slur_start: event.phrase_slur_start,
                            phrase_slur_stop: event.phrase_slur_stop,
                            phrase_slur_placement: event.phrase_slur_placement,
                            start_tick,
                            end_tick: start_tick
                                + u64::from(
                                    notula_model::Duration::new(event.value, event.dots).ticks(),
                                ),
                            staff: self.staves[staff_index].id,
                            staff_top: y0,
                            voice: voice_index,
                            stem_up,
                            ties_below: emit::marks_below(event),
                            notes,
                        });
                    }
                }
                foreground_fermatas.extend(emit::fermatas(&plan.voices, y0, &event_x));
            }

            for mark in self
                .score
                .tempo_directions
                .iter()
                .filter(|mark| mark.measure == measure)
            {
                let text = match mark.direction {
                    notula_model::TempoDirection::Accelerando => "accel.",
                    notula_model::TempoDirection::Ritardando => "rit.",
                    notula_model::TempoDirection::Rallentando => "rall.",
                    notula_model::TempoDirection::ATempo => "a tempo",
                    notula_model::TempoDirection::Rubato => "rubato",
                };
                tempo_texts.push((x_of(mark.tick), text));
            }

            for mark in self
                .score
                .tempo_changes
                .iter()
                .filter(|mark| mark.measure == measure && !mark.hidden)
            {
                let x = x_of(mark.tick);
                for part in rows.parts.iter().filter(|part| !part.is_empty()) {
                    let baseline = emit::metronome_baseline(
                        &metrics.plans[part.start].voices,
                        y_top + rows.y[part.start],
                        x,
                        mark.tempo,
                        &x_of,
                    );
                    emit::metronome_mark(canvas, x, baseline, mark.tempo);
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
            measure_tick += u64::from(info.capacity());
            timeline.push(measure_tick, body_end);
        }

        let volume_bottoms = volume::VolumeMarks {
            staves: &self.staves,
            staff_tops: rows.y.iter().map(|row| y_top + row).collect(),
            measures: system.measures.clone(),
            placed: &placed_events,
            hairpins: &self.hairpins,
            timeline: &timeline,
        }
        .emit(canvas);
        // Tempo directions go below each part, under its lowest dynamics line.
        for part in rows.parts.iter().filter(|part| !part.is_empty()) {
            let bottom = y_top + rows.y[part.end - 1] + STAFF_HEIGHT;
            let below = volume_bottoms[part.end - 1].map_or(bottom, |line| line.max(bottom));
            for &(x, text) in &tempo_texts {
                canvas.text(x, below + 1.8, text, NAME_SIZE, render::Anchor::Start);
            }
        }

        for (event, x, y, below) in foreground_fermatas {
            if below {
                canvas.knockout(x - 1.1, y - 0.1, 2.2, 1.6);
                canvas.centered_glyph(Some(event), x, y, smufl::FERMATA_BELOW, 4.0);
            } else {
                canvas.knockout(x - 1.1, y - 1.5, 2.2, 1.6);
                canvas.centered_glyph(Some(event), x, y, smufl::FERMATA_ABOVE, 4.0);
            }
        }
        for source in placed_events.iter().filter(|event| event.tied) {
            let Some(next_id) = notula_model::edit::adjacent_event(self.score, source.id, true)
            else {
                continue;
            };
            if let Some(target) = placed_events.iter().find(|event| event.id == next_id) {
                if source.staff == target.staff
                    && source.voice == target.voice
                    && source.end_tick == target.start_tick
                {
                    for note in &source.notes {
                        let Some(next_note) =
                            target.notes.iter().find(|next| next.pitch == note.pitch)
                        else {
                            continue;
                        };
                        emit::emit_tie(
                            canvas,
                            source.id,
                            note.x + note.width - 0.05,
                            next_note.x + 0.05,
                            (note.y + next_note.y) / 2.0,
                            source.ties_below,
                        );
                    }
                }
                continue;
            }

            let Some((staff, voice, target_start, target)) = event_details(self.score, next_id)
            else {
                continue;
            };
            if staff != source.staff || voice != source.voice || source.end_tick != target_start {
                continue;
            }
            let EventKind::Chord { pitches, .. } = target.kind else {
                continue;
            };
            for note in &source.notes {
                if pitches.contains(&note.pitch) {
                    // A tie leaving the system runs to its end, but stays long enough to
                    // read as a tie when the note is close to the barline.
                    let start = note.x + note.width - 0.05;
                    emit::emit_tie(
                        canvas,
                        source.id,
                        start,
                        (cursor - 0.3).max(start + MIN_OPEN_TIE),
                        note.y,
                        source.ties_below,
                    );
                }
            }
        }
        let first_measure = system.measures.start;
        let first_body_start = x_start
            + prefix_for(self.score, first_measure, true).width(self.score.measures[first_measure]);
        for target in &placed_events {
            let Some(previous_id) =
                notula_model::edit::adjacent_event(self.score, target.id, false)
            else {
                continue;
            };
            if placed_events.iter().any(|event| event.id == previous_id) {
                continue;
            }
            let Some((staff, voice, source_start, source)) = event_details(self.score, previous_id)
            else {
                continue;
            };
            let EventKind::Chord {
                pitches,
                tied: true,
            } = source.kind
            else {
                continue;
            };
            let source_end = source_start + u64::from(source.duration.ticks());
            if staff != target.staff || voice != target.voice || source_end != target.start_tick {
                continue;
            }
            for note in &target.notes {
                if pitches.contains(&note.pitch) {
                    emit::emit_tie(
                        canvas,
                        previous_id,
                        first_body_start + 0.1,
                        note.x + 0.05,
                        note.y,
                        target.ties_below,
                    );
                }
            }
        }

        for source in placed_events.iter().filter(|event| event.slur_start) {
            let Some(end_id) = slur_endpoint(self.score, source.id, true, false) else {
                continue;
            };
            let above = slur_is_above(source);
            if let Some(target) = placed_events.iter().find(|event| event.id == end_id) {
                let (Some(start_note), Some(end_note)) =
                    (slur_anchor(source, above), slur_anchor(target, above))
                else {
                    continue;
                };
                emit::emit_slur(
                    canvas,
                    source.id,
                    start_note.x + start_note.width - 0.08,
                    end_note.x + 0.08,
                    start_note.y + if above { -0.28 } else { 0.28 },
                    end_note.y + if above { -0.28 } else { 0.28 },
                    above,
                );
            } else {
                let Some(start_note) = slur_anchor(source, above) else {
                    continue;
                };
                emit::emit_slur(
                    canvas,
                    source.id,
                    start_note.x + start_note.width - 0.08,
                    cursor - 0.3,
                    start_note.y + if above { -0.28 } else { 0.28 },
                    start_note.y + if above { -0.28 } else { 0.28 },
                    above,
                );
            }
        }

        for target in placed_events.iter().filter(|event| event.slur_stop) {
            let Some(start_id) = slur_endpoint(self.score, target.id, false, false) else {
                continue;
            };
            if placed_events.iter().any(|event| event.id == start_id) {
                continue;
            }
            let Some((_, _, _, source_event)) = event_details(self.score, start_id) else {
                continue;
            };
            let above = match source_event.markings.slur_placement {
                notula_model::SlurPlacement::Auto => target.stem_up,
                notula_model::SlurPlacement::Above => true,
                notula_model::SlurPlacement::Below => false,
            };
            let Some(end_note) = slur_anchor(target, above) else {
                continue;
            };
            emit::emit_slur(
                canvas,
                start_id,
                first_body_start + 0.3,
                end_note.x + 0.08,
                end_note.y + if above { -0.28 } else { 0.28 },
                end_note.y + if above { -0.28 } else { 0.28 },
                above,
            );
        }

        for source in placed_events.iter().filter(|event| event.phrase_slur_start) {
            let Some(end_id) = slur_endpoint(self.score, source.id, true, true) else {
                continue;
            };
            let above = match source.phrase_slur_placement {
                notula_model::SlurPlacement::Auto => source.stem_up,
                notula_model::SlurPlacement::Above => true,
                notula_model::SlurPlacement::Below => false,
            };
            let boundary_y = |event: &PlacedEvent| {
                if above {
                    event.staff_top - 0.38
                } else {
                    event.staff_top + STAFF_HEIGHT + 0.38
                }
            };
            if let Some(target) = placed_events.iter().find(|event| event.id == end_id) {
                emit::emit_phrase_slur(
                    canvas,
                    source.id,
                    source
                        .notes
                        .iter()
                        .map(|note| note.x + note.width)
                        .fold(f32::NEG_INFINITY, f32::max)
                        - 0.08,
                    target
                        .notes
                        .iter()
                        .map(|note| note.x)
                        .fold(f32::INFINITY, f32::min)
                        + 0.08,
                    boundary_y(source),
                    boundary_y(target),
                    above,
                );
            } else {
                if source.notes.is_empty() {
                    continue;
                }
                emit::emit_phrase_slur(
                    canvas,
                    source.id,
                    source
                        .notes
                        .iter()
                        .map(|note| note.x + note.width)
                        .fold(f32::NEG_INFINITY, f32::max)
                        - 0.08,
                    cursor - 0.3,
                    boundary_y(source),
                    boundary_y(source),
                    above,
                );
            }
        }

        for target in placed_events.iter().filter(|event| event.phrase_slur_stop) {
            let Some(start_id) = slur_endpoint(self.score, target.id, false, true) else {
                continue;
            };
            if placed_events.iter().any(|event| event.id == start_id) {
                continue;
            }
            let Some((_, _, _, source_event)) = event_details(self.score, start_id) else {
                continue;
            };
            let above = match source_event.markings.phrase_slur_placement {
                notula_model::SlurPlacement::Auto => target.stem_up,
                notula_model::SlurPlacement::Above => true,
                notula_model::SlurPlacement::Below => false,
            };
            if target.notes.is_empty() {
                continue;
            }
            let end_x = target
                .notes
                .iter()
                .map(|note| note.x)
                .fold(f32::INFINITY, f32::min)
                + 0.08;
            let boundary_y = if above {
                target.staff_top - 0.38
            } else {
                target.staff_top + STAFF_HEIGHT + 0.38
            };
            emit::emit_phrase_slur(
                canvas,
                start_id,
                first_body_start + 0.3,
                end_x,
                boundary_y,
                boundary_y,
                above,
            );
        }
    }
}

fn short_name(part: &notula_model::Part) -> &str {
    if !part.short_name.is_empty() {
        return &part.short_name;
    }
    match part.name.as_str() {
        "Piano" => "Pno.",
        "Organ" => "Org.",
        "Harp" => "Hp.",
        "Accordion" => "Acc.",
        "Flute" => "Fl.",
        "Oboe" => "Ob.",
        "Clarinet" => "Cl.",
        "Bassoon" => "Bsn.",
        "Horn" => "Hn.",
        "Trumpet" => "Tpt.",
        "Trombone" => "Tbn.",
        "Violin" => "Vln.",
        "Viola" => "Vla.",
        "Cello" => "Vc.",
        "Double Bass" => "Db.",
        "Guitar" => "Gtr.",
        "Voice" => "Vo.",
        _ => &part.name,
    }
}
