<script lang="ts">
  // A suggestion from the appliance library (D-148): which model family a
  // make and model look like, how sure we are, and why. Nothing is applied
  // until the person says so (P2).
  import type { FamilyHit, LibraryMatch } from '../lib/api';

  let {
    match,
    onuse,
    onnone,
    heading = 'From the appliance library',
  }: {
    match: LibraryMatch;
    onuse: (h: FamilyHit) => void;
    onnone?: () => void;
    heading?: string;
  } = $props();

  let more = $state(false);

  const LEVEL: Record<string, string> = {
    model: 'The model number matches',
    possible: 'Possible match: check it',
    brand: 'Same brand: is it one of these?',
  };

  const top = $derived(match.families[0]);
  /** "Rheem Professional Classic ..." even when the family's name leaves the brand out. */
  const full = (h: FamilyHit) =>
    h.name.toLowerCase().startsWith(h.brand_name.toLowerCase()) ? h.name : `${h.brand_name} ${h.name}`;
  const brandOnly = $derived(match.families.every((f) => f.confidence === 'brand'));
  const rest = $derived(brandOnly ? match.families : match.families.slice(1));
</script>

{#if top}
  <div class="card suggest" data-testid="family-suggestion">
    <p class="small muted">{heading}</p>
    {#if !brandOnly}
      <p class="small level" class:strong={top.confidence === 'model'}>{LEVEL[top.confidence]}</p>
      <h3>{full(top)}</h3>
      {#if top.template}<p class="small muted">A kind of {top.template.name.toLowerCase()}</p>{/if}
      <ul class="small why" aria-label="Why it matched">
        {#each top.reasons as r, i (i)}<li>{r}</li>{/each}
      </ul>
      {#if top.corrected_model}<p class="small">If you use it, check the model number: it may be <strong>{top.corrected_model}</strong>.</p>{/if}
      <div class="row">
        <button type="button" class="primary" onclick={() => onuse(top)} data-testid="use-family">Yes, use this</button>
        {#if onnone}<button type="button" class="quiet" onclick={onnone} data-testid="no-family">None of these</button>{/if}
      </div>
      {#if rest.length}
        <button type="button" class="quiet small" onclick={() => (more = !more)}>{more ? 'Hide' : 'Show'} {rest.length} other {rest.length === 1 ? 'possibility' : 'possibilities'}</button>
      {/if}
    {:else}
      <p class="small level">{LEVEL.brand}</p>
      <ul class="small why" aria-label="Why these">
        {#each top.reasons as r, i (i)}<li>{r}</li>{/each}
      </ul>
    {/if}
    {#if brandOnly || more}
      <ul class="list stack">
        {#each rest as h (h.id)}
          <li class="row">
            <span class="grow">{full(h)}{#if h.unverified} <span class="pill">not confirmed</span>{/if}</span>
            <button type="button" onclick={() => onuse(h)}>This one</button>
          </li>
        {/each}
      </ul>
      {#if brandOnly && onnone}<button type="button" class="quiet" onclick={onnone} data-testid="no-family">None of these</button>{/if}
    {/if}
  </div>
{/if}

<style>
  .suggest {
    margin: 0.5rem 0 1rem;
    background: var(--accent-soft);
  }
  h3 {
    margin: 0.2rem 0;
    font-size: 1.05rem;
  }
  .level {
    margin: 0;
  }
  .strong {
    font-weight: 600;
  }
  .why {
    margin: 0.3rem 0;
    padding-left: 1.2rem;
  }
  .list {
    list-style: none;
    padding: 0;
  }
  .grow {
    flex: 1;
  }
</style>
