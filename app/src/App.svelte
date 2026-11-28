<script lang="ts">
  import { onMount } from 'svelte';
  import { connect, call, ApiFailure, type Status } from './lib/api';
  import { app, go, refresh } from './lib/state.svelte';
  import { reminders, resync } from './lib/reminders';
  import { errorText } from './lib/actions';
  import Today from './screens/Today.svelte';
  import TaskDetail from './screens/TaskDetail.svelte';
  import Things from './screens/Things.svelte';
  import Thing from './screens/Thing.svelte';
  import AddThing from './screens/AddThing.svelte';
  import History from './screens/History.svelte';
  import Settings from './screens/Settings.svelte';
  import FirstRun from './screens/FirstRun.svelte';

  let fatal = $state<{ text: string; locked: boolean } | null>(null);

  async function pending() {
    const p = await reminders().takePendingAction();
    if (p?.action === 'reschedule') go(`#/task/${p.taskId}/reschedule`);
    else if (p?.action === 'open') go(`#/task/${p.taskId}`);
  }

  function started(s: Status) {
    app.status = s;
    if (s.initialized) {
      resync();
      pending();
    }
  }

  onMount(() => {
    (async () => {
      try {
        await connect();
        started(await call('status'));
      } catch (e) {
        fatal = { text: errorText(e), locked: e instanceof ApiFailure && e.code === 'locked' };
      }
    })();
    // A notification's Done button may have changed data while we were hidden.
    const onVisible = () => {
      if (document.visibilityState === 'visible' && app.status?.initialized) {
        refresh();
        pending();
      }
    };
    document.addEventListener('visibilitychange', onVisible);
    return () => document.removeEventListener('visibilitychange', onVisible);
  });

  const tabs = [
    { hash: '#/', name: 'today', label: 'Today' },
    { hash: '#/things', name: 'things', label: 'Things' },
    { hash: '#/history', name: 'history', label: 'History' },
    { hash: '#/settings', name: 'settings', label: 'Settings' },
  ];
  const current = $derived(
    app.route.name === 'task' ? 'today' : app.route.name === 'thing' || app.route.name === 'add' ? 'things' : app.route.name,
  );
</script>

{#if fatal}
  <main class="page">
    <h1>{fatal.locked ? 'Already open elsewhere' : 'Something went wrong'}</h1>
    <p role="alert">{fatal.text}</p>
    {#if fatal.locked}
      <p class="muted">
        Housewright keeps your data in this browser and only one tab can use it at a time, so nothing
        gets overwritten.
      </p>
      <button class="primary" onclick={() => location.reload()}>Try again</button>
    {/if}
  </main>
{:else if !app.status}
  <main class="page"><p class="muted">Opening…</p></main>
{:else if !app.status.initialized}
  <main class="page"><FirstRun ondone={started} /></main>
{:else}
  <main class="page">
    {#if app.route.name === 'today'}<Today />
    {:else if app.route.name === 'task'}{#key app.route.id}<TaskDetail id={app.route.id} sheet={app.route.sheet} />{/key}
    {:else if app.route.name === 'things'}<Things />
    {:else if app.route.name === 'thing'}{#key app.route.id}<Thing id={app.route.id} />{/key}
    {:else if app.route.name === 'add'}<AddThing />
    {:else if app.route.name === 'history'}<History />
    {:else if app.route.name === 'settings'}<Settings />
    {/if}
  </main>
  <nav aria-label="Main">
    {#each tabs as t (t.name)}
      <a href={t.hash} aria-current={current === t.name ? 'page' : undefined}>{t.label}</a>
    {/each}
  </nav>
{/if}

{#if app.toast}
  <div class="toast" role="status" aria-live="polite">
    <span>{app.toast.text}</span>
    {#each app.toast.actions ?? [] as a (a.label)}
      <button
        class="quiet"
        onclick={() => {
          app.toast = null;
          a.run();
        }}>{a.label}</button
      >
    {/each}
  </div>
{/if}

<style>
  .page {
    max-width: 40rem;
    margin: 0 auto;
    padding: calc(env(safe-area-inset-top) + 1rem) 1rem calc(6rem + env(safe-area-inset-bottom));
  }
  nav {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    display: flex;
    background: var(--surface);
    border-top: 1px solid var(--line);
    padding-bottom: env(safe-area-inset-bottom);
  }
  nav a {
    flex: 1;
    min-height: 60px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--muted);
    text-decoration: none;
    font-weight: 550;
  }
  nav a[aria-current='page'] {
    color: var(--accent);
    box-shadow: inset 0 3px 0 var(--accent);
  }
  .toast {
    position: fixed;
    left: 0.75rem;
    right: 0.75rem;
    bottom: calc(70px + env(safe-area-inset-bottom));
    max-width: 38rem;
    margin: 0 auto;
    background: var(--text);
    color: var(--bg);
    border-radius: 12px;
    padding: 0.5rem 0.5rem 0.5rem 1rem;
    display: flex;
    align-items: center;
    gap: 0.25rem;
    flex-wrap: wrap;
  }
  .toast span {
    flex: 1;
  }
  .toast button {
    color: var(--bg);
    text-decoration: underline;
  }
</style>
