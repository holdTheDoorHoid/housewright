<script lang="ts">
  import { call, type ItemDetail } from '../lib/api';
  import { app, refresh, toast } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import { resync } from '../lib/reminders';
  import { when } from '../lib/format';
  import TaskRow from './TaskRow.svelte';

  let { id }: { id: string } = $props();
  let d = $state<ItemDetail | null>(null);
  let error = $state<string | null>(null);

  $effect(() => {
    void app.tick;
    call('item', { id })
      .then((x) => (d = x))
      .catch((e) => (error = errorText(e)));
  });

  async function startSmall() {
    try {
      const on = await call('start_small', { item_id: id, n: 3 });
      toast({ text: on.length ? `Switched on ${on.length} tasks.` : 'Nothing left to switch on.' }, 4000);
      refresh();
      resync();
    } catch (e) {
      toast({ text: errorText(e) });
    }
  }

  const off = $derived(d?.tasks.filter((t) => !t.active) ?? []);
  const on = $derived(d?.tasks.filter((t) => t.active) ?? []);
</script>

{#if error}
  <p role="alert">{error}</p>
{:else if d}
  <article>
    <p><a href="#/things">← Things</a></p>
    <h1>{d.row.name}</h1>
    <dl class="facts">
      {#if d.row.room_name}<dt>Room</dt><dd>{d.row.room_name}</dd>{/if}
      {#if d.row.category}<dt>Kind</dt><dd>{d.row.category}</dd>{/if}
      {#if d.make}<dt>Make</dt><dd>{d.make}</dd>{/if}
      {#if d.model}<dt>Model</dt><dd>{d.model}</dd>{/if}
      {#if d.installed}<dt>Installed</dt><dd>{d.installed}</dd>{/if}
      {#each d.fields as f (f.key)}
        <dt>{f.label}</dt>
        <dd>{f.sensitive ? 'Hidden (sensitive)' : f.value}</dd>
      {/each}
    </dl>

    <h2>Tasks</h2>
    {#if on.length}
      <ul class="stack list">
        {#each on as card (card.id)}<TaskRow {card} />{/each}
      </ul>
    {:else}
      <p class="muted">No tasks switched on for this yet.</p>
    {/if}
    {#if off.length}
      <h2>Suggested, switched off</h2>
      <p class="muted small">Start small: switch on the few that matter most, and add more later.</p>
      <p><button class="primary" onclick={startSmall}>Switch on the top {Math.min(3, off.length)}</button></p>
      <ul class="stack list">
        {#each off as card (card.id)}
          <li class="card row off">
            <a href={`#/task/${card.id}`} class="grow">{card.title}</a>
            <span class="muted small">{card.schedule_text}</span>
          </li>
        {/each}
      </ul>
    {/if}

    <h2>History</h2>
    {#if d.history.length === 0}
      <p class="muted">Nothing done yet.</p>
    {:else}
      <ul class="stack list">
        {#each d.history as h (h.id)}
          <li class="card" class:voided={h.voided}>
            <div><strong>{h.task_title}</strong></div>
            <div class="muted small">{when(h.at)} · {h.by_name}{h.voided ? ' · undone' : ''}</div>
            {#each h.notes as n, i (i)}<div>{n}</div>{/each}
          </li>
        {/each}
      </ul>
    {/if}
  </article>
{:else}
  <p class="muted">Loading…</p>
{/if}

<style>
  .facts {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.3rem 1rem;
  }
  .facts dt {
    color: var(--muted);
  }
  .facts dd {
    margin: 0;
  }
  .list {
    padding: 0;
    list-style: none;
    margin: 0;
  }
  .off a {
    color: inherit;
    min-height: var(--tap);
    display: flex;
    align-items: center;
  }
  .grow {
    flex: 1;
  }
  .voided {
    opacity: 0.6;
  }
</style>
