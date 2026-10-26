export type Anchor = "start" | "middle" | "end";

export type Shape =
  | { type: "line"; x1: number; y1: number; x2: number; y2: number; thickness: number }
  | { type: "polygon"; points: [number, number][] }
  | { type: "white_polygon"; points: [number, number][] }
  | { type: "tie"; points: [number, number][] }
  | { type: "slur"; points: [number, number][] }
  | { type: "phrase_slur"; points: [number, number][] }
  | { type: "knockout"; x: number; y: number; width: number; height: number }
  | { type: "glyph"; x: number; y: number; glyph: string; size: number; centered?: boolean }
  | { type: "text"; x: number; y: number; text: string; size: number; anchor: Anchor };

/** Everything that belongs to a note or rest carries that event's id. */
export type Item = Shape & { event?: number; staff?: number; voice?: number };

export interface MeasureArea {
  staff: number;
  measure: number;
  voice_count: number;
  next_measure: number | null;
  time_signature: { numerator: number; denominator: number };
  time_signature_display: "numeric" | "common" | "alla_breve";
  key_fifths: number;
  x_start: number;
  x_end: number;
  /** Top line of the staff. */
  y_top: number;
  /** Onset and x position of each point in time that has something to draw. */
  columns: [number, number][];
}

/** Coordinates are in staff spaces. */
export interface RenderList {
  width: number;
  height: number;
  items: Item[];
  areas: MeasureArea[];
}

/** Ink extents of a glyph at font size 1, relative to its origin. */
interface Ink {
  left: number;
  right: number;
  top: number;
  bottom: number;
  advance: number;
}

const inkCache = new Map<string, Ink>();

function measureInk(glyph: string): Ink {
  let ink = inkCache.get(glyph);
  if (!ink) {
    const context = document.createElement("canvas").getContext("2d");
    if (!context) return { left: 0, right: 0, top: 0, bottom: 0, advance: 0 };
    context.font = "100px Bravura";
    const metrics = context.measureText(glyph);
    ink = {
      left: -metrics.actualBoundingBoxLeft / 100,
      right: metrics.actualBoundingBoxRight / 100,
      top: -metrics.actualBoundingBoxAscent / 100,
      bottom: metrics.actualBoundingBoxDescent / 100,
      advance: metrics.width / 100,
    };
    inkCache.set(glyph, ink);
  }
  return ink;
}

/**
 * Where a glyph is actually drawn, grown by `margin`. Bravura's text box is about four em
 * tall, so pointer hits must use this instead of the text element. Measure only once the
 * font has loaded.
 */
export function glyphInkBox(
  item: Extract<Shape, { type: "glyph" }>,
  margin: number,
): { x: number; y: number; width: number; height: number } {
  const ink = measureInk(item.glyph);
  const shift = item.centered ? (-ink.advance * item.size) / 2 : 0;
  return {
    x: item.x + shift + ink.left * item.size - margin,
    y: item.y + ink.top * item.size - margin,
    width: (ink.right - ink.left) * item.size + 2 * margin,
    height: (ink.bottom - ink.top) * item.size + 2 * margin,
  };
}
