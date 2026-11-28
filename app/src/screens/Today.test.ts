import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import Today from './Today.svelte';
import { useBackend } from '../lib/api';
import { FakeBackend } from '../lib/api/fake';
import { card, today } from '../lib/test-data';
import { app } from '../lib/state.svelte';

describe('Today', () => {
  beforeEach(() => {
    app.toast = null;
  });

  it('lists due tasks with a one-tap Done that asks nothing first (P1)', async () => {
    let done = false;
    const fake = new FakeBackend({
      today: () => (done ? today({ tasks: [] }) : today()),
      complete: () => {
        done = true;
        return { completion_id: 'c1', card: card({ state: 'scheduled' }) };
      },
      reminder_plan: () => [],
    });
    useBackend(fake);
    render(Today);
    const button = await screen.findByRole('button', { name: 'Done: Test the smoke alarm' });
    expect(screen.getByText('Hallway · Smoke alarm')).toBeTruthy();
    await fireEvent.click(button);
    await waitFor(() => expect(fake.calls.some((c) => c.cmd === 'complete')).toBe(true));
    expect(fake.calls.find((c) => c.cmd === 'complete')!.args).toEqual({ id: 't1' });
    // Afterwards: a note is offered, never required, and Undo is there.
    await waitFor(() => expect(app.toast?.text).toBe('Done: Test the smoke alarm.'));
    expect(app.toast?.actions?.map((a) => a.label)).toEqual(['Add details', 'Undo']);
    await screen.findByText('Nothing needs doing right now.');
  });

  it('after a lapse shows welcome back with the top few, not an overdue count (P10)', async () => {
    const focus = [
      card({ id: 'a', title: 'Test the smoke alarm', state: 'overdue' }),
      card({ id: 'b', title: 'Test the sump pump', state: 'overdue' }),
    ];
    const fake = new FakeBackend({
      today: () =>
        today({
          tasks: focus,
          lapsed: true,
          welcome_back: { focus, others_overdue: 34 },
        }),
      mark_seen: () => null,
    });
    useBackend(fake);
    render(Today);
    expect(await screen.findByText('Welcome back')).toBeTruthy();
    expect(screen.getByText(/the 2 things/)).toBeTruthy();
    expect(document.body.textContent).not.toMatch(/34|overdue/i);
    await fireEvent.click(screen.getByRole('button', { name: 'Show my full list' }));
    await waitFor(() => expect(fake.calls.some((c) => c.cmd === 'mark_seen')).toBe(true));
    expect(await screen.findByRole('list', { name: 'Due now and soon' })).toBeTruthy();
  });

  it('with nothing switched on, points to adding a thing', async () => {
    useBackend(new FakeBackend({ today: () => today({ tasks: [], active_count: 0 }) }));
    render(Today);
    expect(await screen.findByText('No tasks are switched on yet.')).toBeTruthy();
    expect(screen.getByRole('link', { name: 'Add a thing' })).toBeTruthy();
  });
});

describe('Today suggestion card', () => {
  it('shows one suggestion with Add, Not for us and Later', async () => {
    const s = {
      key: 'ask:hose-bibs',
      action: { kind: 'add_thing' as const, template: 'hose-bibs', name: 'Outdoor hose taps' },
      title: 'Do you have outdoor taps?',
      body: 'If so, winterise them before the first frost.',
      why: 'Water left in the pipe can freeze.',
      source: 'common plumber guidance',
      add_label: 'Yes, add it',
      priority: 'property_loss' as const,
      seasonal: true,
    };
    let answered = false;
    const fake = new FakeBackend({
      today: () => today({ suggestion: answered ? null : s }),
      answer_suggestion: () => {
        answered = true;
        return { switched_on: [card({ title: 'Winterise the outdoor hose taps' })] };
      },
    });
    useBackend(fake);
    render(Today);
    expect(await screen.findByText('Do you have outdoor taps?')).toBeTruthy();
    expect(screen.getAllByTestId('suggestion')).toHaveLength(1);
    expect(screen.getByRole('button', { name: 'Not for us' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Later' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Yes, add it' }));
    await waitFor(() => expect(fake.calls.find((c) => c.cmd === 'answer_suggestion')!.args).toEqual({ key: 'ask:hose-bibs', answer: 'added' }));
    await waitFor(() => expect(app.toast?.text).toContain('Winterise the outdoor hose taps'));
  });

  it('before the home questions, offers them', async () => {
    useBackend(new FakeBackend({ today: () => today({ tasks: [], active_count: 0, has_profile: false }) }));
    render(Today);
    expect(await screen.findByRole('link', { name: 'Answer the home questions' })).toBeTruthy();
  });
});
