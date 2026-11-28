import { invoke } from '@tauri-apps/api/core';
import type { Backend } from './backend';

/** Native backend: the Tauri commands `api`, `api_upload` and `api_download`
 * wrap `housewright_api::Api`. File bytes travel as base64 text, because on
 * Android the WebView's message bridge cannot carry a raw request body. */
export class TauriBackend implements Backend {
  readonly kind = 'tauri' as const;
  storage(): string {
    return 'on this device (SQLite in the app data folder)';
  }
  call(cmd: string, args: unknown): Promise<unknown> {
    return invoke('api', { cmd, args });
  }
  upload(cmd: string, args: unknown, bytes: Uint8Array): Promise<unknown> {
    return invoke('api_upload', { cmd, args: args ?? {}, data: toBase64(bytes) });
  }
  async download(cmd: string, args: unknown): Promise<Uint8Array> {
    return fromBase64(await invoke<string>('api_download', { cmd, args: args ?? {} }));
  }
}

export function toBase64(bytes: Uint8Array): string {
  let s = '';
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    s += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(s);
}

export function fromBase64(text: string): Uint8Array {
  const s = atob(text);
  const out = new Uint8Array(s.length);
  for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
  return out;
}
