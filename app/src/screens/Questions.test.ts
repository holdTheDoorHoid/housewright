import { describe, it, expect } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/svelte';
import Questions from './Questions.svelte';
import { useBackend } from '../lib/api';
import { FakeBackend } from '../lib/api/fake';

const plan = { switched_on: [], things_recorded: 6, new_things: 6, suggested_count: 10, climate: null };

describe('Questions', () => {
  it('one question at a time, with progress, Back and Not sure; sends only what was answered', async () => {
    const fake = new FakeBackend({ onboard: () => plan });
    useBackend(fake);
    render(Questions);
    expect(screen.getByText(/Question 1 of 10/)).toBeTruthy();
    expect(screen.getByText(/about 3 minutes/)).toBeTruthy();
    // A single-choice answer moves on by itself.
    await fireEvent.click(screen.getByRole('button', { name: 'A house' }));
    expect(screen.getByText(/Question 2 of 10/)).toBeTruthy();
    const smoke = () => within(screen.getByRole('group', { name: 'How many smoke alarms?' }));
    await fireEvent.click(smoke().getByRole('button', { name: '2 or 3' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Next' }));
    expect(screen.getByText(/Question 3 of 10/)).toBeTruthy();
    // Back keeps the earlier answer.
    await fireEvent.click(screen.getByRole('button', { name: 'Back' }));
    expect(smoke().getByRole('button', { name: '2 or 3' }).getAttribute('aria-pressed')).toBe('true');
    await fireEvent.click(screen.getByRole('button', { name: 'Next' }));
    // Skip everything else.
    for (let i = 3; i <= 10; i++) {
      expect(screen.getByText(new RegExp(`Question ${i} of 10`))).toBeTruthy();
      await fireEvent.click(screen.getByRole('button', { name: 'Not sure' }));
    }
    await waitFor(() => expect(fake.calls.some((c) => c.cmd === 'onboard')).toBe(true));
    const sent = (fake.calls.find((c) => c.cmd === 'onboard')!.args as { profile: Record<string, unknown> }).profile;
    expect(sent.home_type).toBe('house');
    expect(sent.smoke_alarms).toBe('two_to_three');
    expect(sent.features).toEqual({});
    expect(sent.zip).toBeUndefined();
  });

  it('Redo starts from the saved answers', async () => {
    useBackend(
      new FakeBackend({
        profile: () => ({ profile: { id: 'p', home_type: 'condo', zip: { value: '19103', sensitivity: 'normal' } }, climate: null }),
      }),
    );
    render(Questions, { redo: true });
    const condo = await screen.findByRole('button', { name: 'A condo' });
    expect(condo.getAttribute('aria-pressed')).toBe('true');
  });
});
