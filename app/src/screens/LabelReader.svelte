<script lang="ts" module>
  export interface LabelChoice {
    make: string;
    model: string;
    serial: string;
    template: { id: string; name: string } | null;
  }
</script>

<script lang="ts">
  // Read brand, model and serial from a photo of a rating plate (D-29),
  // on the device. Everything read is shown and can be corrected; nothing
  // is saved until the person says so.
  import { upload, type LabelRead } from '../lib/api';
  import { readFile } from '../lib/files';
  import { errorText } from '../lib/actions';

  let {
    onuse,
    showTemplate = false,
  }: { onuse: (c: LabelChoice) => void; showTemplate?: boolean } = $props();

  let busy = $state(false);
  let error = $state<string | null>(null);
  let read = $state<LabelRead | null>(null);
  let make = $state('');
  let model = $state('');
  let serial = $state('');
  let useTemplate = $state(true);

  async function pick(e: Event) {
    const input = e.target as HTMLInputElement;
    const f = input.files?.[0];
    input.value = '';
    if (!f) return;
    busy = true;
    error = null;
    read = null;
    try {
      const r = await upload('read_label', {}, await readFile(f));
      read = r;
      make = r.brand ?? '';
      model = r.model ?? '';
      serial = r.serial ?? '';
    } catch (err) {
      error = errorText(err);
    } finally {
      busy = false;
    }
  }

  function use() {
    onuse({
      make: make.trim(),
      model: model.trim(),
      serial: serial.trim(),
      template: useTemplate ? (read?.template ?? null) : null,
    });
    read = null;
  }
</script>

<div class="reader card" data-testid="label-reader">
  {#if !read}
    <p class="small muted">
      Take a photo of the label with the model and serial numbers (often inside a door or on the
      back). It is read on this device; nothing is sent anywhere.
    </p>
    <label class="button" for="label-photo">{busy ? 'Reading the label…' : 'Photograph the label'}</label>
    <input id="label-photo" class="hidden" type="file" accept="image/*" capture="environment" onchange={pick} disabled={busy} />
    {#if busy}<p class="small muted" role="status">This can take a few seconds.</p>{/if}
    {#if error}<p role="alert">{error}</p>{/if}
  {:else}
    <p><strong>Check what was read.</strong> Fix anything that is wrong; labels are hard to read and 0 and O look alike.</p>
    <label for="lr-make">Make</label>
    <input id="lr-make" bind:value={make} placeholder="Not found" />
    <label for="lr-model">Model</label>
    <input id="lr-model" bind:value={model} placeholder="Not found" />
    {#if read.model_alternatives.length}
      <p class="row small">Or: {#each read.model_alternatives as m (m)}<button type="button" class="quiet" onclick={() => (model = m)}>{m}</button>{/each}</p>
    {/if}
    <label for="lr-serial">Serial</label>
    <input id="lr-serial" bind:value={serial} placeholder="Not found" />
    {#if read.serial_alternatives.length}
      <p class="row small">Or: {#each read.serial_alternatives as m (m)}<button type="button" class="quiet" onclick={() => (serial = m)}>{m}</button>{/each}</p>
    {/if}
    {#if showTemplate && read.template}
      <label class="check"><input type="checkbox" bind:checked={useTemplate} /> It looks like a {read.template.name.toLowerCase()}: use that</label>
    {/if}
    <details>
      <summary>Everything the phone read ({read.lines.length} lines)</summary>
      <pre class="small">{read.lines.join('\n') || 'No text found.'}</pre>
    </details>
    <div class="row">
      <button type="button" class="primary" onclick={use}>Use these</button>
      <button type="button" class="quiet" onclick={() => (read = null)}>Try another photo</button>
    </div>
  {/if}
</div>

<style>
  .reader {
    margin: 0.5rem 0 1rem;
  }
  .hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    opacity: 0;
  }
  label.button {
    margin: 0.4rem 0;
  }
  pre {
    white-space: pre-wrap;
    background: var(--surface-2);
    padding: 0.5rem;
    border-radius: 8px;
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
  summary {
    min-height: var(--tap);
    display: flex;
    align-items: center;
    cursor: pointer;
    color: var(--accent);
  }
</style>
