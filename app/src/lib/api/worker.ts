/// <reference lib="webworker" />
// The database worker: owns the WebAssembly core and the OPFS database.
// OPFS sync access handles are only available in dedicated workers, and the
// sahpool VFS holds them exclusively, so exactly one tab may own the database.
// A Web Lock decides which: a second tab is told plainly instead of failing
// on a locked file.

import init, { open_opfs, open_memory, call } from '../../../../web/pkg/housewright_web.js';
import type { FromWorker, ToWorker } from './protocol';

declare const self: DedicatedWorkerGlobalScope;

const post = (m: FromWorker) => self.postMessage(m);
const queue: ToWorker[] = [];
let ready = false;

self.onmessage = (e: MessageEvent<ToWorker>) => {
  if (!ready) {
    queue.push(e.data);
    return;
  }
  run(e.data);
};

function run(m: ToWorker) {
  post({ type: 'result', id: m.id, text: call(m.cmd, m.args) });
}

async function open(): Promise<'opfs' | 'memory'> {
  await init();
  if (typeof navigator.storage?.getDirectory === 'function') {
    try {
      // Ask the browser not to evict the database under storage pressure.
      await navigator.storage.persist?.().catch(() => false);
      return (await open_opfs('housewright.db')) as 'opfs';
    } catch (e) {
      console.warn('OPFS unavailable, using memory', e);
    }
  }
  return open_memory() as 'memory';
}

async function start() {
  const locks = navigator.locks;
  if (!locks) {
    // Very old browser: no way to coordinate tabs; try anyway.
    finish(await open());
    return;
  }
  await locks.request('housewright-database', { ifAvailable: true }, async (lock) => {
    if (!lock) {
      post({ type: 'locked' });
      return;
    }
    finish(await open());
    // Hold the lock for the life of this tab.
    await new Promise(() => {});
  });
}

function finish(storage: 'opfs' | 'memory') {
  ready = true;
  post({ type: 'ready', storage });
  for (const m of queue.splice(0)) run(m);
}

start().catch((e) => post({ type: 'failed', message: String(e?.message ?? e) }));
