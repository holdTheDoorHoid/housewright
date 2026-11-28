<script lang="ts">
  // The home facts sheet (D-27): quick answers about this house, grouped.
  // Sensitive facts (codes, passwords, account numbers) are hidden until
  // tapped, and are never shown in History or in a notification (D-41, D-58).
  import { call, type Facts, type FactInfo, type FactPrompt, type Room } from '../lib/api';
  import { app, toast } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import Attachments from './Attachments.svelte';

  let facts = $state<Facts | null>(null);
  let error = $state<string | null>(null);
  let rooms = $state<Room[]>([]);
  let revealed = $state<Record<string, string>>({});
  let skipped = $state<string[]>([]);
  let answer = $state('');
  let open = $state<string | null>(null);
  // The fact being added or edited.
  let form = $state<{
    id?: string;
    group: string;
    label: string;
    value: string;
    sensitive: boolean;
    room_id: string;
    hidden: boolean;
  } | null>(null);
  let tick = $state(0);

  $effect(() => {
    void app.tick;
    void tick;
    call('facts')
      .then((f) => (facts = f))
      .catch((e) => (error = errorText(e)));
  });
  $effect(() => {
    call('rooms').then((r) => (rooms = r));
  });

  const prompt = $derived<FactPrompt | null>(facts?.prompts.find((p) => !skipped.includes(p.key)) ?? null);

  async function answerPrompt(e: SubmitEvent) {
    e.preventDefault();
    if (!prompt || !answer.trim()) return;
    try {
      await call('save_fact', { group: prompt.group, label: prompt.label, value: answer, key: prompt.key });
      answer = '';
      tick++;
    } catch (err) {
      toast({ text: errorText(err) });
    }
  }

  function startAdd(group: string, sensitiveByDefault: boolean) {
    form = { group, label: '', value: '', sensitive: sensitiveByDefault, room_id: '', hidden: false };
  }

  function startEdit(f: FactInfo) {
    form = {
      id: f.id,
      group: f.group,
      label: f.label,
      value: f.value ?? revealed[f.id] ?? '',
      sensitive: f.sensitive,
      room_id: f.room_id ?? '',
      hidden: f.sensitive && f.value === null && !revealed[f.id],
    };
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (!form) return;
    try {
      await call('save_fact', {
        id: form.id,
        group: form.group,
        label: form.label,
        // A hidden value left empty is kept as it was.
        value: form.hidden && !form.value ? undefined : form.value,
        sensitive: form.sensitive,
        room_id: form.room_id || undefined,
      });
      if (form.id) delete revealed[form.id];
      form = null;
      tick++;
    } catch (err) {
      toast({ text: errorText(err) });
    }
  }

  async function remove(id: string) {
    if (!confirm('Delete this fact?')) return;
    await call('delete_fact', { id });
    form = null;
    tick++;
  }

  async function reveal(f: FactInfo) {
    try {
      revealed[f.id] = (await call('reveal_fact', { id: f.id })).value;
    } catch (err) {
      toast({ text: errorText(err) });
    }
  }
</script>

<section aria-labelledby="home-h">
  <h1 id="home-h">Home facts</h1>
  <p class="row links">
    <a class="button" href="#/binder">House binder</a>
    <a class="button" href="#/costs">Costs</a>
    <a class="button" href="#/labels">Print stickers</a>
    <a class="button" href="#/scan">Scan a sticker</a>
  </p>
  {#if error}
    <p role="alert">{error}</p>
  {:else if !facts}
    <p class="muted">Loading…</p>
  {:else}
    <p class="muted small">
      Quick answers about {facts.property_name}: where the shutoffs are, sizes, paint colours, codes.
      Sensitive ones stay hidden until you tap them, and go in the separate sealed part of the house
      binder.
    </p>

    {#if prompt}
      <form class="card prompt" onsubmit={answerPrompt} data-testid="fact-prompt">
        <label for="prompt-answer">{prompt.question}</label>
        <input id="prompt-answer" bind:value={answer} placeholder={prompt.hint} autocomplete="off" />
        {#if prompt.sensitive}<p class="small muted">This will be kept hidden.</p>{/if}
        <div class="row">
          <button class="primary" type="submit" disabled={!answer.trim()}>Save</button>
          <button type="button" class="quiet" onclick={() => ((skipped = [...skipped, prompt.key]), (answer = ''))}>Not now</button>
        </div>
      </form>
    {/if}

    {#each facts.groups as g (g.key)}
      <h2>{g.label}</h2>
      {#if g.facts.length === 0}
        <p class="muted small">Nothing yet.</p>
      {/if}
      <ul class="stack list">
        {#each g.facts as f (f.id)}
          <li class="card fact" data-testid="fact">
            <div class="row">
              <div class="grow">
                <div class="small muted">{f.label}{f.room_name ? ` · ${f.room_name}` : ''}</div>
                {#if !f.sensitive}
                  <div class="value">{f.value}</div>
                {:else if revealed[f.id]}
                  <div class="value" data-testid="revealed">{revealed[f.id]}</div>
                {:else}
                  <button class="quiet reveal" onclick={() => reveal(f)} aria-label={`Reveal ${f.label}`}>••••••  Tap to reveal</button>
                {/if}
              </div>
              <button class="quiet" onclick={() => startEdit(f)}>Edit</button>
            </div>
            {#if open === f.id || f.attachments.length}
              <Attachments aboutKind="fact" aboutId={f.id} items={f.attachments} documents={false} onchange={() => tick++} />
            {:else}
              <button class="quiet small" onclick={() => (open = f.id)}>Add a photo</button>
            {/if}
          </li>
        {/each}
      </ul>
      {#if form && form.group === g.key && !form.id}
        {@render editor()}
      {:else}
        <p><button class="quiet" onclick={() => startAdd(g.key, g.sensitive_by_default)}>Add to {g.label.toLowerCase()}</button></p>
      {/if}
      {#if form?.id && g.facts.some((f) => f.id === form?.id)}
        {@render editor()}
      {/if}
    {/each}
  {/if}
</section>

{#snippet editor()}
  {#if form}
    <form class="card" onsubmit={save} data-testid="fact-form">
      <label for="ff-label">What</label>
      <input id="ff-label" bind:value={form.label} placeholder="Water main shutoff, gate code, kitchen walls…" required />
      <label for="ff-value">Answer</label>
      <input
        id="ff-value"
        bind:value={form.value}
        placeholder={form.hidden ? 'Set, hidden. Type to replace it.' : ''}
        autocomplete="off"
      />
      {#if form.group === 'finishes'}
        <label for="ff-room">Room</label>
        <select id="ff-room" bind:value={form.room_id}>
          <option value="">Not one room</option>
          {#each rooms as r (r.id)}<option value={r.id}>{r.name}</option>{/each}
        </select>
      {/if}
      <label class="check"><input type="checkbox" bind:checked={form.sensitive} /> Sensitive: keep it hidden and out of the main binder</label>
      <div class="row">
        <button class="primary" type="submit">Save</button>
        <button type="button" class="quiet" onclick={() => (form = null)}>Cancel</button>
        {#if form.id}<button type="button" class="quiet" onclick={() => form?.id && remove(form.id)}>Delete</button>{/if}
      </div>
    </form>
  {/if}
{/snippet}

<style>
  .links {
    flex-wrap: wrap;
  }
  .list {
    padding: 0;
    list-style: none;
    margin: 0;
  }
  .grow {
    flex: 1;
  }
  .value {
    font-weight: 550;
  }
  .reveal {
    padding: 0;
    min-height: 2rem;
  }
  .prompt {
    background: var(--accent-soft);
    margin: 1rem 0;
  }
  .prompt label {
    margin-top: 0;
  }
  .check {
    display: flex;
    gap: 0.6rem;
    align-items: center;
  }
  .check input {
    width: 1.4rem;
    min-height: 1.4rem;
  }
</style>
