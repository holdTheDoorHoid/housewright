<script lang="ts">
  // The people who work on the house (D-28): who they are, what they do,
  // whether their licence and insurance are current, and what they did.
  import { call, type ContractorInfo } from '../lib/api';
  import { app, go } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import { money } from '../lib/money';
  import ContractorForm from './ContractorForm.svelte';

  let list = $state<ContractorInfo[] | null>(null);
  let error = $state<string | null>(null);
  let adding = $state(false);
  let filter = $state('');

  $effect(() => {
    void app.tick;
    call('contractors')
      .then((l) => (list = l))
      .catch((e) => (error = errorText(e)));
  });

  const shown = $derived(
    (list ?? []).filter((c) => {
      const f = filter.trim().toLowerCase();
      if (!f) return true;
      return [c.name, c.company ?? '', ...c.trades].some((x) => x.toLowerCase().includes(f));
    }),
  );
  const currency = $derived(app.status?.currency ?? 'USD');

  function credentialNote(c: ContractorInfo): string | null {
    const bad = [
      c.licence?.state === 'ended' ? 'licence ended' : c.licence?.state === 'ending_soon' ? 'licence ends soon' : null,
      c.insurance?.state === 'ended'
        ? 'insurance ended'
        : c.insurance?.state === 'ending_soon'
          ? 'insurance ends soon'
          : null,
    ].filter(Boolean);
    return bad.length ? bad.join(', ') : null;
  }
</script>

<section aria-labelledby="k-h">
  <p><a href="#/home">← Home facts</a></p>
  <h1 id="k-h">Contractors</h1>
  {#if error}
    <p role="alert">{error}</p>
  {:else if !list}
    <p class="muted">Loading…</p>
  {:else}
    {#if adding}
      <ContractorForm
        onsaved={(l, name) => {
          list = l;
          adding = false;
          const added = l.find((c) => c.name === name);
          if (added?.id) go(`#/contractor/${added.id}`);
        }}
        oncancel={() => (adding = false)}
      />
    {:else}
      <p class="row">
        <button class="primary" onclick={() => (adding = true)}>Add a contractor</button>
      </p>
    {/if}
    {#if list.length === 0}
      <p class="muted">
        Nobody yet. Add the plumber, electrician or handyman you call, with their licence and insurance
        if you have them. After a job is done, “Who did it” links it to them, so you can see what they
        did and what it cost.
      </p>
    {:else}
      {#if list.length > 6}
        <label for="k-filter" class="visually-hidden">Find a contractor</label>
        <input id="k-filter" bind:value={filter} placeholder="Find by name or trade" />
      {/if}
      <ul class="stack list">
        {#each shown as c (c.id)}
          {@const warn = credentialNote(c)}
          <li>
            <a class="card item" href={`#/contractor/${c.id}`} data-testid="contractor-row">
              <div class="grow">
                <strong>{c.name}</strong>{#if c.company}<span class="muted"> · {c.company}</span>{/if}
                {#if c.trades.length}<div class="small muted">{c.trades.join(', ')}</div>{/if}
                <div class="small">
                  {c.jobs ? `${c.jobs} ${c.jobs === 1 ? 'job' : 'jobs'}, ${money(c.spent ?? 0, currency)}` : 'No jobs recorded'}
                  {#if c.emergency}<span class="pill">Emergency</span>{/if}
                  {#if warn}<span class="pill attention">{warn}</span>{/if}
                </div>
              </div>
              {#if c.private?.rating}<span class="small muted" aria-label={`Your rating: ${c.private.rating} of 5`}>{c.private.rating}/5</span>{/if}
            </a>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  .list {
    list-style: none;
    padding: 0;
    margin: 1rem 0 0;
  }
  .item {
    display: flex;
    gap: 0.75rem;
    align-items: center;
    color: inherit;
    text-decoration: none;
  }
  .grow {
    flex: 1;
  }
  .pill {
    margin-left: 0.4rem;
  }
</style>
