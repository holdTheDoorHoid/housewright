import type { Card, DateStr } from './api';

/** Today's civil date in the device's zone, "2026-11-27". */
export function todayStr(now: Date = new Date()): DateStr {
  const y = now.getFullYear();
  const m = String(now.getMonth() + 1).padStart(2, '0');
  const d = String(now.getDate()).padStart(2, '0');
  return `${y}-${m}-${d}`;
}

/** Whole days from a to b (civil dates). */
export function daysBetween(a: DateStr, b: DateStr): number {
  const pa = Date.UTC(+a.slice(0, 4), +a.slice(5, 7) - 1, +a.slice(8, 10));
  const pb = Date.UTC(+b.slice(0, 4), +b.slice(5, 7) - 1, +b.slice(8, 10));
  return Math.round((pb - pa) / 86_400_000);
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

/** "Dec 5" (or "Dec 5, 2027" when not this year). */
export function shortDate(d: DateStr, today: DateStr = todayStr()): string {
  const s = `${MONTHS[+d.slice(5, 7) - 1]} ${+d.slice(8, 10)}`;
  return d.slice(0, 4) === today.slice(0, 4) ? s : `${s}, ${d.slice(0, 4)}`;
}

/** Calm, specific wording for when a task is due. Never a scary count (P10). */
export function dueText(c: Card, today: DateStr = todayStr()): string {
  if (c.state === 'paused') return 'Paused';
  if (c.state === 'not_applicable') return 'Not applicable';
  if (c.state === 'completed') return 'Done';
  if (!c.due) return c.schedule_text;
  const n = daysBetween(today, c.due);
  if (c.state === 'snoozed') return `Moved to ${shortDate(c.due, today)}`;
  if (n === 0) return 'Due today';
  if (n === 1) return 'Due tomorrow';
  if (n > 1 && n <= 14) return `Due in ${n} days`;
  if (n > 14) return `Due ${shortDate(c.due, today)}`;
  if (n === -1) return 'Was due yesterday';
  return `Was due ${shortDate(c.due, today)}`;
}

/** "Hallway · Smoke alarm" */
export function placeText(c: { item_name: string | null; room_name: string | null }): string {
  return [c.room_name, c.item_name].filter(Boolean).join(' · ');
}

export function when(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  });
}

/** "Nov 2026", for when a source was last checked. */
export function monthYear(d: DateStr | null | undefined): string {
  if (!d || d.length < 7) return '';
  return `${MONTHS[+d.slice(5, 7) - 1]} ${d.slice(0, 4)}`;
}
