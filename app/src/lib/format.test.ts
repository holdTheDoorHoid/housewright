import { describe, it, expect } from 'vitest';
import { dueText, daysBetween, shortDate } from './format';
import { card } from './test-data';

describe('format', () => {
  it('counts days across months', () => {
    expect(daysBetween('2026-11-27', '2026-12-02')).toBe(5);
    expect(daysBetween('2026-12-02', '2026-11-27')).toBe(-5);
  });
  it('words due dates calmly', () => {
    const t = '2026-11-27';
    expect(dueText(card({ due: '2026-11-27' }), t)).toBe('Due today');
    expect(dueText(card({ due: '2026-11-28' }), t)).toBe('Due tomorrow');
    expect(dueText(card({ due: '2026-12-02' }), t)).toBe('Due in 5 days');
    expect(dueText(card({ due: '2026-11-01', state: 'overdue' }), t)).toBe('Was due Nov 1');
    expect(dueText(card({ due: '2026-12-05', state: 'snoozed' }), t)).toBe('Moved to Dec 5');
    expect(shortDate('2027-01-03', t)).toBe('Jan 3, 2027');
  });
});
