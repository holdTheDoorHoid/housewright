<script lang="ts">
  import { call, type SuggestedGroup } from '../lib/api';
  import { app, refresh, toast } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import { categoryLabel } from '../lib/labels';

  let groups = $state<SuggestedGroup[] | null>(null);
  let error = $state<string | null>(null);

  $effect(() => {
    void app.tick;
    call('all_suggested')
      .then((g) => (groups = g))
      .catch((e) => (error = errorText(e)));
  });

  async function on(id: string, title: string) {
    try {
      await call('set_active', { id, active: true });
      toast({ text: `Switched on: ${title}` }, 4000);
      refresh();
    } catch (e) {
      toast({ text: errorText(e) });
    }
  }
</script>

<section aria-labelledby="sug-h">
  <p><a href="#/plan">← Your plan</a></p>
  <h1 id="sug-h">Everything we suggested</h1>
  <p class="muted small">
    These are switched off. There is no need to do them all; switch on what is useful to you, when you
    are ready. Housewright will also suggest one at a time on Today.
  </p>
  {#if error}
    <p role="alert">{error}</p>
  {:else if !groups}
    <p class="muted">Loading…</p>
  {:else if groups.length === 0}
    <p>Everything suggested is already switched on.</p>
  {:else}
    {#each groups as g (g.item_id ?? g.item_name)}
      <h2>{g.item_name}{#if g.category}<span class="muted small"> · {categoryLabel(g.category)}</span>{/if}</h2>
      <ul class="stack list">
        {#each g.tasks as t (t.card.id)}
          <li class="card row task">
            <div class="body">
              <a href={`#/task/${t.card.id}`}>{t.card.title}</a>
              {#if t.why}<span class="small block">{t.why}</span>{/if}
              <span class="muted small block">{t.card.schedule_text}{t.source ? ` · ${t.source}` : ''}</span>
            </div>
            <button onclick={() => on(t.card.id, t.card.title)} aria-label={`Switch on: ${t.card.title}`}>Switch on</button>
          </li>
        {/each}
      </ul>
    {/each}
  {/if}
</section>

<style>
  .list {
    padding: 0;
    margin: 0;
    list-style: none;
  }
  .task {
    flex-wrap: nowrap;
  }
  .body {
    flex: 1;
  }
  .block {
    display: block;
  }
</style>
