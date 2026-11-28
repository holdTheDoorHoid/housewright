<script lang="ts">
  import { categoryLabel } from '../lib/labels';
  import { call, type Room, type TemplateSummary, type Card, type FamilyHit, type LibraryMatch } from '../lib/api';
  import FamilySuggestion from './FamilySuggestion.svelte';
  import { go, refresh, toast } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import { resync } from '../lib/reminders';
  import LabelReader, { type LabelChoice } from './LabelReader.svelte';

  let templates = $state<TemplateSummary[]>([]);
  let rooms = $state<Room[]>([]);
  let chosen = $state<TemplateSummary | null>(null);
  let name = $state('');
  let roomId = $state('');
  let newRoom = $state('');
  let make = $state('');
  let model = $state('');
  let serial = $state('');
  let fromLabel = $state(false);
  let installed = $state('');
  let values = $state<Record<string, string>>({});
  let added = $state<{ item_id: string; tasks: Card[] } | null>(null);
  let busy = $state(false);
  let filter = $state('');
  // The appliance library's suggestion for the make and model typed (D-148).
  let family = $state<FamilyHit | null>(null);
  let declined = $state(false);
  let suggestion = $state<LibraryMatch | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    const mk = make.trim();
    const md = model.trim();
    const tpl = chosen?.id ?? null;
    clearTimeout(timer);
    if (family || declined || !mk) {
      suggestion = null;
      return;
    }
    timer = setTimeout(() => {
      call('library_match', { make: mk, model: md, template: tpl })
        .then((m) => (suggestion = m.families.length ? m : null))
        .catch(() => (suggestion = null));
    }, 300);
  });

  function useFamily(h: FamilyHit) {
    family = h;
    suggestion = null;
    if (!chosen && h.template) {
      const t = templates.find((x) => x.id === h.template?.id);
      if (t) pick(t);
    }
  }

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
    if (family?.template && family.template.id !== t.id) family = null;
  }

  /** A read label fills in make, model and serial, and picks the kind of
   * thing when the label says what it is. The person still checks it all
   * before adding. */
  function useLabel(c: LabelChoice) {
    fromLabel = false;
    const t = c.template ? templates.find((x) => x.id === c.template?.id) : undefined;
    if (t && !chosen) pick(t);
    if (c.family) useFamily(c.family);
    make = c.make;
    model = c.model;
    serial = c.serial;
    if (!chosen) toast({ text: 'Now choose what kind of thing it is.' }, 4000);
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
        serial: serial || undefined,
        installed: installed || undefined,
        values,
        family: family?.id ?? (declined ? 'none' : undefined),
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
  {:else if fromLabel}
    <LabelReader onuse={useLabel} showTemplate={true} />
    <button class="quiet" onclick={() => (fromLabel = false)}>Back</button>
  {:else if !chosen}
    <p><button onclick={() => (fromLabel = true)}>Start from a photo of its label</button></p>
    {#if make || model}<p class="small muted">From the label: {[make, model, serial].filter(Boolean).join(' · ')}</p>{/if}
    <label for="filter">What is it?</label>
    <input id="filter" bind:value={filter} placeholder="Search: dryer, gutters, alarm…" />
    <ul class="stack grid">
      {#each shown as t (t.id)}
        <li>
          <button class="card pick" onclick={() => pick(t)}>
            <span class="name">{t.name}</span>
            <span class="muted small">{t.tasks.length} {t.tasks.length === 1 ? 'task' : 'tasks'} · {categoryLabel(t.category)}</span>
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
      <details open={!!(make || model || serial || family)}>
        <summary>More details (optional)</summary>
        <label for="make">Make</label>
        <input id="make" bind:value={make} />
        <label for="model">Model</label>
        <input id="model" bind:value={model} />
        {#if family}
          <p class="small" data-testid="family-chosen">
            Linked to <strong>{family.name}</strong> from the appliance library: its official manual links and facts
            will be on this thing’s page.
            <button type="button" class="quiet" onclick={() => (family = null)}>Remove</button>
          </p>
        {:else if suggestion}
          <FamilySuggestion match={suggestion} onuse={useFamily} onnone={() => (declined = true)} />
        {/if}
        <label for="serial">Serial</label>
        <input id="serial" bind:value={serial} />
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
