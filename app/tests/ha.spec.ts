import { test, expect, type Page } from '@playwright/test';
import { spawn, type ChildProcess } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Phase 7 end to end: a local sync server and a Home Assistant bridge; a
// household made in the browser pairs the bridge with the code and the
// safety number; the bridge marks a task done for Ana and sends meter
// readings; the browser sees both in History "via Home Assistant", and the
// usage task comes due.

const ROOT = new URL('../../target/debug/', import.meta.url).pathname;
const SERVER = process.env.HW_SERVER_BIN ?? join(ROOT, 'housewright-server');
const BRIDGE = process.env.HW_BRIDGE_BIN ?? join(ROOT, 'housewright-bridge');
const PORT = 19000 + Math.floor(Math.random() * 500);
const BPORT = PORT + 500;
const URL_ = `http://127.0.0.1:${PORT}`;
const BURL = `http://127.0.0.1:${BPORT}`;
const TOKEN = 'E2E-TOKEN-1234';
const BTOKEN = 'e2e-bridge-token-0123456789abcdefghijkl';
let dir = '';
const procs: ChildProcess[] = [];

async function up(url: string) {
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(url)).ok) return;
    } catch {
      /* not up yet */
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error(`${url} did not start`);
}

async function bridge(method: string, path: string, body?: unknown) {
  const r = await fetch(`${BURL}/api/v1/${path}`, {
    method,
    headers: { authorization: `Bearer ${BTOKEN}`, 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const v = await r.json();
  if (!r.ok) throw new Error(`${method} ${path}: ${JSON.stringify(v)}`);
  return v;
}

test.beforeAll(async () => {
  dir = mkdtempSync(join(tmpdir(), 'hw-e2e-ha-'));
  procs.push(
    spawn(SERVER, ['--listen', `127.0.0.1:${PORT}`, '--data-dir', join(dir, 'server'), '--registration-token', TOKEN], {
      stdio: 'ignore',
    }),
  );
  procs.push(
    spawn(BRIDGE, ['--data-dir', join(dir, 'bridge'), 'serve', '--listen', `127.0.0.1:${BPORT}`, '--sync-every', '10'], {
      stdio: 'ignore',
      env: { ...process.env, HW_BRIDGE_TOKEN: BTOKEN },
    }),
  );
  await up(`${URL_}/v1/health`);
  await up(`${BURL}/api/v1/health`);
});

test.afterAll(async () => {
  await Promise.all(
    procs.map(
      (p) =>
        new Promise<void>((done) => {
          if (p.exitCode !== null) return done();
          p.on('exit', () => done());
          p.kill('SIGTERM');
        }),
    ),
  );
  rmSync(dir, { recursive: true, force: true });
});

async function goHash(page: Page, hash: string) {
  await page.evaluate((h) => (location.hash = h), hash);
}

async function syncNow(page: Page) {
  await goHash(page, '#/sync');
  await page.getByRole('button', { name: 'Sync now' }).click();
  await expect(page.getByRole('button', { name: 'Sync now' })).toBeEnabled();
}

test('pair the Home Assistant bridge, complete and meter through it, see it in History', async ({ browser }) => {
  test.setTimeout(180_000);
  const errors: string[] = [];
  const ctx = await browser.newContext();
  const a = await ctx.newPage();
  a.on('dialog', (d) => d.accept());
  a.on('pageerror', (e) => errors.push(e.message));

  // A household with sync on.
  await a.goto('/');
  await a.getByLabel('Your name').fill('Ana');
  await a.getByRole('button', { name: 'Next: the home questions', exact: true }).click();
  await expect(a.getByText('Question 1 of 10')).toBeVisible();
  await goHash(a, '#/sync');
  await a.getByLabel('Server address').fill(URL_);
  await a.getByLabel('Name for this device').fill("Ana's browser");
  await a.getByLabel(/Registration token/).fill(TOKEN);
  await a.getByRole('button', { name: 'Turn on sync' }).click();
  await a.getByLabel("I've written it down or printed it").check();
  await a.getByRole('button', { name: 'Done' }).click();
  await expect(a.getByText('Sync is on')).toBeVisible();

  // A washing machine, with its cleaning cycle (every 40 loads) switched on.
  await goHash(a, '#/add');
  await a.getByLabel('What is it?').fill('washing machine');
  await a.getByRole('button', { name: /^Washing machine/ }).click();
  await a.getByRole('button', { name: 'Add', exact: true }).click();
  await a.getByRole('button', { name: /Switch on the top/ }).click();
  await expect(a.getByRole('heading', { level: 1 })).toHaveText(/Washing machine/);
  await goHash(a, '#/suggested');
  // "Switch on the top 3" leaves the cleaning cycle as a suggestion.
  await a.getByRole('button', { name: 'Switch on: Run a washer cleaning cycle' }).click();
  await expect(a.getByRole('button', { name: 'Switch on: Run a washer cleaning cycle' })).toHaveCount(0);

  // The help page is reachable from Sync and devices.
  await goHash(a, '#/sync');
  await a.getByRole('button', { name: 'Add a device or person' }).click();
  await expect(a.getByRole('link', { name: 'How connecting Home Assistant works' })).toBeVisible();
  await a.getByRole('button', { name: 'Home Assistant' }).click();
  const code = (await a.getByTestId('pair-code').textContent())!.trim();
  await expect(a.getByText(/add the Housewright integration/)).toBeVisible();

  // The bridge joins with the code, as the integration's setup does.
  const st0 = await bridge('GET', 'status');
  expect(st0.paired).toBe(false);
  await bridge('POST', 'pairing', { server: URL_, code });
  let p = await bridge('GET', 'pairing');
  for (let i = 0; i < 40 && p.state !== 'compare'; i++) {
    await new Promise((r) => setTimeout(r, 250));
    p = await bridge('GET', 'pairing');
  }
  expect(p.state).toBe('compare');
  const onA = (await a.getByTestId('safety-number').textContent({ timeout: 20_000 }))!.trim();
  expect(p.safety_number).toBe(onA);
  await bridge('POST', 'pairing/confirm', { matches: true });
  await a.getByRole('button', { name: 'They match' }).click();
  await expect(a.getByText(/^Added\. It may take/)).toBeVisible();
  for (let i = 0; i < 40 && p.state !== 'done'; i++) {
    await new Promise((r) => setTimeout(r, 250));
    p = await bridge('GET', 'pairing');
  }
  expect(p.state).toBe('done');
  const st = await bridge('GET', 'status');
  expect(st.paired).toBe(true);
  expect(st.member.home_assistant).toBe(true);
  await a.getByRole('button', { name: 'Close' }).click();
  await goHash(a, '#/settings');
  await goHash(a, '#/sync');
  await expect(a.locator('.devices li', { hasText: 'Home Assistant bridge' })).toHaveCount(1);
  await a.screenshot({ path: 'test-results/ha-devices.png', fullPage: true });

  // Through the bridge: a task done for Ana, and two meter readings.
  const tasks = await bridge('GET', 'tasks');
  const ready = tasks.find((t: { state: string; meter: string | null }) => t.state === 'due' && !t.meter);
  expect(ready).toBeTruthy();
  const done = await bridge('POST', 'complete', { task: ready.id, member: 'Ana' });
  expect(done.state).toBe('ok');
  await bridge('POST', 'readings', { item: 'Washing machine', meter: 'loads', value: 10 });
  const r = await bridge('POST', 'readings', { item: 'Washing machine', meter: 'loads', value: 55 });
  expect(r.tasks.length).toBe(1);
  expect(r.tasks[0].state).toBe('due');
  const synced = await bridge('POST', 'sync');
  expect(synced.sync.pending_changes).toBe(0);

  // The browser sees it all, labelled.
  await syncNow(a);
  await goHash(a, '#/history');
  const history = a.getByTestId('history');
  await expect(history).toContainText(`Ana marked '${ready.title}' done on`);
  await expect(history).toContainText('via Home Assistant');
  await expect(history).toContainText("Recorded a reading of 55 on meter 'Loads washed', via Home Assistant");
  await a.screenshot({ path: 'test-results/ha-history.png', fullPage: true });
  await goHash(a, '#/today');
  await expect(a.getByText('Run a washer cleaning cycle')).toBeVisible();
  await a.screenshot({ path: 'test-results/ha-today.png', fullPage: true });

  // The help page.
  await goHash(a, '#/home-assistant');
  await expect(a.getByRole('heading', { name: 'Connect Home Assistant' })).toBeVisible();
  await expect(a.getByText(/never passes on anything marked sensitive/)).toBeVisible();
  await a.screenshot({ path: 'test-results/ha-help.png', fullPage: true });

  expect(errors).toEqual([]);
  await ctx.close();
});
