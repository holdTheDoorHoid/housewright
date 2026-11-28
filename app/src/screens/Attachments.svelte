<script lang="ts">
  // Photos and documents on a thing, a completion or a fact (D-71). Photos
  // are shrunk on the device before they are kept; PDFs are kept as they
  // are and open in the phone's own viewer.
  import { upload, call, type AttachmentInfo, type AboutKind, type AttachmentKind } from '../lib/api';
  import { attachmentUrl, openFile, readFile, sizeText } from '../lib/files';
  import { toast } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';

  let {
    aboutKind,
    aboutId,
    items,
    documents = true,
    addLabel = 'Add a photo',
    onchange,
  }: {
    aboutKind: AboutKind;
    aboutId: string;
    items: AttachmentInfo[];
    documents?: boolean;
    addLabel?: string;
    onchange: () => void;
  } = $props();

  const KINDS: [AttachmentKind, string][] = [
    ['photo', 'Photo'],
    ['manual', 'Manual'],
    ['receipt', 'Receipt'],
    ['warranty', 'Warranty'],
    ['other', 'Other'],
  ];

  let busy = $state(false);
  let kind = $state<AttachmentKind | ''>('');
  let viewing = $state<{ a: AttachmentInfo; url: string } | null>(null);
  let thumbs = $state<Record<string, string>>({});
  const inputId = $derived(`att-${aboutKind}-${aboutId}`);

  $effect(() => {
    for (const a of items) {
      if (a.has_thumb && !thumbs[a.id]) {
        attachmentUrl(a.id, a.media_type, true).then((u) => (thumbs[a.id] = u)).catch(() => {});
      }
    }
  });

  async function add(e: Event) {
    const input = e.target as HTMLInputElement;
    const f = input.files?.[0];
    if (!f) return;
    busy = true;
    try {
      const bytes = await readFile(f);
      await upload(
        'attach',
        { about_kind: aboutKind, about_id: aboutId, name: f.name, kind: kind || undefined },
        bytes,
      );
      toast({ text: f.type === 'application/pdf' ? 'Document added.' : 'Photo added.' }, 3000);
      onchange();
    } catch (err) {
      toast({ text: errorText(err) });
    } finally {
      busy = false;
      input.value = '';
    }
  }

  async function show(a: AttachmentInfo) {
    try {
      if (a.media_type.startsWith('image/')) {
        viewing = { a, url: await attachmentUrl(a.id, a.media_type) };
      } else {
        await openFile('attachment_file', { id: a.id }, a.name, a.media_type);
      }
    } catch (err) {
      toast({ text: errorText(err) });
    }
  }

  async function remove(a: AttachmentInfo) {
    if (!confirm(`Delete “${a.name}”? This cannot be undone.`)) return;
    try {
      await call('delete_attachment', { id: a.id });
      viewing = null;
      onchange();
    } catch (err) {
      toast({ text: errorText(err) });
    }
  }

  const kindLabel = (k: AttachmentKind) => KINDS.find(([x]) => x === k)?.[1] ?? k;
</script>

<div class="atts">
  {#if items.length}
    <ul class="grid" aria-label="Photos and documents">
      {#each items as a (a.id)}
        <li>
          <button class="tile" onclick={() => show(a)} aria-label={`Open ${a.name}`}>
            {#if a.has_thumb && thumbs[a.id]}
              <img src={thumbs[a.id]} alt={a.name} data-testid="thumb" />
            {:else if a.media_type === 'application/pdf'}
              <span class="doc" aria-hidden="true">PDF</span>
            {:else}
              <span class="doc" aria-hidden="true">…</span>
            {/if}
            <span class="cap small">{a.kind === 'photo' ? '' : kindLabel(a.kind)}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
  <div class="row add">
    <label class="button" for={inputId} aria-disabled={busy}>{busy ? 'Saving…' : addLabel}</label>
    <input
      id={inputId}
      class="hidden"
      type="file"
      accept={documents ? 'image/*,application/pdf' : 'image/*'}
      onchange={add}
      disabled={busy}
    />
    {#if documents}
      <select bind:value={kind} aria-label="What is it?">
        <option value="">Photo or document</option>
        {#each KINDS as [k, l] (k)}<option value={k}>{l}</option>{/each}
      </select>
    {/if}
  </div>
</div>

{#if viewing}
  <div class="viewer" role="dialog" aria-modal="true" aria-label={viewing.a.name}>
    <img src={viewing.url} alt={viewing.a.name} />
    <div class="row bar">
      <span class="small grow">{viewing.a.name} · {sizeText(viewing.a.size)}</span>
      <button class="quiet" onclick={() => viewing && remove(viewing.a)}>Delete</button>
      <button class="primary" onclick={() => (viewing = null)}>Close</button>
    </div>
  </div>
{/if}

<style>
  .grid {
    list-style: none;
    padding: 0;
    margin: 0.5rem 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(88px, 1fr));
    gap: 0.5rem;
  }
  .tile {
    width: 100%;
    aspect-ratio: 1;
    padding: 0;
    overflow: hidden;
    flex-direction: column;
    position: relative;
  }
  .tile img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .doc {
    font-weight: 700;
    color: var(--muted);
  }
  .cap {
    position: absolute;
    bottom: 2px;
    left: 4px;
    color: var(--muted);
  }
  .hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    opacity: 0;
  }
  .add select {
    width: auto;
    flex: 1;
  }
  .add label {
    margin: 0;
  }
  .viewer {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.92);
    display: flex;
    flex-direction: column;
    z-index: 10;
    padding: env(safe-area-inset-top) 0 env(safe-area-inset-bottom);
  }
  .viewer img {
    flex: 1;
    min-height: 0;
    object-fit: contain;
    width: 100%;
  }
  .bar {
    padding: 0.5rem 1rem;
    color: #fff;
  }
  .bar .quiet {
    color: #fff;
  }
  .grow {
    flex: 1;
  }
</style>
