<script lang="ts">
  // The insurance inventory (D-27): one PDF with photos and one spreadsheet
  // (CSV) of the things in this home, room by room, with values and
  // receipts. Sensitive things are left out unless the owner asks.
  import { call, type InsuranceInfo } from '../lib/api';
  import { app, toast } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import { money } from '../lib/money';
  import { openFile, saveFile } from '../lib/files';

  let info = $state<InsuranceInfo | null>(null);
  let error = $state<string | null>(null);
  let sensitive = $state(false);
  let busy = $state<'pdf' | 'csv' | null>(null);

  $effect(() => {
    void app.tick;
    call('insurance')
      .then((x) => (info = x))
      .catch((e) => (error = errorText(e)));
  });

  async function pdf() {
    if (!info) return;
    busy = 'pdf';
    try {
      await openFile('insurance_pdf', { include_sensitive: sensitive }, `${info.filename}.pdf`, 'application/pdf');
    } catch (e) {
      toast({ text: errorText(e) });
    } finally {
      busy = null;
    }
  }

  async function csv() {
    if (!info) return;
    busy = 'csv';
    try {
      await saveFile('insurance_csv', { include_sensitive: sensitive }, `${info.filename}.csv`, 'text/csv');
    } catch (e) {
      toast({ text: errorText(e) });
    } finally {
      busy = null;
    }
  }
</script>

<section aria-labelledby="i-h">
  <p><a href="#/home">← Home facts</a></p>
  <h1 id="i-h">Inventory for insurance</h1>
  {#if error}
    <p role="alert">{error}</p>
  {:else if !info}
    <p class="muted">Loading…</p>
  {:else}
    <p>
      A record of the things in {info.property_name} for your insurer: photos, make, model and serial, when they
      were bought, what they cost and what they are worth now, receipts and warranties. Useful to have before
      anything happens.
    </p>
    <ul class="small" data-testid="inventory-summary">
      <li>{info.things} things; {info.with_photo} with a photo, {info.with_value} with a value, {info.with_receipt} with a receipt.</li>
      {#if info.total_value}<li>Estimated value: {money(info.total_value, info.currency)}.</li>{/if}
      {#if info.total_price}<li>Paid when bought: {money(info.total_price, info.currency)}.</li>{/if}
    </ul>
    {#if info.with_value < info.things}
      <p class="small muted">Add a price or value on a thing's page (“Edit details”) to fill the totals.</p>
    {/if}

    {#if info.can_include_sensitive && (info.sensitive_things > 0 || info.sensitive_details > 0)}
      <div class="card">
        <label class="check">
          <input type="checkbox" bind:checked={sensitive} data-testid="include-sensitive" />
          Include sensitive items
        </label>
        <p class="small muted">
          {info.sensitive_things} {info.sensitive_things === 1 ? 'thing' : 'things'} and {info.sensitive_details}
          {info.sensitive_details === 1 ? 'detail' : 'details'} are marked sensitive (a safe, jewellery, a serial
          number) and are left out unless you tick this. Insurers do need serial numbers of valuables, but a copy
          with them says what you have and where it is: send it only to your insurer and keep it somewhere safe.
        </p>
      </div>
    {/if}

    <div class="row actions">
      <button class="primary" onclick={pdf} disabled={busy !== null} data-testid="insurance-pdf">
        {busy === 'pdf' ? 'Making the PDF…' : 'Make the PDF'}
      </button>
      <button onclick={csv} disabled={busy !== null} data-testid="insurance-csv">
        {busy === 'csv' ? 'Making the spreadsheet…' : 'Save as a spreadsheet (CSV)'}
      </button>
    </div>
    <p class="small muted">Values are your own estimates, not a valuation. Contractor details and private notes are never included.</p>
  {/if}
</section>

<style>
  .check {
    display: flex;
    gap: 0.6rem;
    align-items: center;
    font-weight: 600;
  }
  .check input {
    width: 1.4rem;
    min-height: 1.4rem;
  }
  .actions {
    flex-wrap: wrap;
    margin: 1rem 0;
  }
</style>
