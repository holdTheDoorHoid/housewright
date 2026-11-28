import { describe, it, expect } from 'vitest';
import { toBase64, fromBase64 } from './tauri';

describe('file bytes over the Tauri bridge', () => {
  it('round-trip through base64, including large files', () => {
    const bytes = new Uint8Array(200_001).map((_, i) => (i * 31) % 256);
    expect(fromBase64(toBase64(bytes))).toEqual(bytes);
    expect(toBase64(new Uint8Array([104, 105]))).toBe('aGk=');
  });
});
