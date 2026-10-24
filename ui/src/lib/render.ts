export type Anchor = "start" | "middle" | "end";

export type Shape =
  | { type: "line"; x1: number; y1: number; x2: number; y2: number; thickness: number }
  | { type: "polygon"; points: [number, number][] }
  | { type: "glyph"; x: number; y: number; glyph: string; size: number }
  | { type: "text"; x: number; y: number; text: string; size: number; anchor: Anchor };

/** Everything that belongs to a note or rest carries that event's id. */
export type Item = Shape & { event?: number };

export interface MeasureArea {
  staff: number;
  measure: number;
  next_measure: number | null;
  time_signature: { numerator: number; denominator: number };
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
