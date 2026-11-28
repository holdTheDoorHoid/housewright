import { test, expect, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';

// Phase 4 records on the real web build: photos, Done + photo + cost, the
// facts sheet with a hidden value, the sticker sheet, scanning a sticker
// photo, and a full backup restored into a fresh browser profile.

const PHOTO = readFileSync(new URL('./fixtures/rating-plate-synthetic.jpg', import.meta.url));

async function firstRun(page: Page) {
  await page.goto('/');
  await page.getByLabel('Your name').fill('Alex');
  await page.getByRole('button', { name: 'Next: the home questions', exact: true }).click();
  await expect(page.getByText('Question 1 of 10')).toBeVisible();
}

async function goHash(page: Page, hash: string) {
  await page.evaluate((h) => (location.hash = h), hash);
}

/** Draw the sticker preview's QR SVG into a PNG, as a phone photo would be. */
async function stickerPhoto(page: Page): Promise<Buffer> {
  const b64 = await page.evaluate(async () => {
    const svg = document.querySelector('[data-testid="sticker-preview"] .qr svg')!.outerHTML;
    const img = new Image();
    img.src = 'data:image/svg+xml;base64,' + btoa(svg);
    await img.decode();
    const c = document.createElement('canvas');
    c.width = 700;
    c.height = 500;
    const g = c.getContext('2d')!;
    g.fillStyle = '#b8b2a8';
    g.fillRect(0, 0, 700, 500);
    g.drawImage(img, 200, 100, 300, 300);
    return c.toDataURL('image/png').split(',')[1];
  });
  return Buffer.from(b64, 'base64');
}

test('photos, Done + photo + cost, facts, stickers, scanning and a full backup', async ({ page, browser }) => {
  test.setTimeout(180_000);
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await firstRun(page);

  // A thing, with its tasks switched on.
  await goHash(page, '#/add');
  await page.getByLabel('What is it?').fill('water heater');
  await page.getByRole('button', { name: /^Tank water heater/ }).click();
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await page.getByRole('button', { name: /Switch on the top/ }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText(/Tank water heater/);
  const code = (await page.getByTestId('sticker-code').textContent())!.trim();
  expect(code).toMatch(/^HW-[0-9A-Z]{5}$/);
  const thingUrl = page.url();

  // Add a photo to the thing: stored smaller, shown as a thumbnail.
  await page.getByLabel('Add a photo or PDF').setInputFiles({ name: 'label.jpg', mimeType: 'image/jpeg', buffer: PHOTO });
  await expect(page.getByRole('status')).toContainText('Photo added');
  await expect(page.getByTestId('thumb')).toHaveCount(1);
  await page.getByRole('button', { name: /^Open label/ }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await page.getByRole('button', { name: 'Close', exact: true }).click();

  // Edit details: a field of my own, sensitive because it is a code.
  await page.getByRole('button', { name: 'Edit details' }).click();
  await page.getByLabel('Model').fill('XG40T06EC36U1');
  await page.getByRole('button', { name: 'Add a field of your own' }).click();
  await page.getByLabel('Field name').fill('Closet lock code');
  await page.getByRole('button', { name: 'Add the field' }).click();
  await page.getByLabel(/Closet lock code/).fill('2468');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.getByText('XG40T06EC36U1')).toBeVisible();
  await expect(page.getByText('2468')).toHaveCount(0);
  await page.getByRole('button', { name: 'Hidden · Reveal' }).click();
  await expect(page.getByText('2468')).toBeVisible();

  // Done in one tap from the thing (where a sticker lands), then the
  // optional extras: a photo and a cost.
  await page.getByTestId('next-task').getByRole('button', { name: 'Done' }).click();
  await page.getByRole('status').getByRole('button', { name: 'Add details' }).click();
  const form = page.getByTestId('completion-details');
  await expect(form).toBeVisible();
  await form.getByLabel('Add a photo').setInputFiles({ name: 'after.jpg', mimeType: 'image/jpeg', buffer: PHOTO });
  await expect(page.getByRole('status')).toContainText('Photo added');
  await form.getByLabel('Parts').fill('12.50');
  await form.getByLabel('Contractor').fill('150');
  await form.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.getByText('Cost $162.50')).toBeVisible();

  // The Costs view adds it up.
  await page.getByRole('link', { name: 'Home', exact: true }).click();
  await page.getByRole('link', { name: 'Costs', exact: true }).click();
  await expect(page.getByTestId('cost-total')).toContainText('$162.50');
  await expect(page.getByRole('table', { name: 'By thing' })).toContainText('Tank water heater');

  // Home facts: answer the suggested question, then a sensitive code.
  await page.getByRole('link', { name: 'Home', exact: true }).click();
  const prompt = page.getByTestId('fact-prompt');
  await expect(prompt).toContainText('Where is your water main shutoff?');
  await prompt.getByRole('textbox').fill('Basement, front wall, blue handle');
  await prompt.getByRole('button', { name: 'Save' }).click();
  await expect(page.getByText('Basement, front wall, blue handle')).toBeVisible();
  await page.getByRole('button', { name: 'Add to access' }).click();
  const ff = page.getByTestId('fact-form');
  await ff.getByLabel('What').fill('Gate code');
  await ff.getByLabel('Answer').fill('4417#');
  await expect(ff.getByLabel(/Sensitive/)).toBeChecked();
  await ff.getByRole('button', { name: 'Save' }).click();
  await expect(page.getByRole('button', { name: 'Reveal Gate code' })).toBeVisible();
  await expect(page.getByText('4417#')).toHaveCount(0);
  await page.getByRole('button', { name: 'Reveal Gate code' }).click();
  await expect(page.getByTestId('revealed')).toHaveText('4417#');
  // Never in History.
  await page.getByRole('link', { name: 'History', exact: true }).click();
  await expect(page.getByTestId('history')).toContainText('Gate code');
  await expect(page.getByTestId('history')).not.toContainText('4417#');
  await expect(page.getByTestId('history')).not.toContainText('2468');

  // The sticker sheet: a preview, and a PDF.
  await page.getByRole('link', { name: 'Home', exact: true }).click();
  await page.getByRole('link', { name: 'Print stickers' }).click();
  await expect(page.getByTestId('sticker-preview')).toContainText(code);
  const popup = page.waitForEvent('popup');
  await page.getByRole('button', { name: 'Make the sticker sheet (PDF)' }).click();
  const pdf = await popup;
  expect(pdf.url()).toMatch(/^blob:/);
  await pdf.close();
  const photo = await stickerPhoto(page);

  // Scan a photo of the sticker, and type the code: both find the thing.
  await page.getByRole('link', { name: 'Things', exact: true }).click();
  await page.getByRole('link', { name: 'Scan a sticker' }).click();
  await page.getByLabel(/photo of the sticker/).setInputFiles({ name: 'sticker.png', mimeType: 'image/png', buffer: photo });
  await expect(page).toHaveURL(thingUrl);
  await expect(page.getByTestId('next-task')).toBeVisible();
  await goHash(page, '#/scan');
  await page.getByLabel('Or type the code under it').fill(code.slice(3).toLowerCase());
  await page.getByRole('button', { name: 'Find', exact: true }).click();
  await expect(page).toHaveURL(thingUrl);

  // A full backup, restored into a fresh browser profile, keeps the photos.
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  const dl = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Make a full backup' }).click();
  const zipPath = await (await dl).path();
  const zip = readFileSync(zipPath!);
  expect(zip.subarray(0, 2).toString()).toBe('PK');
  expect(errors).toEqual([]);

  const fresh = await browser.newContext({ baseURL: 'http://localhost:4173' });
  const p2 = await fresh.newPage();
  await p2.goto('/');
  await p2.getByRole('button', { name: 'I have a backup or export to restore' }).click();
  await p2.getByLabel(/Choose a Housewright backup/).setInputFiles({ name: 'backup.zip', mimeType: 'application/zip', buffer: zip });
  await expect(p2.getByRole('heading', { name: 'Today' })).toBeVisible();
  await p2.evaluate((h) => (location.hash = h), new URL(thingUrl).hash);
  await expect(p2.getByTestId('thumb')).toHaveCount(1);
  await expect(p2.getByTestId('sticker-code')).toHaveText(code);
  await p2.getByRole('link', { name: 'Home', exact: true }).click();
  await p2.getByRole('button', { name: 'Reveal Gate code' }).click();
  await expect(p2.getByTestId('revealed')).toHaveText('4417#');
  await fresh.close();
});

test('read a rating plate photo on the device, then check it before saving', async ({ page }) => {
  test.setTimeout(240_000);
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await firstRun(page);
  await goHash(page, '#/add');
  await page.getByRole('button', { name: 'Start from a photo of its label' }).click();
  const t0 = Date.now();
  await page.getByLabel('Photograph the label').setInputFiles({ name: 'plate.jpg', mimeType: 'image/jpeg', buffer: PHOTO });
  const reader = page.getByTestId('label-reader');
  await expect(reader.getByText('Check what was read.')).toBeVisible({ timeout: 180_000 });
  console.log(`label read in the browser in ${Date.now() - t0} ms`);
  await expect(reader.getByLabel('Make')).toHaveValue('Rheem');
  // OCR may read 0 as O; the person corrects it here.
  await expect(reader.getByLabel('Model')).toHaveValue(/XG40T[0O]6EC36U1/);
  await reader.getByLabel('Model').fill('XG40T06EC36U1');
  await expect(reader.getByText(/looks like a tank water heater/)).toBeVisible();
  await reader.getByRole('button', { name: 'Use these' }).click();
  // Nothing is saved yet: the add form is prefilled and waits for the person.
  await expect(page.getByLabel('Name')).toHaveValue('Tank water heater');
  await expect(page.getByLabel('Model')).toHaveValue('XG40T06EC36U1');
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await expect(page.getByTestId('added')).toBeVisible();
  expect(errors).toEqual([]);
});
