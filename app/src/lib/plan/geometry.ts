// Floor plan geometry for the editor's live readouts. Mirrors
// crates/plan (the Rust core keeps the stored numbers and does the binder's
// sums); both are checked against crates/plan/tests/vectors.json, so what
// the editor shows while you drag is what the core will say.
//
// Coordinates are whole millimetres, x right and y down (as on screen).

export interface Pt {
  x: number;
  y: number;
}
export type Units = 'imperial' | 'metric';

export const MM_PER_INCH = 25.4;
export const MAX_COORD = 1_000_000;

export const pt = (x: number, y: number): Pt => ({ x: Math.round(x), y: Math.round(y) });

export function distance(a: Pt, b: Pt): number {
  return Math.hypot(b.x - a.x, b.y - a.y);
}

export function twiceSignedArea(poly: Pt[]): number {
  if (poly.length < 3) return 0;
  let s = 0;
  for (let i = 0; i < poly.length; i++) {
    const p = poly[i];
    const q = poly[(i + 1) % poly.length];
    s += p.x * q.y - q.x * p.y;
  }
  return s;
}

export function areaMm2(poly: Pt[]): number {
  return Math.abs(twiceSignedArea(poly)) / 2;
}

export function perimeter(poly: Pt[]): number {
  if (poly.length < 2) return 0;
  let s = 0;
  for (let i = 0; i < poly.length; i++) s += distance(poly[i], poly[(i + 1) % poly.length]);
  return s;
}

/** Direction from a to b as seen on screen: 0° right, 90° up, in [0, 360). */
export function bearingDeg(a: Pt, b: Pt): number {
  let d = (Math.atan2(-(b.y - a.y), b.x - a.x) * 180) / Math.PI;
  if (d < 0) d += 360;
  return d >= 360 ? 0 : d;
}

/** The angle at `corner` between the walls to `prev` and `next`, in [0, 180]. */
export function cornerAngle(prev: Pt, corner: Pt, next: Pt): number {
  const ux = prev.x - corner.x;
  const uy = prev.y - corner.y;
  const vx = next.x - corner.x;
  const vy = next.y - corner.y;
  const cross = ux * vy - uy * vx;
  const dot = ux * vx + uy * vy;
  if (cross === 0 && dot === 0) return 0;
  return (Math.atan2(Math.abs(cross), dot) * 180) / Math.PI;
}

export function pointAlong(a: Pt, b: Pt, offset: number): Pt {
  const len = distance(a, b);
  if (len === 0) return a;
  const t = offset / len;
  return pt(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
}

/** Distance along a-b (clamped) and distance from it. */
export function project(p: Pt, a: Pt, b: Pt): { along: number; off: number } {
  const len = distance(a, b);
  if (len === 0) return { along: 0, off: distance(p, a) };
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const t = Math.min(1, Math.max(0, ((p.x - a.x) * dx + (p.y - a.y) * dy) / (len * len)));
  return { along: t * len, off: Math.hypot(p.x - (a.x + dx * t), p.y - (a.y + dy * t)) };
}

function orient(a: Pt, b: Pt, c: Pt): number {
  return (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
}

function onSegment(p: Pt, a: Pt, b: Pt): boolean {
  return (
    orient(a, b, p) === 0 &&
    p.x >= Math.min(a.x, b.x) &&
    p.x <= Math.max(a.x, b.x) &&
    p.y >= Math.min(a.y, b.y) &&
    p.y <= Math.max(a.y, b.y)
  );
}

/** Inside the polygon or on its outline. */
export function contains(poly: Pt[], p: Pt): boolean {
  if (poly.length < 3) return false;
  let inside = false;
  for (let i = 0; i < poly.length; i++) {
    const a = poly[i];
    const b = poly[(i + 1) % poly.length];
    if (onSegment(p, a, b)) return true;
    if (a.y > p.y !== b.y > p.y) {
      const o = orient(a, b, p);
      if ((b.y > a.y && o > 0) || (b.y < a.y && o < 0)) inside = !inside;
    }
  }
  return inside;
}

export function rectangle(a: Pt, b: Pt): Pt[] {
  const x0 = Math.min(a.x, b.x);
  const x1 = Math.max(a.x, b.x);
  const y0 = Math.min(a.y, b.y);
  const y1 = Math.max(a.y, b.y);
  if (x0 === x1 || y0 === y1) return [];
  return [pt(x0, y0), pt(x1, y0), pt(x1, y1), pt(x0, y1)];
}

/** An L: the rectangle a-b with its top-right quarter cut away (the inner
 * corner can then be dragged to any size). */
export function lShape(a: Pt, b: Pt): Pt[] {
  const r = rectangle(a, b);
  if (!r.length) return [];
  const [tl, , br] = [r[0], r[1], r[2]];
  const mx = Math.round((tl.x + br.x) / 2);
  const my = Math.round((tl.y + br.y) / 2);
  return [pt(tl.x, tl.y), pt(mx, tl.y), pt(mx, my), pt(br.x, my), pt(br.x, br.y), pt(tl.x, br.y)];
}

/** Nearest whole millimetre to the multiple of `step` nearest to v. */
export function roundTo(v: number, step: number): number {
  if (!(step > 0)) return v;
  return Math.round(Math.round(v / step) * step);
}

export interface SnapOptions {
  grid: number | null;
  endpointTolerance: number;
  angleStep: number | null;
  angleTolerance: number;
}
export type SnapKind = 'free' | 'endpoint' | 'angle' | 'grid';

/** Mirrors housewright_plan::snap: corner, then 90°/45°, then grid. */
export function snap(p: Pt, anchor: Pt | null, corners: Pt[], o: SnapOptions): { point: Pt; kind: SnapKind } {
  if (o.endpointTolerance > 0) {
    let best: Pt | null = null;
    let bd = Infinity;
    for (const c of corners) {
      if (anchor && c.x === anchor.x && c.y === anchor.y) continue;
      const d = distance(c, p);
      if (d <= o.endpointTolerance && d < bd) {
        best = c;
        bd = d;
      }
    }
    if (best) return { point: best, kind: 'endpoint' };
  }
  const grid = o.grid && o.grid > 0 ? o.grid : null;
  if (anchor && o.angleStep && o.angleStep > 0) {
    const dx = p.x - anchor.x;
    const dy = p.y - anchor.y;
    if (Math.hypot(dx, dy) > 0) {
      const ang = (Math.atan2(dy, dx) * 180) / Math.PI;
      const s = Math.round(ang / o.angleStep) * o.angleStep;
      if (Math.abs(ang - s) <= o.angleTolerance) {
        const r = (s * Math.PI) / 180;
        const ux = Math.cos(r);
        const uy = Math.sin(r);
        let l = dx * ux + dy * uy;
        if (grid) l = roundTo(Math.round(l), grid);
        return { point: pt(anchor.x + ux * l, anchor.y + uy * l), kind: 'angle' };
      }
    }
  }
  if (grid) return { point: pt(roundTo(p.x, grid), roundTo(p.y, grid)), kind: 'grid' };
  return { point: p, kind: 'free' };
}

// -- lengths and areas -----------------------------------------------------

export function formatLength(mm: number, units: Units): string {
  const neg = mm < 0;
  const a = Math.abs(mm);
  let s: string;
  if (units === 'imperial') {
    const inches = Math.round(a / MM_PER_INCH);
    const ft = Math.floor(inches / 12);
    const inch = inches % 12;
    s = ft === 0 ? `${inch}"` : `${ft}' ${inch}"`;
  } else {
    s = `${(a / 1000).toFixed(2)} m`;
  }
  return neg ? `-${s}` : s;
}

export function formatArea(mm2: number, units: Units): string {
  if (units === 'imperial') {
    const sqft = Math.round(mm2 / (MM_PER_INCH * 12) ** 2);
    return `${sqft.toLocaleString('en-US')} sq ft`;
  }
  return `${(mm2 / 1_000_000).toFixed(1)} m²`;
}

type Unit = 'ft' | 'in' | 'm' | 'cm' | 'mm';
const UNIT_MM: Record<Unit, number> = { ft: MM_PER_INCH * 12, in: MM_PER_INCH, m: 1000, cm: 10, mm: 1 };
const metric = (u: Unit) => u === 'm' || u === 'cm' || u === 'mm';
const WORDS: Record<string, Unit> = {
  "'": 'ft', '’': 'ft', '′': 'ft', ft: 'ft', foot: 'ft', feet: 'ft',
  '"': 'in', '”': 'in', '″': 'in', "''": 'in', in: 'in', inch: 'in', inches: 'in',
  m: 'm', meter: 'm', meters: 'm', metre: 'm', metres: 'm',
  cm: 'cm', centimeter: 'cm', centimeters: 'cm', centimetre: 'cm', centimetres: 'cm',
  mm: 'mm', millimeter: 'mm', millimeters: 'mm', millimetre: 'mm', millimetres: 'mm',
};
const VULGAR: Record<string, number> = { '½': 0.5, '¼': 0.25, '¾': 0.75, '⅛': 0.125, '⅜': 0.375, '⅝': 0.625, '⅞': 0.875 };

type Tok = { num: number; frac: boolean } | { unit: Unit };

export class LengthError extends Error {}

function lex(src: string): Tok[] {
  const bad = () => new LengthError(`could not read "${src.trim()}" as a length`);
  const s = [...src.trim().toLowerCase()];
  const out: Tok[] = [];
  let i = 0;
  while (i < s.length) {
    const c = s[i];
    if (/\s|,/.test(c)) {
      i++;
    } else if (/[0-9.]/.test(c)) {
      const start = i;
      while (i < s.length && /[0-9.]/.test(s[i])) i++;
      const a = s.slice(start, i).join('');
      if (!/^\d*\.?\d+$|^\d+\.$/.test(a)) throw bad();
      const av = Number(a);
      if (s[i] === '/') {
        i++;
        const s2 = i;
        while (i < s.length && /[0-9]/.test(s[i])) i++;
        const b = Number(s.slice(s2, i).join(''));
        if (!b || i === s2) throw bad();
        out.push({ num: av / b, frac: true });
      } else out.push({ num: av, frac: false });
    } else if (c in VULGAR) {
      out.push({ num: VULGAR[c], frac: true });
      i++;
    } else if (/\p{L}/u.test(c)) {
      const start = i;
      while (i < s.length && /\p{L}/u.test(s[i])) i++;
      const w = s.slice(start, i).join('');
      if (!(w in WORDS)) throw bad();
      out.push({ unit: WORDS[w] });
    } else {
      let w = c;
      if (c === "'" && s[i + 1] === "'") {
        w = "''";
        i += 2;
      } else i++;
      if (!(w in WORDS)) throw bad();
      out.push({ unit: WORDS[w] });
    }
  }
  const joined: Tok[] = [];
  for (const t of out) {
    const last = joined[joined.length - 1];
    if (last && 'num' in last && !last.frac && Number.isInteger(last.num) && 'num' in t && t.frac) {
      last.num += t.num;
      last.frac = true;
    } else joined.push(t);
  }
  return joined;
}

export const MAX_LENGTH_MM = 1_000_000;

/** Read a typed length ("12' 6\"", "3.81 m", "381 cm") into millimetres.
 * Mirrors housewright_plan::parse_length. Throws LengthError. */
export function parseLength(src: string, units: Units): number {
  const toks = lex(src);
  if (!toks.length) throw new LengthError('type a length, like 12\' 6" or 3.81 m');
  const bad = () => new LengthError(`could not read "${src.trim()}" as a length`);
  let total = 0;
  let last: Unit | null = null;
  let system: boolean | null = null;
  let i = 0;
  while (i < toks.length) {
    const t = toks[i];
    if (!('num' in t)) throw bad();
    let unit: Unit;
    const nxt = toks[i + 1];
    if (nxt && 'unit' in nxt) {
      unit = nxt.unit;
      i += 2;
    } else {
      i += 1;
      if (last === null) unit = units === 'imperial' ? 'ft' : 'm';
      else if (last === 'ft') unit = 'in';
      else if (last === 'm') unit = 'cm';
      else if (last === 'cm') unit = 'mm';
      else throw bad();
    }
    if (last !== null && UNIT_MM[unit] >= UNIT_MM[last]) throw bad();
    if (system !== null && system !== metric(unit)) {
      throw new LengthError('use either feet and inches or metres, not both');
    }
    system = metric(unit);
    total += t.num * UNIT_MM[unit];
    last = unit;
  }
  const mm = Math.round(total);
  if (mm > MAX_LENGTH_MM) throw new LengthError('that is longer than a kilometre');
  return mm;
}

/** Like parseLength, but null instead of throwing. */
export function tryLength(src: string, units: Units): number | null {
  try {
    return parseLength(src, units);
  } catch {
    return null;
  }
}

/** The angle readout while drawing: "90°", or "37.5°". */
export function angleText(deg: number): string {
  const r = Math.round(deg * 2) / 2;
  return `${Number.isInteger(r) ? r : r.toFixed(1)}°`;
}

/** The grid step for the household's units: 6 inches or 10 cm. */
export function gridStep(units: Units): number {
  return units === 'imperial' ? 6 * MM_PER_INCH : 100;
}

/** Room label position: centroid, or the middle of the widest stretch
 * through it when the centroid falls outside (mirrors label_point). */
export function labelPoint(poly: Pt[]): Pt {
  const a2 = twiceSignedArea(poly);
  let cx = 0;
  let cy = 0;
  if (a2 === 0 || poly.length < 3) {
    for (const p of poly) {
      cx += p.x;
      cy += p.y;
    }
    return pt(cx / Math.max(1, poly.length), cy / Math.max(1, poly.length));
  }
  for (let i = 0; i < poly.length; i++) {
    const p = poly[i];
    const q = poly[(i + 1) % poly.length];
    const c = p.x * q.y - q.x * p.y;
    cx += (p.x + q.x) * c;
    cy += (p.y + q.y) * c;
  }
  cx /= 3 * a2;
  cy /= 3 * a2;
  const c = pt(cx, cy);
  if (contains(poly, c)) return c;
  const xs: number[] = [];
  for (let i = 0; i < poly.length; i++) {
    const a = poly[i];
    const b = poly[(i + 1) % poly.length];
    if (a.y > cy !== b.y > cy) xs.push(a.x + ((cy - a.y) / (b.y - a.y)) * (b.x - a.x));
  }
  xs.sort((m, n) => m - n);
  let best: [number, number] | null = null;
  for (let i = 0; i + 1 < xs.length; i += 2) {
    if (!best || xs[i + 1] - xs[i] > best[1] - best[0]) best = [xs[i], xs[i + 1]];
  }
  return best ? pt((best[0] + best[1]) / 2, cy) : c;
}

/** Whether an outline crosses itself (mirrors validate_polygon's main check). */
export function selfIntersects(poly: Pt[]): boolean {
  const n = poly.length;
  const touch = (p1: Pt, p2: Pt, q1: Pt, q2: Pt) => {
    const d1 = Math.sign(orient(q1, q2, p1));
    const d2 = Math.sign(orient(q1, q2, p2));
    const d3 = Math.sign(orient(p1, p2, q1));
    const d4 = Math.sign(orient(p1, p2, q2));
    if (d1 * d2 < 0 && d3 * d4 < 0) return true;
    return (
      (d1 === 0 && onSegment(p1, q1, q2)) ||
      (d2 === 0 && onSegment(p2, q1, q2)) ||
      (d3 === 0 && onSegment(q1, p1, p2)) ||
      (d4 === 0 && onSegment(q2, p1, p2))
    );
  };
  for (let i = 0; i < n; i++) {
    for (let j = i + 1; j < n; j++) {
      const neighbours = j === i + 1 || (i === 0 && j === n - 1);
      if (neighbours) continue;
      if (touch(poly[i], poly[(i + 1) % n], poly[j], poly[(j + 1) % n])) return true;
    }
  }
  return false;
}
