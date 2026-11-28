<script lang="ts">
  // The house binder (D-40 to D-43): a printed manual that makes sense
  // without the app, in two volumes. The main binder never holds anything
  // marked sensitive; the sealed volume holds only that (D-41). Each
  // section is numbered on its own, so after a change only the changed
  // sections need printing.
  import {
    call,
    type BinderInfo,
    type BinderPdfArgs,
    type ContractorInfo,
    type Edition,
    type EditionInfo,
    type Paper,
  } from '../lib/api';
  import { app, toast } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import { openFile } from '../lib/files';
  import { shortDate } from '../lib/format';
  import Quirks from './Quirks.svelte';

  let info = $state<BinderInfo | null>(null);
  let error = $state<string | null>(null);
  let edition = $state<Edition>('full');
  let includeCosts = $state(true);
  let includeNotes = $state(true);
  let busy = $state<string | null>(null);
  let location = $state('');
  // Typed but not saved yet: a reload must not wipe it.
  let locationEdited = $state(false);
  let contractors = $state<ContractorInfo[]>([]);
  let form = $state<(ContractorInfo & { tradesText: string }) | null>(null);
  let tick = $state(0);

  $effect(() => {
    void app.tick;
    void tick;
    // Only the handover edition has choices; its change markers must
    // compare with a print made with the same choices.
    const choices = edition === 'handover' ? { include_costs: includeCosts, include_notes: includeNotes } : {};
    call('binder', choices)
      .then((b) => {
        info = b;
        if (!locationEdited) location = b.sealed_location ?? '';
      })
      .catch((e) => (error = errorText(e)));
    call('contractors').then((c) => (contractors = c));
  });

  const ed = $derived<EditionInfo | null>(info?.editions.find((e) => e.edition === edition) ?? null);
  const mainSections = $derived(ed?.sections.filter((s) => s.volume === 'main') ?? []);
  const sealedSections = $derived(ed?.sections.filter((s) => s.volume === 'sealed') ?? []);
  const mainChanged = $derived(mainSections.filter((s) => s.changed).length);
  const sealedChanged = $derived(sealedSections.filter((s) => s.changed).length);

  function pick(e: Edition) {
    edition = e;
    // The handover edition leaves costs out unless asked (D-42).
    includeCosts = e !== 'handover';
    includeNotes = true;
  }

  function filename(volume: 'main' | 'sealed', partial: boolean): string {
    const base = `house-binder-${edition}`;
    return `${base}${volume === 'sealed' ? '-SEALED' : ''}${partial ? '-changed' : ''}.pdf`;
  }

  async function print(volume: 'main' | 'sealed', onlyChanged = false) {
    busy = `${volume}${onlyChanged ? '-changed' : ''}`;
    const args: BinderPdfArgs = {
      edition,
      volume,
      only_changed: onlyChanged,
      ...(edition === 'handover' ? { include_costs: includeCosts, include_notes: includeNotes } : {}),
    };
    try {
      await openFile('binder_pdf', args, filename(volume, onlyChanged), 'application/pdf');
      tick++;
    } catch (e) {
      toast({ text: errorText(e) });
    } finally {
      busy = null;
    }
  }

  async function saveLocation(e: SubmitEvent) {
    e.preventDefault();
    try {
      info = await call('set_sealed_location', { text: location });
      locationEdited = false;
      toast({ text: location.trim() ? 'Saved. It is printed in the binder.' : 'Cleared. The binder will have a line to write on.' });
    } catch (err) {
      toast({ text: errorText(err) });
    }
  }

  async function setPaper(p: Paper) {
    info = await call('set_paper', { paper: p });
  }

  async function setReminder(on: boolean) {
    try {
      info = await call('set_binder_reminder', { on });
      toast({ text: on ? 'You will be reminded once a year.' : 'Yearly reminder switched off.' });
    } catch (err) {
      toast({ text: errorText(err) });
    }
  }

  function startContractor(c?: ContractorInfo) {
    form = c
      ? { ...c, tradesText: c.trades.join(', ') }
      : { name: '', trades: [], phone: '', email: '', notes: '', notes_private: false, emergency: false, tradesText: '' };
  }

  async function saveContractor(e: SubmitEvent) {
    e.preventDefault();
    if (!form) return;
    try {
      const { tradesText, ...rest } = form;
      contractors = await call('save_contractor', {
        ...rest,
        trades: tradesText.split(',').map((t) => t.trim()).filter(Boolean),
      });
      form = null;
      tick++;
    } catch (err) {
      toast({ text: errorText(err) });
    }
  }

  async function removeContractor(id: string) {
    if (!confirm('Remove this contact?')) return;
    await call('delete_contractor', { id });
    form = null;
    tick++;
  }

  const today = $derived(new Date().toISOString().slice(0, 10));
</script>

<section aria-labelledby="binder-h">
  <p><a href="#/home">← Home facts</a></p>
  <h1 id="binder-h">House binder</h1>
  <p class="muted small">
    A printed manual for the house, for the day the person who knows is not there. It makes sense
    without the app. Codes, passwords and account numbers are printed separately, in a sealed volume.
  </p>

  {#if error}
    <p role="alert">{error}</p>
  {:else if !info || !ed}
    <p class="muted">Loading…</p>
  {:else}
    <fieldset class="editions">
      <legend>Which binder?</legend>
      {#each info.editions as e (e.edition)}
        <label class="edition card" class:chosen={e.edition === edition}>
          <input type="radio" name="edition" value={e.edition} checked={e.edition === edition} onchange={() => pick(e.edition)} />
          <span>
            <strong>{e.label}</strong>
            <span class="muted small block">{e.description}</span>
          </span>
        </label>
      {/each}
    </fieldset>

    <h2>What is in it</h2>
    <p class="status" role="status">
      {#if !ed.last_printed}
        Not printed yet.
      {:else if ed.changed === 0}
        Nothing has changed since you printed it on {shortDate(ed.last_printed, today)}.
      {:else}
        {ed.changed} {ed.changed === 1 ? 'section has' : 'sections have'} changed since you printed on
        {shortDate(ed.last_printed, today)}.
      {/if}
    </p>
    <ul class="sections">
      {#each mainSections as s (s.key)}
        <li class="row">
          <span class="grow">{s.title}</span>
          {#if ed.last_printed && s.changed}<span class="pill attention">changed</span>{/if}
        </li>
      {/each}
    </ul>
    {#if ed.has_sealed && info.can_print_sealed}
      <h3>Sealed volume</h3>
      <ul class="sections">
        {#each sealedSections as s (s.key)}
          <li class="row">
            <span class="grow">{s.title}</span>
            {#if ed.last_printed && s.changed}<span class="pill attention">changed</span>{/if}
          </li>
        {/each}
      </ul>
    {/if}

    {#if edition === 'handover'}
      <label class="check"><input type="checkbox" bind:checked={includeCosts} /> Include what work cost</label>
      <label class="check"><input type="checkbox" bind:checked={includeNotes} /> Include your notes about things (check them for anything personal)</label>
    {/if}

    <label for="paper">Paper</label>
    <select id="paper" value={info.paper} onchange={(e) => setPaper((e.currentTarget as HTMLSelectElement).value as Paper)}>
      <option value="letter">US Letter</option>
      <option value="a4">A4</option>
    </select>

    <div class="actions stack">
      {#if ed.last_printed && mainChanged > 0}
        <button class="primary" disabled={!!busy} onclick={() => print('main', true)}>
          {busy === 'main-changed' ? 'Preparing…' : `Print the ${mainChanged} changed ${mainChanged === 1 ? 'section' : 'sections'}`}
        </button>
      {/if}
      <button class:primary={!ed.last_printed || mainChanged === 0} disabled={!!busy} onclick={() => print('main')}>
        {busy === 'main' ? 'Preparing…' : edition === 'emergency' ? 'Print the emergency page' : 'Print the whole binder'}
      </button>
      {#if ed.has_sealed}
        {#if info.can_print_sealed}
          {#if ed.last_printed && sealedChanged > 0}
            <button disabled={!!busy} onclick={() => print('sealed', true)}>
              {busy === 'sealed-changed' ? 'Preparing…' : `Print the ${sealedChanged} changed sealed ${sealedChanged === 1 ? 'section' : 'sections'}`}
            </button>
          {/if}
          <button disabled={!!busy} onclick={() => print('sealed')}>
            {busy === 'sealed' ? 'Preparing…' : 'Print the sealed volume (separately)'}
          </button>
          <p class="muted small">
            The sealed volume is a separate file titled "SEALED — keep in a safe place". Print it
            yourself, and keep it away from the binder.
          </p>
        {:else}
          <p class="muted small">Only the household's owner can print the sealed volume.</p>
        {/if}
      {/if}
    </div>

    {#if info.reminder}
      <div class="card reminder">
        {#if info.reminder.active}
          <p>You will be reminded once a year to reprint the changed sections.</p>
          <button class="quiet" onclick={() => setReminder(false)}>Switch the reminder off</button>
        {:else}
          <p>A binder helps most when it is up to date. Want a reminder once a year to reprint what changed?</p>
          <button onclick={() => setReminder(true)}>Remind me once a year</button>
        {/if}
      </div>
    {/if}

    {#if edition !== 'handover' && edition !== 'emergency'}
      <h2>Where the sealed volume is kept</h2>
      <form onsubmit={saveLocation}>
        <label for="sealed-loc">This is printed in the main binder, so whoever needs it can find it.</label>
        <input id="sealed-loc" bind:value={location} oninput={() => (locationEdited = true)} placeholder="In the fire safe, top shelf of the hall closet" />
        <button type="submit">Save</button>
      </form>
    {/if}

    <h2>Who to call</h2>
    <p class="muted small">
      Contacts marked for emergencies go on the emergency page. Private notes are printed only in the
      sealed volume.
    </p>
    <ul class="sections">
      {#each contractors as c (c.id)}
        <li class="row">
          <span class="grow">
            <strong>{c.name}</strong>
            {#if c.trades.length}<span class="muted"> · {c.trades.join(', ')}</span>{/if}
            {#if c.phone}<span class="block small">{c.phone}</span>{/if}
          </span>
          {#if c.emergency}<span class="pill">emergency</span>{/if}
          <button class="quiet" onclick={() => startContractor(c)}>Edit</button>
        </li>
      {/each}
    </ul>
    {#if form}
      <form class="card" onsubmit={saveContractor}>
        <label for="c-name">Name</label>
        <input id="c-name" bind:value={form.name} placeholder="Ace Plumbing" />
        <label for="c-trades">What they do</label>
        <input id="c-trades" bind:value={form.tradesText} placeholder="plumbing, heating" />
        <label for="c-phone">Phone</label>
        <input id="c-phone" type="tel" bind:value={form.phone} />
        <label for="c-email">Email</label>
        <input id="c-email" type="email" bind:value={form.email} />
        <label class="check"><input type="checkbox" bind:checked={form.emergency} /> Call them in an emergency (on the emergency page)</label>
        <label for="c-notes">Notes</label>
        <textarea id="c-notes" bind:value={form.notes} placeholder="Ask for Dana."></textarea>
        <label class="check"><input type="checkbox" bind:checked={form.notes_private} /> Notes are private: sealed volume only</label>
        <div class="row">
          <button class="primary" type="submit">Save contact</button>
          <button type="button" class="quiet" onclick={() => (form = null)}>Cancel</button>
          {#if form.id}
            <button type="button" class="quiet" onclick={() => form?.id && removeContractor(form.id)}>Remove</button>
          {/if}
        </div>
      </form>
    {:else}
      <button onclick={() => startContractor()}>Add a contact</button>
    {/if}

    {#if app.status?.property}
      <h2>Quirks of the house</h2>
      <p class="muted small">Things only you know. Notes about a single thing are on that thing's page.</p>
      <Quirks aboutKind="property" aboutId={app.status.property.id} />
    {/if}
  {/if}
</section>

<style>
  .editions {
    border: 0;
    padding: 0;
    margin: 0;
  }
  .edition {
    display: flex;
    gap: 0.75rem;
    align-items: flex-start;
    margin: 0.5rem 0;
    font-weight: normal;
    cursor: pointer;
  }
  .edition input {
    width: auto;
    min-height: 0;
    margin-top: 0.3rem;
  }
  .edition.chosen {
    border-color: var(--accent);
  }
  .block {
    display: block;
  }
  .sections {
    list-style: none;
    padding: 0;
    margin: 0.4rem 0;
  }
  .sections li {
    border-bottom: 1px solid var(--line);
    padding: 0.35rem 0;
  }
  .grow {
    flex: 1;
  }
  .check {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    font-weight: normal;
  }
  .check input {
    width: auto;
    min-height: 0;
  }
  .actions {
    margin: 1rem 0;
  }
  .actions button {
    width: 100%;
  }
  .reminder {
    margin: 1rem 0;
  }
</style>
