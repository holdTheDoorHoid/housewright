<script lang="ts">
  // Find a thing from its sticker (D-30): a photo of the QR code, read on
  // this device, or the short code typed in. The phone's own camera app
  // also opens stickers directly (housewright://item/...).
  import { call, upload, type ItemRow } from '../lib/api';
  import { go } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import { readFile } from '../lib/files';

  let code = $state('');
  let busy = $state(false);
  let message = $state<string | null>(null);
  let matches = $state<ItemRow[]>([]);

  function found(items: ItemRow[], none: string) {
    if (items.length === 1) go(`#/thing/${items[0].id}`);
    else if (items.length > 1) {
      matches = items;
      message = 'More than one thing has this code. Which one?';
    } else message = none;
  }

  async function photo(e: Event) {
    const input = e.target as HTMLInputElement;
    const f = input.files?.[0];
    input.value = '';
    if (!f) return;
    busy = true;
    message = null;
    matches = [];
    try {
      const r = await upload('scan_label', {}, await readFile(f));
      found(
        r.items,
        r.texts.length
          ? 'That code is not one of this home’s stickers.'
          : 'No sticker code found in that photo. Try closer, with the whole square in view.',
      );
    } catch (err) {
      message = errorText(err);
    } finally {
      busy = false;
    }
  }

  async function typed(e: SubmitEvent) {
    e.preventDefault();
    message = null;
    matches = [];
    try {
      found(await call('find_label', { text: code }), 'No sticker with that code. Codes look like HW-7K3QM.');
    } catch (err) {
      message = errorText(err);
    }
  }
</script>

<section aria-labelledby="scan-h">
  <p><a href="#/things">← Things</a></p>
  <h1 id="scan-h">Scan a sticker</h1>
  <p class="muted small">The photo is read on this device.</p>
  <label class="button primary" for="scan-photo">{busy ? 'Looking for the code…' : 'Take or choose a photo of the sticker'}</label>
  <input id="scan-photo" class="hidden" type="file" accept="image/*" capture="environment" onchange={photo} disabled={busy} />

  <form onsubmit={typed}>
    <label for="code">Or type the code under it</label>
    <div class="row">
      <input id="code" bind:value={code} placeholder="HW-7K3QM" autocapitalize="characters" autocomplete="off" />
      <button type="submit" disabled={!code.trim()}>Find</button>
    </div>
  </form>
  {#if message}<p role="status">{message}</p>{/if}
  {#if matches.length}
    <ul class="stack list">
      {#each matches as m (m.id)}
        <li><a class="card item" href={`#/thing/${m.id}`}>{m.name}{m.room_name ? ` · ${m.room_name}` : ''}</a></li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    opacity: 0;
  }
  label.button {
    margin: 1rem 0;
    width: 100%;
  }
  .row input {
    flex: 1;
  }
  .list {
    list-style: none;
    padding: 0;
  }
  .item {
    display: flex;
    min-height: var(--tap);
    align-items: center;
    color: inherit;
    text-decoration: none;
  }
</style>
