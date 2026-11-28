import type { Status } from './api';

export type Route =
  | { name: 'today' }
  | { name: 'task'; id: string; sheet?: 'reschedule' }
  | { name: 'things' }
  | { name: 'thing'; id: string }
  | { name: 'add' }
  | { name: 'history' }
  | { name: 'settings' };

export function parse(hash: string): Route {
  const parts = hash.replace(/^#\/?/, '').split('/').filter(Boolean);
  switch (parts[0]) {
    case 'task':
      return parts[1]
        ? { name: 'task', id: parts[1], sheet: parts[2] === 'reschedule' ? 'reschedule' : undefined }
        : { name: 'today' };
    case 'things':
      return { name: 'things' };
    case 'thing':
      return parts[1] ? { name: 'thing', id: parts[1] } : { name: 'things' };
    case 'add':
      return { name: 'add' };
    case 'history':
      return { name: 'history' };
    case 'settings':
      return { name: 'settings' };
    default:
      return { name: 'today' };
  }
}

/** Shared app state. `tick` changes whenever data may have changed elsewhere
 * (a notification's Done button, another screen), so screens reload. */
export const app = $state<{ route: Route; tick: number; status: Status | null; toast: Toast | null }>({
  route: parse(typeof location === 'undefined' ? '' : location.hash),
  tick: 0,
  status: null,
  toast: null,
});

export interface Toast {
  text: string;
  actions?: { label: string; run: () => void }[];
}

export function go(hash: string): void {
  if (location.hash === hash) app.route = parse(hash);
  else location.hash = hash;
}

export function refresh(): void {
  app.tick++;
}

let toastTimer: ReturnType<typeof setTimeout> | null = null;
export function toast(t: Toast, ms = 8000): void {
  app.toast = t;
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (app.toast = null), ms);
}

if (typeof window !== 'undefined') {
  window.addEventListener('hashchange', () => {
    app.route = parse(location.hash);
  });
}
