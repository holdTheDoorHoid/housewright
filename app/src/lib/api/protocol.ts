/** Messages between the page and the database worker. */
export type ToWorker =
  | { id: number; kind?: 'call'; cmd: string; args: string }
  | { id: number; kind: 'upload'; cmd: string; args: string; bytes: Uint8Array }
  | { id: number; kind: 'download'; cmd: string; args: string };
export type FromWorker =
  | { type: 'ready'; storage: 'opfs' | 'memory' }
  | { type: 'locked' }
  | { type: 'failed'; message: string }
  | { type: 'result'; id: number; text: string }
  | { type: 'bytes'; id: number; bytes: Uint8Array };
