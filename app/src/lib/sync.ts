// When to sync (D-97). Sync is off until it is set up (D-11). Once on, the
// app syncs when it opens, when it comes back to the front, a few seconds
// after a change, and every two minutes while it is open. Nothing here
// ever asks the person anything: merging is automatic (D-12).

import { call, type Command, type SyncStatus } from './api';
import { refresh } from './state.svelte';

/** Commands that only read; anything else may have changed data. */
const READS = new Set<string>([
  'status', 'today', 'task', 'items', 'item', 'rooms', 'members', 'templates', 'history',
  'reminder_plan', 'export', 'climate_lookup', 'profile', 'starting_plan', 'all_suggested',
  'attachments', 'facts', 'costs', 'label_sheet', 'find_label', 'binder', 'notes', 'contractors',
  'snooze_options', 'mark_seen', 'reveal_field', 'reveal_fact',
]);

let enabled = false;
let running = false;
let timer: ReturnType<typeof setTimeout> | null = null;
let interval: ReturnType<typeof setInterval> | null = null;
const listeners = new Set<(s: SyncStatus) => void>();

export function onSyncStatus(f: (s: SyncStatus) => void): () => void {
  listeners.add(f);
  return () => listeners.delete(f);
}

function tell(s: SyncStatus) {
  enabled = s.enabled && !s.removed;
  for (const f of listeners) f(s);
}

/** Sync now, quietly: errors are shown on the Sync screen, never as pop-ups. */
export async function syncNow(): Promise<SyncStatus | null> {
  if (running) return null;
  running = true;
  try {
    const r = await call('sync_now');
    if (r.report.pulled_ops > 0 || r.report.files_down > 0) refresh();
    tell(r.status);
    return r.status;
  } catch {
    try {
      const s = await call('sync_status');
      tell(s);
      return s;
    } catch {
      return null;
    }
  } finally {
    running = false;
  }
}

/** After a change: sync a few seconds later (changes in a burst go together). */
export function changed(cmd: Command | string): void {
  if (!enabled || cmd.startsWith('sync_') || READS.has(cmd)) return;
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => {
    timer = null;
    void syncNow();
  }, 3000);
}

/** Start the schedule (after the app has a household). */
export async function startSync(): Promise<void> {
  let s: SyncStatus;
  try {
    s = await call('sync_status');
  } catch {
    return;
  }
  tell(s);
  if (!s.enabled || s.removed) return;
  void syncNow();
  if (!interval) {
    interval = setInterval(() => {
      if (document.visibilityState === 'visible') void syncNow();
    }, 120_000);
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'visible') void syncNow();
    });
  }
}

/** Sync was switched on or off on this device. */
export function syncSettingsChanged(s: SyncStatus): void {
  tell(s);
  if (s.enabled) void startSync();
}
