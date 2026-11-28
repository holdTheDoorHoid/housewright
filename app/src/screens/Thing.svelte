<script lang="ts">
  import { categoryLabel } from '../lib/labels';
  import { call, type ItemDetail, type Card } from '../lib/api';
  import { app, refresh, toast } from '../lib/state.svelte';
  import { errorText, markDone } from '../lib/actions';
  import { resync } from '../lib/reminders';
  import { dueText, when, shortDate } from '../lib/format';
  import { openExternal } from '../lib/links';
  import { money } from '../lib/money';
  import TaskRow from './TaskRow.svelte';
  import Attachments from './Attachments.svelte';
  import Quirks from './Quirks.svelte';
  import ThingEdit from './ThingEdit.svelte';
  import PlanThumb from './PlanThumb.svelte';
  import Maker from './Maker.svelte';

  let { id }: { id: string } = $props();
  let d = $state<ItemDetail | null>(null);
  let error = $state<string | null>(null);
  let editing = $state(false);
  let offered = $state<Card[]>([]);
  let stale = $state<Card[]>([]);
  let revealed = $state<Record<string, string>>({});

  $effect(() => {
    void app.tick;
    call('item', { id })
      .then((x) => (d = x))
      .catch((e) => (error = errorText(e)));
  });

  async function startSmall() {
    try {
      const on = await call('start_small', { item_id: id, n: 3 });
      toast({ text: on.length ? `Switched on ${on.length} tasks.` : 'Nothing left to switch on.' }, 4000);
      refresh();
      resync();
    } catch (e) {
      toast({ text: errorText(e) });
    }
  }

  async function setActive(card: Card, active: boolean) {
    await call('set_active', { id: card.id, active });
    offered = offered.filter((c) => c.id !== card.id);
    stale = stale.filter((c) => c.id !== card.id);
    refresh();
    resync();
  }

  async function reveal(key: string) {
    try {
      revealed[key] = (await call('reveal_field', { item_id: id, key })).value;
    } catch (e) {
      toast({ text: errorText(e) });
    }
  }

  // Offer "show where it is" only once there is a plan (P3: never pushed).
  let hasPlan = $state(false);
  $effect(() => {
    call('floors')
      .then((f) => (hasPlan = f.some((x) => x.drawn)))
      .catch(() => (hasPlan = false));
  });

  const off = $derived(d?.tasks.filter((t) => !t.active) ?? []);
  const on = $derived(d?.tasks.filter((t) => t.active) ?? []);
  const currency = $derived(app.status?.currency ?? 'USD');
</script>

{#if error}
  <p role="alert">{error}</p>
{:else if d}
  <article>
    <p><a href="#/things">← Things</a></p>
    <h1>{d.row.name}{#if d.sensitive} <span class="pill">Private</span>{/if}</h1>
    <p class="muted small">Sticker code <strong data-testid="sticker-code">{d.code}</strong></p>

    {#if d.next}
      <!-- Scanning a sticker lands here: the next job and one tap (P1, P4). -->
      <div class="card next" data-testid="next-task">
        <div class="grow">
          <a href={`#/task/${d.next.id}`}><strong>{d.next.title}</strong></a>
          <div class="small muted">{dueText(d.next)}</div>
        </div>
        <!-- Not due yet: still one tap, but a quieter button, so a sticker
             scanned just to look does not invite logging it twice. -->
        <button
          class="done"
          class:primary={['due_soon', 'due', 'overdue'].includes(d.next.state)}
          onclick={() => d?.next && markDone(d.next).catch((e) => toast({ text: errorText(e) }))}
          >{['due_soon', 'due', 'overdue'].includes(d.next.state) ? 'Done' : 'Done now'}</button
        >
      </div>
    {/if}

    {#if offered.length || stale.length}
      <div class="card offer" data-testid="offered">
        {#if offered.length}
          <p>Now that we know more, {offered.length === 1 ? 'this task applies' : 'these tasks apply'}. Switch on what you want; nothing changes by itself.</p>
          <ul class="stack list">
            {#each offered as c (c.id)}
              <li class="row">
                <a class="grow" href={`#/task/${c.id}`}>{c.title}</a>
                <button onclick={() => setActive(c, true)}>Switch on</button>
              </li>
            {/each}
          </ul>
        {/if}
        {#if stale.length}
          <p>{stale.length === 1 ? 'This task may' : 'These tasks may'} no longer apply:</p>
          <ul class="stack list">
            {#each stale as c (c.id)}
              <li class="row">
                <a class="grow" href={`#/task/${c.id}`}>{c.title}</a>
                <button onclick={() => setActive(c, false)}>Switch off</button>
              </li>
            {/each}
          </ul>
        {/if}
        <button class="quiet" onclick={() => ((offered = []), (stale = []))}>Not now</button>
      </div>
    {/if}

    {#if editing}
      <ThingEdit
        {d}
        ondone={(r) => {
          editing = false;
          if (r) {
            d = r.detail;
            offered = r.offered;
            stale = r.no_longer_apply;
            revealed = {};
            refresh();
            toast({ text: 'Saved.' }, 2500);
          }
        }}
      />
    {:else}
      <dl class="facts">
        {#if d.row.room_name}<dt>Room</dt><dd>{d.row.room_name}</dd>{/if}
        {#if d.row.category}<dt>Kind</dt><dd>{categoryLabel(d.row.category)}</dd>{/if}
        {#if d.make}<dt>Make</dt><dd>{d.make}</dd>{/if}
        {#if d.model}<dt>Model</dt><dd>{d.model}</dd>{/if}
        {#if d.serial}<dt>Serial</dt><dd>{d.serial}</dd>{:else if d.serial_sensitive}
          <dt>Serial</dt><dd>{#if revealed.serial}{revealed.serial}{:else}<button class="quiet reveal" onclick={() => reveal('serial')}>Hidden · Reveal</button>{/if}</dd>
        {/if}
        {#if d.installed}<dt>Installed</dt><dd>{d.installed}</dd>{/if}
        {#if d.purchased}<dt>Bought</dt><dd>{d.purchased}</dd>{/if}
        {#if d.price}<dt>Price paid</dt><dd>{money(d.price.minor, d.price.currency)}</dd>{/if}
        {#if d.value}<dt>Worth now</dt><dd>{money(d.value.minor, d.value.currency)} <span class="small muted">(your estimate)</span></dd>{/if}
        {#if d.warranty}
          <dt>Warranty</dt>
          <dd data-testid="thing-warranty">
            {d.warranty.state === 'ended' ? 'Ended' : 'Until'} {shortDate(d.warranty.ends)}{#if d.warranty.provider}, {d.warranty.provider}{/if}
            {#if d.warranty.state === 'ending_soon'}<span class="pill attention">Ends in {d.warranty.days_left} {d.warranty.days_left === 1 ? 'day' : 'days'}</span>{/if}
            {#if d.warranty.transferable}<span class="small muted">· transfers to a new owner</span>{/if}
          </dd>
        {/if}
        {#each d.fields as f (f.key)}
          <dt>{f.label}</dt>
          <dd>
            {#if !f.sensitive}{f.value === 'photo' ? 'Photo (below)' : f.value.replaceAll('_', ' ')}
            {:else if revealed[f.key]}{revealed[f.key]}
            {:else}<button class="quiet reveal" onclick={() => reveal(f.key)}>Hidden · Reveal</button>{/if}
          </dd>
        {/each}
      </dl>
      {#if d.warranty?.state === 'ending_soon'}
        <div class="card offer" data-testid="warranty-action">
          <p>
            The warranty ends {shortDate(d.warranty.ends)}. If anything has seemed off, book a check-up while it is
            still covered.
          </p>
          {#if d.warranty.register_url}
            <p class="small">Not registered yet? <button class="quiet" onclick={() => d?.warranty?.register_url && openExternal(d.warranty.register_url)}>Open the registration page</button></p>
          {/if}
        </div>
      {:else if d.warranty?.register_url && d.warranty.state === 'active'}
        <p class="small"><button class="quiet" onclick={() => d?.warranty?.register_url && openExternal(d.warranty.register_url)}>Register the product</button></p>
      {/if}
      <p><button onclick={() => (editing = true)}>Edit details</button></p>
    {/if}

    {#if !editing}
      <Maker
        {d}
        onchange={(x) => {
          if (x) d = x;
          refresh();
        }}
      />
    {/if}

    {#if d.pin}
      <h2>Where it is</h2>
      <PlanThumb floorId={d.pin.floor_id} pinId={d.pin.pin_id} name={d.row.name} />
    {:else if hasPlan}
      <p><a class="quiet small" href={`#/plans/place/${d.row.id}`}>Show where it is on the floor plan</a></p>
    {/if}

    <h2>Photos and documents</h2>
    <p class="muted small">The label, the manual, the receipt, the warranty. Photos are made smaller on this device before they are kept.</p>
    <Attachments aboutKind="item" aboutId={d.row.id} items={d.attachments} addLabel="Add a photo or PDF" onchange={refresh} />

    <h2>Quirks only you know</h2>
    <p class="muted small">Printed in the house binder, for whoever looks after this next.</p>
    <Quirks aboutKind="item" aboutId={d.row.id} hint="It clicks twice before it lights. That is normal." />

    <h2>Tasks</h2>
    {#if on.length}
      <ul class="stack list">
        {#each on as card (card.id)}<TaskRow {card} />{/each}
      </ul>
    {:else}
      <p class="muted">No tasks switched on for this yet.</p>
    {/if}
    {#if off.length}
      <h2>Suggested, switched off</h2>
      <p class="muted small">Start small: switch on the few that matter most, and add more later.</p>
      <p><button class="primary" onclick={startSmall}>Switch on the top {Math.min(3, off.length)}</button></p>
      <ul class="stack list">
        {#each off as card (card.id)}
          <li class="card row off">
            <a href={`#/task/${card.id}`} class="grow">{card.title}</a>
            <span class="muted small">{card.schedule_text}</span>
          </li>
        {/each}
      </ul>
    {/if}

    <h2>History</h2>
    {#if d.history.length === 0}
      <p class="muted">Nothing done yet.</p>
    {:else}
      <ul class="stack list">
        {#each d.history as h (h.id)}
          <li class="card" class:voided={h.voided}>
            <div><strong>{h.task_title}</strong></div>
            <div class="muted small">
              {when(h.at)} · {h.by_name}{h.voided ? ' · undone' : ''}{h.cost_total ? ` · ${money(h.cost_total, currency)}` : ''}{h.attachments.length ? ` · ${h.attachments.length} photo${h.attachments.length === 1 ? '' : 's'}` : ''}
            </div>
            {#each h.notes as n, i (i)}<div>{n}</div>{/each}
          </li>
        {/each}
      </ul>
    {/if}
  </article>
{:else}
  <p class="muted">Loading…</p>
{/if}

<style>
  .facts {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.3rem 1rem;
    align-items: center;
  }
  .facts dt {
    color: var(--muted);
  }
  .facts dd {
    margin: 0;
  }
  .reveal {
    min-height: 0;
    padding: 0;
  }
  .list {
    padding: 0;
    list-style: none;
    margin: 0;
  }
  .off a {
    color: inherit;
    min-height: var(--tap);
    display: flex;
    align-items: center;
  }
  .grow {
    flex: 1;
  }
  .voided {
    opacity: 0.6;
  }
  .next {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin: 0.5rem 0 1rem;
  }
  .next a {
    color: inherit;
  }
  .done {
    min-width: 5.5rem;
    min-height: 56px;
  }
  .offer {
    margin: 0.5rem 0 1rem;
    background: var(--accent-soft);
  }
</style>
