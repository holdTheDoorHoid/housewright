<script lang="ts">
  import { call, type HistoryEntry } from '../lib/api';
  import { app } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import { when } from '../lib/format';

  let entries = $state<HistoryEntry[] | null>(null);
  let error = $state<string | null>(null);
  $effect(() => {
    void app.tick;
    call('history')
      .then((h) => (entries = h))
      .catch((e) => (error = errorText(e)));
  });
</script>

<section aria-labelledby="hist-h">
  <h1 id="hist-h">History</h1>
  <p class="muted small">Everything changed in this household, newest first. Values are never shown here.</p>
  {#if error}
    <p role="alert">{error}</p>
  {:else if entries}
    <ul class="list" data-testid="history">
      {#each entries as e, i (i)}
        <li>
          <div>{e.text}</div>
          <div class="muted small">{when(e.at)}</div>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .list {
    padding: 0;
    list-style: none;
  }
  .list li {
    padding: 0.6rem 0;
    border-bottom: 1px solid var(--line);
  }
</style>
