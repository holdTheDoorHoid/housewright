// Reminders: one interface, three implementations.
// - Android (Tauri): the `reminders` mobile plugin schedules alarms that post
//   notifications with Done and Reschedule buttons, and records Done through
//   the core without opening the app (see app/src-tauri/plugins/reminders).
// - Web: the Notifications API, only while the page is open (honest limit:
//   no background reminders on the web without a server; that is Phase 11).
// - Tests: nothing.
import { call, currentBackend, type Reminder } from './api';
import { noticeHash } from './state.svelte';

export type Permission = 'granted' | 'denied' | 'prompt' | 'unsupported';

export interface ReminderService {
  /** Plain words for Settings: what reminders can do here. */
  describe(): string;
  permission(): Promise<Permission>;
  request(): Promise<Permission>;
  /** Recompute the plan from the core and (re)schedule everything. */
  sync(): Promise<number>;
  /** A reminder for a real task in a few seconds. */
  test(inSeconds: number): Promise<Reminder>;
  /** A notification button that needs the app (Reschedule), if one was tapped. */
  takePendingAction(): Promise<{ action: string; taskId: string } | null>;
}

class NoReminders implements ReminderService {
  describe() {
    return 'Reminders are not available here.';
  }
  async permission(): Promise<Permission> {
    return 'unsupported';
  }
  async request(): Promise<Permission> {
    return 'unsupported';
  }
  async sync() {
    return 0;
  }
  async test(): Promise<Reminder> {
    throw new Error('Reminders are not available here.');
  }
  async takePendingAction() {
    return null;
  }
}

class WebReminders implements ReminderService {
  private timers: number[] = [];
  describe() {
    return 'In a browser, reminders appear only while Housewright is open in a tab. For reminders when it is closed, use the Android app.';
  }
  async permission(): Promise<Permission> {
    if (!('Notification' in window)) return 'unsupported';
    return Notification.permission === 'default' ? 'prompt' : Notification.permission;
  }
  async request(): Promise<Permission> {
    if (!('Notification' in window)) return 'unsupported';
    const p = await Notification.requestPermission();
    return p === 'default' ? 'prompt' : p;
  }
  private show(r: Reminder) {
    if (!('Notification' in window) || Notification.permission !== 'granted') return;
    const n = new Notification(r.title, { body: r.body, tag: `task-${r.task_id}` });
    n.onclick = () => {
      window.focus();
      // A notice (warranty, licence) opens what it is about; a task opens the task.
      location.hash = r.open ? noticeHash(r.open) : `#/task/${r.task_id}`;
      n.close();
    };
  }
  private at(r: Reminder) {
    const delay = r.at_ms - Date.now();
    // Timers longer than ~24 days overflow; the plan is re-synced long before.
    if (delay < 0 || delay > 2 ** 31 - 1) return;
    this.timers.push(window.setTimeout(() => this.show(r), delay));
  }
  async sync() {
    this.timers.forEach((t) => clearTimeout(t));
    this.timers = [];
    const plan = await call('reminder_plan', {});
    plan.forEach((r) => this.at(r));
    return plan.length;
  }
  async test(inSeconds: number) {
    const r = await call('test_reminder', { in_seconds: inSeconds });
    this.at(r);
    return r;
  }
  async takePendingAction() {
    return null;
  }
}

class AndroidReminders implements ReminderService {
  private async core() {
    return import('@tauri-apps/api/core');
  }
  describe() {
    return 'Reminders arrive as notifications with Done and Reschedule buttons, even when the app is closed. If you force-stop the app, Android cancels them until you open it again.';
  }
  async permission(): Promise<Permission> {
    try {
      const { checkPermissions } = await this.core();
      const p = await checkPermissions<{ notifications: string }>('reminders');
      return norm(p.notifications);
    } catch {
      return 'unsupported';
    }
  }
  async request(): Promise<Permission> {
    try {
      const { requestPermissions } = await this.core();
      const p = await requestPermissions<{ notifications: string }>('reminders');
      return norm(p.notifications);
    } catch {
      return 'unsupported';
    }
  }
  async sync() {
    try {
      const { invoke } = await this.core();
      const r = await invoke<{ scheduled: number }>('plugin:reminders|sync');
      return r.scheduled;
    } catch (e) {
      console.warn('reminder sync failed', e);
      return 0;
    }
  }
  async test(inSeconds: number) {
    const { invoke } = await this.core();
    return invoke<Reminder>('plugin:reminders|test', { inSeconds });
  }
  async takePendingAction() {
    try {
      const { invoke } = await this.core();
      const r = await invoke<{ action?: string; taskId?: string }>('plugin:reminders|pending');
      return r.action && r.taskId ? { action: r.action, taskId: r.taskId } : null;
    } catch {
      return null;
    }
  }
}

function norm(s: string | undefined): Permission {
  if (s === 'granted' || s === 'denied') return s;
  if (s === 'prompt' || s === 'prompt-with-rationale') return 'prompt';
  return 'unsupported';
}

let service: ReminderService | null = null;

export function reminders(): ReminderService {
  if (service) return service;
  const b = currentBackend();
  if (b?.kind === 'wasm') service = new WebReminders();
  else if (b?.kind === 'tauri' && /android/i.test(navigator.userAgent)) service = new AndroidReminders();
  else service = new NoReminders();
  return service;
}

let timer: ReturnType<typeof setTimeout> | null = null;
/** Re-plan reminders shortly after a change (coalesces bursts of edits). */
export function resync(): void {
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => {
    timer = null;
    reminders()
      .sync()
      .catch(() => {});
  }, 300);
}
