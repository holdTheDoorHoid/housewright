<script lang="ts">
  import { call, type Suggestion, type SuggestionAnswer } from '../lib/api';
  import { refresh, toast } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';

  let { suggestion }: { suggestion: Suggestion } = $props();
  let busy = $state(false);

  async function answer(a: SuggestionAnswer) {
    busy = true;
    try {
      const out = await call('answer_suggestion', { key: suggestion.key, answer: a });
      const text =
        a === 'added'
          ? out.switched_on.length > 0
            ? `Switched on: ${out.switched_on.map((c) => c.title).join(', ')}`
            : 'Added.'
          : a === 'not_for_us'
            ? 'Got it. We will not suggest that again.'
            : 'We will ask again in a few weeks.';
      toast({ text }, 5000);
      refresh();
    } catch (e) {
      toast({ text: errorText(e) });
    } finally {
      busy = false;
    }
  }
</script>

<div class="card suggestion" data-testid="suggestion">
  <p class="muted small label">This week’s suggestion</p>
  <h2>{suggestion.title}</h2>
  <p>{suggestion.body}</p>
  {#if suggestion.why}<p class="small">{suggestion.why}{suggestion.source ? ` (${suggestion.source})` : ''}</p>{/if}
  <div class="row">
    <button class="primary" onclick={() => answer('added')} disabled={busy}>{suggestion.add_label}</button>
    <button onclick={() => answer('not_for_us')} disabled={busy}>Not for us</button>
    <button class="quiet" onclick={() => answer('later')} disabled={busy}>Later</button>
  </div>
</div>

<style>
  .suggestion {
    margin-top: 1.5rem;
    border-color: var(--accent);
  }
  .suggestion h2 {
    margin: 0.1rem 0 0.4rem;
  }
  .label {
    margin: 0;
  }
  .row {
    margin-top: 0.7rem;
  }
</style>
