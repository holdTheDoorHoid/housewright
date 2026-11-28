import { test, expect, type Page } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Phase 8 on the real web build: floor plans. Draw two rooms by dragging,
// name them, drop a thing's pin, add a second floor and draw its walls
// point by point with a typed length, add a door, undo and redo, trace
// over a picture, reload and find it all still there, open the thing's
// page with its plan thumbnail, and print the floor (the binder's
// floor plan page). Screenshots go to docs/phase-8-evidence/.

const EVIDENCE = new URL('../../docs/phase-8-evidence/', import.meta.url).pathname;
const PHOTO = readFileSync(new URL('./fixtures/rating-plate-synthetic.jpg', import.meta.url));

async function firstRun(page: Page) {
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

async function shot(page: Page, name: string) {
  mkdirSync(EVIDENCE, { recursive: true });
  await page.screenshot({ path: join(EVIDENCE, name), fullPage: false });
}

/** The canvas's screen box. */
async function canvas(page: Page) {
  // Whole drawing on screen (a partly visible one would lose taps).
  await page.getByTestId('plan-canvas').evaluate((el) => {
    const r = el.getBoundingClientRect();
    if (r.top < 0 || r.bottom > window.innerHeight) el.scrollIntoView({ block: 'center' });
  });
  const b = await page.getByTestId('plan-canvas').boundingBox();
  if (!b) throw new Error('no canvas');
  return b;
}

async function drag(page: Page, from: [number, number], to: [number, number]) {
  const c = await canvas(page);
  await page.mouse.move(c.x + from[0], c.y + from[1]);
  await page.mouse.down();
  for (let i = 1; i <= 8; i++) {
    await page.mouse.move(c.x + from[0] + ((to[0] - from[0]) * i) / 8, c.y + from[1] + ((to[1] - from[1]) * i) / 8);
  }
}

async function tap(page: Page, at: [number, number]) {
  const c = await canvas(page);
  await page.mouse.click(c.x + at[0], c.y + at[1]);
}

function pdfText(pdf: Buffer): string | null {
  const dir = mkdtempSync(join(tmpdir(), 'hw-plan-'));
  try {
    const f = join(dir, 'p.pdf');
    writeFileSync(f, pdf);
    return execFileSync('pdftotext', [f, '-'], { stdio: ['ignore', 'pipe', 'ignore'] }).toString();
  } catch {
    return null;
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

async function lastPdf(page: Page, before: number): Promise<Buffer> {
  await expect
    .poll(() => page.evaluate(() => (window as unknown as { __pdfs: Blob[] }).__pdfs.length), { timeout: 120_000 })
    .toBeGreaterThan(before);
  const b64 = await page.evaluate(async () => {
    const list = (window as unknown as { __pdfs: Blob[] }).__pdfs;
    const buf = new Uint8Array(await list[list.length - 1].arrayBuffer());
    let s = '';
    for (const x of buf) s += String.fromCharCode(x);
    return btoa(s);
  });
  return Buffer.from(b64, 'base64');
}

test.use({ actionTimeout: 20_000 });

test('floor plans: rooms, pins, floors, walls with typed lengths, doors, undo, tracing, print', async ({ page }) => {
  test.setTimeout(300_000);
  await page.setViewportSize({ width: 1100, height: 1000 });
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await firstRun(page);

  // A thing to pin.
  await goHash(page, '#/add');
  await page.getByLabel('What is it?').fill('water heater');
  await page.getByRole('button', { name: /^Tank water heater/ }).click();
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await page.getByRole('button', { name: /Switch on the top/ }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText(/Tank water heater/);
  // No plan yet: the thing's page does not push one.
  await expect(page.getByText('Show where it is on the floor plan')).toHaveCount(0);

  // The plan is reached from Home, and is optional.
  await page.getByRole('link', { name: 'Home', exact: true }).click();
  await page.getByRole('link', { name: 'Floor plans' }).click();
  await expect(page.getByText(/A floor plan is optional/)).toBeVisible();
  await page.getByRole('button', { name: 'Start a floor plan' }).click();
  await expect(page.getByRole('button', { name: 'Ground floor', pressed: true })).toBeVisible();

  // Stage A: drag out a room; the readout shows its size as you drag.
  await page.getByRole('button', { name: 'Room', exact: true }).click();
  await drag(page, [120, 100], [330, 280]);
  await expect(page.getByTestId('plan-readout')).toContainText('sq ft');
  await expect(page.getByTestId('plan-readout')).toContainText('×');
  await shot(page, 'desktop-drawing-a-room.png');
  await page.mouse.up();
  await page.getByLabel('Room', { exact: true }).fill('Kitchen');
  await page.getByRole('button', { name: 'Save room' }).click();
  await expect(page.getByTestId('room-sheet')).toContainText('Kitchen');
  // A second room beside it, as an L.
  await page.getByRole('button', { name: 'Room', exact: true }).click();
  await page.getByRole('button', { name: 'L shape' }).click();
  await drag(page, [330, 100], [560, 360]);
  await page.mouse.up();
  await page.getByLabel('Room', { exact: true }).fill('Living room');
  await page.getByRole('button', { name: 'Save room' }).click();
  await expect(page.getByRole('button', { name: /^Living room\s+\d+ sq ft/ })).toBeVisible();
  await expect(page.getByTestId('plan-total')).toContainText('sq ft');
  const total = (await page.getByTestId('plan-total').textContent())!;

  // A pin for the water heater, dropped in the kitchen: it is now in the kitchen.
  await page.getByRole('button', { name: 'Pin', exact: true }).click();
  await tap(page, [200, 190]);
  const sheet = page.getByTestId('pin-sheet');
  await sheet.getByLabel('Thing', { exact: true }).selectOption({ index: 1 });
  await expect(sheet.getByLabel('Thing', { exact: true })).toHaveValue(/.+/);
  await sheet.getByRole('button', { name: 'Place pin' }).click();
  await expect(page.getByRole('status')).toContainText('is now in Kitchen');
  await expect(page.getByText('1. Tank water heater')).toBeVisible();

  // Stage B: a second floor, walls tapped corner by corner.
  await page.getByText('Add a floor').click();
  await page.getByLabel('New floor name').fill('Basement');
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Basement', pressed: true })).toBeVisible();
  await page.getByRole('button', { name: 'Walls' }).click();
  await tap(page, [150, 120]);
  const c = await canvas(page);
  await page.mouse.move(c.x + 420, c.y + 122);
  await expect(page.getByTestId('plan-readout')).toContainText('0°');
  await tap(page, [420, 122]);
  const c2 = await canvas(page);
  await page.mouse.move(c2.x + 422, c2.y + 330);
  await expect(page.getByTestId('plan-readout')).toContainText('corner 90°');
  await shot(page, 'desktop-walls-live-readout.png');
  // Type the next wall's length instead of tapping.
  await page.getByLabel("Next wall's length").fill('12\' 6"');
  await expect(page.getByText(/12' 6", towards the pointer/)).toBeVisible();
  await page.getByRole('button', { name: 'Add wall' }).click();
  await expect(page.getByText(/^Walls \(2\)/)).toBeAttached();
  const c3 = await canvas(page);
  await page.mouse.move(c3.x + 150, c3.y + 330);
  await tap(page, [150, 330]);
  await page.getByRole('button', { name: 'Close the shape' }).click();
  await expect(page.getByRole('status')).toContainText('Walls closed');
  // The second wall is exactly the typed length.
  await page.getByText(/^Walls \(\d+\), doors and windows/).click();
  await page.getByRole('button', { name: /Wall 2\s/ }).click();
  await expect(page.getByTestId('wall-sheet')).toContainText('Wall, 12\' 6"');
  // Name the space the walls closed.
  await page.getByRole('button', { name: /Unnamed space: name it/ }).click();
  await page.getByLabel('Room', { exact: true }).fill('Workshop');
  await page.getByRole('button', { name: 'Save room' }).click();
  await expect(page.getByRole('button', { name: /Workshop\s+\d+ sq ft/ })).toBeVisible();

  // A door on the first wall, then undo and redo it.
  await page.getByRole('button', { name: 'Door', exact: true }).click();
  await tap(page, [285, 121]);
  await expect(page.getByTestId('opening-sheet')).toContainText('Door');
  await page.getByLabel('Width').fill('3\'');
  await page.getByTestId('opening-sheet').getByRole('button', { name: 'Save' }).click();
  await expect(page.getByTestId('opening-sheet')).toContainText('3\' 0" wide');
  await page.getByRole('button', { name: 'Swing other side' }).click();
  await expect(page.getByText(/Walls \(4\), doors and windows \(1\)/)).toBeVisible();
  await page.getByRole('button', { name: 'Undo' }).click();
  await page.getByRole('button', { name: 'Undo' }).click();
  await page.getByRole('button', { name: 'Undo' }).click();
  await expect(page.getByText(/doors and windows \(0\)/)).toBeVisible();
  await page.getByRole('button', { name: 'Redo' }).click();
  await expect(page.getByText(/doors and windows \(1\)/)).toBeVisible();
  await page.getByRole('button', { name: 'Move', exact: true }).click();
  await shot(page, 'desktop-basement-walls-door.png');

  // Trace over a picture.
  await page.getByText('Trace over a plan').click();
  await page.getByLabel('Choose a photo of a plan').setInputFiles({ name: 'old-plan.jpg', mimeType: 'image/jpeg', buffer: PHOTO });
  await expect(page.getByTestId('plan-canvas').locator('image')).toHaveCount(1, { timeout: 30_000 });
  await page.getByLabel('How wide the drawing is in real life').fill("30'");
  await page.getByRole('button', { name: 'Set', exact: true }).click();
  await expect(page.getByLabel('How wide the drawing is in real life')).toHaveAttribute('placeholder', "30' 0\"");
  await canvas(page);
  await page.getByTestId('plan-canvas').evaluate((el) => el.scrollIntoView({ block: 'start' }));
  await shot(page, 'desktop-trace-over-picture.png');

  // Reload: everything is still there.
  await page.reload();
  await expect(page.getByRole('button', { name: 'Basement', pressed: true })).toBeVisible();
  await expect(page.getByRole('button', { name: /Workshop\s+\d+ sq ft/ })).toBeVisible();
  await expect(page.getByTestId('plan-canvas').locator('image')).toHaveCount(1);
  await page.getByRole('button', { name: 'Ground floor' }).click();
  await expect(page.getByTestId('plan-total')).toHaveText(total);
  await expect(page.getByText('1. Tank water heater')).toBeVisible();
  await shot(page, 'desktop-ground-floor.png');

  // The thing's page shows where it is.
  await page.getByText('1. Tank water heater').click();
  await page.getByRole('link', { name: 'Open Tank water heater' }).click();
  await expect(page.getByTestId('plan-thumb')).toContainText('Ground floor · Kitchen · pin 1');
  await shot(page, 'desktop-thing-thumbnail.png');
  await page.getByTestId('plan-thumb').click();
  await expect(page.getByTestId('pin-sheet')).toContainText('Pin 1: Tank water heater');

  // Print the floor: the binder's floor plan page, typeset in the browser.
  const before = await page.evaluate(() => (window as unknown as { __pdfs: Blob[] }).__pdfs.length);
  const popup = page.waitForEvent('popup').catch(() => null);
  await page.getByText('This floor', { exact: true }).click();
  await page.getByRole('button', { name: 'Print this floor (PDF)' }).click();
  const pdf = await lastPdf(page, before);
  (await popup)?.close();
  expect(pdf.subarray(0, 4).toString()).toBe('%PDF');
  mkdirSync(EVIDENCE, { recursive: true });
  writeFileSync(join(EVIDENCE, 'web-ground-floor.pdf'), pdf);
  const t = pdfText(pdf);
  if (t) {
    expect(t).toContain('Floor plan: Ground floor');
    expect(t).toContain('Kitchen');
    expect(t).toContain('Tank water heater');
  }
  expect(errors).toEqual([]);
});

test('floor plans on a phone-sized screen', async ({ browser }) => {
  test.setTimeout(180_000);
  const ctx = await browser.newContext({ viewport: { width: 390, height: 844 }, hasTouch: true, deviceScaleFactor: 2 });
  const page = await ctx.newPage();
  await firstRun(page);
  await goHash(page, '#/plans');
  await page.getByRole('button', { name: 'Start a floor plan' }).click();
  await page.getByRole('button', { name: 'Room', exact: true }).click();
  await drag(page, [60, 60], [220, 220]);
  await page.mouse.up();
  await page.getByLabel('Room', { exact: true }).fill('Bedroom');
  await page.getByRole('button', { name: 'Save room' }).click();
  await expect(page.getByTestId('plan-total')).toContainText('sq ft');
  await page.getByRole('button', { name: 'Walls' }).click();
  await tap(page, [230, 60]);
  await page.mouse.move((await canvas(page)).x + 330, (await canvas(page)).y + 61);
  await page.evaluate(() => window.scrollTo(0, 0));
  await shot(page, 'phone-walls.png');
  await page.getByRole('button', { name: 'Stop here' }).click();
  await page.getByRole('button', { name: 'Move', exact: true }).click();
  await tap(page, [140, 140]);
  await expect(page.getByTestId('room-sheet')).toContainText('Bedroom');
  await shot(page, 'phone-room-selected.png');
  await page.getByTestId('room-sheet').scrollIntoViewIfNeeded();
  await shot(page, 'phone-room-sheet.png');
  await ctx.close();
});
