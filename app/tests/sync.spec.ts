import { test, expect, type Page, type BrowserContext } from '@playwright/test';
import { spawn, type ChildProcess } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, readdirSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Phase 6 on the real web build: a local sync server, one browser profile
// sets up sync, a second profile pairs with the code, both edit (also
// while the server is unreachable), they converge, a photo crosses over,
// and the second device is removed and can no longer pull.

const PHOTO = readFileSync(new URL('./fixtures/rating-plate-synthetic.jpg', import.meta.url));
const BIN = process.env.HW_SERVER_BIN ?? new URL('../../target/debug/housewright-server', import.meta.url).pathname;
const PORT = 18000 + Math.floor(Math.random() * 1000);
const URL_ = `http://127.0.0.1:${PORT}`;
const TOKEN = 'E2E-TOKEN-1234';
let dataDir = '';
let server: ChildProcess | null = null;

async function startServer() {
  server = spawn(BIN, ['--listen', `127.0.0.1:${PORT}`, '--data-dir', dataDir, '--registration-token', TOKEN], {
    stdio: ['ignore', 'ignore', 'pipe'],
  });
  for (let i = 0; i < 100; i++) {
    try {
      const r = await fetch(`${URL_}/v1/health`);
      if (r.ok) return;
    } catch {
      /* not up yet */
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error('server did not start');
}

async function stopServer() {
  if (!server) return;
  const s = server;
  server = null;
  await new Promise<void>((done) => {
    s.on('exit', () => done());
    s.kill('SIGTERM');
  });
}

test.beforeAll(async () => {
  dataDir = mkdtempSync(join(tmpdir(), 'hw-e2e-sync-'));
  await startServer();
});

test.afterAll(async () => {
  await stopServer();
  rmSync(dataDir, { recursive: true, force: true });
});

async function goHash(page: Page, hash: string) {
  await page.evaluate((h) => (location.hash = h), hash);
}

async function syncNow(page: Page) {
  await goHash(page, '#/sync');
  await page.getByRole('button', { name: 'Sync now' }).click();
  await expect(page.getByRole('button', { name: 'Sync now' })).toBeEnabled();
}

async function newProfile(ctx: BrowserContext): Promise<Page> {
  const page = await ctx.newPage();
  page.on('dialog', (d) => d.accept());
  return page;
}

test('set up sync, pair a second profile, edit on both, converge, photo, remove', async ({ browser }) => {
  test.setTimeout(240_000);
  const errors: string[] = [];
  const ctxA = await browser.newContext();
  const ctxB = await browser.newContext();
  const a = await newProfile(ctxA);
  const b = await newProfile(ctxB);
  a.on('pageerror', (e) => errors.push(`A: ${e.message}`));
  b.on('pageerror', (e) => errors.push(`B: ${e.message}`));

  // A: a household, then Settings shows sync off by default (D-11).
  await a.goto('/');
  await a.getByLabel('Your name').fill('Ana');
  await a.getByRole('button', { name: 'Next: the home questions', exact: true }).click();
  await expect(a.getByText('Question 1 of 10')).toBeVisible();
  await goHash(a, '#/settings');
  await expect(a.getByText(/Off\. Everything stays on this device/)).toBeVisible();
  await a.getByRole('link', { name: 'Sync and devices' }).click();
  await expect(a.getByText('Coming later')).toBeVisible();
  // A wrong address is explained plainly.
  await a.getByLabel('Server address').fill('http://example.org');
  await a.getByRole('button', { name: 'Turn on sync' }).click();
  await expect(a.getByRole('alert')).toContainText('https://');
  await a.getByLabel('Server address').fill(URL_);
  await a.getByLabel('Name for this device').fill("Ana's browser");
  // Without the server's registration token, no household (D-105).
  await a.getByRole('button', { name: 'Turn on sync' }).click();
  await expect(a.getByRole('alert')).toContainText('registration token');
  await a.getByLabel(/Registration token/).fill(TOKEN);
  await a.getByRole('button', { name: 'Turn on sync' }).click();

  // The recovery key, shown once, with the warning (D-15).
  const key = (await a.getByTestId('recovery-key').textContent())!.trim();
  expect(key).toMatch(/^([A-Z2-7]{4}-){8}[A-Z2-7]{4}$/);
  await expect(a.getByText(/nobody can recover the data/)).toBeVisible();
  await expect(a.getByRole('button', { name: 'Done' })).toBeDisabled();
  await a.screenshot({ path: 'test-results/sync-recovery-key.png', fullPage: true });
  await a.getByLabel("I've written it down or printed it").check();
  await a.getByRole('button', { name: 'Done' }).click();
  await expect(a.getByText('Sync is on')).toBeVisible();
  await expect(a.getByTestId('recovery-key')).toHaveCount(0);

  // A adds a thing.
  await goHash(a, '#/add');
  await a.getByLabel('What is it?').fill('water heater');
  await a.getByRole('button', { name: /^Tank water heater/ }).click();
  await a.getByRole('button', { name: 'Add', exact: true }).click();
  await a.getByRole('button', { name: /Switch on the top/ }).click();
  await expect(a.getByRole('heading', { level: 1 })).toHaveText(/Tank water heater/);
  const thingHash = new URL(a.url()).hash;

  // A: add a person; the code and QR appear.
  await goHash(a, '#/sync');
  await a.getByRole('button', { name: 'Add a device or person' }).click();
  await a.getByRole('button', { name: 'Someone else in my home' }).click();
  await a.getByLabel('Their name').fill('Ben');
  await a.getByRole('button', { name: 'Next', exact: true }).click();
  const code = (await a.getByTestId('pair-code').textContent())!.trim();
  expect(code).toMatch(/^([A-Z2-7]{4}-){5}[A-Z2-7]{4}$/);
  await expect(a.locator('.qr svg')).toHaveCount(1);
  await a.screenshot({ path: 'test-results/sync-invite.png', fullPage: true });

  // B: a fresh profile joins with the code.
  await b.goto('/');
  await b.getByRole('button', { name: 'Join a household on another device' }).click();
  await b.getByLabel('Server address').fill(URL_);
  await b.getByLabel('Code').fill(code.toLowerCase());
  await b.getByLabel('Name for this device').fill("Ben's browser");
  await b.getByRole('button', { name: 'Join', exact: true }).click();
  const onB = (await b.getByTestId('safety-number').textContent({ timeout: 20_000 }))!.trim();
  const onA = (await a.getByTestId('safety-number').textContent({ timeout: 20_000 }))!.trim();
  expect(onB).toMatch(/^\d{3} \d{3}$/);
  expect(onA).toBe(onB);
  await a.screenshot({ path: 'test-results/sync-pair-owner.png', fullPage: true });
  await b.screenshot({ path: 'test-results/sync-pair-new.png', fullPage: true });
  // The owner cannot finish until the new device has said yes (D-102).
  await expect(a.getByRole('button', { name: 'They match' })).toBeDisabled();
  await b.getByRole('button', { name: 'They match' }).click();
  await a.getByRole('button', { name: 'They match' }).click();
  await expect(a.getByText(/^Added\. It may take/)).toBeVisible();
  await expect(b.getByRole('heading', { name: 'Today' })).toBeVisible({ timeout: 30_000 });
  await goHash(b, '#/things');
  await expect(b.getByText('Tank water heater')).toBeVisible();

  // Both devices listed on A, with names and last seen.
  await goHash(a, '#/sync');
  await expect(a.getByText("Ben's browser")).toBeVisible();
  await expect(a.locator('.devices li')).toHaveCount(2);

  // The server goes away; both edit; it comes back; they converge.
  await stopServer();
  await goHash(a, thingHash);
  await a.getByRole('button', { name: 'Edit details' }).click();
  await a.getByLabel('Model').fill('MODEL-FROM-ANA');
  await a.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(a.getByText('MODEL-FROM-ANA')).toBeVisible();
  await goHash(b, '#/add');
  await b.getByLabel('What is it?').fill('sump pump');
  await b.getByRole('button', { name: /^Sump pump/ }).click();
  await b.getByRole('button', { name: 'Add', exact: true }).click();
  await b.getByRole('button', { name: /Switch on the top/ }).click();
  await expect(b.getByRole('heading', { level: 1 })).toHaveText(/Sump pump/);
  await syncNow(b);
  await expect(b.getByText(/Can't reach the sync server/)).toBeVisible();
  await expect(b.getByText(/changes waiting to send/)).toBeVisible();
  await startServer();
  await syncNow(a);
  await syncNow(b);
  await syncNow(a);
  await expect(b.getByText(/Last synced/)).toBeVisible();
  await goHash(a, '#/things');
  await expect(a.getByText('Sump pump')).toBeVisible();
  await goHash(b, thingHash);
  await expect(b.getByText('MODEL-FROM-ANA')).toBeVisible();

  // A photo attached on A shows up on B.
  await goHash(a, thingHash);
  await a.getByLabel('Add a photo or PDF').setInputFiles({ name: 'label.jpg', mimeType: 'image/jpeg', buffer: PHOTO });
  await expect(a.getByTestId('thumb')).toHaveCount(1);
  await syncNow(a);
  await syncNow(b);
  await goHash(b, thingHash);
  await expect(b.getByTestId('thumb')).toHaveCount(1);
  await expect(b.getByTestId('thumb')).toHaveJSProperty('complete', true);
  const w = await b.getByTestId('thumb').evaluate((img: HTMLImageElement) => img.naturalWidth);
  expect(w).toBeGreaterThan(0);

  // The server's files hold none of it in readable form.
  const bytes = Buffer.concat(
    walk(dataDir).map((f) => readFileSync(f)),
  );
  for (const s of ['Tank water heater', 'MODEL-FROM-ANA', 'Sump pump', "Ben's browser", "Ana's browser"]) {
    expect(bytes.includes(Buffer.from(s)), `server files contain ${s}`).toBe(false);
  }

  // A removes B; B can no longer pull.
  await goHash(a, '#/sync');
  const benRow = a.locator('.devices li', { hasText: "Ben's browser" });
  await benRow.getByRole('button', { name: 'Remove' }).click();
  await expect(a.locator('.devices li')).toHaveCount(1);
  await goHash(a, thingHash);
  await a.getByRole('button', { name: 'Edit details' }).click();
  await a.getByLabel('Model').fill('AFTER-REMOVAL');
  await a.getByRole('button', { name: 'Save', exact: true }).click();
  await syncNow(a);
  await goHash(b, '#/sync');
  // B may already have found out by itself (it syncs in the background).
  const now = b.getByRole('button', { name: 'Sync now' });
  if (await now.isVisible()) await now.click();
  await expect(b.getByText(/removed from the household/).first()).toBeVisible();
  await b.screenshot({ path: 'test-results/sync-b-removed.png', fullPage: true });
  await goHash(b, thingHash);
  await expect(b.getByText('AFTER-REMOVAL')).toHaveCount(0);
  await expect(b.getByText('MODEL-FROM-ANA')).toBeVisible();

  await a.screenshot({ path: 'test-results/sync-a.png', fullPage: true });
  expect(errors).toEqual([]);
  await ctxA.close();
  await ctxB.close();
});

function walk(dir: string): string[] {
  const out: string[] = [];
  for (const n of readdirSync(dir)) {
    const p = join(dir, n);
    if (statSync(p).isDirectory()) out.push(...walk(p));
    else out.push(p);
  }
  return out;
}
