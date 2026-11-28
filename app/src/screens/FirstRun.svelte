<script lang="ts">
  import { call, type Status } from '../lib/api';
  import { errorText } from '../lib/actions';

  let { ondone }: { ondone: (s: Status) => void } = $props();
  let yourName = $state('');
  let propertyName = $state('Home');
  let householdName = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);
  let importing = $state(false);
  const timeZone = Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC';

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = null;
    try {
      ondone(
        await call('init', {
          your_name: yourName,
          property_name: propertyName,
          household_name: householdName,
          time_zone: timeZone,
        }),
      );
    } catch (err) {
      error = errorText(err);
    } finally {
      busy = false;
    }
  }

  async function importFile(e: Event) {
    const f = (e.target as HTMLInputElement).files?.[0];
    if (!f) return;
    busy = true;
    try {
      ondone(await call('import', { json: await f.text() }));
    } catch (err) {
      error = errorText(err);
    } finally {
      busy = false;
    }
  }
</script>

<section class="first" aria-labelledby="first-h">
  <h1 id="first-h">Welcome to Housewright</h1>
  <p>Three quick questions to get started. That is all for now.</p>
  <p class="muted small">
    Everything stays on this device. There is no account and nothing is sent anywhere.
  </p>
  <form onsubmit={submit}>
    <label for="you">1. Your name</label>
    <input id="you" bind:value={yourName} autocomplete="given-name" required />
    <label for="home">2. What do you call this home?</label>
    <input id="home" bind:value={propertyName} required />
    <label for="hh">3. Household name <span class="muted">(optional)</span></label>
    <input id="hh" bind:value={householdName} placeholder={yourName ? `${yourName}'s household` : 'The Smiths'} />
    <p class="muted small">Time zone: {timeZone}</p>
    {#if error}<p role="alert">{error}</p>{/if}
    <button class="primary wide" type="submit" disabled={busy}>Start</button>
  </form>
  {#if importing}
    <label for="imp">Choose a Housewright export file</label>
    <input id="imp" type="file" accept="application/json,.json" onchange={importFile} />
  {:else}
    <p><button class="quiet" onclick={() => (importing = true)}>I have an export file to restore</button></p>
  {/if}
</section>

<style>
  .first {
    max-width: 32rem;
    margin: 0 auto;
  }
  .wide {
    width: 100%;
    margin-top: 1.2rem;
  }
</style>
