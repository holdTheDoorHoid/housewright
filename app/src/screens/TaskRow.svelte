<script lang="ts">
  import type { Card } from '../lib/api';
  import { dueText, placeText } from '../lib/format';
  import { markDone, errorText } from '../lib/actions';
  import { toast } from '../lib/state.svelte';

  let { card, showDone = true }: { card: Card; showDone?: boolean } = $props();
  let busy = $state(false);

  async function done() {
    busy = true;
    try {
      await markDone(card);
    } catch (e) {
      toast({ text: errorText(e) });
    } finally {
      busy = false;
    }
  }
</script>

<li class="card task" data-testid="task-row">
  <a class="body" href={`#/task/${card.id}`}>
    <span class="title">{card.title}</span>
    {#if placeText(card)}<span class="muted small">{placeText(card)}</span>{/if}
    <span class="small due" class:attention={card.state === 'overdue'}>{dueText(card)}</span>
    {#if card.needs_review}<span class="pill attention">Moved a few times: review</span>{/if}
  </a>
  {#if showDone}
    <button class="primary done" onclick={done} disabled={busy} aria-label={`Done: ${card.title}`}>
      Done
    </button>
  {/if}
</li>

<style>
  .task {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    list-style: none;
  }
  .body {
    flex: 1;
    display: flex;
    flex-direction: column;
    color: inherit;
    text-decoration: none;
    min-height: var(--tap);
    justify-content: center;
  }
  .title {
    font-weight: 600;
  }
  .due {
    color: var(--muted);
  }
  .due.attention {
    color: var(--attention);
  }
  .done {
    min-width: 5.5rem;
    min-height: 56px;
  }
</style>
