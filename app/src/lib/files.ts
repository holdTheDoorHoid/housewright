// Files in and out of the core: reading a picked file, showing stored
// photos, and handing PDFs and backups to the platform (D-71, D-75).
//
// On the web a file is opened as a blob URL or downloaded. On Android the
// WebView cannot show a PDF or download a file, so the core writes it to
// the app's cache and the phone's own viewer or share sheet opens it.

import { download, currentBackend, type DownloadCommands } from './api';

export async function readFile(f: File): Promise<Uint8Array> {
  return new Uint8Array(await f.arrayBuffer());
}

const urls = new Map<string, string>();

/** A blob URL for a stored attachment (cached for the page's life). */
export async function attachmentUrl(id: string, mediaType: string, thumb = false): Promise<string> {
  const key = `${id}:${thumb ? 't' : 'f'}`;
  const hit = urls.get(key);
  if (hit) return hit;
  const bytes = await download('attachment_file', { id, thumb });
  const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type: thumb ? 'image/jpeg' : mediaType }));
  urls.set(key, url);
  return url;
}

function isTauri(): boolean {
  return currentBackend()?.kind === 'tauri';
}

async function native<C extends keyof DownloadCommands>(
  how: 'view_file' | 'share_file',
  cmd: C,
  args: DownloadCommands[C],
  filename: string,
  mime: string,
  title: string,
): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  const path = await invoke<string>('save_for_viewer', { cmd, args: args ?? {}, filename });
  await invoke(`plugin:reminders|${how}`, { path, mime, title });
}

/** Open a file with whatever shows it best: a new tab on the web, the
 * phone's viewer on Android. */
export async function openFile<C extends keyof DownloadCommands>(
  cmd: C,
  args: DownloadCommands[C],
  filename: string,
  mime: string,
): Promise<void> {
  if (isTauri()) return native('view_file', cmd, args, filename, mime, filename);
  const bytes = await download(cmd, args);
  const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type: mime }));
  const w = window.open(url, '_blank');
  if (!w) saveUrl(url, filename);
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}

/** Save a file the person keeps (a backup, a sticker sheet): a download on
 * the web, the share sheet on Android (Drive, Files, email...). */
export async function saveFile<C extends keyof DownloadCommands>(
  cmd: C,
  args: DownloadCommands[C],
  filename: string,
  mime: string,
): Promise<void> {
  if (isTauri()) return native('share_file', cmd, args, filename, mime, filename);
  const bytes = await download(cmd, args);
  const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type: mime }));
  saveUrl(url, filename);
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}

function saveUrl(url: string, filename: string) {
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
}

/** "1.2 MB" */
export function sizeText(n: number): string {
  if (n < 1024) return `${n} bytes`;
  if (n < 1024 * 1024) return `${Math.round(n / 1024)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}
