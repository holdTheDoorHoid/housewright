import type { Backend } from './backend';
import { ApiFailure } from './backend';

/** A scriptable backend for component tests. */
export class FakeBackend implements Backend {
  readonly kind = 'test' as const;
  calls: { cmd: string; args: unknown }[] = [];
  constructor(private handlers: Record<string, (args: any) => unknown>) {}
  storage() {
    return 'in a test';
  }
  async call(cmd: string, args: unknown): Promise<unknown> {
    this.calls.push({ cmd, args });
    const h = this.handlers[cmd];
    if (!h) throw new ApiFailure({ code: 'invalid', message: `no fake for ${cmd}` });
    return h(args);
  }
  async upload(cmd: string, args: unknown, bytes: Uint8Array): Promise<unknown> {
    return this.call(cmd, { ...(args as object), __bytes: bytes.length });
  }
  async download(cmd: string, args: unknown): Promise<Uint8Array> {
    await this.call(cmd, args);
    return new Uint8Array([1, 2, 3]);
  }
}
