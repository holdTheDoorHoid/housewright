<script lang="ts">
  import { call, currentBackend, type Weekday } from '../lib/api';
  import { app, toast } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import { reminders, resync, type Permission } from '../lib/reminders';
  import { saveFile } from '../lib/files';
  import { CURRENCIES } from '../lib/money';
  import type { SyncStatus } from '../lib/api';
  import { when, shortDate } from '../lib/format';
  import type { LibraryStatus } from '../lib/api';
  import { checkLibrary } from '../lib/library';

  // The appliance library (D-147, D-148).
  let lib = $state<LibraryStatus | null>(null);
  let libBusy = $state(false);
  let libMessage = $state('');
  $effect(() => {
    call('library').then(
      (s) => (lib = s),
      () => (lib = null),
    );
  });

  async function setLibAuto(auto: boolean) {
    try {
      lib = await call('library_set_updates', { auto });
    } catch (err) {
      toast({ text: errorText(err) });
    }
  }

  async function checkLib() {
    libBusy = true;
    libMessage = '';
    try {
      const r = await checkLibrary();
      lib = r.status;
      libMessage = r.message;
    } catch (err) {
      const msg = errorText(err);
      libMessage = /fetch|network|load/i.test(msg)
        ? 'Could not reach the library site. Check the connection and try again.'
        : msg;
    } finally {
      libBusy = false;
    }
  }

  async function resetLib() {
    lib = await call('library_reset');
    libMessage = 'Using the library that came with the app.';
  }

  let sync = $state<SyncStatus | null>(null);
  $effect(() => {
    call('sync_status').then(
      (s) => (sync = s),
      () => (sync = null),
    );
  });

  const DAYS: [Weekday, string][] = [
    ['monday', 'Mon'],
    ['tuesday', 'Tue'],
    ['wednesday', 'Wed'],
    ['thursday', 'Thu'],
    ['friday', 'Fri'],
    ['saturday', 'Sat'],
    ['sunday', 'Sun'],
  ];

  const me = app.status?.me;
  const prefs = me?.reminders ?? {};
  let quietOn = $state(!!prefs.quiet_hours);
  let quietStart = $state(prefs.quiet_hours?.start.slice(0, 5) ?? '21:00');
  let quietEnd = $state(prefs.quiet_hours?.end.slice(0, 5) ?? '08:00');
  let windowOn = $state(!!prefs.maintenance_window);
  let winDays = $state<Weekday[]>(prefs.maintenance_window?.days ?? ['saturday', 'sunday']);
  let winStart = $state(prefs.maintenance_window?.start.slice(0, 5) ?? '10:00');
  let winEnd = $state(prefs.maintenance_window?.end.slice(0, 5) ?? '12:00');
  let permission = $state<Permission>('unsupported');
  let planned = $state<number | null>(null);

  $effect(() => {
    reminders()
      .permission()
      .then((p) => (permission = p));
  });

  async function savePrefs(e: SubmitEvent) {
    e.preventDefault();
    try {
      const m = await call('set_reminder_prefs', {
        prefs: {
          quiet_hours: quietOn ? { start: quietStart, end: quietEnd } : null,
          maintenance_window: windowOn ? { days: winDays, start: winStart, end: winEnd } : null,
        },
      });
      if (app.status) app.status.me = m;
      resync();
      toast({ text: 'Saved. Reminders now follow these times.' }, 4000);
    } catch (err) {
      toast({ text: errorText(err) });
    }
  }

  async function allow() {
    permission = await reminders().request();
    if (permission === 'granted') planned = await reminders().sync();
  }

  async function tryOne() {
    try {
      if (permission !== 'granted') permission = await reminders().request();
      const r = await reminders().test(30);
      toast({ text: `A reminder for “${r.title}” will arrive in about 30 seconds. You can close the app.` }, 6000);
    } catch (err) {
      toast({ text: errorText(err) });
    }
  }

  async function doExport() {
    const r = await call('export');
    const url = URL.createObjectURL(new Blob([r.json], { type: 'application/json' }));
    const a = document.createElement('a');
    a.href = url;
    a.download = r.filename;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  let currency = $state(app.status?.currency ?? 'USD');
  async function saveCurrency() {
    try {
      app.status = await call('set_currency', { currency });
      toast({ text: `Costs are now in ${currency}.` }, 3000);
    } catch (err) {
      toast({ text: errorText(err) });
    }
  }

  async function fullBackup() {
    try {
      const day = new Date().toISOString().slice(0, 10);
      await saveFile('full_backup', undefined, `housewright-backup-${day}.zip`, 'application/zip');
    } catch (err) {
      toast({ text: errorText(err) });
    }
  }

  function toggleDay(d: Weekday) {
    winDays = winDays.includes(d) ? winDays.filter((x) => x !== d) : [...winDays, d];
  }
</script>

<section aria-labelledby="set-h">
  <h1 id="set-h">Settings</h1>
  {#if me}<p class="muted">Signed in on this device as {me.name}. No account needed.</p>{/if}

  <h2>Reminders</h2>
  <p class="muted small">{reminders().describe()}</p>
  {#if permission === 'prompt'}
    <button class="primary" onclick={allow}>Allow reminders</button>
  {:else if permission === 'denied'}
    <p class="pill attention">Notifications are blocked. Allow them in your device or browser settings.</p>
  {:else if permission === 'granted'}
    <p class="small">Notifications are allowed.{planned !== null ? ` ${planned} reminders planned.` : ''}</p>
  {/if}
  {#if permission !== 'unsupported'}
    <p><button onclick={tryOne}>Try a reminder now</button></p>
  {/if}

  <form onsubmit={savePrefs} class="card">
    <label class="check"><input type="checkbox" bind:checked={quietOn} /> Quiet hours</label>
    {#if quietOn}
      <div class="row">
        <label for="qs" class="inline">From</label><input id="qs" type="time" bind:value={quietStart} />
        <label for="qe" class="inline">to</label><input id="qe" type="time" bind:value={quietEnd} />
      </div>
      <p class="muted small">No reminders in this time. They wait until it ends.</p>
    {/if}
    <label class="check"><input type="checkbox" bind:checked={windowOn} /> A usual time for jobs around the house</label>
    {#if windowOn}
      <div class="row days" role="group" aria-label="Days">
        {#each DAYS as [d, label] (d)}
          <button type="button" aria-pressed={winDays.includes(d)} class:primary={winDays.includes(d)} onclick={() => toggleDay(d)}
            >{label}</button
          >
        {/each}
      </div>
      <div class="row">
        <label for="ws" class="inline">From</label><input id="ws" type="time" bind:value={winStart} />
        <label for="we" class="inline">to</label><input id="we" type="time" bind:value={winEnd} />
      </div>
      <p class="muted small">Reminders arrive at the start of this time, on the first of these days after a task is due.</p>
    {/if}
    <button class="primary" type="submit">Save reminder times</button>
  </form>

  <h2>Sync</h2>
  {#if sync && !sync.available}
    <p class="muted small">This version of the app cannot sync.</p>
  {:else if sync?.enabled}
    <p class="small">
      {#if sync.removed}This device was removed from the household and no longer syncs.
      {:else if sync.last_error}<span class="pill attention">Not synced</span> {sync.last_error}
      {:else if sync.last_sync}On. Last synced {when(sync.last_sync)}.
      {:else}On.{/if}
    </p>
    {#if sync.clock_warning}<p class="pill attention">{sync.clock_warning}</p>{/if}
    {#if sync.notices.length > 0}<p class="pill attention">Your household's devices changed: see Sync and devices.</p>{/if}
  {:else}
    <p class="muted small">Off. Everything stays on this device. Turn sync on to use Housewright on more than one device or with other people.</p>
  {/if}
  <p><a class="button" href="#/sync">Sync and devices</a></p>
  <p><a href="#/home-assistant">Connect Home Assistant</a></p>

  <h2>Your home</h2>
  <p class="muted small">
    Answer the home questions again if something changed, or to fill in what you skipped. This only adds
    things and suggestions; nothing you already have is changed or removed.
  </p>
  <p><a class="button" href="#/questions/redo">Redo the home questions</a></p>

  <h2>Your data</h2>
  <p class="muted small">Stored {currentBackend()?.storage() ?? 'on this device'}.</p>
  <p class="muted small">
    An export is one JSON file with everything, including the full history. It can be restored on a new
    device or read by other programs (see FILE-FORMAT.md).
  </p>
  <p class="muted small">
    A full backup is one .zip file with the export and every photo and document. Keep it somewhere
    safe, like a cloud drive or a USB stick.
  </p>
  <p class="row">
    <button class="primary" onclick={fullBackup}>Make a full backup</button>
    <button onclick={doExport}>Export (JSON, no photos)</button>
  </p>
  <p class="muted small">
    To restore, start on a fresh install and choose “I have a backup or export to restore”.
  </p>

  <h2>Appliance library</h2>
  {#if lib}
    <p class="small" data-testid="library-version">
      Version {lib.version}, published {shortDate(lib.published)}: {lib.brands} brands, {lib.families} model families.
      {lib.from_update ? 'Downloaded update.' : 'Came with the app.'}
    </p>
    <p class="muted small">
      Links to manufacturers’ official manuals and maintenance facts in our own words, used to suggest what a
      thing is from its make and model. It is on this device and works offline.
    </p>
    <label class="check">
      <input type="checkbox" checked={lib.auto} onchange={(e) => setLibAuto((e.target as HTMLInputElement).checked)} data-testid="library-auto" />
      Get appliance library updates (checks once a week when the app is open)
    </label>
    <p class="muted small">
      A check downloads the public library file from GitHub. Nothing about your home is sent: the request is the
      same for everyone. A new version is used only if it passes every check.
    </p>
    <p class="row">
      <button onclick={checkLib} disabled={libBusy} data-testid="library-check">{libBusy ? 'Checking…' : 'Check now'}</button>
      {#if lib.from_update}<button class="quiet" onclick={resetLib}>Go back to the version that came with the app</button>{/if}
    </p>
    {#if libMessage}<p class="small" role="status" data-testid="library-message">{libMessage}</p>{/if}
    {#if lib.checked_at}<p class="muted small">Last checked {when(lib.checked_at)}.</p>{/if}
  {/if}

  <h2>Costs</h2>
  <div class="row">
    <label for="currency" class="inline">Currency</label>
    <select id="currency" bind:value={currency} onchange={saveCurrency}>
      {#each CURRENCIES as c (c)}<option value={c}>{c}</option>{/each}
    </select>
  </div>

  <h2>About</h2>
  <p class="muted small">Housewright is free software under the AGPL-3.0.</p>
  <p class="muted small">
    Label reading uses the ocrs models (github.com/robertknight/ocrs-models), trained on the HierText
    dataset by Google Research, licensed under CC BY-SA 4.0.
  </p>
  {#if lib}
    <p class="muted small" data-testid="library-credit">
      The Housewright appliance library is by Housewright appliance library contributors,
      github.com/holdTheDoorHoid/housewright-library, licensed under CC BY 4.0
      (creativecommons.org/licenses/by/4.0); version {lib.version}, commit {lib.commit.slice(0, 7)}. Manuals and pages it links
      to belong to their manufacturers; brand names are their owners’ trademarks.
    </p>
  {/if}
</section>

<style>
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
  .inline {
    margin: 0;
  }
  .row input {
    width: auto;
    flex: 1;
  }
  .days button {
    min-width: 3rem;
    padding: 0 0.4rem;
  }
  form.card {
    margin-top: 1rem;
  }
</style>
