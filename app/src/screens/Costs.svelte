<script lang="ts">
  // What maintenance cost (D-27): totals by year, by thing and by home.
  // Costs are added after Done, never required (P1).
  import { call, type CostSummary, type CostRow } from '../lib/api';
  import { app } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import { money } from '../lib/money';

  let s = $state<CostSummary | null>(null);
  let error = $state<string | null>(null);

  $effect(() => {
    void app.tick;
    call('costs')
      .then((x) => (s = x))
      .catch((e) => (error = errorText(e)));
  });
</script>

{#snippet table(caption: string, rows: CostRow<unknown>[], first: string)}
  <table>
    <caption>{caption}</caption>
    <thead>
      <tr><th scope="col">{first}</th><th scope="col">Parts</th><th scope="col">Labour</th><th scope="col">Contractor</th><th scope="col">Total</th></tr>
    </thead>
    <tbody>
      {#each rows as r (String(r.key))}
        <tr>
          <th scope="row">{r.label}</th>
          <td>{money(r.split.parts, s?.currency)}</td>
          <td>{money(r.split.labour, s?.currency)}</td>
          <td>{money(r.split.contractor, s?.currency)}</td>
          <td><strong>{money(r.split.total, s?.currency)}</strong></td>
        </tr>
      {/each}
    </tbody>
  </table>
{/snippet}

<section aria-labelledby="costs-h">
  <p><a href="#/home">← Home facts</a></p>
  <h1 id="costs-h">Costs</h1>
  {#if error}
    <p role="alert">{error}</p>
  {:else if !s}
    <p class="muted">Loading…</p>
  {:else if s.total.total === 0}
    <p class="muted">
      No costs recorded yet. After you mark a job done, choose “Add details” to note what parts,
      labour or a contractor cost. It is always optional.
    </p>
  {:else}
    <p data-testid="cost-total">Everything so far: <strong>{money(s.total.total, s.currency)}</strong></p>
    {#if s.total.other}<p class="muted small">Amounts marked “other” ({money(s.total.other, s.currency)}) are in the totals.</p>{/if}
    {@render table('By year', s.by_year, 'Year')}
    {@render table('By thing', s.by_item, 'Thing')}
    {#if s.by_property.length > 1}{@render table('By home', s.by_property, 'Home')}{/if}
    {#if s.other_currency_lines}
      <p class="muted small">{s.other_currency_lines} amounts in another currency are not in these totals.</p>
    {/if}
  {/if}
</section>

<style>
  table {
    width: 100%;
    border-collapse: collapse;
    margin: 1rem 0 1.5rem;
    font-size: 0.92rem;
    display: block;
    overflow-x: auto;
  }
  caption {
    text-align: left;
    font-weight: 650;
    padding-bottom: 0.4rem;
  }
  th,
  td {
    padding: 0.4rem 0.5rem;
    border-bottom: 1px solid var(--line);
    text-align: right;
    white-space: nowrap;
  }
  th[scope='row'],
  thead th:first-child {
    text-align: left;
    white-space: normal;
  }
</style>
