// Drives a browser profile for the Phase 6 Android check (not part of the
// Playwright suite): joins the household the emulator set up, then follows
// steps written to files in DIR. See docs/phase-6-evidence/README.md.
import { chromium } from '@playwright/test';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';

const DIR = process.argv[2];
const SERVER = process.argv[3] ?? 'http://127.0.0.1:8787';
const wait = async (name) => {
  while (!existsSync(join(DIR, name))) await new Promise((r) => setTimeout(r, 500));
  return readFileSync(join(DIR, name), 'utf8').trim();
};
const say = (name, text) => writeFileSync(join(DIR, name), text);
const shot = (page, name) => page.screenshot({ path: join(DIR, `${name}.png`), fullPage: true });

const browser = await chromium.launch({ channel: 'chrome', headless: true });
const ctx = await browser.newContext();
const page = await ctx.newPage();
page.on('dialog', (d) => d.accept());
const go = (h) => page.evaluate((x) => (location.hash = x), h);
const syncNow = async () => {
  await go('#/sync');
  try {
    await page.getByRole('button', { name: 'Sync now' }).click({ timeout: 120_000 });
  } catch (e) {
    await shot(page, 'error-sync-screen');
    say('error.txt', await page.locator('main').innerText());
    throw e;
  }
  await page.waitForTimeout(3000);
};

await page.goto('http://127.0.0.1:4173/');
const code = await wait('code.txt');
await page.getByRole('button', { name: 'Join a household on another device' }).click();
await page.getByLabel('Server address').fill(SERVER);
await page.getByLabel('Code').fill(code);
await page.getByLabel('Name for this device').fill('Browser on the laptop');
await page.getByRole('button', { name: 'Join', exact: true }).click();
const sas = (await page.getByTestId('safety-number').textContent({ timeout: 60_000 })).trim();
say('browser-sas.txt', sas);
await shot(page, 'b1-compare');
await wait('confirm.txt');
await page.getByRole('button', { name: 'They match' }).click();
await page.getByRole('heading', { name: 'Today' }).waitFor({ timeout: 600_000 });
await go('#/things');
await page.waitForTimeout(1000);
await shot(page, 'b2-things-after-join');
say('b2.txt', await page.locator('main').innerText());

await wait('step3.txt');
await go('#/add');
await page.getByLabel('What is it?').fill('sump pump');
await page.getByRole('button', { name: /^Sump pump/ }).click();
await page.getByRole('button', { name: 'Add', exact: true }).click();
await page.getByRole('button', { name: /Switch on the top/ }).click();
await page.getByRole('heading', { level: 1, name: /Sump pump/ }).waitFor();
await syncNow();
await shot(page, 'b3-after-adding');
say('b3.txt', 'added');

await wait('step4.txt');
await syncNow();
await go('#/things');
await page.waitForTimeout(1000);
await shot(page, 'b4-things-final');
say('b4.txt', await page.locator('main').innerText());

await wait('step5.txt');
await syncNow();
await go('#/sync');
await page.waitForTimeout(1000);
await shot(page, 'b5-sync-screen');
say('b5.txt', await page.locator('main').innerText());
await browser.close();
