import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { QUESTIONS, answered, clear, fromProfile, toSend } from './questions';
import { CATEGORY_LABELS } from './labels';

describe('home questions', () => {
  it('are about ten, each with a title, starting with the home and its alarms', () => {
    expect(QUESTIONS.length).toBe(10);
    expect(QUESTIONS[0].id).toBe('home_type');
    expect(QUESTIONS[1].id).toBe('alarms');
    expect(new Set(QUESTIONS.map((q) => q.id)).size).toBe(10);
  });

  it('Not sure clears an answer; unticked checklist items are not sent', () => {
    const alarms = QUESTIONS.find((q) => q.id === 'alarms')!;
    const a = { smoke_alarms: 'one' as const, co_alarms: 'none' as const, home_type: 'house' as const };
    expect(answered(alarms, a)).toBe(true);
    const c = clear(alarms, a);
    expect(answered(alarms, c)).toBe(false);
    expect(c.home_type).toBe('house');
    const sent = toSend({ features: { pool: true, deck: false }, heating: [], zip: '' });
    expect(sent.features).toEqual({ pool: true });
    expect('heating' in sent).toBe(false);
    expect('zip' in sent).toBe(false);
  });

  it('a stored profile comes back as answers for Redo', () => {
    const a = fromProfile({ id: 'p1', zip: { value: '19103', sensitivity: 'normal' as const }, home_type: 'condo' as const });
    expect(a).toEqual({ zip: '19103', home_type: 'condo' });
    expect(fromProfile(null)).toEqual({});
  });

  it('match the Rust enums', () => {
    const rust = readFileSync(resolve(process.cwd(), '../crates/core/src/profile.rs'), 'utf8');
    const features = QUESTIONS.find((q) => q.id === 'features')!.parts[0];
    if (features.kind !== 'checklist') throw new Error('features is a checklist');
    // Every checklist key is a Feature variant (snake_case of the Rust name).
    const variants = [...rust.slice(rust.indexOf('pub enum Feature')).matchAll(/^\s{4}([A-Z][A-Za-z]+),$/gm)]
      .map((m) => m[1].replace(/([a-z])([A-Z])/g, '$1_$2').toLowerCase())
      .slice(0, 21);
    expect(features.options.map((o) => o[0]).sort()).toEqual(variants.sort());
  });
});

describe('category labels', () => {
  it('match the fixed list in the Rust template test', () => {
    const rust = readFileSync(resolve(process.cwd(), '../crates/core/tests/templates.rs'), 'utf8');
    const block = rust.slice(rust.indexOf('pub const CATEGORIES'), rust.indexOf('];', rust.indexOf('pub const CATEGORIES')));
    const pairs = Object.fromEntries([...block.matchAll(/\("([a-z_]+)", "([^"]+)"\)/g)].map((m) => [m[1], m[2]]));
    expect(pairs).toEqual(CATEGORY_LABELS);
  });
});
