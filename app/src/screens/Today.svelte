<script lang="ts">
  import { call, type Today } from '../lib/api';
  import { app } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import TaskRow from './TaskRow.svelte';

  let today = $state<Today | null>(null);
  let error = $state<string | null>(null);
  let dismissed = $state(false);

  $effect(() => {
    void app.tick;
    call('today')
      .then((t) => {
        today = t;
        error = null;
      })
      .catch((e) => (error = errorText(e)));
  });

  async function back() {
    dismissed = true;
    await call('mark_seen');
  }

  const showWelcome = $derived(!!today?.welcome_back && !dismissed && today.welcome_back.focus.length > 0);
</script>

<section aria-labelledby="today-h">
  <h1 id="today-h">Today</h1>
  {#if error}
    <p role="alert">{error}</p>
  {:else if !today}
    <p class="muted">Loading…</p>
  {:else if showWelcome && today.welcome_back}
    <div class="card welcome" data-testid="welcome-back">
      <h2>Welcome back</h2>
      <p>
        Here {today.welcome_back.focus.length === 1 ? 'is the one thing' : `are the ${today.welcome_back.focus.length} things`}
        that matter most right now. The rest can wait.
      </p>
      <ul class="stack list">
        {#each today.welcome_back.focus as card (card.id)}
          <TaskRow {card} />
        {/each}
      </ul>
      <button class="quiet" onclick={back}>Show my full list</button>
    </div>
  {:else}
    {#if today.tasks.length === 0}
      <div class="card">
        {#if today.active_count === 0}
          <p>No tasks are switched on yet.</p>
          <p class="muted">
            Add a thing in your home, like a smoke alarm or water heater, and Housewright will suggest
            a small plan.
          </p>
          <p><a class="button primary" href="#/add">Add a thing</a></p>
        {:else}
          <p>Nothing needs doing right now.</p>
        {/if}
      </div>
    {:else}
      <ul class="stack list" aria-label="Due now and soon">
        {#each today.tasks as card (card.id)}
          <TaskRow {card} />
        {/each}
      </ul>
    {/if}
    {#if today.coming_up.length > 0}
      <h2>Coming up</h2>
      <ul class="stack list">
        {#each today.coming_up as card (card.id)}
          <TaskRow {card} showDone={false} />
        {/each}
      </ul>
    {/if}
    {#if today.suggested_count > 0}
      <p class="muted small suggest">
        {today.suggested_count} more suggested tasks are switched off. You can switch them on from each
        thing when you are ready.
      </p>
    {/if}
  {/if}
</section>

<style>
  .list {
    padding: 0;
    margin: 0;
  }
  .welcome h2 {
    margin-top: 0;
  }
  .suggest {
    margin-top: 1.5rem;
  }
</style>
