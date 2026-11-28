import type { ApiError, Command, Commands } from './types';

/** What every backend provides: run one command, resolve with its result or
 * reject with an {@link ApiError}. */
export interface Backend {
  readonly kind: 'tauri' | 'wasm' | 'test';
  /** Where the data lives, for the Settings screen. */
  storage(): string;
  call(cmd: string, args: unknown): Promise<unknown>;
  /** A command that takes a file's bytes as well as arguments. */
  upload(cmd: string, args: unknown, bytes: Uint8Array): Promise<unknown>;
  /** A command that returns bytes. */
  download(cmd: string, args: unknown): Promise<Uint8Array>;
}

export class ApiFailure extends Error {
  readonly code: string;
  constructor(e: ApiError) {
    super(e.message);
    this.code = e.code;
  }
}

/** Unwrap the `{"ok": ...}` / `{"error": ...}` envelope used by the wasm and
 * JNI bridges. */
export function unwrap(text: string): unknown {
  const v = JSON.parse(text) as { ok?: unknown; error?: ApiError };
  if (v.error) throw new ApiFailure(v.error);
  return v.ok ?? null;
}

export type Args<C extends Command> = Commands[C][0];
export type Result<C extends Command> = Commands[C][1];
