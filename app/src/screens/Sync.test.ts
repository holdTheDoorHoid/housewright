import { describe, it, expect, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/svelte';
import Sync from './Sync.svelte';
import { useBackend, type SyncStatus } from '../lib/api';
import { FakeBackend } from '../lib/api/fake';
import { app } from '../lib/state.svelte';

function status(over: Partial<SyncStatus> = {}): SyncStatus {
  return {
    available: true,
    enabled: false,
    server: null,
    role: null,
    last_sync: null,
    last_error: null,
    last_error_code: null,
    clock_warning: null,
    clock_ahead: false,
    removed: false,
    pending_changes: 0,
    recovery_confirmed: false,
    refused: 0,
    notices: [],
    warnings: [],
    ...over,
  };
}

describe('Sync screen', () => {
  afterEach(() => cleanup());
  app.status = {
    initialized: true,
    household_name: 'Home',
    units: 'imperial',
    me: { id: 'm1', name: 'Ana', role: 'owner', reminders: {} },
    property: null,
    currency: 'USD',
  };

  it('is off by default, offers your own server, and hosting only as coming later', async () => {
    useBackend(new FakeBackend({ sync_status: () => status() }));
    render(Sync);
    expect(await screen.findByText(/Sync is off/)).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Turn on sync' })).toBeTruthy();
    expect(screen.getByText(/Coming later/)).toBeTruthy();
    expect(screen.queryByRole('button', { name: /sign up/i })).toBeNull();
  });

  it('shows the recovery key once and needs a confirmation', async () => {
    const fake = new FakeBackend({
      sync_status: () => status(),
      sync_setup: () => ({ recovery_key: 'ABCD-EFGH-IJKL-MNOP-QRST-UVWX-YZ23-4567-ABCD', status: status({ enabled: true }) }),
      sync_confirm_recovery: () => status({ enabled: true, recovery_confirmed: true }),
      sync_devices: () => [],
      sync_now: () => ({ report: {}, status: status({ enabled: true, recovery_confirmed: true }) }),
    });
    useBackend(fake);
    render(Sync);
    await fireEvent.input(await screen.findByLabelText('Server address'), { target: { value: 'https://hw.example.org' } });
    await fireEvent.submit(screen.getByRole('button', { name: 'Turn on sync' }).closest('form')!);
    expect(await screen.findByTestId('recovery-key')).toBeTruthy();
    expect(screen.getByText(/nobody can recover the data/)).toBeTruthy();
    const done = screen.getByRole('button', { name: 'Done' }) as HTMLButtonElement;
    expect(done.disabled).toBe(true);
    await fireEvent.click(screen.getByLabelText("I've written it down or printed it"));
    expect(done.disabled).toBe(false);
    await fireEvent.click(done);
    expect(await screen.findByText('Sync is on')).toBeTruthy();
    expect(screen.queryByTestId('recovery-key')).toBeNull();
  });

  it('says plainly when the clock is wrong or the device was removed', async () => {
    useBackend(
      new FakeBackend({
        sync_status: () =>
          status({ enabled: true, recovery_confirmed: true, clock_warning: "This device's clock looks wrong" }),
        sync_devices: () => [],
      }),
    );
    render(Sync);
    expect(await screen.findByText(/clock looks wrong/)).toBeTruthy();
    cleanup();
    useBackend(new FakeBackend({ sync_status: () => status({ enabled: true, removed: true }) }));
    render(Sync);
    expect(await screen.findByText(/was removed from the household/)).toBeTruthy();
  });

  it('shows device changes made elsewhere until dismissed', async () => {
    useBackend(
      new FakeBackend({
        sync_status: () =>
          status({
            enabled: true,
            recovery_confirmed: true,
            notices: [{ at: null, text: '“Tablet” was added to the household by the recovery key.' }],
          }),
        sync_devices: () => [],
        sync_dismiss_notices: () => status({ enabled: true, recovery_confirmed: true }),
      }),
    );
    render(Sync);
    expect(await screen.findByText(/was added to the household by the recovery key/)).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'OK' }));
    expect(screen.queryByTestId('notices')).toBeNull();
  });
});
