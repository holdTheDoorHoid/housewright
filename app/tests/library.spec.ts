import { test, expect, type Page, type Request } from '@playwright/test';
import { readFileSync, readdirSync, statSync, mkdirSync } from 'node:fs';
import { join, relative } from 'node:path';
import { gzipSync } from 'node:zlib';
import { createHash } from 'node:crypto';

// The appliance library on the real web build (D-147 to D-152): a family
// suggested from a typed make and model and from a label photo, the
// manufacturer's section on a thing's page, the manufacturer-or-generic
// choice, library updates (valid and tampered, never touching the real
// network) and the honest fallback for manuals on the web.

const PHOTO = readFileSync(new URL('./fixtures/rating-plate-synthetic.jpg', import.meta.url));
const LIBRARY = new URL('../../library/', import.meta.url).pathname;
const EVIDENCE = new URL('../../docs/library-evidence/', import.meta.url).pathname;
mkdirSync(EVIDENCE, { recursive: true });
const RAW = 'https://raw.githubusercontent.com/holdTheDoorHoid/housewright-library/published/';

async function firstRun(page: Page) {
  await page.goto('/');
  await page.getByLabel('Your name').fill('Alex');
  await page.getByRole('button', { name: 'Next: the home questions', exact: true }).click();
  await expect(page.getByText('Question 1 of 10')).toBeVisible();
}

async function goHash(page: Page, hash: string) {
  await page.evaluate((h) => (location.hash = h), hash);
}

async function addKind(page: Page, search: string, button: RegExp) {
  await goHash(page, '#/add');
  await page.getByLabel('What is it?').fill(search);
  await page.getByRole('button', { name: button }).click();
  await page.getByText('More details (optional)').click();
}

/** Every request that leaves for anywhere but the app itself. */
function outside(page: Page): Request[] {
  const seen: Request[] = [];
  page.on('request', (r) => {
    if (!r.url().startsWith('http://localhost')) seen.push(r);
  });
  return seen;
}

test('typing a make and model suggests a family; the page shows it and the choice', async ({ page }) => {
  test.setTimeout(120_000);
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  const sent = outside(page);
  await firstRun(page);

  // A strong match: the model number fits the family's pattern.
  await addKind(page, 'air', /^Central air conditioner/);
  await page.getByLabel('Make').fill('Rheem');
  await page.getByLabel('Model').fill('RA16AY');
  const s = page.getByTestId('family-suggestion');
  await expect(s).toContainText('The model number matches');
  await expect(s).toContainText('Rheem Endeavor Line air conditioners');
  await expect(s).toContainText('RA##@@*');
  await page.screenshot({ path: `${EVIDENCE}typed-suggestion.png`, fullPage: true });
  await s.getByTestId('use-family').click();
  await expect(page.getByTestId('family-chosen')).toContainText('Rheem Endeavor Line');
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await page.getByRole('button', { name: 'Not now' }).click();
  const maker = page.getByTestId('maker');
  await expect(maker.getByRole('heading', { name: 'From Rheem' })).toBeVisible();
  await expect(maker.getByTestId('maker-links')).toContainText('Product page');
  await expect(maker.getByTestId('maker-facts')).toContainText('RA19AY');
  await expect(maker).toContainText('CC BY 4.0');
  await expect(maker).toContainText(/checked Nov 2026/);

  // The manufacturer's advice next to the usual one; the person chooses.
  await addKind(page, 'washing', /^Washing machine/);
  await page.getByLabel('Make').fill('Whirlpool');
  await page.getByTestId('family-suggestion').getByRole('button', { name: 'This one' }).first().waitFor();
  await page
    .getByTestId('family-suggestion')
    .getByRole('listitem')
    .filter({ hasText: 'Whirlpool front-load washers' })
    .getByRole('button', { name: 'This one' })
    .click();
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await page.getByRole('button', { name: 'Not now' }).click();
  const g = page.getByTestId('guidance');
  await expect(g).toContainText('Whirlpool');
  await expect(g).toContainText('Usual advice');
  await page.screenshot({ path: `${EVIDENCE}guidance-choice.png`, fullPage: true });
  await g.getByTestId('follow-maker').click();
  await expect(g).toContainText('Following Whirlpool’s advice.');
  await g.getByRole('link').first().click();
  await expect(page.getByTestId('task-source')).toContainText('manufacturer guidance (Whirlpool)');
  await expect(page.getByTestId('task-source').getByRole('button', { name: 'the manufacturer’s page' })).toBeVisible();
  await page.goBack();
  await page.getByTestId('guidance').getByRole('button', { name: 'Use the usual advice instead' }).click();
  await expect(page.getByTestId('guidance')).toContainText('Following the usual advice.');

  // Typing never left the device.
  expect(sent.map((r) => r.url())).toEqual([]);
  expect(errors).toEqual([]);
});

test('parts as data, and the honest manual fallback on the web', async ({ page, context }) => {
  test.setTimeout(120_000);
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  // The manufacturer's PDF opens in a new tab; answer it here so the test
  // stays off the network.
  await context.route('https://docs.zephyronline.com/**', (r) =>
    r.fulfill({ status: 200, contentType: 'application/pdf', body: '%PDF-1.4\n%%EOF' }),
  );
  await firstRun(page);
  await addKind(page, 'range hood', /^Range hood/);
  await page.getByLabel('Make').fill('Zephyr');
  const s = page.getByTestId('family-suggestion');
  await expect(s).toContainText('Same brand');
  await s.getByRole('button', { name: 'This one' }).first().click();
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await page.getByRole('button', { name: 'Not now' }).click();

  const parts = page.getByTestId('maker-parts');
  await expect(parts).toContainText('Z0F-C091');
  await expect(parts).toContainText('not confirmed');

  const popup = page.waitForEvent('popup');
  await page.getByTestId('save-manual').first().click();
  const tab = await popup;
  expect(tab.url()).toContain('docs.zephyronline.com');
  await tab.close();
  const fb = page.getByTestId('manual-web-fallback');
  await expect(fb).toContainText('browsers do not allow it');
  await page.screenshot({ path: `${EVIDENCE}web-manual-fallback.png`, fullPage: true });
  await fb.getByLabel('Add the PDF you saved').setInputFiles({
    name: 'sienab_manual.pdf',
    mimeType: 'application/pdf',
    buffer: Buffer.from('%PDF-1.4\n%%EOF'),
  });
  await expect(page.getByRole('status')).toContainText('Manual added');
  await expect(page.getByRole('button', { name: 'Open sienab_manual.pdf' })).toBeVisible();
  expect(errors).toEqual([]);
});

test('a label photo suggests the brand’s families to choose from', async ({ page }) => {
  test.setTimeout(240_000);
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await firstRun(page);
  await goHash(page, '#/add');
  await page.getByRole('button', { name: 'Start from a photo of its label' }).click();
  await page.getByLabel('Photograph the label').setInputFiles({ name: 'plate.jpg', mimeType: 'image/jpeg', buffer: PHOTO });
  const reader = page.getByTestId('label-reader');
  await expect(reader.getByText('Check what was read.')).toBeVisible({ timeout: 180_000 });
  await reader.getByLabel('Model').fill('XG40T06EC36U1');
  // The plate's model fits no pattern the library knows for Rheem tank
  // heaters, so it offers the brand's tank heaters by name.
  const s = reader.getByTestId('family-suggestion');
  await expect(s).toContainText('Same brand');
  await expect(s).toContainText('“Rheem” is how Rheem is written on its labels');
  await page.screenshot({ path: `${EVIDENCE}label-suggestion.png`, fullPage: true });
  await s.getByRole('listitem').filter({ hasText: /gas/i }).getByRole('button', { name: 'This one' }).click();
  await expect(reader.getByTestId('family-chosen')).toBeVisible();
  await reader.getByRole('button', { name: 'Use these' }).click();
  await expect(page.getByTestId('family-chosen')).toBeVisible();
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await page.getByRole('button', { name: 'Not now' }).click();
  await expect(page.getByTestId('maker').getByRole('heading', { name: 'From Rheem' })).toBeVisible();
  expect(errors).toEqual([]);
});

/** A release made from library/, as `library-sync bundle` would publish it. */
function release(version: string): { manifest: string; gz: Buffer } {
  const files: Record<string, string> = {};
  const walk = (dir: string) => {
    for (const n of readdirSync(dir).sort()) {
      const p = join(dir, n);
      if (statSync(p).isDirectory()) walk(p);
      else if (p.endsWith('.toml')) files[relative(LIBRARY, p)] = readFileSync(p, 'utf8');
    }
  };
  walk(LIBRARY);
  files['SNAPSHOT.toml'] = files['SNAPSHOT.toml'].replace(/^version = ".*"$/m, `version = "${version}"`);
  const commit = /commit = "([0-9a-f]+)"/.exec(files['SNAPSHOT.toml'])![1];
  const sorted = Object.fromEntries(Object.entries(files).sort(([a], [b]) => (a < b ? -1 : 1)));
  const gz = gzipSync(JSON.stringify({ format: 'housewright-library-bundle', bundle: 1, files: sorted }));
  const manifest = JSON.stringify({
    format: 'housewright-library-release',
    library_format: 1,
    version,
    commit,
    published: '2099-01-01',
    bundle: `library-${version}.json.gz`,
    sha256: createHash('sha256').update(gz).digest('hex'),
    size: gz.length,
  });
  return { manifest, gz };
}

test('library updates: nothing about the home is sent; tampered files are refused', async ({ page }) => {
  test.setTimeout(120_000);
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  const { manifest, gz } = release('2099.1.1');
  let serveTampered = true;
  const asked: Request[] = [];
  await page.route(`${RAW}**`, (route) => {
    asked.push(route.request());
    const url = route.request().url();
    if (url.endsWith('/latest.json')) return route.fulfill({ status: 200, contentType: 'text/plain', body: manifest });
    if (url.endsWith('/library-2099.1.1.json.gz')) {
      const body = Buffer.from(gz);
      if (serveTampered) body[body.length >> 1] ^= 1;
      return route.fulfill({ status: 200, contentType: 'application/octet-stream', body });
    }
    return route.fulfill({ status: 404, body: 'Not found' });
  });
  await firstRun(page);
  // Something about the home that must never leave it.
  await addKind(page, 'air', /^Central air conditioner/);
  await page.getByLabel('Make').fill('Rheem');
  await page.getByLabel('Model').fill('RA16AY-SECRET');
  await page.getByRole('button', { name: 'Add', exact: true }).click();

  await goHash(page, '#/settings');
  const v = page.getByTestId('library-version');
  await expect(v).toContainText('Version 2026.11.28');
  await expect(v).toContainText('Came with the app');
  await expect(page.getByTestId('library-auto')).not.toBeChecked();
  await expect(page.getByTestId('library-credit')).toContainText('CC BY 4.0');

  // Tampered: refused, nothing changes.
  await page.getByTestId('library-check').click();
  await expect(page.getByTestId('library-message')).toContainText('checksum');
  await expect(v).toContainText('Version 2026.11.28');
  await page.screenshot({ path: `${EVIDENCE}update-tampered.png`, fullPage: true });

  // Valid: used.
  serveTampered = false;
  await page.getByTestId('library-check').click();
  await expect(page.getByTestId('library-message')).toContainText('now version 2099.1.1');
  await expect(v).toContainText('Version 2099.1.1');
  await expect(v).toContainText('Downloaded update');
  await page.screenshot({ path: `${EVIDENCE}update-valid.png`, fullPage: true });

  // Kept across a reload; then back to the built-in one.
  await page.reload();
  await goHash(page, '#/settings');
  await expect(page.getByTestId('library-version')).toContainText('Version 2099.1.1');
  await page.getByRole('button', { name: 'Go back to the version that came with the app' }).click();
  await expect(page.getByTestId('library-version')).toContainText('Version 2026.11.28');

  // Every request was a plain GET of a public file: no query, no body, no
  // cookies, no referrer, nothing typed into the app.
  expect(asked.length).toBeGreaterThanOrEqual(4);
  for (const r of asked) {
    expect(r.method()).toBe('GET');
    expect(r.url()).toMatch(/^https:\/\/raw\.githubusercontent\.com\/holdTheDoorHoid\/housewright-library\/published\/(latest\.json|library-[0-9.]+\.json\.gz)$/);
    expect(r.postData()).toBeNull();
    const h = await r.allHeaders();
    expect(h['cookie']).toBeUndefined();
    expect(h['referer']).toBeUndefined();
    expect(JSON.stringify(h)).not.toContain('SECRET');
  }
  expect(errors).toEqual([]);
});
