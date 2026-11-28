<script lang="ts">
  import { call, type Room, type TemplateSummary, type Card } from '../lib/api';
  import { go, refresh, toast } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import { resync } from '../lib/reminders';

  let templates = $state<TemplateSummary[]>([]);
  let rooms = $state<Room[]>([]);
  let chosen = $state<TemplateSummary | null>(null);
  let name = $state('');
  let roomId = $state('');
  let newRoom = $state('');
  let make = $state('');
  let model = $state('');
  let installed = $state('');
  let values = $state<Record<string, string>>({});
  let added = $state<{ item_id: string; tasks: Card[] } | null>(null);
  let busy = $state(false);
  let filter = $state('');

  $effect(() => {
    call('templates').then((t) => (templates = t));
    call('rooms').then((r) => (rooms = r));
  });

  const shown = $derived(
    templates.filter(
      (t) =>
        !filter ||
        t.name.toLowerCase().includes(filter.toLowerCase()) ||
        (t.category ?? '').includes(filter.toLowerCase()),
    ),
  );

  function pick(t: TemplateSummary) {
    chosen = t;
    name = t.name;
    values = {};
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (!chosen) return;
    busy = true;
    try {
      added = await call('add_from_template', {
        template: chosen.id,
        name,
        room_id: roomId && roomId !== '__new' ? roomId : undefined,
        new_room: roomId === '__new' ? newRoom : undefined,
        make,
        model,
        installed: installed || undefined,
        values,
      });
      refresh();
    } catch (err) {
      toast({ text: errorText(err) });
    } finally {
      busy = false;
    }
  }

  async function startSmall() {
    if (!added) return;
    const on = await call('start_small', { item_id: added.item_id, n: 3 });
    resync();
    refresh();
    toast({ text: `Switched on ${on.length} tasks. They are on Today when due.` }, 5000);
    go(`#/thing/${added.item_id}`);
  }
</script>

<section aria-labelledby="add-h">
  <p><a href="#/things">← Things</a></p>
  <h1 id="add-h">Add a thing</h1>

  {#if added}
    <div class="card stack" data-testid="added">
      <p><strong>{name}</strong> is recorded, with {added.tasks.length} suggested tasks.</p>
      <p class="muted">
        They start switched off so you do not land on a long list. Start small: switch on the most
        important few now and add more later.
      </p>
      <ul>
        {#each added.tasks as t (t.id)}<li>{t.title} <span class="muted small">({t.priority_label})</span></li>{/each}
      </ul>
      <div class="row">
        <button class="primary" onclick={startSmall}>Switch on the top {Math.min(3, added.tasks.length)}</button>
        <button onclick={() => added && go(`#/thing/${added.item_id}`)}>Not now</button>
      </div>
    </div>
  {:else if !chosen}
    <label for="filter">What is it?</label>
    <input id="filter" bind:value={filter} placeholder="Search: dryer, gutters, alarm…" />
    <ul class="stack grid">
      {#each shown as t (t.id)}
        <li>
          <button class="card pick" onclick={() => pick(t)}>
            <span class="name">{t.name}</span>
            <span class="muted small">{t.tasks.length} tasks · {t.category ?? 'general'}</span>
          </button>
        </li>
      {/each}
    </ul>
  {:else}
    <form onsubmit={save}>
      <p class="muted">{chosen.description ?? ''}</p>
      <label for="name">Name</label>
      <input id="name" bind:value={name} />
      <label for="room">Room</label>
      <select id="room" bind:value={roomId}>
        <option value="">Not sure / skip</option>
        {#each rooms as r (r.id)}<option value={r.id}>{r.name}</option>{/each}
        <option value="__new">A new room…</option>
      </select>
      {#if roomId === '__new'}
        <label for="newroom">New room name</label>
        <input id="newroom" bind:value={newRoom} placeholder="Basement" />
      {/if}
      <details>
        <summary>More details (optional)</summary>
        <label for="make">Make</label>
        <input id="make" bind:value={make} />
        <label for="model">Model</label>
        <input id="model" bind:value={model} />
        <label for="installed">Installed on</label>
        <input id="installed" type="date" bind:value={installed} />
        {#each chosen.fields.filter((f) => f.type !== 'photo') as f (f.key)}
          <label for={`f-${f.key}`}>{f.label}{f.type === 'number' && f.unit ? ` (${f.unit})` : ''}</label>
          {#if f.type === 'choice'}
            <select id={`f-${f.key}`} bind:value={values[f.key]}>
              <option value="">Not sure</option>
              {#each f.options as o (o)}<option value={o}>{o}</option>{/each}
            </select>
          {:else if f.type === 'yes_no'}
            <select id={`f-${f.key}`} bind:value={values[f.key]}>
              <option value="">Not sure</option>
              <option value="yes">Yes</option>
              <option value="no">No</option>
            </select>
          {:else if f.type === 'date'}
            <input id={`f-${f.key}`} type="date" bind:value={values[f.key]} />
          {:else if f.type === 'number'}
            <input id={`f-${f.key}`} inputmode="decimal" bind:value={values[f.key]} />
          {:else}
            <input id={`f-${f.key}`} bind:value={values[f.key]} />
          {/if}
          {#if f.help}<p class="muted small">{f.help}</p>{/if}
        {/each}
      </details>
      <div class="row submit">
        <button class="primary" type="submit" disabled={busy}>Add</button>
        <button type="button" class="quiet" onclick={() => (chosen = null)}>Back</button>
      </div>
    </form>
  {/if}
</section>

<style>
  .grid {
    padding: 0;
    list-style: none;
  }
  .pick {
    width: 100%;
    flex-direction: column;
    align-items: flex-start;
    text-align: left;
    gap: 0;
    padding: 0.7rem 1rem;
  }
  .name {
    font-weight: 600;
  }
  details {
    margin-top: 1rem;
  }
  summary {
    min-height: var(--tap);
    display: flex;
    align-items: center;
    cursor: pointer;
    color: var(--accent);
  }
  .submit {
    margin-top: 1.2rem;
  }
</style>
