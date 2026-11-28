import { describe, it, expect, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/svelte';
import Binder from './Binder.svelte';
import { useBackend, type BinderInfo, type EditionInfo } from '../lib/api';
import { FakeBackend } from '../lib/api/fake';

function edition(over: Partial<EditionInfo> = {}): EditionInfo {
  return {
    edition: 'full',
    label: 'Household binder',
    description: 'Everything about running the house.',
    has_sealed: true,
    last_printed: '2026-11-02',
    changed: 2,
    sections: [
      { key: 'emergency', volume: 'main', short: 'Emergency', title: 'In an emergency', changed: false, last_printed: '2026-11-02', fingerprint: 'a' },
      { key: 'facts', volume: 'main', short: 'Facts', title: 'Home facts', changed: true, last_printed: '2026-11-02', fingerprint: 'b' },
      { key: 'sealed:codes', volume: 'sealed', short: 'Codes', title: 'Codes and access', changed: true, last_printed: '2026-11-02', fingerprint: 'c' },
    ],
    ...over,
  };
}

function info(over: Partial<BinderInfo> = {}): BinderInfo {
  return {
    property_name: 'Home',
    editions: [edition()],
    sealed_location: null,
    can_print_sealed: true,
    reminder: { task_id: 't9', active: false },
    paper: 'letter',
    ...over,
  };
}

describe('House binder screen', () => {
  afterEach(() => cleanup());

  it('says what changed since the last print and offers to print just that', async () => {
    useBackend(new FakeBackend({ binder: () => info(), contractors: () => [], notes: () => [] }));
    render(Binder);
    expect(await screen.findByText(/2 sections have changed since you printed on/)).toBeTruthy();
    expect(screen.getAllByText('changed')).toHaveLength(2);
    expect(screen.getByRole('button', { name: 'Print the 1 changed section' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Print the 1 changed sealed section' })).toBeTruthy();
    // The reminder is an offer, never switched on by itself.
    expect(screen.getByRole('button', { name: 'Remind me once a year' })).toBeTruthy();
  });

  it('only the owner is offered the sealed volume', async () => {
    useBackend(
      new FakeBackend({ binder: () => info({ can_print_sealed: false }), contractors: () => [], notes: () => [] }),
    );
    render(Binder);
    expect(await screen.findByText(/Only the household's owner can print the sealed volume/)).toBeTruthy();
    expect(screen.queryByRole('button', { name: /sealed volume \(separately\)/ })).toBeNull();
  });

  it('saves where the sealed volume is kept', async () => {
    const fake = new FakeBackend({
      binder: () => info(),
      contractors: () => [],
      notes: () => [],
      set_sealed_location: (a: { text: string }) => info({ sealed_location: a.text }),
    });
    useBackend(fake);
    render(Binder);
    const input = await screen.findByLabelText(/printed in the main binder/);
    await fireEvent.input(input, { target: { value: 'In the fire safe' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(fake.calls.find((c) => c.cmd === 'set_sealed_location')?.args).toEqual({ text: 'In the fire safe' });
  });
});
