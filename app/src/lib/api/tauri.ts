import { invoke } from '@tauri-apps/api/core';
import type { Backend } from './backend';

/** Native backend: the Tauri commands `api`, `api_upload` and `api_download`
 * wrap `housewright_api::Api`. File bytes travel as raw IPC bodies, not
 * JSON, so a photo is not inflated by base64. */
export class TauriBackend implements Backend {
  readonly kind = 'tauri' as const;
  storage(): string {
    return 'on this device (SQLite in the app data folder)';
  }
  call(cmd: string, args: unknown): Promise<unknown> {
    return invoke('api', { cmd, args });
  }
  upload(cmd: string, args: unknown, bytes: Uint8Array): Promise<unknown> {
    return invoke('api_upload', bytes, {
      headers: { 'x-cmd': cmd, 'x-args': encodeURIComponent(JSON.stringify(args ?? {})) },
    });
  }
  async download(cmd: string, args: unknown): Promise<Uint8Array> {
    const r = await invoke<ArrayBuffer>('api_download', { cmd, args });
    return new Uint8Array(r);
  }
}
