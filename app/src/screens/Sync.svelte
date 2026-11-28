<script lang="ts">
  // Sync and devices (Phase 6). Off by default (D-11); self-hosted servers
  // only for now (the hosted service is Phase 11). Merging is automatic:
  // nothing here ever asks anyone to choose between two versions (D-12).
  import { onDestroy } from 'svelte';
  import {
    call,
    currentBackend,
    type Invitation,
    type PairState,
    type Role,
    type SyncDevice,
    type SyncStatus,
  } from '../lib/api';
  import { app, toast } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import { openFile } from '../lib/files';
  import { when } from '../lib/format';
  import { onSyncStatus, syncNow, syncSettingsChanged } from '../lib/sync';

  const me = app.status?.me;
  const isOwner = me?.role === 'owner';
  const kind = currentBackend()?.kind === 'tauri' ? 'phone' : 'browser';

  let status = $state<SyncStatus | null>(null);
  let devices = $state<SyncDevice[] | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);

  // Setting up.
  let server = $state('');
  let token = $state('');
  let deviceName = $state(me ? `${me.name}'s ${kind}` : `This ${kind}`);
  // Shown once, then forgotten.
  let recoveryKey = $state<string | null>(null);
  let wroteItDown = $state(false);

  // Adding a device or person.
  let adding = $state<'choose' | 'device' | 'person' | null>(null);
  let personName = $state('');
  let personRole = $state<Role>('member');
  let invite = $state<Invitation | null>(null);
  let pair = $state<PairState | null>(null);
  let poll: ReturnType<typeof setInterval> | null = null;

  async function load() {
    try {
      status = await call('sync_status');
      if (status.enabled && !status.removed) {
        devices = await call('sync_devices').catch(() => null);
      }
    } catch (e) {
      error = errorText(e);
    }
  }
  load();
  // Background syncs update what is shown here.
  const unlisten = onSyncStatus((s) => {
    if (!recoveryKey) status = s;
  });

  onDestroy(() => {
    unlisten();
    stopPolling();
    if (invite && pair?.state !== 'done') void call('sync_invite_cancel').catch(() => {});
  });

  async function setup(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = null;
    try {
      const r = await call('sync_setup', {
        server,
        device_name: deviceName,
        registration_token: token || undefined,
      });
      recoveryKey = r.recovery_key;
      status = r.status;
      syncSettingsChanged(r.status);
    } catch (err) {
      error = errorText(err);
    } finally {
      busy = false;
    }
  }

  async function printSealed() {
    if (!recoveryKey) return;
    try {
      await openFile(
        'binder_pdf',
        { edition: 'full', volume: 'sealed', recovery_key: recoveryKey },
        'house-binder-sealed-volume.pdf',
        'application/pdf',
      );
    } catch (err) {
      toast({ text: errorText(err) });
    }
  }

  async function keySaved() {
    status = await call('sync_confirm_recovery');
    recoveryKey = null;
    wroteItDown = false;
    await load();
  }

  async function newKey() {
    // The current key confirms it, so a stolen phone cannot lock the
    // owner out (D-101).
    const current = prompt(
      'Making a new recovery key needs the current one (it is in the sealed volume of the house binder). Type it here. The old key stops working straight away.',
    );
    if (!current) return;
    busy = true;
    try {
      const r = await call('sync_new_recovery_key', { recovery_key: current });
      recoveryKey = r.recovery_key;
      status = r.status;
    } catch (err) {
      toast({ text: errorText(err) });
    } finally {
      busy = false;
    }
  }

  async function now() {
    busy = true;
    const s = await syncNow();
    if (s) status = s;
    devices = await call('sync_devices').catch(() => devices);
    busy = false;
  }

  async function startInvite(who: 'device' | 'person') {
    error = null;
    try {
      invite = await call(
        'sync_invite',
        who === 'person' ? { who, name: personName, role: personRole } : { who },
      );
      pair = { state: 'waiting' };
      stopPolling();
      poll = setInterval(checkInvite, 1500);
    } catch (err) {
      error = errorText(err);
    }
  }

  function stopPolling() {
    if (poll) clearInterval(poll);
    poll = null;
  }

  async function checkInvite() {
    try {
      const p = await call('sync_invite_poll');
      if (p.state === 'done' || p.state === 'failed' || (p.state === 'compare' && p.other_confirmed)) stopPolling();
      pair = p;
    } catch (err) {
      // Offline for a moment: keep trying quietly.
      error = errorText(err);
    }
  }

  async function answer(matches: boolean) {
    busy = true;
    try {
      pair = await call('sync_invite_confirm', { matches });
      if (pair.state === 'done') {
        toast({ text: 'Added. The other device is getting everything now.' }, 5000);
        await load();
      }
    } catch (err) {
      error = errorText(err);
    } finally {
      busy = false;
    }
  }

  function closeInvite() {
    stopPolling();
    if (invite && pair?.state !== 'done') void call('sync_invite_cancel').catch(() => {});
    invite = null;
    pair = null;
    adding = null;
    personName = '';
  }

  async function remove(d: SyncDevice) {
    let recovery_key: string | undefined;
    if (d.role === 'owner') {
      // Removing an owner's device needs the recovery key (D-101).
      const k = prompt(
        `Removing “${d.name}”, an owner's device, needs the household's recovery key (it is in the sealed volume of the house binder). Type it here. The device stops syncing straight away and cannot read anything added from now on.`,
      );
      if (!k) return;
      recovery_key = k;
    } else if (
      !confirm(
        `Remove “${d.name}”? It stops syncing straight away and cannot read anything added from now on. What is already on it stays there.`,
      )
    )
      return;
    busy = true;
    try {
      devices = await call('sync_remove_device', { id: d.id, recovery_key });
      toast({ text: `Removed “${d.name}”. The household key was changed.` }, 5000);
    } catch (err) {
      toast({ text: errorText(err) });
    } finally {
      busy = false;
    }
  }

  async function stop() {
    if (!confirm('Stop syncing on this device? Everything stays here, but changes will no longer reach your other devices.'))
      return;
    status = await call('sync_stop');
    devices = null;
    syncSettingsChanged(status);
  }

  const roleText: Record<string, string> = { owner: 'Owner', member: 'Member', limited: 'Limited' };
</script>

<section aria-labelledby="sync-h">
  <p><a href="#/settings">‹ Settings</a></p>
  <h1 id="sync-h">Sync and devices</h1>

  {#if error}<p role="alert">{error}</p>{/if}

  {#if !status}
    <p class="muted">Loading…</p>
  {:else if !status.available}
    <p>This version of the app cannot sync.</p>
  {:else if recoveryKey}
    <!-- Shown once. -->
    <div class="card key-card" aria-labelledby="key-h">
      <h2 id="key-h">Your recovery key</h2>
      <p>
        This key is the only way to get your household's data back if every phone and computer with it
        is lost. It is shown <strong>only now</strong>: Housewright does not keep a copy anywhere.
      </p>
      <p class="key" data-testid="recovery-key">{recoveryKey}</p>
      <p>
        Write it down, or print it on the sealed volume of the house binder, and keep it somewhere safe
        and private. Anyone with this key and your server address can read everything.
      </p>
      <p class="pill attention">
        If every device is lost <strong>and</strong> this key is lost, nobody can recover the data — not
        you, not the server, not us.
      </p>
      {#if isOwner}
        <p><button onclick={printSealed}>Print the sealed volume with this key</button></p>
      {/if}
      <label class="check"><input type="checkbox" bind:checked={wroteItDown} /> I've written it down or printed it</label>
      <button class="primary wide" disabled={!wroteItDown} onclick={keySaved}>Done</button>
    </div>
  {:else if !status.enabled}
    <p>
      Sync is off. Everything stays on this device, and the app works fully without it. Turn it on to use
      Housewright on several devices, or with other people in your home.
    </p>
    <p class="muted small">
      Everything is encrypted on this device before it is sent. The server stores only data it cannot read:
      not your tasks, notes, photos or codes. There is no account and no password; each device has its own key.
    </p>
    {#if !isOwner}
      <p class="muted">Ask the household's owner to set up sync and add this device.</p>
    {:else}
      <div class="card">
        <h2>Your own server</h2>
        <p class="muted small">
          A Housewright sync server you or someone you trust runs (see SELF-HOSTING.md). Type its address.
        </p>
        <form onsubmit={setup}>
          <label for="srv">Server address</label>
          <input id="srv" bind:value={server} placeholder="https://housewright.example.org" inputmode="url" autocomplete="url" required />
          <label for="dev">Name for this device</label>
          <input id="dev" bind:value={deviceName} required />
          <label for="tok">Registration token <span class="muted">(printed by the server when it starts)</span></label>
          <input id="tok" bind:value={token} autocomplete="off" />
          <button class="primary wide" type="submit" disabled={busy}>{busy ? 'Setting up…' : 'Turn on sync'}</button>
        </form>
      </div>
      <div class="card muted">
        <h2>Housewright hosting</h2>
        <p class="small">Coming later: sync without running a server. It will be paid, and just as encrypted.</p>
      </div>
    {/if}
  {:else if status.removed}
    <p>This device was removed from the household, so it no longer syncs. What is already here stays on this device.</p>
    <p><button onclick={stop}>Forget the sync settings on this device</button></p>
  {:else}
    {#if status.notices.length > 0}
      <div class="card" role="status" data-testid="notices">
        <h2>Changes to your household's devices</h2>
        <ul>
          {#each status.notices as n, i (i)}<li class="small">{n.text}{#if n.at} <span class="muted">({when(n.at)})</span>{/if}</li>{/each}
        </ul>
        <p class="small muted">If you don't recognise one of these, remove that device and make a new recovery key.</p>
        <button onclick={async () => (status = await call('sync_dismiss_notices'))}>OK</button>
      </div>
    {/if}
    {#each status.warnings as w (w)}<p class="pill attention">{w}</p>{/each}
    <div class="card" aria-live="polite">
      <p>
        <strong>Sync is on</strong> with <span class="small">{status.server}</span>.
      </p>
      {#if status.last_error}
        <p class="pill attention" role="status">{status.last_error}</p>
      {/if}
      {#if status.clock_warning}<p class="pill attention">{status.clock_warning}</p>{/if}
      <p class="small" data-testid="last-synced">
        {status.last_sync ? `Last synced ${when(status.last_sync)}.` : 'Not synced yet.'}
        {#if status.pending_changes > 0}{status.pending_changes} changes waiting to send.{/if}
      </p>
      <button onclick={now} disabled={busy}>Sync now</button>
    </div>

    {#if !status.recovery_confirmed && isOwner}
      <div class="card">
        <p class="pill attention">The recovery key was not confirmed as saved.</p>
        <p class="small">Without it, losing every device means losing the data. Make a new one and keep it safe.</p>
        <button onclick={newKey} disabled={busy}>Make a new recovery key</button>
      </div>
    {/if}

    <h2>Devices</h2>
    {#if devices === null}
      <p class="muted small">The device list needs the server; it will show when it can be reached.</p>
    {:else}
      <ul class="devices">
        {#each devices as d (d.id)}
          <li class="card">
            <div>
              <strong>{d.name}</strong>{#if d.this_device} <span class="pill">this device</span>{/if}
              <p class="small muted">
                {d.member_name} · {roleText[d.role] ?? d.role}
                {#if d.last_seen} · last seen {when(d.last_seen)}{/if}
              </p>
            </div>
            {#if isOwner && !d.this_device}
              <button class="quiet" onclick={() => remove(d)} disabled={busy}>Remove</button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}

    {#if isOwner}
      {#if !adding}
        <p><button class="primary" onclick={() => (adding = 'choose')}>Add a device or person</button></p>
      {:else if !invite}
        <div class="card">
          {#if adding === 'choose'}
            <p>Who is it for?</p>
            <p class="row">
              <button onclick={() => { adding = 'device'; startInvite('device'); }}>Another device of mine</button>
              <button onclick={() => (adding = 'person')}>Someone else in my home</button>
            </p>
          {:else if adding === 'person'}
            <form onsubmit={(e) => { e.preventDefault(); startInvite('person'); }}>
              <label for="pn">Their name</label>
              <input id="pn" bind:value={personName} required />
              <label for="pr">What they can do</label>
              <select id="pr" bind:value={personRole}>
                <option value="member">Member: sees and edits the whole plan</option>
                <option value="limited">Limited: sees only tasks given to them</option>
              </select>
              {#if personRole === 'limited'}
                <p class="muted small">
                  “Limited” is about what the app shows them. Their device still holds the household's data, so
                  only choose it for someone you would trust with that (see SECURITY.md).
                </p>
              {/if}
              <button class="primary" type="submit">Next</button>
            </form>
          {/if}
          <p><button class="quiet" onclick={closeInvite}>Cancel</button></p>
        </div>
      {:else}
        <div class="card pairing">
          {#if pair?.state === 'waiting'}
            <h2>On the new device</h2>
            <p>Open Housewright, choose “Join a household on another device”, and enter:</p>
            <div class="qr" aria-hidden="true">{@html invite.qr_svg}</div>
            <p class="small">Server: <strong>{invite.server}</strong></p>
            <p class="code" data-testid="pair-code">{invite.code}</p>
            <p class="muted small">This code works once, for ten minutes. It is not your household's key.</p>
            <p class="muted small">Waiting for the other device…</p>
          {:else if pair?.state === 'compare'}
            <h2>Check the number</h2>
            <p>A device called “{pair.other_name}” wants to join. Its screen should show this number:</p>
            <p class="safety" data-testid="safety-number">{pair.safety_number}</p>
            <p class="small">
              Only say yes if the new device shows this same number. If it shows an error or a different number,
              say no: nothing is shared.
            </p>
            {#if !pair.other_confirmed}
              <p class="muted small" data-testid="waiting-new-device">Waiting for the new device to say the numbers match…</p>
            {/if}
            <p class="row">
              <button class="primary" onclick={() => answer(true)} disabled={busy || !pair.other_confirmed}>They match</button>
              <button onclick={() => answer(false)} disabled={busy}>They don't match</button>
            </p>
          {:else if pair?.state === 'done'}
            <p>Added. It may take a minute for everything to arrive on the new device.</p>
          {:else if pair?.state === 'failed'}
            <p role="alert">{pair.reason}</p>
          {/if}
          <p><button class="quiet" onclick={closeInvite}>{pair?.state === 'done' ? 'Close' : 'Cancel'}</button></p>
        </div>
      {/if}

      <h2>Recovery key</h2>
      <p class="muted small">
        If the paper with the recovery key is lost, or someone saw it who shouldn't have, make a new one. The old
        one stops working.
      </p>
      <p><button onclick={newKey} disabled={busy}>Make a new recovery key</button></p>
    {/if}

    <h2>This device</h2>
    <p><button class="quiet" onclick={stop}>Stop syncing on this device</button></p>
  {/if}
</section>

<style>
  .key,
  .code,
  .safety {
    font-family: ui-monospace, 'SFMono-Regular', Menlo, monospace;
    font-size: 1.25rem;
    font-weight: 700;
    letter-spacing: 0.05em;
    word-break: break-all;
    background: var(--surface-2);
    padding: 0.75rem;
    border-radius: 8px;
    text-align: center;
    user-select: all;
  }
  .safety {
    font-size: 2rem;
  }
  .qr {
    width: 12rem;
    margin: 0 auto;
  }
  .qr :global(svg) {
    width: 100%;
    height: auto;
  }
  .devices {
    list-style: none;
    padding: 0;
  }
  .devices li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.5rem;
  }
  .devices p {
    margin: 0.2rem 0 0;
  }
  .check {
    display: flex;
    gap: 0.6rem;
    align-items: center;
    min-height: var(--tap);
  }
  .check input {
    width: 1.4rem;
    min-height: 1.4rem;
  }
  .card {
    margin-top: 1rem;
  }
</style>
