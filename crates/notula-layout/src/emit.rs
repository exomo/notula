//! Turns planned events into drawing primitives once their horizontal positions are known.

use notula_model::{Articulation, EventId, NoteValue, Tempo};

use crate::engrave::{Body, EventPlan, NotePlan, VoicePlan, dot_advance};
use crate::render::{Anchor, Canvas};
use crate::smufl;

const STEM_LENGTH: f32 = 3.5;
const MIN_BEAMED_STEM: f32 = 3.0;
const EXTRA_BEAM_STEM: f32 = 0.75;
const BEAM_STUB: f32 = 1.0;
const MAX_BEAM_DROP: f32 = 1.0;
const MIDDLE_LINE: f32 = 2.0;
const ARTICULATION_GAP: f32 = 1.0;
const NOTEHEAD_HALF: f32 = 0.5;
const METRONOME_SIZE: f32 = 2.6;
const METRONOME_TEXT_SIZE: f32 = 2.0;
const METRONOME_DOT_GAP: f32 = 0.15;
const METRONOME_DOT_ADVANCE: f32 = 0.4;
const METRONOME_TEXT_GAP: f32 = 0.6;
/// Distance from the staff top, or the highest notehead or stem under the mark, to its baseline.
const METRONOME_CLEARANCE: f32 = 1.5;
/// Rough advance of a digit or `=` in the text font, relative to its size.
const METRONOME_CHAR_WIDTH: f32 = 0.55;

struct Stem {
    x: f32,
    /// Where the stem leaves the noteheads, relative to the staff top.
    base_y: f32,
    /// The notehead nearest to the stem end.
    near_y: f32,
}

pub(crate) fn head_x(note: &NotePlan, ev: &EventPlan, x: f32, head_width: f32) -> f32 {
    match (note.displaced, ev.stem_up) {
        (false, _) => x,
        (true, true) => x + head_width - smufl::STEM_THICKNESS,
        (true, false) => x - head_width + smufl::STEM_THICKNESS,
    }
}

/// A slur, phrase slur, or tie: its ends, which side it bows to, and how far its middle bows
/// out from the line between the ends.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Arc {
    pub start: (f32, f32),
    pub end: (f32, f32),
    pub above: bool,
    pub depth: f32,
}

/// Least room between a slur and the notes it passes over.
const ARC_CLEARANCE: f32 = 0.4;

impl Arc {
    /// A slur between two points, bowing as far as its length suggests.
    pub fn slur(start: (f32, f32), end: (f32, f32), above: bool) -> Self {
        let depth = ((end.0 - start.0) * 0.12).clamp(0.5, 2.0);
        Self {
            start,
            end,
            above,
            depth,
        }
    }

    pub fn phrase(start: (f32, f32), end: (f32, f32), above: bool) -> Self {
        let depth = ((end.0 - start.0) * 0.12).clamp(0.8, 3.0);
        Self {
            start,
            end,
            above,
            depth,
        }
    }

    /// The arc's outer edge at `x`, if `x` lies between its ends.
    pub fn top_at(&self, x: f32) -> Option<f32> {
        let width = self.end.0 - self.start.0;
        if width <= 0.0 || x < self.start.0 || x > self.end.0 {
            return None;
        }
        let t = (x - self.start.0) / width;
        let line = self.start.1 + (self.end.1 - self.start.1) * t;
        let out = if self.above { -1.0 } else { 1.0 };
        Some(line + out * self.depth * (std::f32::consts::PI * t).sin())
    }

    /// Bows the arc out to pass `obstacles` (points on the outer edge of noteheads, stems, and
    /// beams under it) with some room, up to `max_depth`; beyond that, both ends move out too.
    pub fn clear(&mut self, obstacles: &[(f32, f32)], max_depth: f32) {
        let width = self.end.0 - self.start.0;
        if width <= 0.0 {
            return;
        }
        // Distances measured outwards, away from the notes.
        let out = if self.above { -1.0 } else { 1.0 };
        let needs: Vec<(f32, f32)> = obstacles
            .iter()
            .filter(|(x, _)| self.start.0 < *x && *x < self.end.0)
            .map(|&(x, y)| {
                let t = (x - self.start.0) / width;
                let line = self.start.1 + (self.end.1 - self.start.1) * t;
                (
                    (std::f32::consts::PI * t).sin(),
                    out * (y - line) + ARC_CLEARANCE,
                )
            })
            .filter(|(_, need)| *need > 0.0)
            .collect();
        let wanted = needs
            .iter()
            .map(|(bow, need)| need / bow.max(0.05))
            .fold(self.depth, f32::max);
        self.depth = wanted.min(max_depth.max(self.depth));
        let lift = needs
            .iter()
            .map(|(bow, need)| need - self.depth * bow)
            .fold(0.0, f32::max);
        self.start.1 += out * lift;
        self.end.1 += out * lift;
    }
}

/// Draws a tie and returns its shape, so marks above it can keep clear.
pub(crate) fn emit_tie(
    canvas: &mut Canvas,
    event: EventId,
    start: (f32, f32),
    end: (f32, f32),
    below: bool,
) -> Arc {
    let depth = ((end.0 - start.0) * 0.24).clamp(0.5, 1.0);
    let arc = Arc {
        start,
        end,
        above: !below,
        depth,
    };
    emit_curve(canvas, event, &arc, false);
    arc
}

pub(crate) fn emit_slur(canvas: &mut Canvas, event: EventId, arc: &Arc) {
    emit_curve(canvas, event, arc, true);
}

pub(crate) fn emit_phrase_slur(canvas: &mut Canvas, event: EventId, arc: &Arc) {
    let width = arc.end.0 - arc.start.0;
    if width <= 0.4 {
        return;
    }
    let (start_x, start_y, end_y) = (arc.start.0, arc.start.1, arc.end.1);
    let direction = if arc.above { -1.0 } else { 1.0 };
    let depth = arc.depth;
    let thickness = 0.23;
    let segments = 16;
    let band = |outer_depth: f32, inner_depth: f32| {
        let mut points = Vec::with_capacity((segments + 1) * 2);
        for segment in 0..=segments {
            let t = segment as f32 / segments as f32;
            let curve = (std::f32::consts::PI * t).sin();
            points.push((
                start_x + width * t,
                start_y + (end_y - start_y) * t + direction * outer_depth * curve,
            ));
        }
        for segment in (0..=segments).rev() {
            let t = segment as f32 / segments as f32;
            let curve = (std::f32::consts::PI * t).sin();
            points.push((
                start_x + width * t,
                start_y + (end_y - start_y) * t + direction * inner_depth * curve,
            ));
        }
        points
    };
    canvas.white_polygon(&band(depth + 0.18, (depth - thickness - 0.18).max(0.08)));
    canvas.phrase_slur(event, &band(depth, depth - thickness));
}

fn emit_curve(canvas: &mut Canvas, event: EventId, arc: &Arc, slur: bool) {
    let (start_x, start_y, end_y) = (arc.start.0, arc.start.1, arc.end.1);
    let width = arc.end.0 - start_x;
    if width <= 0.4 {
        return;
    }
    let direction = if arc.above { -1.0 } else { 1.0 };
    let depth = arc.depth;
    let thickness = if slur { 0.18 } else { 0.12 };
    let segments = 12;
    let band = |outer_depth: f32, inner_depth: f32| {
        let mut points = Vec::with_capacity((segments + 1) * 2);
        for segment in 0..=segments {
            let t = segment as f32 / segments as f32;
            let curve = (std::f32::consts::PI * t).sin();
            points.push((
                start_x + width * t,
                start_y + (end_y - start_y) * t + direction * outer_depth * curve,
            ));
        }
        for segment in (0..=segments).rev() {
            let t = segment as f32 / segments as f32;
            let curve = (std::f32::consts::PI * t).sin();
            points.push((
                start_x + width * t,
                start_y + (end_y - start_y) * t + direction * inner_depth * curve,
            ));
        }
        points
    };
    let underlay = band(depth + 0.16, (depth - thickness - 0.16).max(0.08));
    let curve = band(depth, depth - thickness);
    canvas.white_polygon(&underlay);
    if slur {
        canvas.slur(event, &curve);
    } else {
        canvas.tie(event, &curve);
    }
}

fn stem_geometry(ev: &EventPlan, notes: &[NotePlan], x: f32) -> Stem {
    let head_width = smufl::notehead(ev.value).1;
    let top = notes.iter().map(|n| n.y).fold(f32::INFINITY, f32::min);
    let bottom = notes.iter().map(|n| n.y).fold(f32::NEG_INFINITY, f32::max);
    if ev.stem_up {
        Stem {
            x: x + head_width - smufl::STEM_THICKNESS / 2.0,
            base_y: bottom - smufl::STEM_ANCHOR_OFFSET,
            near_y: top,
        }
    } else {
        Stem {
            x: x + smufl::STEM_THICKNESS / 2.0,
            base_y: top + smufl::STEM_ANCHOR_OFFSET,
            near_y: bottom,
        }
    }
}

fn on_line(y: f32) -> bool {
    (y * 2.0).rem_euclid(2.0) == 0.0
}

fn draw_dots(canvas: &mut Canvas, ev: &EventPlan, x: f32, ys: &[f32]) {
    for i in 0..ev.dots {
        for &y in ys {
            canvas.glyph(
                Some(ev.id),
                x + ev.dot_x + f32::from(i) * dot_advance(),
                y,
                smufl::AUGMENTATION_DOT,
                4.0,
            );
        }
    }
}

/// Whether an event's marks go below it: on the notehead side, or with another voice in the
/// staff on the stem side, away from that voice.
pub(crate) fn marks_below(ev: &EventPlan) -> bool {
    ev.stem_up != ev.shares_staff
}

fn draw_articulations(
    canvas: &mut Canvas,
    ev: &EventPlan,
    y0: f32,
    center_x: f32,
    (top, bottom): (f32, f32),
) {
    let mut above_index = 0;
    let mut below_index = 0;
    for &articulation in &ev.articulations {
        if articulation == Articulation::Fermata {
            continue;
        }
        let below = marks_below(ev);
        let y = if below {
            let y = articulation_y(bottom + 0.75 + below_index as f32 * ARTICULATION_GAP, true);
            below_index += 1;
            y
        } else {
            let y = articulation_y(top - 0.75 - above_index as f32 * ARTICULATION_GAP, false);
            above_index += 1;
            y
        };
        canvas.centered_glyph(
            Some(ev.id),
            center_x,
            y0 + y,
            smufl::articulation(articulation, below),
            4.0,
        );
    }
}

/// Fermatas of a staff's events as (event, centre x, baseline y, below). They clear every
/// voice at their onset, so they are drawn after the other marks; a fermata below uses the
/// inverted glyph.
pub(crate) fn fermatas(
    voices: &[VoicePlan],
    y0: f32,
    x_of: &dyn Fn(&EventPlan) -> f32,
) -> Vec<(EventId, f32, f32, bool)> {
    voices
        .iter()
        .flat_map(|voice| &voice.events)
        .filter(|event| event.articulations.contains(&Articulation::Fermata))
        .map(|event| {
            let below = event.shares_staff && !event.stem_up;
            let reaches = voices.iter().flat_map(|voice| {
                voice
                    .events
                    .iter()
                    .enumerate()
                    .filter(|(_, other)| other.onset == event.onset)
                    .map(move |(index, _)| extent(voice, index, !below))
            });
            let y = if below {
                let bottom = reaches.fold(f32::NEG_INFINITY, f32::max);
                articulation_y(bottom + ARTICULATION_GAP, true)
            } else {
                let top = reaches.fold(f32::INFINITY, f32::min);
                articulation_y(top - ARTICULATION_GAP, false)
            };
            let x = x_of(event);
            let center_x = match &event.body {
                Body::Rest { .. } => x + smufl::rest(event.value).1 / 2.0,
                Body::Chord { notes } => {
                    let head_width = smufl::notehead(event.value).1;
                    let left = notes
                        .iter()
                        .map(|note| head_x(note, event, x, head_width))
                        .fold(f32::INFINITY, f32::min);
                    let right = notes
                        .iter()
                        .map(|note| head_x(note, event, x, head_width) + head_width)
                        .fold(f32::NEG_INFINITY, f32::max);
                    (left + right) / 2.0
                }
            };
            (event.id, center_x, y0 + y, below)
        })
        .collect()
}

fn articulation_y(target: f32, below: bool) -> f32 {
    let half_space = if below {
        (target * 2.0).ceil() / 2.0
    } else {
        (target * 2.0).floor() / 2.0
    };
    if half_space.fract() == 0.0 {
        half_space + if below { 0.5 } else { -0.5 }
    } else {
        half_space
    }
}

pub(crate) fn upper_extent(voice: &VoicePlan, event_index: usize) -> f32 {
    extent(voice, event_index, true)
}

pub(crate) fn lower_extent(voice: &VoicePlan, event_index: usize) -> f32 {
    extent(voice, event_index, false)
}

/// How far an event reaches above (`above`) or below its staff position, including a stem or
/// beam on that side; staff coordinates grow downwards.
fn extent(voice: &VoicePlan, event_index: usize, above: bool) -> f32 {
    let event = &voice.events[event_index];
    let notes = match &event.body {
        Body::Rest { y } => return *y,
        Body::Chord { notes } => notes,
    };
    let ys = notes.iter().map(|note| note.y);
    let (edge, sign) = if above {
        (ys.fold(f32::INFINITY, f32::min), -1.0)
    } else {
        (ys.fold(f32::NEG_INFINITY, f32::max), 1.0)
    };
    if event.stem_up != above {
        return edge;
    }
    let outer = |a: f32, b: f32| if above { a.min(b) } else { a.max(b) };

    let reach = if let Some(group) = voice
        .beams
        .iter()
        .find(|group| group.contains(&event_index))
    {
        let levels = group
            .iter()
            .map(|&index| smufl::flag_count(voice.events[index].value))
            .max()
            .unwrap_or(1);
        MIN_BEAMED_STEM + levels.saturating_sub(1) as f32 * EXTRA_BEAM_STEM + MAX_BEAM_DROP
    } else {
        let length =
            STEM_LENGTH + smufl::flag_count(event.value).saturating_sub(1) as f32 * EXTRA_BEAM_STEM;
        return outer(edge, outer(edge + sign * length, MIDDLE_LINE));
    };
    outer(edge, edge + sign * reach)
}

pub(crate) fn emit_voice(
    canvas: &mut Canvas,
    voice: &VoicePlan,
    y0: f32,
    x_of: &dyn Fn(u32) -> f32,
) {
    let mut beamed = vec![false; voice.events.len()];
    for &i in voice.beams.iter().flatten() {
        beamed[i] = true;
    }
    for (index, (ev, &beamed)) in voice.events.iter().zip(&beamed).enumerate() {
        let reach = (upper_extent(voice, index), lower_extent(voice, index));
        emit_event(canvas, ev, x_of(ev.onset) + ev.shift, y0, !beamed, reach);
    }
    for group in &voice.beams {
        emit_beam(canvas, voice, group, y0, x_of);
    }
}

/// `reach` is how far the event extends above and below, stems included; articulations go
/// beyond it.
fn emit_event(
    canvas: &mut Canvas,
    ev: &EventPlan,
    x: f32,
    y0: f32,
    own_stem: bool,
    reach: (f32, f32),
) {
    match &ev.body {
        Body::Rest { y } => {
            let (glyph, width, _) = smufl::rest(ev.value);
            canvas.glyph(Some(ev.id), x, y0 + y, glyph, 4.0);
            draw_dots(canvas, ev, x, &[y0 + 1.5]);
            draw_articulations(canvas, ev, y0, x + width / 2.0, reach);
        }
        Body::Chord { notes } => {
            let (head, head_width) = smufl::notehead(ev.value);
            let mut ledgers: Vec<(f32, f32, f32)> = Vec::new();
            let mut dot_ys: Vec<f32> = Vec::new();
            let mut notehead_left = f32::INFINITY;
            let mut notehead_right = f32::NEG_INFINITY;

            for note in notes {
                let hx = head_x(note, ev, x, head_width);
                notehead_left = notehead_left.min(hx);
                notehead_right = notehead_right.max(hx + head_width);
                canvas.glyph(Some(ev.id), hx, y0 + note.y, head, 4.0);

                if let Some(accidental) = &note.accidental {
                    canvas.glyph(
                        Some(ev.id),
                        x + accidental.dx,
                        y0 + note.y,
                        accidental.glyph,
                        4.0,
                    );
                }

                let mut add_ledger = |y: f32| match ledgers.iter_mut().find(|l| l.0 == y) {
                    Some(line) => {
                        line.1 = line.1.min(hx - smufl::LEDGER_EXTENSION);
                        line.2 = line.2.max(hx + head_width + smufl::LEDGER_EXTENSION);
                    }
                    None => ledgers.push((
                        y,
                        hx - smufl::LEDGER_EXTENSION,
                        hx + head_width + smufl::LEDGER_EXTENSION,
                    )),
                };
                let mut line = -1.0;
                while line >= note.y {
                    add_ledger(line);
                    line -= 1.0;
                }
                let mut line = 5.0;
                while line <= note.y {
                    add_ledger(line);
                    line += 1.0;
                }

                if ev.dots > 0 {
                    let dot_y = y0
                        + if on_line(note.y) {
                            note.y - 0.5
                        } else {
                            note.y
                        };
                    if !dot_ys.contains(&dot_y) {
                        dot_ys.push(dot_y);
                    }
                }
            }

            for (y, x1, x2) in ledgers {
                canvas.line(
                    Some(ev.id),
                    (x1, y0 + y),
                    (x2, y0 + y),
                    smufl::LEDGER_THICKNESS,
                );
            }
            draw_dots(canvas, ev, x, &dot_ys);

            draw_articulations(
                canvas,
                ev,
                y0,
                (notehead_left + notehead_right) / 2.0,
                reach,
            );

            if own_stem && ev.value != NoteValue::Whole {
                emit_free_stem(canvas, ev, notes, x, y0);
            }
        }
    }
}

fn emit_free_stem(canvas: &mut Canvas, ev: &EventPlan, notes: &[NotePlan], x: f32, y0: f32) {
    let stem = stem_geometry(ev, notes, x);
    let flags = smufl::flag_count(ev.value);
    let length = STEM_LENGTH + flags.saturating_sub(1) as f32 * EXTRA_BEAM_STEM;
    let tip = if ev.stem_up {
        (stem.near_y - length).min(MIDDLE_LINE)
    } else {
        (stem.near_y + length).max(MIDDLE_LINE)
    };
    canvas.line(
        Some(ev.id),
        (stem.x, y0 + stem.base_y),
        (stem.x, y0 + tip),
        smufl::STEM_THICKNESS,
    );
    if let Some((glyph, anchor)) = smufl::flag(ev.value, ev.stem_up) {
        canvas.glyph(
            Some(ev.id),
            stem.x - smufl::STEM_THICKNESS / 2.0,
            y0 + tip + anchor,
            glyph,
            4.0,
        );
    }
}

fn emit_beam(
    canvas: &mut Canvas,
    voice: &VoicePlan,
    group: &[usize],
    y0: f32,
    x_of: &dyn Fn(u32) -> f32,
) {
    let stems: Vec<(&EventPlan, Stem)> = group
        .iter()
        .filter_map(|&i| {
            let ev = &voice.events[i];
            match &ev.body {
                Body::Chord { notes } => {
                    Some((ev, stem_geometry(ev, notes, x_of(ev.onset) + ev.shift)))
                }
                Body::Rest { .. } => None,
            }
        })
        .collect();
    let (Some((first_ev, first)), Some((_, last))) = (stems.first(), stems.last()) else {
        return;
    };
    let up = first_ev.stem_up;
    let levels = stems
        .iter()
        .map(|(ev, _)| smufl::flag_count(ev.value))
        .max()
        .unwrap_or(1);
    let min_stem = MIN_BEAMED_STEM + levels.saturating_sub(1) as f32 * EXTRA_BEAM_STEM;

    let span = last.x - first.x;
    let drop = ((last.near_y - first.near_y) * 0.5).clamp(-MAX_BEAM_DROP, MAX_BEAM_DROP);
    let slope = if span > 0.0 { drop / span } else { 0.0 };
    let anchors = stems.iter().map(|(_, s)| {
        if up {
            s.near_y - min_stem - slope * (s.x - first.x)
        } else {
            s.near_y + min_stem - slope * (s.x - first.x)
        }
    });
    let start_y = if up {
        anchors.fold(f32::INFINITY, f32::min)
    } else {
        anchors.fold(f32::NEG_INFINITY, f32::max)
    };
    let beam_y = |x: f32| y0 + start_y + slope * (x - first.x);

    for (ev, stem) in &stems {
        canvas.line(
            Some(ev.id),
            (stem.x, y0 + stem.base_y),
            (stem.x, beam_y(stem.x)),
            smufl::STEM_THICKNESS,
        );
    }

    let half = smufl::STEM_THICKNESS / 2.0;
    for level in 0..levels {
        let mut index = 0;
        while index < stems.len() {
            if smufl::flag_count(stems[index].0.value) <= level {
                index += 1;
                continue;
            }
            let mut end = index;
            while end + 1 < stems.len() && smufl::flag_count(stems[end + 1].0.value) > level {
                end += 1;
            }
            let (ev, from) = &stems[index];
            let (xa, xb) = if end > index {
                (from.x - half, stems[end].1.x + half)
            } else if index + 1 == stems.len() {
                (from.x + half - BEAM_STUB, from.x + half)
            } else {
                (from.x - half, from.x - half + BEAM_STUB)
            };
            beam_polygon(canvas, ev, &beam_y, (xa, xb), level, up);
            index = end + 1;
        }
    }
}

fn beam_polygon(
    canvas: &mut Canvas,
    ev: &EventPlan,
    beam_y: &dyn Fn(f32) -> f32,
    (xa, xb): (f32, f32),
    level: usize,
    up: bool,
) {
    let offset = level as f32 * (smufl::BEAM_THICKNESS + smufl::BEAM_SPACING);
    let (near, far) = if up {
        (offset, offset + smufl::BEAM_THICKNESS)
    } else {
        (-offset - smufl::BEAM_THICKNESS, -offset)
    };
    canvas.polygon(
        Some(ev.id),
        &[
            (xa, beam_y(xa) + near),
            (xb, beam_y(xb) + near),
            (xb, beam_y(xb) + far),
            (xa, beam_y(xa) + far),
        ],
    );
}

fn metronome_text(tempo: Tempo) -> String {
    format!("= {}", tempo.units_per_minute)
}

/// Where the text of a metronome mark starts, relative to the mark's x.
fn metronome_text_offset(tempo: Tempo) -> f32 {
    let scale = METRONOME_SIZE / 4.0;
    smufl::METRONOME_NOTE_WIDTH * scale
        + f32::from(tempo.beat_unit_dots) * (METRONOME_DOT_GAP + METRONOME_DOT_ADVANCE * scale)
        + METRONOME_TEXT_GAP
}

/// Horizontal room a metronome mark such as "♩ = 120" needs.
pub(crate) fn metronome_width(tempo: Tempo) -> f32 {
    metronome_text_offset(tempo)
        + metronome_text(tempo).chars().count() as f32 * METRONOME_CHAR_WIDTH * METRONOME_TEXT_SIZE
}

/// Baseline for a metronome mark at `x` that clears the staff and the events beneath it.
pub(crate) fn metronome_baseline(
    voices: &[VoicePlan],
    y0: f32,
    x: f32,
    tempo: Tempo,
    x_of: &dyn Fn(u32) -> f32,
) -> f32 {
    let right = x + metronome_width(tempo);
    let top = voices
        .iter()
        .flat_map(|voice| (0..voice.events.len()).map(move |index| (voice, index)))
        .filter(|&(voice, index)| {
            let event = &voice.events[index];
            let column = x_of(event.onset) + event.shift;
            column + event.right >= x && column - event.left <= right
        })
        .map(|(voice, index)| upper_extent(voice, index))
        .fold(0.0, f32::min);
    y0 + top - METRONOME_CLEARANCE
}

/// Height of a fermata glyph from its baseline.
const FERMATA_HEIGHT: f32 = 1.5;
/// Room a slur needs beyond the notes it starts or ends on.
const SLUR_ROOM: f32 = 1.5;
/// Height of a metronome mark above its baseline.
const METRONOME_HEIGHT: f32 = 3.0;

/// How far a staff's events reach above and below its top line in one measure: notes, stems,
/// beams, articulations, fermatas, and slur ends; staff coordinates grow downwards.
pub(crate) fn staff_reach(voices: &[VoicePlan]) -> (f32, f32) {
    let (mut top, mut bottom) = (0.0_f32, 4.0_f32);
    for voice in voices {
        for (index, event) in voice.events.iter().enumerate() {
            let mut above = upper_extent(voice, index) - NOTEHEAD_HALF;
            let mut below = lower_extent(voice, index) + NOTEHEAD_HALF;
            let marks = event
                .articulations
                .iter()
                .filter(|articulation| **articulation != Articulation::Fermata)
                .count() as f32;
            if marks > 0.0 {
                let room = 0.75 + marks * ARTICULATION_GAP;
                if marks_below(event) {
                    below += room;
                } else {
                    above -= room;
                }
            }
            let slurred = event.slur_start
                || event.slur_stop
                || event.phrase_slur_start
                || event.phrase_slur_stop;
            if slurred {
                // As drawn: slurs follow the stem side unless placed explicitly.
                if event.stem_up {
                    above -= SLUR_ROOM;
                } else {
                    below += SLUR_ROOM;
                }
            }
            top = top.min(above);
            bottom = bottom.max(below);
        }
    }
    for (_, _, y, below) in fermatas(voices, 0.0, &|_| 0.0) {
        if below {
            bottom = bottom.max(y + FERMATA_HEIGHT);
        } else {
            top = top.min(y - FERMATA_HEIGHT);
        }
    }
    (top, bottom)
}

/// Font size of accordion register symbols; at the standard size they would be over three
/// spaces tall.
const REGISTER_SIZE: f32 = 3.75;
/// Room between register symbols and the staff or the notes below them.
const REGISTER_GAP: f32 = 0.8;

/// Height of a register symbol as drawn.
pub(crate) fn register_height() -> f32 {
    smufl::REGISTER_DIAMETER * REGISTER_SIZE / 4.0
}

/// Horizontal room a register symbol needs around its column: (left, right).
pub(crate) fn register_extent() -> (f32, f32) {
    let half = register_height() / 2.0;
    let center = smufl::notehead(notula_model::NoteValue::Quarter).1 / 2.0;
    (half - center, half + center)
}

/// Where the bottom of a staff's register symbols lies in a system, relative to its top
/// line: one line for the whole system, above the staff and everything its notes carry
/// (stems, beams, articulations, fermatas, slur ends). `None` when no event in `measures`
/// has a register.
pub(crate) fn register_line<'a>(measures: impl Iterator<Item = &'a [VoicePlan]>) -> Option<f32> {
    let mut any = false;
    let mut top = 0.0_f32;
    for voices in measures {
        any |= voices
            .iter()
            .flat_map(|voice| &voice.events)
            .any(|event| event.register.is_some());
        top = top.min(staff_reach(voices).0);
    }
    any.then_some(top - REGISTER_GAP)
}

/// Draws a register symbol centred on `center_x` with its bottom on `bottom`.
pub(crate) fn register_mark(
    canvas: &mut Canvas,
    event: EventId,
    center_x: f32,
    bottom: f32,
    register: notula_model::Register,
) {
    let scale = REGISTER_SIZE / 4.0;
    let (body, dots) = smufl::register(register);
    let left = center_x - smufl::REGISTER_DIAMETER * scale / 2.0;
    canvas.glyph(Some(event), left, bottom, body, REGISTER_SIZE);
    let half_dot = smufl::REGISTER_DOT_SIZE / 2.0;
    for (x, y) in dots {
        canvas.glyph(
            Some(event),
            left + (x - half_dot) * scale,
            bottom - (y - half_dot) * scale,
            smufl::REGISTER_DOT,
            REGISTER_SIZE,
        );
    }
}

/// Where the top of a metronome mark lies above a staff whose content reaches up to `top`.
pub(crate) fn metronome_top(top: f32) -> f32 {
    top.min(0.0) - METRONOME_CLEARANCE - METRONOME_HEIGHT
}

/// Draws a metronome mark starting at `x`, with `baseline` under the notehead and text.
pub(crate) fn metronome_mark(canvas: &mut Canvas, x: f32, baseline: f32, tempo: Tempo) {
    let scale = METRONOME_SIZE / 4.0;
    let note_y = baseline - smufl::METRONOME_NOTE_DEPTH * scale;
    canvas.glyph(
        None,
        x,
        note_y,
        smufl::metronome_note(tempo.beat_unit),
        METRONOME_SIZE,
    );
    let mut dot_x = x + smufl::METRONOME_NOTE_WIDTH * scale;
    for _ in 0..tempo.beat_unit_dots {
        dot_x += METRONOME_DOT_GAP;
        canvas.glyph(None, dot_x, note_y, smufl::METRONOME_DOT, METRONOME_SIZE);
        dot_x += METRONOME_DOT_ADVANCE * scale;
    }
    canvas.text(
        x + metronome_text_offset(tempo),
        baseline,
        &metronome_text(tempo),
        METRONOME_TEXT_SIZE,
        Anchor::Start,
    );
}
