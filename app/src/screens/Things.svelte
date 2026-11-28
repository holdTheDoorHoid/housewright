<script lang="ts">
  import { call, type ItemRow } from '../lib/api';
  import { app } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import { shortDate } from '../lib/format';
  import { categoryLabel } from '../lib/labels';

  let items = $state<ItemRow[] | null>(null);
  let error = $state<string | null>(null);
  let by = $state<'room' | 'category'>('room');

  $effect(() => {
    void app.tick;
    call('items')
      .then((i) => (items = i))
      .catch((e) => (error = errorText(e)));
  });

  const groups = $derived.by(() => {
    const m = new Map<string, ItemRow[]>();
    for (const i of items ?? []) {
      const k = by === 'room' ? (i.room_name ?? 'No room yet') : i.category ? categoryLabel(i.category) : 'Other';
      m.set(k, [...(m.get(k) ?? []), i]);
    }
    return [...m.entries()].sort(([a], [b]) => a.localeCompare(b));
  });

  const label = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
</script>

<section aria-labelledby="things-h">
  <div class="row head">
    <h1 id="things-h">Things</h1>
    <a class="button primary" href="#/add">Add a thing</a>
  </div>
  <p class="row">
    <a class="button" href="#/scan">Scan a sticker</a>
    <a class="button" href="#/labels">Print stickers</a>
  </p>
  <div class="row" role="radiogroup" aria-label="Group by">
    <button role="radio" aria-checked={by === 'room'} class:primary={by === 'room'} onclick={() => (by = 'room')}
      >By room</button
    >
    <button
      role="radio"
      aria-checked={by === 'category'}
      class:primary={by === 'category'}
      onclick={() => (by = 'category')}>By kind</button
    >
  </div>
  {#if error}
    <p role="alert">{error}</p>
  {:else if items && items.length === 0}
    <p class="muted">Nothing recorded yet. Start with something important, like a smoke alarm.</p>
  {:else}
    {#each groups as [name, rows] (name)}
      <h2>{label(name)}</h2>
      <ul class="stack list">
        {#each rows as i (i.id)}
          <li>
            <a class="card item" href={`#/thing/${i.id}`}>
              <span class="name">{i.name}</span>
              <span class="muted small">
                {#if i.next_due}Next: {shortDate(i.next_due)}{:else}No tasks switched on{/if}
              </span>
            </a>
          </li>
        {/each}
      </ul>
    {/each}
  {/if}
</section>

<style>
  .head {
    justify-content: space-between;
  }
  .list {
    padding: 0;
    list-style: none;
    margin: 0;
  }
  .item {
    display: flex;
    flex-direction: column;
    color: inherit;
    text-decoration: none;
    min-height: var(--tap);
  }
  .name {
    font-weight: 600;
  }
</style>
