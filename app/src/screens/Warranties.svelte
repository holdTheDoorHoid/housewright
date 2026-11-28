<script lang="ts">
  // Warranties (D-27, P11): what is still covered, what ends soon, what has
  // ended. A warranty that ends soon comes with something to do (P9).
  import { call, type Warranties, type WarrantyRow } from '../lib/api';
  import { app, toast } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import { shortDate } from '../lib/format';
  import { openExternal } from '../lib/links';

  let w = $state<Warranties | null>(null);
  let error = $state<string | null>(null);

  $effect(() => {
    void app.tick;
    call('warranties')
      .then((x) => (w = x))
      .catch((e) => (error = errorText(e)));
  });

  async function register(url: string) {
    try {
      await openExternal(url);
    } catch (e) {
      toast({ text: errorText(e) });
    }
  }

  const left = (r: WarrantyRow) =>
    r.days_left === 0 ? 'ends today' : r.days_left === 1 ? 'ends tomorrow' : `${r.days_left} days left`;
</script>

{#snippet row(r: WarrantyRow)}
  <li class="card" data-testid="warranty-row">
    <div class="row">
      <a class="grow" href={`#/thing/${r.item_id}`}><strong>{r.name}</strong></a>
      <span class="small">{r.state === 'ended' ? 'Ended' : 'Until'} {shortDate(r.ends, w?.today)}</span>
    </div>
    <div class="small muted">
      {[r.room_name, r.provider, r.transferable ? 'transfers to a new owner' : null].filter(Boolean).join(' · ')}
    </div>
    {#if r.state === 'ending_soon'}
      <p class="small">
        <span class="pill attention">{left(r)}</span>
        Book a check-up while it is covered: anything wrong found now can still be fixed under the warranty.
      </p>
    {/if}
    {#if r.register_url && r.state !== 'ended'}
      <button class="quiet small" onclick={() => r.register_url && register(r.register_url)}>Registration page</button>
    {/if}
  </li>
{/snippet}

<section aria-labelledby="w-h">
  <p><a href="#/home">← Home facts</a></p>
  <h1 id="w-h">Warranties</h1>
  {#if error}
    <p role="alert">{error}</p>
  {:else if !w}
    <p class="muted">Loading…</p>
  {:else}
    {#if w.ending_soon.length + w.active.length + w.ended.length === 0}
      <p class="muted">
        No warranties recorded yet. On a thing's page, choose “Edit details” and give its warranty: how
        many years, or the end date. You get one reminder about two months before it ends and one a week
        before.
      </p>
    {/if}
    {#if w.ending_soon.length}
      <h2>Ending soon</h2>
      <ul class="stack list" data-testid="ending-soon">{#each w.ending_soon as r (r.item_id)}{@render row(r)}{/each}</ul>
    {/if}
    {#if w.active.length}
      <h2>Covered</h2>
      <ul class="stack list" data-testid="covered">{#each w.active as r (r.item_id)}{@render row(r)}{/each}</ul>
    {/if}
    {#if w.ended.length}
      <h2>Ended</h2>
      <ul class="stack list" data-testid="ended">{#each w.ended as r (r.item_id)}{@render row(r)}{/each}</ul>
    {/if}
    {#if w.without}
      <p class="small muted">{w.without} {w.without === 1 ? 'thing has' : 'things have'} no warranty recorded.</p>
    {/if}
  {/if}
</section>

<style>
  .list {
    list-style: none;
    padding: 0;
  }
  .grow {
    flex: 1;
  }
</style>
