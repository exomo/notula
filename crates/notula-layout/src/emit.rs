//! Turns planned events into drawing primitives once their horizontal positions are known.

use notula_model::NoteValue;

use crate::engrave::{Body, EventPlan, NotePlan, VoicePlan, dot_advance};
use crate::render::Canvas;
use crate::smufl;

const STEM_LENGTH: f32 = 3.5;
const MIN_BEAMED_STEM: f32 = 3.0;
const EXTRA_BEAM_STEM: f32 = 0.75;
const BEAM_STUB: f32 = 1.0;
const MAX_BEAM_DROP: f32 = 1.0;
const MIDDLE_LINE: f32 = 2.0;

struct Stem {
    x: f32,
    /// Where the stem leaves the noteheads, relative to the staff top.
    base_y: f32,
    /// The notehead nearest to the stem end.
    near_y: f32,
}

fn head_x(note: &NotePlan, ev: &EventPlan, x: f32, head_width: f32) -> f32 {
    match (note.displaced, ev.stem_up) {
        (false, _) => x,
        (true, true) => x + head_width - smufl::STEM_THICKNESS,
        (true, false) => x - head_width + smufl::STEM_THICKNESS,
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
            let (glyph, _, _) = smufl::rest(ev.value);
            canvas.glyph(Some(ev.id), x, y0 + y, glyph, 4.0);
            draw_dots(canvas, ev, x, &[y0 + 1.5]);
        }
        Body::Chord { notes } => {
            let (head, head_width) = smufl::notehead(ev.value);
            let mut ledgers: Vec<(f32, f32, f32)> = Vec::new();
            let mut dot_ys: Vec<f32> = Vec::new();

            for note in notes {
                let hx = head_x(note, ev, x, head_width);
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
