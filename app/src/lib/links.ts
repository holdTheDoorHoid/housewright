// Links that leave the app: the phone's dialer, the mail app, and official
// web pages (a recall notice, a registration page, a contractor's site).
//
// On the web these are ordinary links. Inside the Android app the WebView
// does not open them itself, so the reminders plugin hands them to the
// phone (ACTION_DIAL for a number, ACTION_SENDTO for mail, ACTION_VIEW for
// https); it accepts only those three kinds.
import { currentBackend } from './api';

/** "tel:+12155550101" from "(215) 555-0101 ext 4"; null when there is no number. */
export function telHref(phone: string | null | undefined): string | null {
  if (!phone) return null;
  const main = phone.split(/ext|x|#/i)[0];
  const digits = main.replace(/[^\d+]/g, '').replace(/(?!^)\+/g, '');
  return digits.replace('+', '').length >= 3 ? `tel:${digits}` : null;
}

/** "mailto:ana@example.com", or null for anything that is not an address. */
export function mailHref(email: string | null | undefined): string | null {
  if (!email) return null;
  const e = email.trim();
  return /^[^\s@<>"]+@[^\s@<>"]+\.[^\s@<>"]+$/.test(e) ? `mailto:${encodeURIComponent(e).replace('%40', '@')}` : null;
}

/** Only http(s) links are shown as links. */
export function webHref(url: string | null | undefined): string | null {
  if (!url) return null;
  return /^https?:\/\/[^\s]+$/i.test(url) ? url : null;
}

/** Open one of the hrefs above. */
export async function openExternal(href: string): Promise<void> {
  if (currentBackend()?.kind === 'tauri') {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('plugin:reminders|open_link', { url: href });
    return;
  }
  if (href.startsWith('https:') || href.startsWith('http:')) {
    window.open(href, '_blank', 'noopener,noreferrer');
  } else {
    location.href = href;
  }
}
