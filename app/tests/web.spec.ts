import { test, expect } from '@playwright/test';

test('first run, add a thing, start small, Done, history, persistence, second tab', async ({
  page,
  context,
}) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto('/');

  // First run: says how long it is, asks three things.
  await expect(page.getByRole('heading', { name: 'Welcome to Housewright' })).toBeVisible();
  await expect(page.getByText('Three quick questions')).toBeVisible();
  await page.getByLabel('1. Your name').fill('Alex');
  await page.getByRole('button', { name: 'Start' }).click();
  await expect(page.getByRole('heading', { name: 'Today' })).toBeVisible();
  await expect(page.getByText('No tasks are switched on yet.')).toBeVisible();

  // Add a smoke alarm from a template, in a new room.
  await page.getByRole('link', { name: 'Add a thing' }).first().click();
  await page.getByLabel('What is it?').fill('smoke');
  await page.getByRole('button', { name: /Smoke alarm/i }).click();
  await page.getByLabel('Room').selectOption('__new');
  await page.getByLabel('New room name').fill('Hallway');
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await expect(page.getByTestId('added')).toContainText('suggested tasks');
  await page.getByRole('button', { name: /Switch on the top/ }).click();
  await expect(page.getByRole('status')).toContainText('Switched on 3 tasks');

  // Today shows the switched-on tasks with a one-tap Done.
  await page.getByRole('link', { name: 'Today', exact: true }).click();
  const rows = page.getByRole('list', { name: 'Due now and soon' }).getByTestId('task-row');
  await expect(rows.first()).toBeVisible();
  const title = (await rows.first().locator('.title').textContent())!.trim();
  const before = await rows.count();
  await rows.first().getByRole('button', { name: /^Done/ }).click();
  await expect(page.getByRole('status')).toContainText(`Done: ${title}`);
  await expect(rows).toHaveCount(before - 1);

  // Task detail shows why and steps.
  await page.getByRole('link', { name: 'Things', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Things', level: 1 })).toBeVisible();
  await page.getByRole('link', { name: /^Smoke alarm/ }).click();
  await expect(page.getByRole('heading', { name: 'Smoke alarm', level: 1 })).toBeVisible();
  await page.getByRole('link', { name: new RegExp('^' + title.replace(/[()]/g, '\\$&')) }).click();
  await expect(page.getByRole('heading', { name: 'Why it matters' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Steps' })).toBeVisible();

  // History records it.
  await page.getByRole('link', { name: 'History', exact: true }).click();
  await expect(page.getByTestId('history')).toContainText(`marked '${title}' done`);

  // A second tab is told plainly that the database is in use.
  const second = await context.newPage();
  await second.goto('/');
  await expect(second.getByRole('heading', { name: 'Already open elsewhere' })).toBeVisible();
  await second.close();

  // Data survives a reload (OPFS).
  await page.reload();
  await page.getByRole('link', { name: 'History', exact: true }).click();
  await expect(page.getByTestId('history')).toContainText(`marked '${title}' done`);
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await expect(page.getByText('private browser storage')).toBeVisible();

  expect(errors).toEqual([]);
});
