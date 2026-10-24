use noten_model::{EventId, StaffId, TimeSignature};
use serde::Serialize;

/// Coordinates are in staff spaces (the distance between two staff lines), y pointing down.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderList {
    pub width: f32,
    pub height: f32,
    pub items: Vec<Item>,
    /// Where each measure of each staff ended up, for hit-testing and the input cursor.
    pub areas: Vec<MeasureArea>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MeasureArea {
    pub staff: StaffId,
    pub measure: usize,
    /// Following measure in this system, if this is not the system's final bar.
    pub next_measure: Option<usize>,
    pub time_signature: TimeSignature,
    /// The part of the measure after clef and signatures, up to the barline.
    pub x_start: f32,
    pub x_end: f32,
    /// Top line of the staff.
    pub y_top: f32,
    /// Onset and x position of each point in time that has something to draw.
    pub columns: Vec<(u32, f32)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    /// Set for everything that belongs to a note or rest, for hit-testing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<EventId>,
    #[serde(flatten)]
    pub shape: Shape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    Start,
    Middle,
    End,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Shape {
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        thickness: f32,
    },
    Polygon {
        points: Vec<[f32; 2]>,
    },
    /// A SMuFL glyph; `size` is the font size in staff spaces (4 for the standard size).
    Glyph {
        x: f32,
        y: f32,
        glyph: char,
        size: f32,
    },
    Text {
        x: f32,
        y: f32,
        text: String,
        size: f32,
        anchor: Anchor,
    },
}

fn round(value: f32) -> f32 {
    (value * 1000.0).round() / 1000.0
}

#[derive(Default)]
pub(crate) struct Canvas {
    pub items: Vec<Item>,
    pub areas: Vec<MeasureArea>,
}

impl Canvas {
    pub fn line(
        &mut self,
        event: Option<EventId>,
        from: (f32, f32),
        to: (f32, f32),
        thickness: f32,
    ) {
        self.items.push(Item {
            event,
            shape: Shape::Line {
                x1: round(from.0),
                y1: round(from.1),
                x2: round(to.0),
                y2: round(to.1),
                thickness,
            },
        });
    }

    pub fn polygon(&mut self, event: Option<EventId>, points: &[(f32, f32)]) {
        self.items.push(Item {
            event,
            shape: Shape::Polygon {
                points: points.iter().map(|&(x, y)| [round(x), round(y)]).collect(),
            },
        });
    }

    pub fn glyph(&mut self, event: Option<EventId>, x: f32, y: f32, glyph: char, size: f32) {
        self.items.push(Item {
            event,
            shape: Shape::Glyph {
                x: round(x),
                y: round(y),
                glyph,
                size: round(size),
            },
        });
    }

    pub fn text(&mut self, x: f32, y: f32, text: &str, size: f32, anchor: Anchor) {
        self.items.push(Item {
            event: None,
            shape: Shape::Text {
                x: round(x),
                y: round(y),
                text: text.to_owned(),
                size,
                anchor,
            },
        });
    }
}
