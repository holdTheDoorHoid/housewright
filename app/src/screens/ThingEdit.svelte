<script lang="ts">
  // Edit a thing's details and fields (D-22). Sensitive values are never
  // sent to this screen; they show as "set, hidden" with a Reveal button,
  // and leaving the box empty keeps them as they are.
  import { call, upload, type ItemDetail, type Card, type Room, type FieldDef, type WarrantyIn } from '../lib/api';
  import { app, toast } from '../lib/state.svelte';
  import { resync } from '../lib/reminders';
  import { errorText } from '../lib/actions';
  import { readFile } from '../lib/files';
  import LabelReader, { type LabelChoice } from './LabelReader.svelte';

  let {
    d,
    ondone,
  }: {
    d: ItemDetail;
    ondone: (r: { detail: ItemDetail; offered: Card[]; no_longer_apply: Card[] } | null) => void;
  } = $props();

  // A snapshot of the thing when editing starts; the form is a local copy.
  // svelte-ignore state_referenced_locally
  const start = d;
  let rooms = $state<Room[]>([]);
  let name = $state(start.row.name);
  let make = $state(start.make ?? '');
  let model = $state(start.model ?? '');
  let serial = $state(start.serial ?? '');
  let installed = $state(start.installed ?? '');
  let purchased = $state(start.purchased ?? '');
  let roomId = $state(start.row.room_id ?? '');
  const amount = (m: { minor: number } | null) => (m ? (m.minor / 100).toFixed(2) : '');
  let price = $state(amount(start.price));
  let value = $state(amount(start.value));
  // The warranty: a length in years or months, or an end date.
  const w0 = start.warranty;
  let wHow = $state<'none' | 'length' | 'date'>(w0 ? (w0.months ? 'length' : 'date') : 'none');
  let wYears = $state(w0?.months ? String(w0.months % 12 === 0 ? w0.months / 12 : w0.months) : '');
  let wUnit = $state<'years' | 'months'>(w0?.months && w0.months % 12 !== 0 ? 'months' : 'years');
  let wEnds = $state(w0?.ends ?? '');
  let wProvider = $state(w0?.provider ?? '');
  let wLink = $state(w0?.register_url ?? '');
  let wTransfers = $state(w0?.transferable ?? false);
  const owner = $derived(app.status?.me?.role === 'owner');
  let wholePrivate = $state(start.sensitive);
  let serialPrivate = $state(start.serial_sensitive);
  let values = $state<Record<string, string>>(
    Object.fromEntries(start.edit.map((f) => [f.def.key, f.value ?? ''])),
  );
  let edit = $state(start.edit);
  let reading = $state(false);
  let busy = $state(false);
  // Adding a field.
  let adding = $state(false);
  let newLabel = $state('');
  let newType = $state<FieldDef['type']>('text');
  let newUnit = $state('');
  let newOptions = $state('');
  let newSensitive = $state<'auto' | 'yes' | 'no'>('auto');
  let newScope = $state<'item' | 'template'>('item');

  $effect(() => {
    call('rooms').then((r) => (rooms = r));
  });

  function fromLabel(c: LabelChoice) {
    if (c.make) make = c.make;
    if (c.model) model = c.model;
    if (c.serial) serial = c.serial;
    reading = false;
  }

  async function reveal(key: string) {
    try {
      const r = await call('reveal_field', { item_id: start.row.id, key });
      values[key] = r.value;
    } catch (e) {
      toast({ text: errorText(e) });
    }
  }

  async function photoField(key: string, e: Event) {
    const input = e.target as HTMLInputElement;
    const f = input.files?.[0];
    input.value = '';
    if (!f) return;
    try {
      const a = await upload('attach', { about_kind: 'item', about_id: start.row.id, name: f.name }, await readFile(f));
      values[key] = a.id;
      toast({ text: 'Photo added. Save to keep it in this field.' }, 3000);
    } catch (err) {
      toast({ text: errorText(err) });
    }
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    try {
      const changed: Record<string, string | null> = {};
      for (const f of edit) {
        const v = (values[f.def.key] ?? '').trim();
        // A hidden sensitive value is kept unless something new was typed.
        if (f.sensitive && f.has_value && v === '') continue;
        if (v !== (f.value ?? '')) changed[f.def.key] = v === '' ? null : v;
      }
      const r = await call('update_item', {
        id: start.row.id,
        name,
        make,
        model,
        serial: start.serial_sensitive && serial === '' ? undefined : serial,
        installed,
        purchased,
        room_id: roomId,
        values: changed,
        price,
        value,
        warranty: warrantyArgs(),
        ...(owner && wholePrivate !== start.sensitive ? { sensitive: wholePrivate } : {}),
        ...(owner && serialPrivate !== start.serial_sensitive ? { serial_sensitive: serialPrivate } : {}),
      });
      // A warranty's dates decide its notices (D-140).
      resync();
      ondone(r);
    } catch (err) {
      toast({ text: errorText(err) });
    } finally {
      busy = false;
    }
  }

  function warrantyArgs(): WarrantyIn | undefined {
    if (wHow === 'none') return w0 ? { remove: true } : undefined;
    const common = { provider: wProvider, register_url: wLink, transferable: wTransfers };
    if (wHow === 'date') return wEnds ? { ends: wEnds, ...common } : undefined;
    const n = Number(wYears);
    if (!wYears.trim() || !Number.isFinite(n) || n <= 0) return undefined;
    return { months: Math.round(wUnit === 'years' ? n * 12 : n), ...common };
  }

  async function addField() {
    try {
      const r = await call('add_field', {
        item_id: start.row.id,
        label: newLabel,
        type: newType,
        unit: newType === 'number' ? newUnit : undefined,
        options:
          newType === 'choice'
            ? newOptions
                .split(',')
                .map((o) => o.trim())
                .filter(Boolean)
            : undefined,
        sensitive: newSensitive === 'auto' ? undefined : newSensitive === 'yes',
        to_template: newScope === 'template',
      });
      edit = r.edit;
      for (const f of r.edit) if (!(f.def.key in values)) values[f.def.key] = f.value ?? '';
      adding = false;
      newLabel = '';
      newOptions = '';
      newUnit = '';
      toast({ text: 'Field added.' }, 3000);
    } catch (err) {
      toast({ text: errorText(err) });
    }
  }

  async function removeField(key: string) {
    if (!confirm('Remove this field and its value from this thing?')) return;
    try {
      const r = await call('remove_field', { item_id: start.row.id, key });
      edit = r.edit;
      delete values[key];
    } catch (err) {
      toast({ text: errorText(err) });
    }
  }

  const TYPES: [FieldDef['type'], string][] = [
    ['text', 'Text'],
    ['number', 'Number'],
    ['date', 'Date'],
    ['yes_no', 'Yes or no'],
    ['choice', 'Choice'],
    ['photo', 'Photo'],
  ];
  const human = (o: string) => o.replaceAll('_', ' ');
</script>

<form class="card edit" onsubmit={save} data-testid="thing-edit">
  <h2>Edit details</h2>
  {#if reading}
    <LabelReader onuse={fromLabel} />
  {:else}
    <p><button type="button" onclick={() => (reading = true)}>Read the label from a photo</button></p>
  {/if}
  <label for="e-name">Name</label>
  <input id="e-name" bind:value={name} required />
  <label for="e-room">Room</label>
  <select id="e-room" bind:value={roomId}>
    <option value="">No room</option>
    {#each rooms as r (r.id)}<option value={r.id}>{r.name}</option>{/each}
  </select>
  <label for="e-make">Make</label>
  <input id="e-make" bind:value={make} />
  <label for="e-model">Model</label>
  <input id="e-model" bind:value={model} />
  <label for="e-serial">Serial</label>
  <input id="e-serial" bind:value={serial} placeholder={start.serial_sensitive ? 'Set, hidden' : ''} />
  <div class="row two">
    <div>
      <label for="e-inst">Installed</label>
      <input id="e-inst" type="date" bind:value={installed} />
    </div>
    <div>
      <label for="e-purch">Bought</label>
      <input id="e-purch" type="date" bind:value={purchased} />
    </div>
  </div>

  {#if owner}
    <label class="check"><input type="checkbox" bind:checked={serialPrivate} /> Keep the serial number private</label>
    <label class="check">
      <input type="checkbox" bind:checked={wholePrivate} data-testid="thing-private" />
      Private thing (a safe, jewellery): keep it out of the main binder and the insurance inventory unless you ask
    </label>
  {/if}
  <div class="row two">
    <div>
      <label for="e-price">Price paid</label>
      <input id="e-price" inputmode="decimal" bind:value={price} placeholder="0.00" />
    </div>
    <div>
      <label for="e-value">Worth now (your guess)</label>
      <input id="e-value" inputmode="decimal" bind:value={value} placeholder="0.00" />
    </div>
  </div>
  <p class="small muted">For the insurance inventory. In your household's currency.</p>

  <fieldset class="warranty">
    <legend>Warranty</legend>
    <label for="e-w-how" class="visually-hidden">Warranty</label>
    <select id="e-w-how" bind:value={wHow}>
      <option value="none">None recorded</option>
      <option value="length">It lasts a number of years or months</option>
      <option value="date">It ends on a date</option>
    </select>
    {#if wHow === 'length'}
      <div class="row">
        <label for="e-w-len" class="visually-hidden">How long</label>
        <input id="e-w-len" inputmode="numeric" bind:value={wYears} placeholder="5" />
        <select bind:value={wUnit} aria-label="Years or months">
          <option value="years">years</option>
          <option value="months">months</option>
        </select>
      </div>
      <p class="small muted">Counted from the earlier of the bought and installed dates{installed || purchased ? '' : ': add one above'}.</p>
    {:else if wHow === 'date'}
      <label for="e-w-ends">Ends on</label>
      <input id="e-w-ends" type="date" bind:value={wEnds} />
    {/if}
    {#if wHow !== 'none'}
      <label for="e-w-prov">Who gives it (optional)</label>
      <input id="e-w-prov" bind:value={wProvider} placeholder="The maker, the installer, the shop" />
      <label for="e-w-link">Registration page (optional)</label>
      <input id="e-w-link" bind:value={wLink} placeholder="https://…" autocomplete="off" />
      <label class="check"><input type="checkbox" bind:checked={wTransfers} /> It transfers to a new owner</label>
      <p class="small muted">You get one reminder about two months before it ends and one a week before.</p>
    {/if}
  </fieldset>

  {#each edit as f (f.def.key)}
    {@const k = f.def.key}
    <label for={`ef-${k}`}>
      {f.def.label}{f.def.type === 'number' && f.def.unit ? ` (${f.def.unit})` : ''}
      {#if f.sensitive}<span class="pill">Sensitive</span>{/if}
    </label>
    {#if f.def.type === 'choice'}
      <select id={`ef-${k}`} bind:value={values[k]}>
        <option value="">Not sure</option>
        {#each f.def.options as o (o)}<option value={o}>{human(o)}</option>{/each}
      </select>
    {:else if f.def.type === 'yes_no'}
      <select id={`ef-${k}`} bind:value={values[k]}>
        <option value="">Not sure</option>
        <option value="yes">Yes</option>
        <option value="no">No</option>
      </select>
    {:else if f.def.type === 'date'}
      <input id={`ef-${k}`} type="date" bind:value={values[k]} />
    {:else if f.def.type === 'number'}
      <input id={`ef-${k}`} inputmode="decimal" bind:value={values[k]} />
    {:else if f.def.type === 'photo'}
      <div class="row">
        <span class="small muted">{values[k] ? 'Photo chosen' : 'No photo'}</span>
        <label class="button" for={`ef-${k}`}>Choose a photo</label>
        <input id={`ef-${k}`} class="hidden" type="file" accept="image/*" onchange={(e) => photoField(k, e)} />
      </div>
    {:else}
      <input
        id={`ef-${k}`}
        bind:value={values[k]}
        placeholder={f.sensitive && f.has_value ? 'Set, hidden' : ''}
        autocomplete="off"
      />
    {/if}
    <div class="row small">
      {#if f.sensitive && f.has_value && !values[k]}
        <button type="button" class="quiet" onclick={() => reveal(k)}>Reveal</button>
      {/if}
      {#if f.origin === 'item'}
        <button type="button" class="quiet" onclick={() => removeField(k)}>Remove field</button>
      {:else if f.origin === 'household'}
        <span class="muted">Added to every {start.row.template?.replaceAll('-', ' ') ?? 'thing of this kind'} here</span>
      {/if}
    </div>
  {/each}

  {#if adding}
    <div class="card newfield" data-testid="new-field">
      <label for="nf-label">Field name</label>
      <input id="nf-label" bind:value={newLabel} placeholder="Filter size, gate code…" />
      <label for="nf-type">Kind of value</label>
      <select id="nf-type" bind:value={newType}>
        {#each TYPES as [t, l] (t)}<option value={t}>{l}</option>{/each}
      </select>
      {#if newType === 'number'}
        <label for="nf-unit">Unit (optional)</label>
        <input id="nf-unit" bind:value={newUnit} placeholder="in, gal, psi" />
      {:else if newType === 'choice'}
        <label for="nf-opts">Choices, separated by commas</label>
        <input id="nf-opts" bind:value={newOptions} placeholder="gas, oil, electric" />
      {/if}
      <label for="nf-sens">Sensitive?</label>
      <select id="nf-sens" bind:value={newSensitive}>
        <option value="auto">Decide from the name (codes and passwords are sensitive)</option>
        <option value="yes">Yes: hide it and keep it out of the main binder</option>
        <option value="no">No</option>
      </select>
      {#if start.row.template}
        <label for="nf-scope">Add it to</label>
        <select id="nf-scope" bind:value={newScope}>
          <option value="item">Just this thing</option>
          <option value="template">Every thing of this kind in this home</option>
        </select>
      {/if}
      <div class="row">
        <button type="button" class="primary" onclick={addField} disabled={!newLabel.trim()}>Add the field</button>
        <button type="button" class="quiet" onclick={() => (adding = false)}>Cancel</button>
      </div>
    </div>
  {:else}
    <p><button type="button" class="quiet" onclick={() => (adding = true)}>Add a field of your own</button></p>
  {/if}

  <div class="row submit">
    <button class="primary" type="submit" disabled={busy}>Save</button>
    <button type="button" class="quiet" onclick={() => ondone(null)}>Cancel</button>
  </div>
</form>

<style>
  .edit h2 {
    margin-top: 0;
  }
  .two > div {
    flex: 1;
  }
  .hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    opacity: 0;
  }
  .newfield {
    margin-top: 1rem;
    background: var(--surface-2);
  }
  .submit {
    margin-top: 1.2rem;
  }
  label .pill {
    margin-left: 0.4rem;
  }
  .warranty {
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 0.5rem 0.75rem 0.75rem;
    margin: 1rem 0 0;
  }
  .warranty legend {
    font-weight: 600;
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
