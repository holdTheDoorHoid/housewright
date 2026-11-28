/** Messages between the page and the database worker. */
export type ToWorker = { id: number; cmd: string; args: string };
export type FromWorker =
  | { type: 'ready'; storage: 'opfs' | 'memory' }
  | { type: 'locked' }
  | { type: 'failed'; message: string }
  | { type: 'result'; id: number; text: string };
