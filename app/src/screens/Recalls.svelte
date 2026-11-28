<script lang="ts">
  // US product recalls (D-27), checked on this device. Off until switched
  // on. The phone downloads the public recall list from the US Consumer
  // Product Safety Commission by date only and compares it with your things
  // here; nothing about your home is sent anywhere.
  import { call, type Recalls, type RecallMatch } from '../lib/api';
  import { app, refresh, toast } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import { shortDate, when } from '../lib/format';
  import { openExternal } from '../lib/links';

  let r = $state<Recalls | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let progress = $state('');
  let showDismissed = $state(false);

  $effect(() => {
    void app.tick;
    call('recalls')
      .then((x) => (r = x))
      .catch((e) => (error = errorText(e)));
  });

  const sleep = (ms: number) => new Promise((res) => setTimeout(res, ms));

  /** Download what is missing, one date range at a time, politely. */
  async function check() {
    if (!r) return;
    busy = true;
    try {
      let plan = r.next;
      if (plan.length === 0) plan = (await call('recalls')).next;
      for (let i = 0; i < plan.length; i++) {
        const w = plan[i];
        progress = plan.length > 1 ? `Downloading ${i + 1} of ${plan.length}…` : 'Downloading…';
        // The request carries two dates and nothing else: no identifiers,
        // no cookies, no referrer.
        const res = await fetch(w.url, {
          credentials: 'omit',
          referrerPolicy: 'no-referrer',
          cache: 'no-store',
          headers: { Accept: 'application/json' },
        });
        if (!res.ok) throw new Error(`The recall service answered ${res.status}. Try again later.`);
        await call('recall_import', { json: await res.text(), start: w.start, end: w.end });
        if (i < plan.length - 1) await sleep(r.pause_ms);
      }
      r = await call('recalls');
      progress = '';
      refresh();
      toast(
        {
          text: r.matches.length
            ? `${r.matches.length} possible ${r.matches.length === 1 ? 'match' : 'matches'} to look at.`
            : 'No recalls found for your things.',
        },
        5000,
      );
    } catch (e) {
      progress = '';
      const msg = errorText(e);
      toast({
        text: /fetch|network|load/i.test(msg)
          ? 'Could not reach the recall service. Check the connection and try again.'
          : msg,
      });
    } finally {
      busy = false;
    }
  }

  async function setOn(on: boolean) {
    try {
      r = await call('set_recalls', { on });
      refresh();
      if (on) await check();
    } catch (e) {
      toast({ text: errorText(e) });
    }
  }

  async function notMine(m: RecallMatch) {
    r = await call('dismiss_recall', { item_id: m.item_id, number: m.recall.number });
    refresh();
    toast({ text: `Noted: not your ${m.item_name}.` }, 3000);
  }

  async function undo(m: RecallMatch) {
    if (!m.dismissal_id) return;
    r = await call('undismiss_recall', { id: m.dismissal_id });
    refresh();
  }

  async function openNotice(url: string) {
    try {
      await openExternal(url);
    } catch (e) {
      toast({ text: errorText(e) });
    }
  }

  const LEVEL: Record<string, string> = {
    model: 'Your model is named in this recall',
    model_series: 'Your model looks like part of a recalled series',
    brand_and_kind: 'Same brand and kind of thing: check the model numbers',
  };
</script>

{#snippet match(m: RecallMatch, dismissed: boolean)}
  <li class="card" class:strong={m.level === 'model'} data-testid="recall-match">
    <div class="small muted">{LEVEL[m.level]}</div>
    <h3><a href={`#/thing/${m.item_id}`}>{m.item_name}</a>{#if m.room_name}<span class="muted small"> · {m.room_name}</span>{/if}</h3>
    <p><strong>{m.recall.title}</strong> <span class="small muted">({shortDate(m.recall.date)}, recall {m.recall.number})</span></p>
    <ul class="small evidence" aria-label="Why it matched">
      {#each m.evidence as e, i (i)}<li>{e}</li>{/each}
    </ul>
    {#if m.recall.hazard}<p class="small"><strong>Hazard:</strong> {m.recall.hazard}</p>{/if}
    {#if m.recall.remedy}<p class="small"><strong>What to do:</strong> {m.recall.remedy}</p>{/if}
    <div class="row">
      <button class="primary" onclick={() => openNotice(m.recall.url)}>Read the official notice</button>
      {#if dismissed}
        <button class="quiet" onclick={() => undo(m)}>Show it again</button>
      {:else}
        <button class="quiet" onclick={() => notMine(m)}>Not mine</button>
      {/if}
    </div>
  </li>
{/snippet}

<section aria-labelledby="r-h">
  <p><a href="#/home">← Home facts</a></p>
  <h1 id="r-h">Product recalls</h1>
  {#if error}
    <p role="alert">{error}</p>
  {:else if !r}
    <p class="muted">Loading…</p>
  {:else if !r.enabled}
    <div class="card stack">
      <p>
        Check your things against US product recalls. Your phone downloads the public recall list from the US
        Consumer Product Safety Commission; nothing about your home is sent.
      </p>
      <p class="small muted">
        The first check downloads about ten years of recalls (roughly 13 MB, in yearly parts); later checks
        fetch only what is new. Matching uses the make and model you recorded, on this device.
      </p>
      <p><button class="primary" onclick={() => setOn(true)} data-testid="recalls-on">Turn on recall checks</button></p>
    </div>
  {:else}
    <p class="small muted">
      {#if r.synced_to}
        {r.stored} recalls on this device, up to {shortDate(r.synced_to)}{#if r.checked_at}; checked {when(r.checked_at)}{/if}.
      {:else}
        Not downloaded yet.
      {/if}
    </p>
    <p class="row">
      <button onclick={check} disabled={busy} data-testid="recalls-check">{busy ? progress || 'Checking…' : 'Check now'}</button>
    </p>
    {#if r.synced_to}
      {#if r.matches.length === 0}
        <p data-testid="no-recalls">No recalls found for your things. Things with a make and model recorded are matched best.</p>
      {:else}
        <p>
          These may apply. Matches are made on this device from the make and model you recorded, so check the
          official notice before acting.
        </p>
        <ul class="stack list">{#each r.matches as m (m.item_id + m.recall.number)}{@render match(m, false)}{/each}</ul>
      {/if}
    {/if}
    {#if r.dismissed.length}
      <p>
        <button class="quiet small" onclick={() => (showDismissed = !showDismissed)}>
          {showDismissed ? 'Hide' : 'Show'} {r.dismissed.length} marked “not mine”
        </button>
      </p>
      {#if showDismissed}
        <ul class="stack list">{#each r.dismissed as m (m.item_id + m.recall.number)}{@render match(m, true)}{/each}</ul>
      {/if}
    {/if}
    <p class="small muted">
      Recall data: US Consumer Product Safety Commission (public domain). Only recalls announced in the US are
      covered.
    </p>
    <p><button class="quiet small" onclick={() => setOn(false)}>Turn off recall checks and remove the downloaded list</button></p>
  {/if}
</section>

<style>
  .list {
    list-style: none;
    padding: 0;
  }
  .strong {
    border-left: 4px solid var(--accent);
  }
  h3 {
    margin: 0.2rem 0;
    font-size: 1.05rem;
  }
  .evidence {
    margin: 0.3rem 0;
    padding-left: 1.2rem;
  }
</style>
