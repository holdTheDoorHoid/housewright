<script lang="ts">
  // A new device joins an existing household (pairing, D-94), or gets
  // everything back with the recovery key (D-91). Both start on a device
  // with nothing on it yet.
  import { onDestroy } from 'svelte';
  import { call, currentBackend, type PairState, type Status } from '../lib/api';
  import { errorText } from '../lib/actions';

  let { mode, ondone, onback }: { mode: 'join' | 'restore'; ondone: (s: Status) => void; onback: () => void } =
    $props();
  const kind = currentBackend()?.kind === 'tauri' ? 'phone' : 'browser';
  let server = $state('');
  let code = $state('');
  let key = $state('');
  let deviceName = $state(`My ${kind}`);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let pair = $state<PairState | null>(null);
  let poll: ReturnType<typeof setInterval> | null = null;

  onDestroy(() => stop());

  function stop() {
    if (poll) clearInterval(poll);
    poll = null;
  }

  async function finish() {
    stop();
    ondone(await call('status'));
  }

  async function step(p: PairState) {
    pair = p;
    if (p.state === 'done') await finish();
    else if (p.state === 'failed' || p.state === 'compare') stop();
  }

  async function join(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = null;
    try {
      await step(await call('sync_join', { server, code, device_name: deviceName }));
      poll = setInterval(async () => {
        try {
          await step(await call('sync_join_poll'));
        } catch (err) {
          error = errorText(err);
        }
      }, 1500);
    } catch (err) {
      error = errorText(err);
    } finally {
      busy = false;
    }
  }

  async function answer(matches: boolean) {
    busy = true;
    try {
      const p = await call('sync_join_confirm', { matches });
      await step(p);
      if (p.state === 'waiting_for_other') {
        poll = setInterval(async () => {
          try {
            await step(await call('sync_join_poll'));
          } catch (err) {
            error = errorText(err);
          }
        }, 1500);
      }
    } catch (err) {
      error = errorText(err);
    } finally {
      busy = false;
    }
  }

  async function restore(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = null;
    try {
      ondone(await call('sync_restore', { server, recovery_key: key, device_name: deviceName }));
    } catch (err) {
      error = errorText(err);
    } finally {
      busy = false;
    }
  }
</script>

<section class="first" aria-labelledby="join-h">
  {#if mode === 'join'}
    <h1 id="join-h">Join a household</h1>
    {#if !pair}
      <p>On a device that already has the household, an owner opens Settings, Sync and devices, “Add a device or person”. Type what it shows.</p>
      <form onsubmit={join}>
        <label for="jsrv">Server address</label>
        <input id="jsrv" bind:value={server} placeholder="https://housewright.example.org" inputmode="url" />
        <label for="jcode">Code</label>
        <input id="jcode" bind:value={code} placeholder="ABCD-EFGH-IJKL-MNOP-QRST-UVWX" autocomplete="off" autocapitalize="characters" required />
        <label for="jdev">Name for this device</label>
        <input id="jdev" bind:value={deviceName} required />
        {#if error}<p role="alert">{error}</p>{/if}
        <button class="primary wide" type="submit" disabled={busy}>Join</button>
      </form>
    {:else if pair.state === 'waiting'}
      <p>Connected. Waiting for the other device…</p>
    {:else if pair.state === 'compare'}
      <p>Check that the other device shows the same number:</p>
      <p class="safety" data-testid="safety-number">{pair.safety_number}</p>
      <p class="muted small">
        If the numbers are different, choose “They don't match”: nothing is shared. Comparing the two screens is what
        keeps someone else from joining in your place, so look at both.
      </p>
      <p class="row">
        <button class="primary" onclick={() => answer(true)} disabled={busy}>They match</button>
        <button onclick={() => answer(false)} disabled={busy}>They don't match</button>
      </p>
    {:else if pair.state === 'waiting_for_other'}
      <p>Now confirm on the other device. Then everything arrives here.</p>
    {:else if pair.state === 'failed'}
      <p role="alert">{pair.reason}</p>
      <button onclick={() => (pair = null)}>Try again</button>
    {/if}
    {#if error && pair}<p role="alert">{error}</p>{/if}
  {:else}
    <h1 id="join-h">Restore with a recovery key</h1>
    <p>For when every device with the household is lost. You need the sync server's address and the recovery key from setup (it is in the sealed volume of the house binder).</p>
    <form onsubmit={restore}>
      <label for="rsrv">Server address</label>
      <input id="rsrv" bind:value={server} placeholder="https://housewright.example.org" inputmode="url" required />
      <label for="rkey">Recovery key</label>
      <input id="rkey" bind:value={key} placeholder="ABCD-EFGH-…" autocomplete="off" autocapitalize="characters" required />
      <label for="rdev">Name for this device</label>
      <input id="rdev" bind:value={deviceName} required />
      {#if error}<p role="alert">{error}</p>{/if}
      <button class="primary wide" type="submit" disabled={busy}>{busy ? 'Restoring…' : 'Restore'}</button>
    </form>
  {/if}
  <p><button class="quiet" onclick={() => { stop(); onback(); }}>Back</button></p>
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
  .safety {
    font-family: ui-monospace, Menlo, monospace;
    font-size: 2rem;
    font-weight: 700;
    text-align: center;
    background: var(--surface-2);
    padding: 0.75rem;
    border-radius: 8px;
  }
</style>
