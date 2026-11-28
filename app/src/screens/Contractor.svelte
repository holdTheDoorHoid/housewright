<script lang="ts">
  // One contractor's page: call or email them, their licence and insurance
  // (with the certificate attached), the jobs they did and what they cost.
  import { call, type ContractorPage } from '../lib/api';
  import { app, go, toast } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import { money, COST_KINDS } from '../lib/money';
  import { telHref, mailHref, webHref, openExternal } from '../lib/links';
  import { shortDate as date } from '../lib/format';
  import Attachments from './Attachments.svelte';
  import ContractorForm from './ContractorForm.svelte';

  let { id }: { id: string } = $props();
  let page = $state<ContractorPage | null>(null);
  let error = $state<string | null>(null);
  let editing = $state(false);
  let tick = $state(0);

  $effect(() => {
    void app.tick;
    void tick;
    call('contractor', { id })
      .then((p) => (page = p))
      .catch((e) => (error = errorText(e)));
  });

  const c = $derived(page?.contractor);
  const tel = $derived(telHref(c?.phone));
  const mail = $derived(mailHref(c?.email));
  const web = $derived(webHref(c?.website));

  async function open(href: string) {
    try {
      await openExternal(href);
    } catch (e) {
      toast({ text: errorText(e) });
    }
  }

  async function remove() {
    if (!c?.id) return;
    if (!confirm(`Remove ${c.name}? The jobs they did stay in the history.`)) return;
    await call('delete_contractor', { id: c.id });
    go('#/contractors');
  }

  const STATE: Record<string, string> = {
    ok: 'Current',
    ending_soon: 'Ends soon',
    ended: 'Ended',
    no_date: 'No end date recorded',
  };
  const kindLabel = (k: string) => COST_KINDS.find(([x]) => x === k)?.[1] ?? k;
</script>

<section aria-labelledby="kp-h">
  <p><a href="#/contractors">← Contractors</a></p>
  {#if error}
    <p role="alert">{error}</p>
  {:else if !page || !c}
    <p class="muted">Loading…</p>
  {:else if editing}
    <h1 id="kp-h">Edit {c.name}</h1>
    <ContractorForm
      start={c}
      onsaved={() => {
        editing = false;
        tick++;
      }}
      oncancel={() => (editing = false)}
    />
  {:else}
    <h1 id="kp-h">{c.name}</h1>
    {#if c.company}<p class="muted">{c.company}</p>{/if}
    {#if c.trades.length}<p>{c.trades.join(', ')}</p>{/if}

    <div class="row actions">
      {#if tel}<button class="primary" onclick={() => open(tel)} data-testid="call">Call {c.phone}</button>{/if}
      {#if mail}<button onclick={() => open(mail)} data-testid="email">Email</button>{/if}
      {#if web}<button class="quiet" onclick={() => open(web)}>Website</button>{/if}
      <button class="quiet" onclick={() => (editing = true)}>Edit</button>
    </div>
    {#if c.service_area}<p class="small muted">Works in: {c.service_area}</p>{/if}
    {#if c.emergency}<p class="small"><span class="pill">Emergency</span> Listed on the binder's first page.</p>{/if}

    <h2>Licence and insurance</h2>
    <dl class="pairs">
      <dt>Licence</dt>
      <dd data-testid="licence">
        {#if c.licence}
          {c.licence.number ?? 'Hidden'}{#if c.licence.expires}, ends {date(c.licence.expires)}{/if}
          <span class="pill" class:attention={c.licence.state === 'ended' || c.licence.state === 'ending_soon'}>{STATE[c.licence.state]}</span>
        {:else}<span class="muted">Not recorded</span>{/if}
      </dd>
      <dt>Insurance</dt>
      <dd data-testid="insurance">
        {#if c.insurance}
          {#if c.insurance.number}{c.insurance.number}{#if c.insurance.expires}, {/if}{/if}{#if c.insurance.expires}ends {date(c.insurance.expires)}{/if}
          <span class="pill" class:attention={c.insurance.state === 'ended' || c.insurance.state === 'ending_soon'}>{STATE[c.insurance.state]}</span>
        {:else}<span class="muted">Not recorded</span>{/if}
      </dd>
    </dl>
    {#if (c.licence?.state === 'ending_soon' || c.licence?.state === 'ended' || c.insurance?.state === 'ending_soon' || c.insurance?.state === 'ended')}
      <p class="small">Before they next work on the house, ask for the renewed {c.licence?.state === 'ending_soon' || c.licence?.state === 'ended' ? 'licence' : 'insurance certificate'} and update it here.</p>
    {/if}
    <h3 class="small">Certificate and other papers</h3>
    <Attachments aboutKind="contractor" aboutId={c.id ?? id} items={page.attachments} addLabel="Attach the insurance certificate" onchange={() => tick++} />

    {#if c.notes}
      <h2>Notes</h2>
      <p>{c.notes}{#if c.notes_private} <span class="pill">Kept out of the main binder</span>{/if}</p>
    {/if}

    {#if c.private}
      <section class="card private" aria-labelledby="kp-private" data-testid="private">
        <h2 id="kp-private">Just for you <span class="pill">Private</span></h2>
        <p class="small muted">Never printed or shared outside your household.</p>
        <p>
          Your rating: <strong>{c.private.rating ? `${c.private.rating} of 5` : 'not rated'}</strong>.
          Hire again: <strong>{c.private.would_hire_again === true ? 'yes' : c.private.would_hire_again === false ? 'no' : 'not sure yet'}</strong>.
        </p>
        {#if c.private.notes}<p>{c.private.notes}</p>{/if}
      </section>
    {/if}

    <h2>Jobs</h2>
    {#if page.jobs.length === 0}
      <p class="muted small">
        None yet. After you mark a job done, choose “Add details”, then “Who did it”, to link it here.
      </p>
    {:else}
      <p data-testid="contractor-total">
        {page.jobs.length} {page.jobs.length === 1 ? 'job' : 'jobs'}, <strong>{money(c.spent ?? 0, page.currency)}</strong> in all.
      </p>
      <ul class="stack jobs">
        {#each page.jobs as j (j.completion_id)}
          <li class="card" data-testid="job">
            <div class="row">
              <a class="grow" href={`#/task/${j.task_id}`}>{j.task_title}</a>
              <strong>{j.total ? money(j.total, page.currency) : ''}</strong>
            </div>
            <div class="small muted">
              {date(j.at.slice(0, 10))}{#if j.item_name} · {#if j.item_id}<a href={`#/thing/${j.item_id}`}>{j.item_name}</a>{:else}{j.item_name}{/if}{/if}{#if j.room_name} · {j.room_name}{/if}
            </div>
            {#if j.costs.length > 1}
              <div class="small muted">{j.costs.map((l) => `${kindLabel(l.kind)} ${money(l.minor, l.currency)}`).join(', ')}</div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}

    <p><button class="quiet" onclick={remove}>Remove this contractor</button></p>
  {/if}
</section>

<style>
  .actions {
    flex-wrap: wrap;
    margin: 0.75rem 0;
  }
  .pairs {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.4rem 1rem;
  }
  .pairs dt {
    color: var(--muted);
  }
  .pairs dd {
    margin: 0;
  }
  .private {
    background: var(--surface-2);
    margin: 1rem 0;
  }
  .private h2 {
    margin-top: 0;
  }
  .jobs {
    list-style: none;
    padding: 0;
  }
  .grow {
    flex: 1;
  }
</style>
