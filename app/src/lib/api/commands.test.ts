import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { COMMANDS, UPLOAD_COMMANDS, DOWNLOAD_COMMANDS } from './types';

describe('command list', () => {
  it('matches the Rust command surface', () => {
    const rust = readFileSync(
      resolve(process.cwd(), '../crates/api/src/lib.rs'),
      'utf8',
    );
    const block = rust.slice(rust.indexOf('pub const COMMANDS'), rust.indexOf('];', rust.indexOf('pub const COMMANDS')));
    const names = [...block.matchAll(/"([a-z_]+)"/g)].map((m) => m[1]);
    expect(names).toEqual(COMMANDS);
  });

  it('matches the Rust file commands', () => {
    const rust = readFileSync(resolve(process.cwd(), '../crates/api/src/records.rs'), 'utf8');
    const list = (name: string) => {
      const start = rust.indexOf(`pub const ${name}`);
      return [...rust.slice(start, rust.indexOf('];', start)).matchAll(/"([a-z_]+)"/g)].map((m) => m[1]);
    };
    expect(list('UPLOAD_COMMANDS')).toEqual(UPLOAD_COMMANDS);
    expect(list('DOWNLOAD_COMMANDS')).toEqual(DOWNLOAD_COMMANDS);
  });
});
