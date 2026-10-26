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

pub(crate) fn emit_tie(
    canvas: &mut Canvas,
    event: EventId,
    start_x: f32,
    end_x: f32,
    y: f32,
    stem_up: bool,
) {
    emit_curve(canvas, event, start_x, end_x, y, y, stem_up, false);
}

pub(crate) fn emit_slur(
    canvas: &mut Canvas,
    event: EventId,
    start_x: f32,
    end_x: f32,
    start_y: f32,
    end_y: f32,
    above: bool,
) {
    emit_curve(canvas, event, start_x, end_x, start_y, end_y, above, true);
}

pub(crate) fn emit_phrase_slur(
    canvas: &mut Canvas,
    event: EventId,
    start_x: f32,
    end_x: f32,
    start_y: f32,
    end_y: f32,
    above: bool,
) {
    let width = end_x - start_x;
    if width <= 0.4 {
        return;
    }
    let direction = if above { -1.0 } else { 1.0 };
    let depth = (width * 0.12).clamp(0.8, 3.0);
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

fn emit_curve(
    canvas: &mut Canvas,
    event: EventId,
    start_x: f32,
    end_x: f32,
    start_y: f32,
    end_y: f32,
    direction_hint: bool,
    slur: bool,
) {
    let width = end_x - start_x;
    if width <= 0.4 {
        return;
    }
    let direction = if slur {
        if direction_hint { -1.0 } else { 1.0 }
    } else if direction_hint {
        1.0
    } else {
        -1.0
    };
    let depth = if slur {
        (width * 0.12).clamp(0.5, 2.0)
    } else {
        (width * 0.24).clamp(0.5, 1.0)
    };
    let thickness = if slur { 0.18 } else { 0.12 };
    let base_offset = if slur { 0.0 } else { direction * 0.12 };
    let segments = 12;
    let band = |outer_depth: f32, inner_depth: f32| {
        let mut points = Vec::with_capacity((segments + 1) * 2);
        for segment in 0..=segments {
            let t = segment as f32 / segments as f32;
            let curve = (std::f32::consts::PI * t).sin();
            points.push((
                start_x + width * t,
                start_y + (end_y - start_y) * t + base_offset + direction * outer_depth * curve,
            ));
        }
        for segment in (0..=segments).rev() {
            let t = segment as f32 / segments as f32;
            let curve = (std::f32::consts::PI * t).sin();
            points.push((
                start_x + width * t,
                start_y + (end_y - start_y) * t + base_offset + direction * inner_depth * curve,
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

fn draw_articulations(
    canvas: &mut Canvas,
    ev: &EventPlan,
    y0: f32,
    center_x: f32,
    top: f32,
    bottom: f32,
) {
    let mut above_index = 0;
    let mut below_index = 0;
    for &articulation in &ev.articulations {
        if articulation == Articulation::Fermata {
            continue;
        }
        let below = ev.stem_up;
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

pub(crate) fn fermatas(
    voices: &[VoicePlan],
    y0: f32,
    x_of: &dyn Fn(u32) -> f32,
) -> Vec<(EventId, f32, f32)> {
    voices
        .iter()
        .flat_map(|voice| &voice.events)
        .filter(|event| event.articulations.contains(&Articulation::Fermata))
        .map(|event| {
            let top = voices
                .iter()
                .flat_map(|voice| {
                    voice
                        .events
                        .iter()
                        .enumerate()
                        .filter(|(_, other)| other.onset == event.onset)
                        .map(|(index, _)| upper_extent(voice, index))
                })
                .fold(f32::INFINITY, f32::min);
            let x = x_of(event.onset);
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
            (
                event.id,
                center_x,
                y0 + articulation_y(top - ARTICULATION_GAP, false),
            )
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

fn upper_extent(voice: &VoicePlan, event_index: usize) -> f32 {
    let event = &voice.events[event_index];
    let Body::Chord { notes } = &event.body else {
        return match &event.body {
            Body::Rest { y } => *y,
            Body::Chord { .. } => unreachable!(),
        };
    };
    let top = notes
        .iter()
        .map(|note| note.y)
        .fold(f32::INFINITY, f32::min);
    if !event.stem_up {
        return top;
    }

    if let Some(group) = voice
        .beams
        .iter()
        .find(|group| group.contains(&event_index))
    {
        let levels = group
            .iter()
            .map(|&index| smufl::flag_count(voice.events[index].value))
            .max()
            .unwrap_or(1);
        top.min(
            top - MIN_BEAMED_STEM
                - (levels.saturating_sub(1) as f32 * EXTRA_BEAM_STEM)
                - MAX_BEAM_DROP,
        )
    } else {
        let length =
            STEM_LENGTH + smufl::flag_count(event.value).saturating_sub(1) as f32 * EXTRA_BEAM_STEM;
        top.min((top - length).min(MIDDLE_LINE))
    }
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
    for (ev, &beamed) in voice.events.iter().zip(&beamed) {
        emit_event(canvas, ev, x_of(ev.onset), y0, !beamed);
    }
    for group in &voice.beams {
        emit_beam(canvas, voice, group, y0, x_of);
    }
}

fn emit_event(canvas: &mut Canvas, ev: &EventPlan, x: f32, y0: f32, own_stem: bool) {
    match &ev.body {
        Body::Rest { y } => {
            let (glyph, width, _) = smufl::rest(ev.value);
            canvas.glyph(Some(ev.id), x, y0 + y, glyph, 4.0);
            draw_dots(canvas, ev, x, &[y0 + 1.5]);
            draw_articulations(canvas, ev, y0, x + width / 2.0, *y, *y);
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

            let top = notes
                .iter()
                .map(|note| note.y)
                .fold(f32::INFINITY, f32::min);
            let bottom = notes
                .iter()
                .map(|note| note.y)
                .fold(f32::NEG_INFINITY, f32::max);
            draw_articulations(
                canvas,
                ev,
                y0,
                (notehead_left + notehead_right) / 2.0,
                top,
                bottom,
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
                Body::Chord { notes } => Some((ev, stem_geometry(ev, notes, x_of(ev.onset)))),
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
            let column = x_of(event.onset);
            column + event.right >= x && column - event.left <= right
        })
        .map(|(voice, index)| upper_extent(voice, index))
        .fold(0.0, f32::min);
    y0 + top - METRONOME_CLEARANCE
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
