import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { COMMANDS } from './types';

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
});
