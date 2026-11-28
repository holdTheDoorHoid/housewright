import { describe, it, expect } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import Reschedule from './Reschedule.svelte';
import { useBackend } from '../lib/api';
import { FakeBackend } from '../lib/api/fake';
import { card } from '../lib/test-data';

const options = () => [
  { key: 'tomorrow', label: 'Tomorrow', date: '2026-11-28' },
  { key: 'weekend', label: 'This weekend', date: '2026-11-28' },
];

describe('Reschedule (P7)', () => {
  it('offers moments, and snoozes to the chosen date', async () => {
    const fake = new FakeBackend({ snooze_options: options, snooze: () => card(), reminder_plan: () => [] });
    useBackend(fake);
    let closed = false;
    render(Reschedule, { card: card(), interval: '1 month', onclose: () => (closed = true) });
    await fireEvent.click(await screen.findByRole('button', { name: /This weekend/ }));
    await waitFor(() => expect(closed).toBe(true));
    expect(fake.calls.find((c) => c.cmd === 'snooze')!.args).toEqual({ id: 't1', until: '2026-11-28' });
  });

  it('after three snoozes leads with review choices', async () => {
    const fake = new FakeBackend({
      snooze_options: options,
      members: () => [{ id: 'm1', name: 'Alex', role: 'owner', reminders: {} }],
      review: () => card(),
      reminder_plan: () => [],
    });
    useBackend(fake);
    render(Reschedule, {
      card: card({ needs_review: true, snooze_count: 3 }),
      interval: '1 month',
      onclose: () => {},
    });
    expect(await screen.findByText('This keeps getting moved')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'This does not apply to my home' }));
    await waitFor(() =>
      expect(fake.calls.find((c) => c.cmd === 'review')?.args).toEqual({ id: 't1', action: 'not_applicable' }),
    );
  });
});
