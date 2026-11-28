<script lang="ts">
  // Snooze = reschedule to a moment (P7). After three moves the core flags the
  // task for review, and the choices that make it go away for good come first.
  import { call, type Card, type Member, type SnoozeOption } from '../lib/api';
  import { refresh, toast } from '../lib/state.svelte';
  import { resync } from '../lib/reminders';
  import { errorText } from '../lib/actions';
  import { todayStr, shortDate } from '../lib/format';

  let { card, interval, onclose }: { card: Card; interval: string | null; onclose: () => void } =
    $props();

  let options = $state<SnoozeOption[]>([]);
  let members = $state<Member[]>([]);
  let picked = $state('');
  let every = $state('');
  let who = $state('');
  let busy = $state(false);

  $effect(() => {
    call('snooze_options').then((o) => (options = o));
    if (card.needs_review) call('members').then((m) => (members = m));
    every = interval ?? '';
  });

  async function act(f: () => Promise<unknown>, msg: string) {
    busy = true;
    try {
      await f();
      refresh();
      resync();
      toast({ text: msg }, 4000);
      onclose();
    } catch (e) {
      toast({ text: errorText(e) });
    } finally {
      busy = false;
    }
  }

  const snooze = (date: string) =>
    act(() => call('snooze', { id: card.id, until: date }), `Moved to ${shortDate(date)}.`);
</script>

<div class="card sheet" role="dialog" aria-labelledby="resched-h" data-testid="reschedule">
  {#if card.needs_review}
    <h2 id="resched-h">This keeps getting moved</h2>
    <p class="muted">
      It has been put off {card.snooze_count} times. That usually means the plan is wrong, not you. What
      would help?
    </p>
    <div class="stack">
      <form
        class="row"
        onsubmit={(e) => {
          e.preventDefault();
          act(() => call('review', { id: card.id, action: 'change_interval', every }), 'Updated how often.');
        }}
      >
        <label for="every" class="grow">Do it less often: every</label>
        <input id="every" bind:value={every} placeholder="6 months" />
        <button type="submit" disabled={busy || !every}>Change</button>
      </form>
      {#if members.length > 1}
        <form
          class="row"
          onsubmit={(e) => {
            e.preventDefault();
            act(() => call('review', { id: card.id, action: 'reassign', member: who }), 'Handed over.');
          }}
        >
          <label for="who" class="grow">Give it to someone else</label>
          <select id="who" bind:value={who}>
            {#each members.filter((m) => m.id !== card.owner_id) as m (m.id)}
              <option value={m.id}>{m.name}</option>
            {/each}
          </select>
          <button type="submit" disabled={busy || !who}>Hand over</button>
        </form>
      {/if}
      <button
        disabled={busy}
        onclick={() =>
          act(() => call('review', { id: card.id, action: 'not_applicable' }), 'Marked not applicable.')}
      >
        This does not apply to my home
      </button>
      <button
        disabled={busy}
        onclick={() => act(() => call('review', { id: card.id, action: 'pause' }), 'Paused.')}
      >
        Pause it for now
      </button>
      <p class="muted small">Or find a pro to do it, and mark it done when they have.</p>
    </div>
    <h2>Or move it once more</h2>
  {:else}
    <h2 id="resched-h">When would suit you?</h2>
  {/if}
  <div class="choices">
    {#each options as o (o.key)}
      <button disabled={busy} onclick={() => snooze(o.date)}>
        {o.label} <span class="muted small">{shortDate(o.date)}</span>
      </button>
    {/each}
  </div>
  <form
    class="row pick"
    onsubmit={(e) => {
      e.preventDefault();
      if (picked) snooze(picked);
    }}
  >
    <label for="pick" class="grow">Pick a day</label>
    <input id="pick" type="date" min={todayStr()} bind:value={picked} />
    <button type="submit" disabled={busy || !picked}>Move</button>
  </form>
  <button class="quiet" onclick={onclose}>Cancel</button>
</div>

<style>
  .sheet {
    margin: 1rem 0;
  }
  .sheet h2:first-child {
    margin-top: 0;
  }
  .choices {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.5rem;
  }
  .choices button {
    flex-direction: column;
    gap: 0;
    padding: 0.4rem;
  }
  .grow {
    flex-basis: 100%;
    margin: 0.6rem 0 0;
  }
  .row input,
  .row select {
    flex: 1;
    width: auto;
  }
</style>
