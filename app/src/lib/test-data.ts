import type { Card, Today } from './api';

export function card(over: Partial<Card> = {}): Card {
  return {
    id: 't1',
    title: 'Test the smoke alarm',
    item_id: 'i1',
    item_name: 'Smoke alarm',
    room_name: 'Hallway',
    state: 'due',
    state_label: 'due',
    priority: 'life_safety',
    priority_label: 'life safety',
    due: '2026-11-27',
    earliest: '2026-11-22',
    overdue_after: '2026-12-02',
    last_done: null,
    snooze_count: 0,
    needs_review: false,
    minutes: 5,
    owner_id: 'm1',
    owner_name: 'Alex',
    active: true,
    schedule_text: 'every 1 month',
    ...over,
  };
}

export function today(over: Partial<Today> = {}): Today {
  return {
    suggestion: null,
    has_profile: true,
    tasks: [card()],
    coming_up: [],
    lapsed: false,
    welcome_back: null,
    active_count: 1,
    suggested_count: 0,
    ...over,
  };
}
