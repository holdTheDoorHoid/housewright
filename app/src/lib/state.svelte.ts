import type { Status } from './api';

export type Route =
  | { name: 'today' }
  | { name: 'task'; id: string; sheet?: 'reschedule' | 'details' }
  | { name: 'things' }
  | { name: 'thing'; id: string }
  | { name: 'add' }
  | { name: 'history' }
  | { name: 'settings' }
  | { name: 'sync' }
  | { name: 'home-assistant' }
  | { name: 'questions'; redo: boolean }
  | { name: 'plan' }
  | { name: 'suggested' }
  | { name: 'home' }
  | { name: 'costs' }
  | { name: 'labels' }
  | { name: 'scan' }
  | { name: 'binder' }
  | { name: 'contractors' }
  | { name: 'contractor'; id: string }
  | { name: 'warranties' }
  | { name: 'recalls' }
  | { name: 'insurance' }
  | { name: 'plans'; floor: string | null; pin: string | null; place: string | null };

export function parse(hash: string): Route {
  const parts = hash.replace(/^#\/?/, '').split('/').filter(Boolean);
  switch (parts[0]) {
    case 'task':
      return parts[1]
        ? {
            name: 'task',
            id: parts[1],
            sheet: parts[2] === 'reschedule' || parts[2] === 'details' ? parts[2] : undefined,
          }
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
    case 'sync':
      return { name: 'sync' };
    case 'home-assistant':
      return { name: 'home-assistant' };
    case 'questions':
      return { name: 'questions', redo: parts[1] === 'redo' };
    case 'plan':
      return { name: 'plan' };
    case 'suggested':
      return { name: 'suggested' };
    case 'home':
      return { name: 'home' };
    case 'costs':
      return { name: 'costs' };
    case 'labels':
      return { name: 'labels' };
    case 'scan':
      return { name: 'scan' };
    case 'binder':
      return { name: 'binder' };
    case 'contractors':
      return { name: 'contractors' };
    case 'contractor':
      return parts[1] ? { name: 'contractor', id: parts[1] } : { name: 'contractors' };
    case 'warranties':
      return { name: 'warranties' };
    case 'recalls':
      return { name: 'recalls' };
    case 'insurance':
      return { name: 'insurance' };
    case 'plans':
      // #/plans, #/plans/<floor>, #/plans/<floor>/pin/<pin>, #/plans/place/<thing>
      if (parts[1] === 'place') return { name: 'plans', floor: null, pin: null, place: parts[2] ?? null };
      return { name: 'plans', floor: parts[1] ?? null, pin: parts[2] === 'pin' ? (parts[3] ?? null) : null, place: null };
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

/** Where a notice's `open` link (`item/<id>`, `contractor/<id>`) goes. */
export function noticeHash(open: string): string {
  const [kind, id] = open.split('/');
  if (kind === 'item' && id) return `#/thing/${id}`;
  if (kind === 'contractor' && id) return `#/contractor/${id}`;
  return '#/';
}
