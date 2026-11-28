<script lang="ts">
  import { call, currentBackend, type Weekday } from '../lib/api';
  import { app, toast } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import { reminders, resync, type Permission } from '../lib/reminders';

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

  <h2>Your data</h2>
  <p class="muted small">Stored {currentBackend()?.storage() ?? 'on this device'}.</p>
  <p class="muted small">
    An export is one JSON file with everything, including the full history. It can be restored on a new
    device or read by other programs (see FILE-FORMAT.md).
  </p>
  <button onclick={doExport}>Export everything</button>
  <p class="muted small">To restore an export, start on a fresh install and choose “I have an export file”.</p>

  <h2>About</h2>
  <p class="muted small">Housewright is free software under the AGPL-3.0.</p>
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
