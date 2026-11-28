import { describe, expect, it } from 'vitest';
import { alongDirection, fit, record, rename, roomReadout, toPlan, wallReadout, zoomAt, type History } from './editor';

describe('readouts while drawing', () => {
  it('shows a wall length, its direction and the corner it makes', () => {
    const r = wallReadout({ x: 0, y: 0 }, { x: 3048, y: 0 }, { x: 3048, y: -3810 }, 'imperial');
    expect(r).toEqual({ length: '12\' 6"', direction: '90°', corner: '90°' });
    const m = wallReadout(null, { x: 0, y: 0 }, { x: 1000, y: 1000 }, 'metric');
    expect(m).toEqual({ length: '1.41 m', direction: '315°', corner: null });
  });
  it('shows a room size and area while dragging', () => {
    expect(roomReadout({ x: 0, y: 0 }, { x: 3658, y: 3048 }, 'imperial')).toBe('12\' 0" × 10\' 0" · 120 sq ft');
    expect(roomReadout({ x: 4000, y: 3000 }, { x: 0, y: 0 }, 'metric')).toBe('4.00 m × 3.00 m · 12.0 m²');
  });
  it('places a typed length along the pointer direction', () => {
    expect(alongDirection({ x: 0, y: 0 }, { x: 0, y: 50 }, 3810)).toEqual({ x: 0, y: 3810 });
    expect(alongDirection({ x: 10, y: 10 }, null, 1000)).toEqual({ x: 1010, y: 10 });
  });
});

describe('view', () => {
  it('zooms around the pointer', () => {
    const v = { x: 0, y: 0, k: 0.1 };
    const before = toPlan(v, 100, 50);
    const z = zoomAt(v, 2, 100, 50);
    expect(z.k).toBe(0.2);
    const after = toPlan(z, 100, 50);
    expect(after.x).toBeCloseTo(before.x);
    expect(after.y).toBeCloseTo(before.y);
  });
  it('fits the drawing on screen', () => {
    const v = fit([{ x: 0, y: 0 }, { x: 10000, y: 5000 }], 400, 300, 0);
    expect(toPlan(v, 200, 150)).toEqual({ x: 5000, y: 2500 });
    expect(v.k).toBeCloseTo(0.04);
  });
});

describe('undo history', () => {
  const s = (n: number) => ({ walls: [{ id: `w${n}` }], openings: [], pins: [], labels: [], rooms: [] });
  it('records steps that changed something and clears redo', () => {
    let h: History = { undo: [], redo: [{ floor: 'f', before: s(0), after: s(1), what: 'x' }] };
    h = record(h, { floor: 'f', before: s(1), after: s(1), what: 'nothing' });
    expect(h.undo).toHaveLength(0);
    h = record(h, { floor: 'f', before: s(1), after: s(2), what: 'draw a wall' });
    expect(h.undo).toHaveLength(1);
    expect(h.redo).toHaveLength(0);
  });
  it('follows things made again under new ids', () => {
    const h = rename({ undo: [{ floor: 'f', before: s(1), after: s(2), what: 'x' }], redo: [] }, { w1: 'w9' });
    expect(JSON.stringify(h)).not.toContain('"w1"');
    expect(JSON.stringify(h)).toContain('"w9"');
  });
});
