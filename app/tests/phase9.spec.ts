import { test, expect, type Page } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync, rmSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Phase 9 on the real web build: a contractor with a licence and a private
// rating, a job linked to them after Done, their page's totals; warranties
// in their three states; a recall check against a recorded CPSC response
// (the network is intercepted) with a match and "not mine"; and the
// insurance inventory as a PDF and a CSV, a private thing left out unless
// asked for.

const RECALLS = readFileSync(new URL('../../crates/core/tests/fixtures/cpsc/recalls-2026-10.json', import.meta.url));

function isoIn(days: number): string {
  const d = new Date(Date.now() + days * 86_400_000);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

async function firstRun(page: Page) {
  // Keep every PDF and CSV the page makes, so the test can read them.
  await page.addInitScript(() => {
    const keep: Blob[] = [];
    (window as unknown as { __files: Blob[] }).__files = keep;
    const orig = URL.createObjectURL.bind(URL);
    URL.createObjectURL = (b: Blob | MediaSource) => {
      if (b instanceof Blob && (b.type === 'application/pdf' || b.type === 'text/csv')) keep.push(b);
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

/** Click a button that makes a file; return its bytes. */
async function fileFrom(page: Page, testId: string): Promise<Buffer> {
  const count = () => page.evaluate(() => (window as unknown as { __files: Blob[] }).__files.length);
  const before = await count();
  const popup = page.waitForEvent('popup', { timeout: 120_000 }).catch(() => null);
  await page.getByTestId(testId).click();
  await expect.poll(count, { timeout: 120_000 }).toBeGreaterThan(before);
  if (testId.endsWith('pdf')) await (await popup)?.close();
  const b64 = await page.evaluate(async () => {
    const list = (window as unknown as { __files: Blob[] }).__files;
    const buf = new Uint8Array(await list[list.length - 1].arrayBuffer());
    let s = '';
    for (const x of buf) s += String.fromCharCode(x);
    return btoa(s);
  });
  return Buffer.from(b64, 'base64');
}

function pdfText(pdf: Buffer): string | null {
  const dir = mkdtempSync(join(tmpdir(), 'hw-ins-'));
  try {
    const f = join(dir, 'i.pdf');
    writeFileSync(f, pdf);
    return execFileSync('pdftotext', [f, '-'], { stdio: ['ignore', 'pipe', 'ignore'] }).toString();
  } catch {
    return null;
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

async function addThing(page: Page, search: string, pick: RegExp, switchOn = false) {
  await goHash(page, '#/add');
  await page.getByLabel('What is it?').fill(search);
  await page.getByRole('button', { name: pick }).click();
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await page.getByRole('button', { name: switchOn ? /Switch on the top/ : 'Not now' }).click();
  await expect(page.getByRole('button', { name: 'Edit details' })).toBeVisible();
}

test('contractors, warranties, recalls and the insurance inventory', async ({ page }) => {
  test.setTimeout(400_000);
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  // Recall downloads never leave the machine in this test: every request to
  // the CPSC answers with the recorded response, and the requests are kept
  // to check what was sent.
  const asked: string[] = [];
  await page.route('https://www.saferproducts.gov/**', async (route) => {
    asked.push(route.request().url());
    const url = new URL(route.request().url());
    // Only the newest window holds the recorded recalls; older ones are empty.
    const body = url.searchParams.get('RecallDateEnd')! >= '2026-10-30' ? RECALLS : Buffer.from('[]');
    await route.fulfill({ status: 200, contentType: 'application/json', body, headers: { 'access-control-allow-origin': '*' } });
  });
  await firstRun(page);

  // -- a contractor with a licence ending soon and a private rating --------
  await goHash(page, '#/contractors');
  await expect(page.getByRole('heading', { name: 'Contractors', level: 1 })).toBeVisible();
  await page.getByRole('button', { name: 'Add a contractor' }).click();
  const form = page.getByTestId('contractor-form');
  await form.getByLabel('Name', { exact: true }).fill('Ana Ruiz');
  await form.getByLabel('Company (optional)').fill('Ruiz Plumbing');
  await form.getByLabel('What they do').fill('plumbing, water heaters');
  await form.getByLabel('Phone').fill('(215) 555-0101');
  await form.getByLabel('Email').fill('ana@ruiz.example');
  await form.getByLabel('Licence number').fill('PL-4411');
  await form.getByLabel('Licence ends').fill(isoIn(20));
  await form.getByLabel('Your rating').selectOption('5');
  await form.getByLabel('Would you hire them again?').selectOption('yes');
  await form.getByLabel('Private notes').fill('Fair prices, call before 8');
  await form.getByRole('button', { name: 'Save' }).click();
  await expect(page.getByRole('heading', { name: 'Ana Ruiz', level: 1 })).toBeVisible();
  await expect(page.getByTestId('licence')).toContainText('PL-4411');
  await expect(page.getByTestId('licence')).toContainText('Ends soon');
  await expect(page.getByTestId('private')).toContainText('5 of 5');
  await expect(page.getByTestId('private')).toContainText('Fair prices, call before 8');
  await expect(page.getByTestId('call')).toHaveText('Call (215) 555-0101');
  await expect(page.locator('.toast')).toHaveCount(0, { timeout: 15_000 });
  await page.screenshot({ path: '../docs/phase-9-evidence/contractor-page.png', fullPage: true });

  // -- a job: Done first, then "who did it" and the cost -----------------
  await addThing(page, 'water heater', /^Tank water heater/, true);
  const thingUrl = page.url();
  await page.getByTestId('next-task').getByRole('button', { name: 'Done' }).click();
  await page.getByRole('status').getByRole('button', { name: 'Add details' }).click();
  const details = page.getByTestId('completion-details');
  await details.getByLabel('Who did it').selectOption({ label: 'Ana Ruiz, Ruiz Plumbing' });
  await details.getByLabel('Contractor', { exact: true }).fill('180');
  await details.getByLabel('Parts', { exact: true }).fill('20.50');
  await details.getByRole('button', { name: 'Save' }).click();
  await expect(page.getByRole('link', { name: 'Ana Ruiz, Ruiz Plumbing' })).toBeVisible();
  await page.getByRole('link', { name: 'Ana Ruiz, Ruiz Plumbing' }).click();
  await expect(page.getByTestId('contractor-total')).toContainText('1 job');
  await expect(page.getByTestId('contractor-total')).toContainText('$200.50');
  await expect(page.getByTestId('job')).toContainText('Tank water heater');

  // -- warranties: ending soon, covered, ended ----------------------------
  await page.goto(thingUrl);
  await page.getByRole('button', { name: 'Edit details' }).click();
  await page.getByLabel('Make', { exact: true }).fill('Dreo');
  await page.getByLabel('Model', { exact: true }).fill('DR-HSH015');
  await page.getByLabel('Serial', { exact: true }).fill('SN-HEATER-77');
  await page.getByLabel('Price paid').fill('1,299.99');
  await page.getByLabel('Worth now (your guess)').fill('900');
  await page.getByLabel('Warranty', { exact: true }).selectOption('date');
  await page.getByLabel('Ends on').fill(isoIn(30));
  await page.getByLabel('Registration page (optional)').fill('www.example.com/register');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.getByTestId('thing-warranty')).toContainText('Ends in 30 days');
  await expect(page.getByTestId('warranty-action')).toContainText('book a check-up');

  await addThing(page, 'dishwasher', /^Dishwasher/);
  await page.getByRole('button', { name: 'Edit details' }).click();
  await page.getByLabel('Bought', { exact: true }).fill(isoIn(-400));
  await page.getByLabel('Warranty', { exact: true }).selectOption('length');
  await page.getByLabel('How long').fill('5');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.getByTestId('thing-warranty')).toContainText('Until');

  await addThing(page, 'dryer', /^Clothes dryer/);
  await page.getByRole('button', { name: 'Edit details' }).click();
  await page.getByLabel('Warranty', { exact: true }).selectOption('date');
  await page.getByLabel('Ends on').fill(isoIn(-10));
  // A private thing, for the inventory.
  await page.getByTestId('thing-private').check();
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.getByTestId('thing-warranty')).toContainText('Ended');

  await goHash(page, '#/warranties');
  await expect(page.getByTestId('ending-soon').getByTestId('warranty-row')).toHaveCount(1);
  await expect(page.getByTestId('ending-soon')).toContainText('Tank water heater');
  await expect(page.getByTestId('covered')).toContainText('Dishwasher');
  await expect(page.getByTestId('ended')).toContainText('Clothes dryer');
  await expect(page.locator('.toast')).toHaveCount(0, { timeout: 15_000 });
  await page.screenshot({ path: '../docs/phase-9-evidence/warranties.png', fullPage: true });

  // -- recalls: off until switched on; then a match with its evidence -----
  await goHash(page, '#/recalls');
  await expect(page.getByText('nothing about your home is sent')).toBeVisible();
  expect(asked).toHaveLength(0);
  await page.getByTestId('recalls-on').click();
  await expect(page.getByTestId('recall-match')).toHaveCount(1, { timeout: 120_000 });
  const m = page.getByTestId('recall-match');
  await expect(m).toContainText('Your model is named in this recall');
  await expect(m).toContainText('DR-HSH015');
  await expect(m.getByRole('button', { name: 'Read the official notice' })).toBeVisible();
  // Only dates were asked for: no make, model, serial or anything else.
  expect(asked.length).toBeGreaterThanOrEqual(10);
  for (const u of asked) {
    const q = new URL(u);
    expect([...q.searchParams.keys()]).toEqual(['format', 'RecallDateStart', 'RecallDateEnd']);
    expect(u).not.toMatch(/DR-HSH015|Dreo|SN-HEATER/i);
  }
  await expect(page.locator('.toast')).toHaveCount(0, { timeout: 15_000 });
  await page.screenshot({ path: '../docs/phase-9-evidence/recall-match.png', fullPage: true });
  await page.getByRole('link', { name: 'Today' }).click();
  await expect(page.getByTestId('recall-card')).toBeVisible();
  await page.getByTestId('recall-card').click();
  await page.getByRole('button', { name: 'Not mine' }).click();
  await expect(page.getByTestId('no-recalls')).toBeVisible();
  await page.getByRole('link', { name: 'Today' }).click();
  await expect(page.getByTestId('recall-card')).toHaveCount(0);

  // -- the insurance inventory: the private thing left out by default -----
  await goHash(page, '#/insurance');
  await expect(page.getByTestId('inventory-summary')).toContainText('Estimated value: $900.00');
  const pdf = await fileFrom(page, 'insurance-pdf');
  expect(pdf.subarray(0, 4).toString()).toBe('%PDF');
  writeFileSync('../docs/phase-9-evidence/insurance-inventory.pdf', pdf);
  const csv = (await fileFrom(page, 'insurance-csv')).toString('utf8');
  expect(csv).toContain('Tank water heater');
  expect(csv).toContain('1299.99,900.00,USD');
  expect(csv).not.toContain('Clothes dryer');
  expect(csv).not.toContain('Ana Ruiz');
  expect(csv).not.toContain('Fair prices');
  const text = pdfText(pdf);
  if (text !== null) {
    const t = text.replace(/\s+/g, ' ');
    expect(t).toContain('Home inventory');
    expect(t).toContain('Tank water heater');
    expect(t).not.toContain('Clothes dryer');
    expect(t).not.toContain('Fair prices');
  }
  // Asked for: the private thing is in.
  await page.getByTestId('include-sensitive').check();
  const all = (await fileFrom(page, 'insurance-csv')).toString('utf8');
  expect(all).toContain('Clothes dryer');
  await expect(page.locator('.toast')).toHaveCount(0, { timeout: 15_000 });
  await page.screenshot({ path: '../docs/phase-9-evidence/insurance.png', fullPage: true });

  expect(errors).toEqual([]);
});

test('phone-sized contractor list and page', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await firstRun(page);
  await goHash(page, '#/contractors');
  await page.getByRole('button', { name: 'Add a contractor' }).click();
  await page.getByTestId('contractor-form').getByLabel('Name', { exact: true }).fill('Bright Spark Electric');
  await page.getByTestId('contractor-form').getByLabel('Phone').fill('555 0102');
  await page.getByTestId('contractor-form').getByRole('button', { name: 'Save' }).click();
  await expect(page.getByRole('heading', { name: 'Bright Spark Electric', level: 1 })).toBeVisible();
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth + 1);
  expect(overflow).toBe(false);
  await expect(page.locator('.toast')).toHaveCount(0, { timeout: 15_000 });
  await page.screenshot({ path: '../docs/phase-9-evidence/contractor-phone.png', fullPage: true });
});
