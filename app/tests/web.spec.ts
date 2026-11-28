import { test, expect } from '@playwright/test';

test('first run, home questions, starting plan, Today, suggestion, Done, history, persistence, second tab', async ({
  page,
  context,
}) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto('/');

  // First run: says how long it is before asking anything (P3, anti-pattern).
  await expect(page.getByRole('heading', { name: 'Welcome to Housewright' })).toBeVisible();
  await expect(page.getByText('about 10 questions about the house, about 3 minutes')).toBeVisible();
  await page.getByLabel('Your name').fill('Alex');
  await page.getByRole('button', { name: 'Next: the home questions' }).click();

  // One question per screen, with progress, Back and Not sure.
  await expect(page.getByText('Question 1 of 10')).toBeVisible();
  await expect(page.getByRole('navigation', { name: 'Main' })).toHaveCount(0);
  await page.getByRole('button', { name: 'A house' }).click();
  await expect(page.getByText('Question 2 of 10')).toBeVisible();
  await page.getByRole('group', { name: 'How many smoke alarms?' }).getByRole('button', { name: '2 or 3' }).click();
  await page.getByRole('button', { name: 'Battery' }).click();
  await page.getByRole('group', { name: 'How many carbon monoxide (CO) alarms?' }).getByRole('button', { name: 'One' }).click();
  await page.getByRole('button', { name: 'Next' }).click();
  await expect(page.getByText('Question 3 of 10')).toBeVisible();
  await page.getByRole('button', { name: /Gas furnace/ }).click();
  await page.getByRole('button', { name: 'Next' }).click();
  await page.getByRole('button', { name: 'A gas tank' }).click();
  await expect(page.getByText('Question 5 of 10')).toBeVisible();
  await page.getByLabel('ZIP code').fill('19103');
  await page.getByLabel('ZIP code').blur();
  await expect(page.getByTestId('climate')).toContainText('Average first frost around Nov');
  await page.getByRole('button', { name: 'Next' }).click();
  await page.getByRole('button', { name: 'Not sure' }).click(); // water and sewer
  await expect(page.getByText('Question 7 of 10')).toBeVisible();
  await page.getByRole('button', { name: 'Basement' }).click();
  await page.getByRole('button', { name: 'Sump pump' }).click();
  await page.getByRole('button', { name: 'Washing machine' }).click();
  await page.getByRole('button', { name: 'Next' }).click();
  await page.getByRole('button', { name: 'Not sure' }).click(); // cooling
  await page.getByRole('button', { name: 'Not sure' }).click(); // built
  await expect(page.getByText('Question 10 of 10')).toBeVisible();
  await page.getByRole('button', { name: 'More adults' }).click();
  await page.getByRole('button', { name: 'Show my plan' }).click();

  // The starting plan: a few tasks with why and source, honest progress.
  await expect(page.getByRole('heading', { name: 'Your starting plan' })).toBeVisible();
  const planned = page.getByTestId('plan-task');
  await expect(planned).toHaveCount(4);
  await expect(planned.first()).toContainText('Test the CO alarm');
  await expect(planned.first()).toContainText('Source:');
  await expect(page.getByTestId('recorded')).toContainText(/Your home, so far: \d+ things recorded/);
  await expect(page.getByText('Seasonal jobs follow 19103')).toBeVisible();
  // The full list is one tap away, but we never land there.
  await expect(page.getByRole('link', { name: 'See everything we suggested' })).toBeVisible();
  await page.getByRole('link', { name: 'Go to Today' }).click();

  // Today: only the switched-on tasks, and one suggestion card.
  await expect(page.getByRole('heading', { name: 'Today' })).toBeVisible();
  const rows = page.getByTestId('task-row');
  await expect(rows.first()).toBeVisible();
  expect(await rows.count()).toBeLessThanOrEqual(4);
  await expect(page.getByTestId('suggestion')).toHaveCount(1);
  await page.getByTestId('suggestion').getByRole('button', { name: 'Later' }).click();
  await expect(page.getByRole('status')).toContainText('ask again in a few weeks');
  await expect(page.getByTestId('suggestion')).toHaveCount(0);

  // See everything we suggested: switched off, grouped by thing.
  await page.getByRole('link', { name: 'See everything we suggested' }).click();
  await expect(page.getByRole('heading', { name: 'Everything we suggested' })).toBeVisible();
  await expect(page.getByRole('heading', { name: /Furnace/ })).toBeVisible();
  await page.getByRole('link', { name: 'Today', exact: true }).click();

  // One-tap Done from Today.
  const due = page.getByRole('list', { name: 'Due now and soon' }).getByTestId('task-row');
  await expect(due.first()).toBeVisible();
  const title = (await due.first().locator('.title').textContent())!.trim();
  const before = await due.count();
  await due.first().getByRole('button', { name: /^Done/ }).click();
  await expect(page.getByRole('status')).toContainText(`Done: ${title}`);
  await expect(due).toHaveCount(before - 1);

  // Add a thing from a template still works, with start small.
  await page.getByRole('link', { name: 'Things', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Things', level: 1 })).toBeVisible();
  await page.getByRole('link', { name: 'Add a thing' }).first().click();
  await page.getByLabel('What is it?').fill('fridge');
  await page.getByRole('button', { name: /Refrigerator/i }).click();
  await page.getByLabel('Room').selectOption({ label: 'Basement' });
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await expect(page.getByTestId('added')).toContainText('suggested tasks');
  await page.getByRole('button', { name: /Switch on the top/ }).click();
  await expect(page.getByRole('status')).toContainText(/Switched on \d+ tasks/);

  // Task detail shows why and steps.
  await page.getByRole('link', { name: 'Things', exact: true }).click();
  await page.getByRole('link', { name: /^Smoke alarms/ }).click();
  await expect(page.getByRole('heading', { name: 'Smoke alarms', level: 1 })).toBeVisible();
  await page.getByRole('link', { name: /^Test the smoke alarm/ }).click();
  await expect(page.getByRole('heading', { name: 'Why it matters' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Steps' })).toBeVisible();

  // History records the Done.
  await page.getByRole('link', { name: 'History', exact: true }).click();
  await expect(page.getByTestId('history')).toContainText(`marked '${title}' done`);

  // Redo the home questions only adds.
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await page.getByRole('link', { name: 'Redo the home questions' }).click();
  await expect(page.getByRole('button', { name: 'A house' })).toHaveAttribute('aria-pressed', 'true');
  for (let i = 1; i <= 6; i++) await page.getByRole('button', { name: 'Next' }).click();
  await expect(page.getByText('Question 7 of 10')).toBeVisible();
  await page.getByRole('button', { name: 'Pool' }).click();
  for (let i = 7; i <= 9; i++) await page.getByRole('button', { name: 'Next' }).click();
  await page.getByRole('button', { name: 'Show my plan' }).click();
  await expect(page.getByRole('heading', { name: 'Your answers are saved' })).toBeVisible();
  await expect(page.getByText('1 new thing was added')).toBeVisible();
  await expect(page.getByTestId('plan-task').first()).toBeVisible();

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
