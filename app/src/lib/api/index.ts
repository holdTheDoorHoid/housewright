import type { Backend, Args, Result } from './backend';
import { ApiFailure } from './backend';
import type { Command, DownloadCommands, UploadCommands } from './types';

export * from './types';
export { ApiFailure } from './backend';
export type { Backend } from './backend';

declare const __BACKEND__: 'tauri' | 'wasm' | 'test';

let backend: Backend | null = null;

/** Use a given backend (tests). */
export function useBackend(b: Backend): void {
  backend = b;
}

/** Pick and start the backend for this build. The branch on `__BACKEND__` is
 * resolved at build time, so the web bundle does not carry Tauri code and the
 * app bundle does not carry the 5 MB WebAssembly core. */
export async function connect(): Promise<Backend> {
  if (backend) return backend;
  if (__BACKEND__ === 'wasm') {
    const { WasmBackend } = await import('./wasm');
    backend = await WasmBackend.start();
  } else if (__BACKEND__ === 'tauri') {
    const { TauriBackend } = await import('./tauri');
    backend = new TauriBackend();
  } else {
    throw new Error('no backend set; call useBackend() in tests');
  }
  return backend;
}

export function currentBackend(): Backend | null {
  return backend;
}

/** Run one typed command. */
export async function call<C extends Command>(cmd: C, args?: Args<C>): Promise<Result<C>> {
  const b = backend ?? (await connect());
  try {
    return (await b.call(cmd, args ?? {})) as Result<C>;
  } catch (e) {
    if (e instanceof ApiFailure) throw e;
    if (e && typeof e === 'object' && 'code' in e && 'message' in e) {
      throw new ApiFailure(e as { code: string; message: string });
    }
    throw new ApiFailure({ code: 'internal', message: String(e) });
  }
}

function wrap(e: unknown): ApiFailure {
  if (e instanceof ApiFailure) return e;
  if (e && typeof e === 'object' && 'code' in e && 'message' in e) {
    return new ApiFailure(e as { code: string; message: string });
  }
  return new ApiFailure({ code: 'internal', message: String(e) });
}

/** Run a command that takes a file. */
export async function upload<C extends keyof UploadCommands>(
  cmd: C,
  args: UploadCommands[C][0],
  bytes: Uint8Array,
): Promise<UploadCommands[C][1]> {
  const b = backend ?? (await connect());
  try {
    return (await b.upload(cmd, args ?? {}, bytes)) as UploadCommands[C][1];
  } catch (e) {
    throw wrap(e);
  }
}

/** Run a command that returns bytes. */
export async function download<C extends keyof DownloadCommands>(
  cmd: C,
  args: DownloadCommands[C],
): Promise<Uint8Array> {
  const b = backend ?? (await connect());
  try {
    return await b.download(cmd, args ?? {});
  } catch (e) {
    throw wrap(e);
  }
}
