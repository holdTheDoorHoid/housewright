import { describe, it, expect, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/svelte';
import Plans from './Plans.svelte';
import { useBackend, type PlanInfo } from '../lib/api';
import { FakeBackend } from '../lib/api/fake';

// jsdom has no ResizeObserver (bind:clientWidth uses it).
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
} as unknown as typeof ResizeObserver;

const floor = { id: 'f1', name: 'Ground floor', level: 0, drawn: true, rooms: 1, area_text: '120 sq ft' };

function plan(over: Partial<PlanInfo> = {}): PlanInfo {
  return {
    floor,
    floors: [floor, { ...floor, id: 'f2', name: 'Basement', level: -1, drawn: false, rooms: 0, area_text: null }],
    units: 'imperial',
    rooms: [
      {
        id: 'r1',
        name: 'Kitchen',
        outline: [{ x: 0, y: 0 }, { x: 3658, y: 0 }, { x: 3658, y: 3048 }, { x: 0, y: 3048 }],
        area_mm2: 11_149_584,
        area_text: '120 sq ft',
        perimeter_text: '44\' 0"',
        label_at: { x: 1829, y: 1524 },
        things: 2,
      },
    ],
    walls: [{ id: 'w1', a: { x: 0, y: 0 }, b: { x: 3658, y: 0 }, thickness: 115, length_mm: 3658, length_text: '12\' 0"' }],
    openings: [],
    pins: [
      { id: 'p1', number: 1, at: { x: 500, y: 500 }, item_id: 'i1', item_name: 'Fridge', label: null, wall_id: null, sensitive: false, room_name: 'Kitchen', fact_id: null, fact_label: null },
    ],
    labels: [],
    spaces: [{ outline: [{ x: 0, y: 4000 }, { x: 1000, y: 4000 }, { x: 1000, y: 5000 }], area_text: '5 sq ft', label_at: { x: 600, y: 4600 } }],
    total_area_text: '120 sq ft',
    background: null,
    corners: [{ x: 0, y: 0 }, { x: 3658, y: 0 }],
    can_edit: true,
    state: { walls: [], openings: [], pins: [], labels: [], rooms: [] },
    ...over,
  };
}

function backend(p = plan()) {
  return new FakeBackend({
    floors: () => p.floors,
    floor_plan: () => p,
    items: () => [{ id: 'i1', name: 'Fridge', room_name: 'Kitchen' }],
    facts: () => ({ property_name: 'Home', groups: [], prompts: [] }),
    rooms: () => [{ id: 'r1', name: 'Kitchen' }, { id: 'r2', name: 'Den' }],
  });
}

describe('Floor plans screen', () => {
  afterEach(() => cleanup());

  it('says a plan is optional when there is none', async () => {
    useBackend(new FakeBackend({ floors: () => [], items: () => [], facts: () => ({ groups: [] }), rooms: () => [] }));
    render(Plans, { floorId: null });
    expect(await screen.findByText(/A floor plan is optional/)).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Start a floor plan' })).toBeTruthy();
  });

  it('shows the total area, the floors, and everything as lists', async () => {
    useBackend(backend());
    render(Plans, { floorId: 'f1' });
    expect((await screen.findByTestId('plan-total')).textContent).toContain('120 sq ft');
    expect(screen.getByRole('button', { name: 'Basement' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Ground floor' }).getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByText('Unnamed space: name it')).toBeTruthy();
    expect(screen.getByText('1. Fridge')).toBeTruthy();
    expect(screen.getByText('Wall 1')).toBeTruthy();
  });

  it('opens a room from the list with its area and perimeter', async () => {
    useBackend(backend());
    render(Plans, { floorId: 'f1' });
    await fireEvent.click(await screen.findByRole('button', { name: /Kitchen\s*120 sq ft/ }));
    const sheet = await screen.findByTestId('room-sheet');
    expect(sheet.textContent).toContain('120 sq ft · around it 44\' 0"');
    expect(sheet.textContent).toContain('2 things in this room');
  });

  it('offers existing rooms when naming a space', async () => {
    useBackend(backend());
    render(Plans, { floorId: 'f1' });
    await fireEvent.click(await screen.findByText('Unnamed space: name it'));
    const sheet = await screen.findByTestId('room-sheet');
    expect(sheet.textContent).toContain('Name this room');
    // Kitchen is already on the plan; Den is not.
    expect(screen.getByRole('button', { name: 'Den' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Kitchen' })).toBeNull();
  });

  it('opens a thing from its pin', async () => {
    useBackend(backend());
    render(Plans, { floorId: 'f1' });
    await fireEvent.click(await screen.findByText('1. Fridge'));
    const link = await screen.findByRole('link', { name: 'Open Fridge' });
    expect(link.getAttribute('href')).toBe('#/thing/i1');
  });

  it('hides the drawing tools from people who cannot change the plan', async () => {
    useBackend(backend(plan({ can_edit: false })));
    render(Plans, { floorId: 'f1' });
    await screen.findByTestId('plan-total');
    expect(screen.queryByRole('toolbar')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Undo' })).toBeNull();
  });
});
