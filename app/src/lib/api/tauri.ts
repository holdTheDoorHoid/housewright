import { invoke } from '@tauri-apps/api/core';
import type { Backend } from './backend';

/** Native backend: one Tauri command, `api`, wraps `housewright_api::Api::call`. */
export class TauriBackend implements Backend {
  readonly kind = 'tauri' as const;
  storage(): string {
    return 'on this device (SQLite in the app data folder)';
  }
  call(cmd: string, args: unknown): Promise<unknown> {
    return invoke('api', { cmd, args });
  }
}
