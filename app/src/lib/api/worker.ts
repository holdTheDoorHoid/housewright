/// <reference lib="webworker" />
// The database worker: owns the WebAssembly core and the OPFS database.
// OPFS sync access handles are only available in dedicated workers, and the
// sahpool VFS holds them exclusively, so exactly one tab may own the database.
// A Web Lock decides which: a second tab is told plainly instead of failing
// on a locked file.

import init, {
  open_opfs,
  open_memory,
  call,
  call_upload,
  call_download,
  load_ocr,
  has_ocr,
} from '../../../../web/pkg/housewright_web.js';
import type { FromWorker, ToWorker } from './protocol';

declare const self: DedicatedWorkerGlobalScope;

// Sync's network transport (D-97): a synchronous request, which dedicated
// workers allow. The core calls this through `hwHttp`; nothing else in the
// app makes network requests to the sync server.
(self as unknown as { hwHttp: unknown }).hwHttp = (
  method: string,
  url: string,
  headersJson: string,
  body: Uint8Array,
): { status: number; headers: string; body: Uint8Array } => {
  const x = new XMLHttpRequest();
  x.open(method, url, false);
  x.responseType = 'arraybuffer';
  x.timeout = 60_000;
  for (const [k, v] of JSON.parse(headersJson) as [string, string][]) {
    if (k.toLowerCase() === 'content-type' && method === 'GET') continue;
    x.setRequestHeader(k, v);
  }
  try {
    x.send(method === 'GET' || method === 'DELETE' || body.length === 0 ? null : body.slice());
  } catch (e) {
    throw String((e as Error)?.message ?? e);
  }
  if (x.status === 0) throw 'no answer from the server';
  return {
    status: x.status,
    headers: x.getAllResponseHeaders(),
    body: new Uint8Array((x.response as ArrayBuffer | null) ?? new ArrayBuffer(0)),
  };
};

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
  if (m.kind === 'upload') {
    if (m.cmd === 'read_label') {
      // Label reading needs the models; fetch them from the app's own
      // files the first time (never from anywhere else, D-74).
      void ocrReady().then(
        () => post({ type: 'result', id: m.id, text: call_upload(m.cmd, m.args, m.bytes) }),
        (e) =>
          post({
            type: 'result',
            id: m.id,
            text: JSON.stringify({ error: { code: 'ocr_unavailable', message: String(e?.message ?? e) } }),
          }),
      );
      return;
    }
    post({ type: 'result', id: m.id, text: call_upload(m.cmd, m.args, m.bytes) });
  } else if (m.kind === 'download' && m.cmd === 'binder_pdf') {
    void binderPdf(m.args).then(
      (bytes) => self.postMessage({ type: 'bytes', id: m.id, bytes } satisfies FromWorker, [bytes.buffer]),
      (e) => {
        const text = String(e?.message ?? e);
        post({
          type: 'result',
          id: m.id,
          text: text.startsWith('{') ? text : JSON.stringify({ error: { code: 'internal', message: text } }),
        });
      },
    );
  } else if (m.kind === 'download') {
    try {
      const bytes = call_download(m.cmd, m.args);
      self.postMessage({ type: 'bytes', id: m.id, bytes } satisfies FromWorker, [bytes.buffer]);
    } catch (e) {
      post({ type: 'result', id: m.id, text: String(e) });
    }
  } else {
    post({ type: 'result', id: m.id, text: call(m.cmd, m.args) });
  }
}

// The binder's typesetter (Typst) is a separate WebAssembly module of about
// 11 MB, fetched from the app's own files the first time someone prints
// (D-80). This module makes the bundle; that one makes the PDF; then the
// print is recorded here.
type BinderModule = typeof import('../../../../web-binder/pkg/housewright_binder_web.js');
let binderLoading: Promise<BinderModule> | null = null;
function binderModule(): Promise<BinderModule> {
  binderLoading ??= (async () => {
    const mod = await import('../../../../web-binder/pkg/housewright_binder_web.js');
    await mod.default();
    return mod;
  })().catch((e) => {
    binderLoading = null;
    throw e;
  });
  return binderLoading;
}

async function binderPdf(argsJson: string): Promise<Uint8Array<ArrayBuffer>> {
  const bundle = call_download('binder_bundle', argsJson);
  const mod = await binderModule();
  const pdf = mod.render(bundle);
  const args = JSON.parse(argsJson || '{}') as { record?: boolean };
  if (args.record !== false) {
    const out = JSON.parse(call('record_binder_print', mod.receipt(bundle)));
    if (out.error) throw new Error(JSON.stringify(out));
  }
  return pdf as Uint8Array<ArrayBuffer>;
}

let ocrLoading: Promise<void> | null = null;
function ocrReady(): Promise<void> {
  if (has_ocr()) return Promise.resolve();
  ocrLoading ??= (async () => {
    // The models sit beside the app as `ocr/*.onnx` (see vite.config.ts).
    // In a built app this worker runs from `<base>/assets/`.
    const here = self.location.href;
    const base = here.includes('/assets/') ? here.slice(0, here.indexOf('/assets/') + 1) : `${self.location.origin}/`;
    const get = async (name: string) => {
      const r = await fetch(`${base}ocr/${name}`);
      if (!r.ok) throw new Error('Label reading is not included in this copy of the app.');
      return new Uint8Array(await r.arrayBuffer());
    };
    const [det, rec] = await Promise.all([get('text-detection.onnx'), get('text-recognition.onnx')]);
    load_ocr(det, rec);
  })().catch((e) => {
    ocrLoading = null;
    throw e;
  });
  return ocrLoading;
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
