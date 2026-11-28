<script lang="ts">
  // After Done (P1): a note, a photo, what it cost, and the task's own
  // readings (pressure, pass/fail). Every part is optional, and the job is
  // already recorded whether or not any of this is filled in.
  import { call, type CompletionInfo, type FieldDef, type TaskDetail, type CostKind, type ContractorInfo } from '../lib/api';
  import { app, toast } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import { COST_KINDS, money } from '../lib/money';
  import Attachments from './Attachments.svelte';

  let {
    completion,
    fields,
    onsaved,
    onclose,
  }: {
    completion: CompletionInfo;
    fields: FieldDef[];
    onsaved: (t: TaskDetail) => void;
    onclose: () => void;
  } = $props();

  let note = $state('');
  let costs = $state<Record<CostKind, string>>({ parts: '', labour: '', contractor: '', other: '' });
  let values = $state<Record<string, string>>({});
  let busy = $state(false);
  // Who did it (D-28): the household, or a contractor. Asked only here,
  // after Done (P1).
  let contractors = $state<ContractorInfo[]>([]);
  // svelte-ignore state_referenced_locally
  let who = $state(completion.contractor_id ?? '');
  let newName = $state('');
  $effect(() => {
    call('contractors')
      .then((l) => (contractors = l))
      .catch(() => (contractors = []));
  });
  const currency = $derived(app.status?.currency ?? 'USD');
  const prefix = $derived(`cd-${completion.id}`);

  async function save(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    try {
      const anyCost = Object.values(costs).some((v) => v.trim());
      let contractorId: string | undefined = who === 'new' ? undefined : who;
      if (who === 'new' && newName.trim()) {
        const list = await call('save_contractor', {
          name: newName.trim(),
          trades: [],
          notes_private: false,
          emergency: false,
        });
        contractorId = list.find((c) => c.name === newName.trim())?.id;
      }
      const t = await call('add_details', {
        completion_id: completion.id,
        note: note.trim() || undefined,
        costs: anyCost ? costs : undefined,
        values,
        contractor_id: contractorId === (completion.contractor_id ?? '') ? undefined : contractorId,
      });
      toast({ text: 'Saved with the record.' }, 3000);
      onsaved(t);
      onclose();
    } catch (err) {
      toast({ text: errorText(err) });
    } finally {
      busy = false;
    }
  }
  const human = (o: string) => o.replaceAll('_', ' ');
  /** Bring the form into view when it opens (it sits below the steps). */
  function reveal(node: HTMLElement) {
    node.scrollIntoView?.({ block: 'start', behavior: 'smooth' });
  }
</script>

<form class="details" onsubmit={save} data-testid="completion-details" use:reveal>
  <p class="muted small">All optional. The job is already recorded.</p>
  <h3>Photo</h3>
  <Attachments
    aboutKind="completion"
    aboutId={completion.id}
    items={completion.attachments}
    documents={false}
    addLabel="Add a photo"
    onchange={async () => {
      onsaved(await call('task', { id: completion.task_id }));
    }}
  />

  <h3>Who did it</h3>
  <label for={`${prefix}-who`} class="visually-hidden">Who did it</label>
  <select id={`${prefix}-who`} bind:value={who}>
    <option value="">We did (or not a contractor)</option>
    {#each contractors as c (c.id)}
      <option value={c.id}>{c.name}{c.company ? `, ${c.company}` : ''}</option>
    {/each}
    <option value="new">A contractor not listed yet…</option>
  </select>
  {#if who === 'new'}
    <label for={`${prefix}-who-new`}>Their name</label>
    <input id={`${prefix}-who-new`} bind:value={newName} autocomplete="off" />
    <p class="small muted">Add their phone, licence and insurance later on the Contractors page.</p>
  {/if}

  {#if fields.length}
    <h3>Readings</h3>
    {#each fields as f (f.key)}
      <label for={`${prefix}-${f.key}`}>{f.label}{f.type === 'number' && f.unit ? ` (${f.unit})` : ''}</label>
      {#if f.type === 'choice'}
        <select id={`${prefix}-${f.key}`} bind:value={values[f.key]}>
          <option value="">Skip</option>
          {#each f.options as o (o)}<option value={o}>{human(o)}</option>{/each}
        </select>
      {:else if f.type === 'yes_no'}
        <select id={`${prefix}-${f.key}`} bind:value={values[f.key]}>
          <option value="">Skip</option>
          <option value="yes">Yes</option>
          <option value="no">No</option>
        </select>
      {:else if f.type === 'date'}
        <input id={`${prefix}-${f.key}`} type="date" bind:value={values[f.key]} />
      {:else if f.type === 'number'}
        <input id={`${prefix}-${f.key}`} inputmode="decimal" bind:value={values[f.key]} />
      {:else if f.type !== 'photo'}
        <input id={`${prefix}-${f.key}`} bind:value={values[f.key]} />
      {/if}
    {/each}
  {/if}

  <h3>Cost</h3>
  {#if completion.cost_total !== null}
    <p class="small">Recorded: {money(completion.cost_total, currency)}. New amounts replace it.</p>
  {/if}
  <div class="costs">
    {#each COST_KINDS as [k, l] (k)}
      <div>
        <label for={`${prefix}-cost-${k}`}>{l}</label>
        <input id={`${prefix}-cost-${k}`} inputmode="decimal" placeholder="0.00" bind:value={costs[k]} />
      </div>
    {/each}
  </div>
  <p class="muted small">In {currency}. Change the currency in Settings.</p>

  <label for={`${prefix}-note`}>Note</label>
  <textarea id={`${prefix}-note`} bind:value={note} placeholder="Anything worth remembering"></textarea>
  <div class="row">
    <button class="primary" type="submit" disabled={busy}>Save</button>
    <button type="button" class="quiet" onclick={onclose}>Close</button>
  </div>
</form>

<style>
  .details h3 {
    font-size: 1rem;
    margin: 1rem 0 0.3rem;
  }
  .costs {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 0 0.75rem;
  }
</style>
