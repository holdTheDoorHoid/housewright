import { describe, it, expect } from 'vitest';
import { telHref, mailHref, webHref } from './links';
import { noticeHash } from './state.svelte';

describe('links that leave the app', () => {
  it('makes dialer links from phone numbers as people write them', () => {
    expect(telHref('(215) 555-0101')).toBe('tel:2155550101');
    expect(telHref('+1 215 555 0101 ext 4')).toBe('tel:+12155550101');
    expect(telHref('call me')).toBeNull();
    expect(telHref(null)).toBeNull();
  });
  it('accepts only real-looking email addresses', () => {
    expect(mailHref('ana@ruiz.example')).toBe('mailto:ana@ruiz.example');
    expect(mailHref('not an address')).toBeNull();
    expect(mailHref('a@b.c?subject=x"')).toBeNull();
  });
  it('shows only http and https links', () => {
    expect(webHref('https://example.com')).toBe('https://example.com');
    expect(webHref('javascript:alert(1)')).toBeNull();
  });
  it('sends notices to what they are about', () => {
    expect(noticeHash('item/abc')).toBe('#/thing/abc');
    expect(noticeHash('contractor/k1')).toBe('#/contractor/k1');
    expect(noticeHash('nonsense')).toBe('#/');
  });
});
