// The appliance library (D-147 to D-152): optional updates and saving a
// manufacturer's manual onto the device.
//
// Updates: the device reads two public files from the library's GitHub
// repository (a small release note, then the library file it names). The
// requests are the same for everyone: no cookies, no referrer, nothing
// about the home. The core checks the file's size and checksum against the
// release note and runs every library check before using it (D-148).
import { call, currentBackend, upload, type LibraryStatus, type ManualSource } from './api';

const NO_STORE: RequestInit = {
  credentials: 'omit',
  referrerPolicy: 'no-referrer',
  cache: 'no-store',
};

/** Days between automatic checks, when they are switched on. */
export const AUTO_DAYS = 7;

export interface UpdateResult {
  status: LibraryStatus;
  message: string;
  updated: boolean;
}

/** Look for a newer published library and use it if it passes every check. */
export async function checkLibrary(): Promise<UpdateResult> {
  let status = await call('library');
  const res = await fetch(status.manifest_url, NO_STORE);
  if (res.status === 404) {
    return { status, updated: false, message: 'No library updates have been published yet.' };
  }
  if (!res.ok) throw new Error(`The library site answered ${res.status}. Try again later.`);
  const manifest = await res.text();
  const c = await call('library_check', { manifest });
  if (!c.newer || !c.bundle_url) return { status: await call('library'), updated: false, message: c.message };
  const b = await fetch(c.bundle_url, NO_STORE);
  if (!b.ok) throw new Error(`The library site answered ${b.status}. Try again later.`);
  const bytes = new Uint8Array(await b.arrayBuffer());
  status = await upload('library_import', { manifest }, bytes);
  return { status, updated: true, message: `The appliance library is now version ${status.version}.` };
}

/** At most weekly, when switched on; quiet on failure. */
export async function autoCheckLibrary(): Promise<void> {
  try {
    const s = await call('library');
    if (!s.auto) return;
    const last = s.checked_at ? Date.parse(s.checked_at) : 0;
    if (Date.now() - last < AUTO_DAYS * 86_400_000) return;
    await checkLibrary();
  } catch {
    // Offline or the site is down: try again another day.
  }
}

/** Whether this build can download a manual itself (the Android and desktop app). */
export function canSaveManual(): boolean {
  return currentBackend()?.kind === 'tauri';
}

/** Download the official manual straight from the manufacturer onto this
 * device and keep it with the thing (native app only; the browser's
 * cross-site rules usually block it on the web). */
export async function saveManual(itemId: string, url: string): Promise<void> {
  if (!canSaveManual()) throw new Error('This version of the app cannot download manuals.');
  // Checked by the core before anything is downloaded.
  const src: ManualSource = await call('manual_source', { item_id: itemId, url });
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('save_manual', { itemId, url: src.url });
}
