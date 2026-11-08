//! Barlines at measure ends, repeat signs, and the room they take in a measure.

use notula_model::{Barline, MeasureInfo};

use crate::render::Canvas;
use crate::smufl;

/// SMuFL `repeatDot`, centred on its baseline.
const REPEAT_DOT: char = '\u{E044}';
const DOT_WIDTH: f32 = 0.4;
/// Room between a repeat's dots and its thin line.
const DOT_GAP: f32 = 0.3;
/// Room between two lines of a barline.
const LINE_GAP: f32 = smufl::FINAL_BARLINE_GAP;
/// Room between a start repeat's dots and the first note.
const START_TRAIL: f32 = 0.6;
/// The dots sit in the second and third spaces of the staff.
const DOT_HEIGHTS: [f32; 2] = [1.5, 2.5];

/// How a measure ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum End {
    Regular,
    Double,
    Final,
    Repeat,
}

/// The barline at the end of a measure: a repeat end, or its own style; the last measure of
/// the score always ends with a final barline.
pub(crate) fn end(info: MeasureInfo, last: bool) -> End {
    if info.bars.repeat_end.is_some() {
        End::Repeat
    } else if last {
        End::Final
    } else {
        match info.bars.barline {
            Barline::Regular => End::Regular,
            Barline::Double => End::Double,
            Barline::Final => End::Final,
        }
    }
}

/// Room the barline takes inside the measure, before its end.
pub(crate) fn end_room(end: End) -> f32 {
    match end {
        End::Regular => 0.0,
        End::Double => smufl::THIN_BARLINE + LINE_GAP + smufl::THIN_BARLINE,
        End::Final => smufl::THICK_BARLINE + LINE_GAP + smufl::THIN_BARLINE,
        End::Repeat => {
            smufl::THICK_BARLINE + LINE_GAP + smufl::THIN_BARLINE + DOT_GAP + DOT_WIDTH + 0.3
        }
    }
}

/// Room a repeat start takes at the start of a measure: thick and thin line, dots.
pub(crate) const START_ROOM: f32 =
    smufl::THICK_BARLINE + LINE_GAP + smufl::THIN_BARLINE + DOT_GAP + DOT_WIDTH + START_TRAIL;

fn dots(canvas: &mut Canvas, x: f32, staff_tops: &[f32]) {
    for &top in staff_tops {
        for height in DOT_HEIGHTS {
            canvas.glyph(None, x, top + height, REPEAT_DOT, 4.0);
        }
    }
}

/// Draws the barline ending at `x` across one part, from `top` to `bottom`, with repeat dots
/// on each of its staves.
pub(crate) fn draw_end(
    canvas: &mut Canvas,
    end: End,
    x: f32,
    (top, bottom): (f32, f32),
    staff_tops: &[f32],
) {
    let thin = |canvas: &mut Canvas, x: f32| {
        canvas.line(None, (x, top), (x, bottom), smufl::THIN_BARLINE);
    };
    match end {
        End::Regular => thin(canvas, x),
        End::Double => {
            thin(canvas, x - smufl::THIN_BARLINE / 2.0);
            thin(canvas, x - smufl::THIN_BARLINE * 1.5 - LINE_GAP);
        }
        End::Final | End::Repeat => {
            let thick = x - smufl::THICK_BARLINE / 2.0;
            let thin_x = thick - smufl::THICK_BARLINE / 2.0 - LINE_GAP - smufl::THIN_BARLINE / 2.0;
            thin(canvas, thin_x);
            canvas.line(None, (thick, top), (thick, bottom), smufl::THICK_BARLINE);
            if end == End::Repeat {
                dots(
                    canvas,
                    thin_x - smufl::THIN_BARLINE / 2.0 - DOT_GAP - DOT_WIDTH,
                    staff_tops,
                );
            }
        }
    }
}

/// Draws a repeat start beginning at `x`: a thick line (left out when it would double a thick
/// line ending at `x`), a thin line, and dots.
pub(crate) fn draw_start(
    canvas: &mut Canvas,
    x: f32,
    (top, bottom): (f32, f32),
    staff_tops: &[f32],
    thick: bool,
) {
    if thick {
        let thick_x = x + smufl::THICK_BARLINE / 2.0;
        canvas.line(
            None,
            (thick_x, top),
            (thick_x, bottom),
            smufl::THICK_BARLINE,
        );
    }
    let thin_x = x + smufl::THICK_BARLINE + LINE_GAP + smufl::THIN_BARLINE / 2.0;
    canvas.line(None, (thin_x, top), (thin_x, bottom), smufl::THIN_BARLINE);
    dots(
        canvas,
        thin_x + smufl::THIN_BARLINE / 2.0 + DOT_GAP,
        staff_tops,
    );
}
