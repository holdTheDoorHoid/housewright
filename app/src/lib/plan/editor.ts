// Pure helpers for the floor plan editor: the view (pan and zoom), the
// readouts shown while drawing, and the undo history. Kept out of the
// component so they can be tested on their own.

import type { PlanState } from '../api';
import {
  angleText,
  areaMm2,
  bearingDeg,
  cornerAngle,
  distance,
  formatArea,
  formatLength,
  rectangle,
  type Pt,
  type Units,
} from './geometry';

/** What part of the plan is on screen: the plan point at the top-left
 * corner and the zoom (screen pixels per plan millimetre). */
export interface View {
  x: number;
  y: number;
  k: number;
}

export const MIN_K = 0.004; // about 250 m across a phone
export const MAX_K = 2; // a millimetre is two pixels

export function clampK(k: number): number {
  return Math.min(MAX_K, Math.max(MIN_K, k));
}

/** Screen pixel (relative to the drawing's corner) to plan millimetres. */
export function toPlan(v: View, sx: number, sy: number): Pt {
  return { x: v.x + sx / v.k, y: v.y + sy / v.k };
}

/** Zoom by `f` keeping the plan point under screen pixel (sx, sy) still. */
export function zoomAt(v: View, f: number, sx: number, sy: number): View {
  const k = clampK(v.k * f);
  const p = toPlan(v, sx, sy);
  return { k, x: p.x - sx / k, y: p.y - sy / k };
}

/** A view showing `pts` (or a 6 m square at the origin) in a w x h screen. */
export function fit(pts: Pt[], w: number, h: number, pad = 40): View {
  if (!pts.length || w <= 0 || h <= 0) {
    const k = clampK(Math.min(w || 360, h || 360) / 7000);
    return { k, x: -500, y: -500 };
  }
  let x0 = Infinity;
  let y0 = Infinity;
  let x1 = -Infinity;
  let y1 = -Infinity;
  for (const p of pts) {
    x0 = Math.min(x0, p.x);
    y0 = Math.min(y0, p.y);
    x1 = Math.max(x1, p.x);
    y1 = Math.max(y1, p.y);
  }
  const bw = Math.max(x1 - x0, 2000);
  const bh = Math.max(y1 - y0, 2000);
  const k = clampK(Math.min((w - 2 * pad) / bw, (h - 2 * pad) / bh));
  const cx = (x0 + x1) / 2;
  const cy = (y0 + y1) / 2;
  return { k, x: cx - w / 2 / k, y: cy - h / 2 / k };
}

/** The readout while drawing a wall from `anchor` to `cursor` (after
 * `prev`, the wall before it, if any). */
export function wallReadout(
  prev: Pt | null,
  anchor: Pt,
  cursor: Pt,
  units: Units,
): { length: string; direction: string; corner: string | null } {
  const len = distance(anchor, cursor);
  return {
    length: formatLength(Math.round(len), units),
    direction: angleText(bearingDeg(anchor, cursor)),
    corner: prev && len > 0 ? angleText(cornerAngle(prev, anchor, cursor)) : null,
  };
}

/** The readout while dragging out a room. */
export function roomReadout(a: Pt, b: Pt, units: Units): string {
  const r = rectangle(a, b);
  const w = Math.abs(b.x - a.x);
  const h = Math.abs(b.y - a.y);
  return `${formatLength(w, units)} × ${formatLength(h, units)} · ${formatArea(areaMm2(r), units)}`;
}

/** The point `len` millimetres from `anchor` towards `toward` (straight
 * right when they are the same point): a typed wall length. */
export function alongDirection(anchor: Pt, toward: Pt | null, len: number): Pt {
  let dx = 1;
  let dy = 0;
  if (toward) {
    const d = distance(anchor, toward);
    if (d > 0) {
      dx = (toward.x - anchor.x) / d;
      dy = (toward.y - anchor.y) / d;
    }
  }
  return { x: Math.round(anchor.x + dx * len), y: Math.round(anchor.y + dy * len) };
}

// -- undo and redo --------------------------------------------------------

/** One step: the floor's stored state before and after it. */
export interface Step {
  floor: string;
  before: PlanState;
  after: PlanState;
  what: string;
}

export interface History {
  undo: Step[];
  redo: Step[];
}

export const MAX_STEPS = 50;

export function record(h: History, s: Step): History {
  if (JSON.stringify(s.before) === JSON.stringify(s.after)) return h;
  return { undo: [...h.undo, s].slice(-MAX_STEPS), redo: [] };
}

/** Undoing a delete makes the thing again under a new id (deletes are
 * final in the core); rewrite every remembered step to use the new ids. */
export function rename(h: History, renamed: Record<string, string>): History {
  const ids = Object.entries(renamed);
  if (!ids.length) return h;
  const fix = (s: Step): Step => {
    let t = JSON.stringify(s);
    for (const [from, to] of ids) t = t.split(from).join(to);
    return JSON.parse(t) as Step;
  };
  return { undo: h.undo.map(fix), redo: h.redo.map(fix) };
}
