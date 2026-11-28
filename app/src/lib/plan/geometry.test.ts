import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  angleText,
  areaMm2,
  contains,
  cornerAngle,
  formatArea,
  formatLength,
  labelPoint,
  lShape,
  parseLength,
  perimeter,
  pt,
  rectangle,
  roundTo,
  selfIntersects,
  snap,
  tryLength,
  type Pt,
  type Units,
} from './geometry';

// The same vectors the Rust crate checks (crates/plan/tests/props.rs).
const vectors = JSON.parse(
  readFileSync(resolve(import.meta.dirname, '../../../../crates/plan/tests/vectors.json'), 'utf8'),
) as {
  parse: [string, Units, number][];
  parse_errors: [string, Units][];
  format: [number, Units, string][];
  area: [[number, number][], number, string, string][];
  angle: [[number, number], [number, number], [number, number], number][];
};

const P = ([x, y]: [number, number]): Pt => ({ x, y });

describe('shared vectors with the Rust core', () => {
  it.each(vectors.parse)('reads %s (%s)', (s, u, mm) => {
    expect(parseLength(s, u)).toBe(mm);
  });
  it.each(vectors.parse_errors)('refuses %s (%s)', (s, u) => {
    expect(() => parseLength(s, u)).toThrow();
  });
  it.each(vectors.format)('writes %d mm (%s) as %s', (mm, u, s) => {
    expect(formatLength(mm, u)).toBe(s);
  });
  it.each(vectors.area)('areas', (poly, mm2, sqft, m2) => {
    const p = poly.map(P);
    expect(areaMm2(p)).toBe(mm2);
    expect(formatArea(mm2, 'imperial')).toBe(sqft);
    expect(formatArea(mm2, 'metric')).toBe(m2);
  });
  it.each(vectors.angle)('angles', (a, b, c, deg) => {
    expect(cornerAngle(P(a), P(b), P(c))).toBeCloseTo(deg, 9);
  });
});

describe('length typing', () => {
  it('reads the ways people write lengths', () => {
    expect(parseLength('12\' 6"', 'imperial')).toBe(3810);
    expect(parseLength('12’ 6”', 'imperial')).toBe(3810);
    expect(parseLength('6½"', 'imperial')).toBe(165);
    expect(parseLength('3.81', 'metric')).toBe(3810);
    expect(parseLength('3.81 m', 'imperial')).toBe(3810);
    expect(tryLength('banana', 'imperial')).toBeNull();
    expect(() => parseLength('3 m 2 ft', 'metric')).toThrow('either feet and inches or metres');
    expect(() => parseLength('', 'metric')).toThrow('type a length');
  });
  it('survives a round trip within the display precision', () => {
    for (let mm = 0; mm < 40000; mm += 37) {
      expect(Math.abs(parseLength(formatLength(mm, 'metric'), 'metric') - mm)).toBeLessThanOrEqual(5);
      expect(Math.abs(parseLength(formatLength(mm, 'imperial'), 'imperial') - mm)).toBeLessThanOrEqual(13.2);
    }
  });
});

describe('shapes and readouts', () => {
  it('rectangles and L shapes', () => {
    const r = rectangle(pt(4000, 3000), pt(0, 0));
    expect(areaMm2(r)).toBe(12_000_000);
    expect(perimeter(r)).toBe(14_000);
    const l = lShape(pt(0, 0), pt(6000, 4000));
    expect(areaMm2(l)).toBe(18_000_000);
    expect(selfIntersects(l)).toBe(false);
    expect(contains(l, labelPoint(l))).toBe(true);
    expect(selfIntersects([pt(0, 0), pt(10, 10), pt(10, 0), pt(0, 10)])).toBe(true);
  });
  it('snaps like the core', () => {
    const o = { grid: 152.4, endpointTolerance: 150, angleStep: 45, angleTolerance: 6 };
    expect(roundTo(600, 152.4)).toBe(610);
    expect(snap(pt(1040, 990), pt(0, 0), [pt(1000, 1000)], o)).toEqual({ point: pt(1000, 1000), kind: 'endpoint' });
    const s = snap(pt(3004, 80), pt(0, 0), [], { ...o, grid: 10 });
    expect(s).toEqual({ point: pt(3000, 0), kind: 'angle' });
    expect(snap(pt(2003, 1001), pt(0, 0), [], { ...o, grid: 10 }).kind).toBe('grid');
  });
  it('angle text', () => {
    expect(angleText(90)).toBe('90°');
    expect(angleText(37.4)).toBe('37.5°');
  });
});
