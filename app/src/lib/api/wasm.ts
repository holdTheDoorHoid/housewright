import type { Backend } from './backend';
import { ApiFailure, unwrap } from './backend';
import type { FromWorker } from './protocol';

/** Web backend: the core as WebAssembly in a worker, SQLite on OPFS. */
export class WasmBackend implements Backend {
  readonly kind = 'wasm' as const;
  private next = 1;
  private pending = new Map<number, (text: string) => void>();
  private constructor(
    private worker: Worker,
    private where: 'opfs' | 'memory',
  ) {
    worker.addEventListener('message', (e: MessageEvent<FromWorker>) => {
      if (e.data.type === 'result') {
        const r = this.pending.get(e.data.id);
        this.pending.delete(e.data.id);
        r?.(e.data.text);
      }
    });
  }

  static start(): Promise<WasmBackend> {
    const worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' });
    return new Promise((resolve, reject) => {
      const first = (e: MessageEvent<FromWorker>) => {
        const m = e.data;
        if (m.type === 'ready') {
          worker.removeEventListener('message', first);
          resolve(new WasmBackend(worker, m.storage));
        } else if (m.type === 'locked') {
          worker.terminate();
          reject(
            new ApiFailure({
              code: 'locked',
              message:
                'Housewright is already open in another tab or window. Close it there, then reload this page.',
            }),
          );
        } else if (m.type === 'failed') {
          worker.terminate();
          reject(new ApiFailure({ code: 'internal', message: m.message }));
        }
      };
      worker.addEventListener('message', first);
      worker.addEventListener('error', (e) =>
        reject(new ApiFailure({ code: 'internal', message: e.message || 'worker failed to start' })),
      );
    });
  }

  storage(): string {
    return this.where === 'opfs'
      ? 'in this browser (SQLite in private browser storage)'
      : 'in memory only: this browser cannot store it, so it is lost when the tab closes';
  }

  call(cmd: string, args: unknown): Promise<unknown> {
    const id = this.next++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, (text) => {
        try {
          resolve(unwrap(text));
        } catch (e) {
          reject(e);
        }
      });
      this.worker.postMessage({ id, cmd, args: JSON.stringify(args ?? {}) });
    });
  }
}
