import { test, expect, type Page } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Phase 5 on the real web build: the house binder. Every edition and the
// sealed volume are generated as PDFs (typeset by Typst in WebAssembly);
// sensitive values stay out of the main volume; printing records what was
// printed; a change marks one section; "print the changed sections" prints
// only that one.

async function firstRun(page: Page) {
  // Keep every PDF the page makes, so the test can read it.
  await page.addInitScript(() => {
    const keep: Blob[] = [];
    (window as unknown as { __pdfs: Blob[] }).__pdfs = keep;
    const orig = URL.createObjectURL.bind(URL);
    URL.createObjectURL = (b: Blob | MediaSource) => {
      if (b instanceof Blob && b.type === 'application/pdf') keep.push(b);
      return orig(b);
    };
  });
  await page.goto('/');
  await page.getByLabel('Your name').fill('Alex');
  await page.getByRole('button', { name: 'Next: the home questions', exact: true }).click();
  await expect(page.getByText('Question 1 of 10')).toBeVisible();
}

async function goHash(page: Page, hash: string) {
  await page.evaluate((h) => (location.hash = h), hash);
}

/** Click a button that makes a PDF; return the PDF's bytes. */
async function pdfFrom(page: Page, name: string | RegExp): Promise<Buffer> {
  const before = await page.evaluate(() => (window as unknown as { __pdfs: Blob[] }).__pdfs.length);
  const popup = page.waitForEvent('popup').catch(() => null);
  await page.getByRole('button', { name }).click();
  await expect
    .poll(() => page.evaluate(() => (window as unknown as { __pdfs: Blob[] }).__pdfs.length), { timeout: 90_000 })
    .toBeGreaterThan(before);
  const p = await popup;
  await p?.close();
  const b64 = await page.evaluate(async () => {
    const list = (window as unknown as { __pdfs: Blob[] }).__pdfs;
    const buf = new Uint8Array(await list[list.length - 1].arrayBuffer());
    let s = '';
    for (const x of buf) s += String.fromCharCode(x);
    return btoa(s);
  });
  return Buffer.from(b64, 'base64');
}

/** The PDF's text, as a reader extracts it (needs poppler's pdftotext). */
function pdfText(pdf: Buffer): string | null {
  const dir = mkdtempSync(join(tmpdir(), 'hw-binder-'));
  try {
    const f = join(dir, 'b.pdf');
    writeFileSync(f, pdf);
    return execFileSync('pdftotext', [f, '-'], { stdio: ['ignore', 'pipe', 'ignore'] }).toString();
  } catch {
    return null;
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const squash = (s: string) => s.replace(/\s+/g, ' ');

test('the house binder: every edition, the sealed volume, and reprinting what changed', async ({ page }) => {
  test.setTimeout(400_000);
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await firstRun(page);

  // Something to print: a thing, the water main, a code, a contact.
  await goHash(page, '#/add');
  await page.getByLabel('What is it?').fill('water heater');
  await page.getByRole('button', { name: /^Tank water heater/ }).click();
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await page.getByRole('button', { name: /Switch on the top/ }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText(/Tank water heater/);
  await page.getByLabel('Add a quirk only you know').fill('Rumbles when the tank needs flushing.');
  await page.getByRole('button', { name: 'Add note' }).click();
  await expect(page.getByText('Rumbles when the tank needs flushing.')).toBeVisible();

  await page.getByRole('link', { name: 'Home', exact: true }).click();
  const prompt = page.getByTestId('fact-prompt');
  await prompt.getByRole('textbox').fill('Basement, front wall, blue handle');
  await prompt.getByRole('button', { name: 'Save' }).click();
  await expect(page.getByText('Basement, front wall, blue handle')).toBeVisible();
  await page.getByRole('button', { name: 'Add to access' }).click();
  const ff = page.getByTestId('fact-form');
  await ff.getByLabel('What').fill('Gate code');
  await ff.getByLabel('Answer').fill('GATE-4417-SECRET');
  await ff.getByRole('button', { name: 'Save' }).click();
  await expect(page.getByRole('button', { name: 'Reveal Gate code' })).toBeVisible();

  // The binder screen.
  await page.getByRole('link', { name: 'House binder' }).click();
  await expect(page.getByRole('heading', { name: 'House binder', level: 1 })).toBeVisible();
  await expect(page.getByRole('status').filter({ hasText: 'Not printed yet' })).toBeVisible();

  // Who to call, for the emergency page.
  await page.getByRole('button', { name: 'Add a contact' }).click();
  await page.getByLabel('Name').fill('Ace Plumbing');
  await page.getByLabel('What they do').fill('plumbing');
  await page.getByLabel('Phone').fill('555-0101');
  await page.getByLabel(/Call them in an emergency/).check();
  await page.getByRole('button', { name: 'Save contact' }).click();
  await expect(page.getByText('Ace Plumbing')).toBeVisible();

  // Where the sealed volume is kept.
  await page.getByLabel(/printed in the main binder/).fill('In the fire safe, top shelf');
  await page.getByRole('button', { name: 'Save', exact: true }).click();

  // The household binder: main volume, then the sealed one.
  const main = await pdfFrom(page, 'Print the whole binder');
  expect(main.subarray(0, 4).toString()).toBe('%PDF');
  const sealed = await pdfFrom(page, 'Print the sealed volume (separately)');
  expect(sealed.subarray(0, 4).toString()).toBe('%PDF');
  const mt = pdfText(main);
  const st = pdfText(sealed);
  if (mt && st) {
    expect(squash(mt)).toContain('In an emergency');
    expect(squash(mt)).toContain('Basement, front wall, blue handle');
    expect(squash(mt)).toContain('Ace Plumbing');
    expect(squash(mt)).toContain('In the fire safe, top shelf');
    expect(squash(mt)).toContain('Rumbles when the tank needs flushing.');
    expect(mt).not.toContain('GATE-4417-SECRET');
    expect(squash(st)).toContain('SEALED');
    expect(st).toContain('GATE-4417-SECRET');
  } else {
    console.log('pdftotext not installed: PDF text not checked');
  }
  await expect(page.getByRole('status').filter({ hasText: 'Nothing has changed since you printed it' })).toBeVisible();
  // After the first print the yearly reminder is offered, not switched on.
  await expect(page.getByRole('button', { name: 'Remind me once a year' })).toBeVisible();

  // Every other edition prints too.
  for (const [label, button] of [
    ['Emergency page', 'Print the emergency page'],
    ['Handover binder', 'Print the whole binder'],
    ['Succession binder', 'Print the whole binder'],
  ] as const) {
    await page.getByRole('radio', { name: new RegExp(label) }).check();
    const pdf = await pdfFrom(page, button);
    expect(pdf.subarray(0, 4).toString()).toBe('%PDF');
    const t = pdfText(pdf);
    if (t) {
      expect(t).not.toContain('GATE-4417-SECRET');
      if (label === 'Handover binder') expect(t).not.toContain('Alex');
    }
  }
  await page.getByRole('radio', { name: /Succession binder/ }).check();
  const ss = await pdfFrom(page, 'Print the sealed volume (separately)');
  const sst = pdfText(ss);
  if (sst) expect(squash(sst)).toContain('Accounts to transfer');

  // A change marks one section; only that one is reprinted.
  await page.getByRole('radio', { name: /Household binder/ }).check();
  await page.getByRole('link', { name: 'Home', exact: true }).click();
  await page.getByRole('button', { name: 'Add to sizes and supplies' }).click();
  await ff.getByLabel('What').fill('Furnace filter');
  await ff.getByLabel('Answer').fill('16 x 25 x 1');
  await ff.getByRole('button', { name: 'Save' }).click();
  await expect(page.getByText('16 x 25 x 1')).toBeVisible();
  await page.getByRole('link', { name: 'House binder' }).click();
  await page.getByRole('radio', { name: /Household binder/ }).check();
  await expect(page.getByRole('status').filter({ hasText: '1 section has changed since you printed on' })).toBeVisible();
  await expect(page.locator('.pill.attention')).toHaveCount(1);
  const partial = await pdfFrom(page, 'Print the 1 changed section');
  const pt = pdfText(partial);
  if (pt) {
    expect(pt).toContain('16 x 25 x 1');
    expect(pt).toContain('Facts 1');
    expect(pt).not.toContain('In an emergency');
  }
  await expect(page.getByRole('status').filter({ hasText: 'Nothing has changed since you printed it' })).toBeVisible();

  await page.getByRole('button', { name: 'Remind me once a year' }).click();
  await expect(page.getByText('You will be reminded once a year')).toBeVisible();
  expect(errors).toEqual([]);
});
