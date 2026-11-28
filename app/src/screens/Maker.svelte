<script lang="ts">
  // "From <Brand>" on a thing's page (D-149 to D-151): what the appliance
  // library knows about its model family. Official links open outside the
  // app; facts are in our own words, each with its source and when it was
  // checked; parts are listed as data; where the maker's advice for a task
  // differs from the usual, both are shown and the person chooses (P2).
  import { call, upload, type FamilyHit, type Follows, type ItemDetail, type MakerManual } from '../lib/api';
  import { toast } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import { openExternal } from '../lib/links';
  import { monthYear } from '../lib/format';
  import { canSaveManual, saveManual } from '../lib/library';
  import { readFile } from '../lib/files';
  import FamilySuggestion from './FamilySuggestion.svelte';

  let { d, onchange }: { d: ItemDetail; onchange: (d: ItemDetail | null) => void } = $props();

  let saving = $state<string | null>(null);
  let webFallback = $state<MakerManual | null>(null);
  let attaching = $state(false);

  const m = $derived(d.maker);
  const f = $derived(m?.family ?? null);

  async function setFamily(family: string | null) {
    try {
      onchange(await call('set_family', { item_id: d.row.id, family }));
      if (family && family !== 'none') toast({ text: 'Linked. The manufacturer’s details are below.' }, 3500);
    } catch (e) {
      toast({ text: errorText(e) });
    }
  }

  async function use(h: FamilyHit) {
    await setFamily(h.id);
  }

  async function choose(taskId: string, follow: Follows) {
    try {
      onchange(await call('choose_guidance', { task_id: taskId, follow }));
      toast({ text: follow === 'maker' ? 'This task now follows the manufacturer.' : 'This task keeps the usual advice.' }, 3000);
    } catch (e) {
      toast({ text: errorText(e) });
    }
  }

  async function open(url: string) {
    try {
      await openExternal(url);
    } catch (e) {
      toast({ text: errorText(e) });
    }
  }

  async function save(man: MakerManual) {
    if (!canSaveManual()) {
      // The browser's cross-site rules stop a web page from downloading a
      // file from another company's site, so open the page instead.
      webFallback = man;
      await open(man.url);
      return;
    }
    saving = man.url;
    try {
      await saveManual(d.row.id, man.url);
      toast({ text: 'Saved the manual with this thing. It stays on your devices.' }, 4000);
      onchange(null);
    } catch (e) {
      toast({ text: errorText(e) });
    } finally {
      saving = null;
    }
  }

  async function attachDownloaded(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    attaching = true;
    try {
      await upload('attach', { about_kind: 'item', about_id: d.row.id, kind: 'manual', name: file.name }, await readFile(file));
      toast({ text: 'Manual added to this thing.' }, 3000);
      webFallback = null;
      onchange(null);
    } catch (err) {
      toast({ text: errorText(err) });
    } finally {
      attaching = false;
    }
  }

  const PAGE: Record<string, string> = {
    'manual-pdf': 'Manual (PDF)',
    'product-page': 'Product page',
    'support-page': 'Support page',
    'manual-search': 'Find your manual by model',
    'parts-page': 'Parts',
  };
</script>

{#snippet source(url: string, checked: string | null, unverified: boolean)}
  <span class="small muted">
    <button type="button" class="quiet link" onclick={() => open(url)}>Source</button>{#if checked}, checked {monthYear(checked)}{/if}{#if unverified}
      <span class="pill">not confirmed</span>{/if}
  </span>
{/snippet}

{#if !m && d.family_suggestion}
  <FamilySuggestion
    match={d.family_suggestion}
    heading={`About your ${d.make ?? ''} ${d.model ?? ''}`.trim()}
    onuse={use}
    onnone={() => setFamily('none')}
  />
{:else if m && !f}
  <p class="small muted" data-testid="maker-gone">
    The appliance library no longer lists the model family this was linked to.
    <button type="button" class="quiet" onclick={() => setFamily(null)}>Unlink it</button>
  </p>
{:else if m && f}
  <section class="maker" data-testid="maker" aria-labelledby="maker-h">
    <h2 id="maker-h">From {f.brand_name}</h2>
    <p class="small">
      <strong>{f.name}</strong>{#if f.years} · {f.years}{/if}{#if f.status === 'discontinued'} · no longer made{/if}
      {#if f.unverified}<span class="pill">not confirmed</span>{/if}
    </p>
    {#if f.description}<p class="small muted">{f.description}</p>{/if}
    {#if m.other_kind}
      <p class="small pill attention">This family is a different kind of thing from this one, so its task advice is not offered.</p>
    {/if}

    <h3>Official manual and support</h3>
    <ul class="list stack" data-testid="maker-links">
      {#each f.manuals as man (man.url)}
        <li class="row">
          <button type="button" class="quiet link grow" onclick={() => open(man.url)}>{PAGE[man.kind] ?? 'Page'}: {man.title}</button>
          {#if man.pdf}
            <button type="button" onclick={() => save(man)} disabled={saving !== null} data-testid="save-manual">
              {saving === man.url ? 'Saving…' : canSaveManual() ? 'Save the official manual to this thing' : 'Open the official manual'}
            </button>
          {/if}
        </li>
      {/each}
      {#if f.manual_search_url}
        <li><button type="button" class="quiet link" onclick={() => f?.manual_search_url && open(f.manual_search_url)}>Find a manual by model number on {f.brand_name}’s site</button></li>
      {/if}
      <li>
        <button type="button" class="quiet link" onclick={() => f && open(f.support_url)}>{f.brand_name} support</button>{#if f.phone}
          <span class="small"> · {f.phone}</span>{/if}
      </li>
    </ul>
    {#if webFallback}
      <div class="card offer" data-testid="manual-web-fallback">
        <p class="small">
          A web page cannot download a file from {f.brand_name}’s site for you; browsers do not allow it. The manual
          has opened in a new tab: save it there, then add it here. On the Android app, Housewright saves it for you.
        </p>
        <label class="button" for="manual-file">{attaching ? 'Adding…' : 'Add the PDF you saved'}</label>
        <input id="manual-file" class="hidden" type="file" accept="application/pdf" onchange={attachDownloaded} />
        <button type="button" class="quiet" onclick={() => (webFallback = null)}>Not now</button>
      </div>
    {/if}
    {#if !f.manuals.some((x) => x.pdf)}
      <p class="small muted">Manuals are kept on the manufacturer’s site. Open the page, and if you save the PDF you can add it below under Photos and documents.</p>
    {/if}

    {#if m.guidance.length}
      <h3>Where {f.brand_name}’s advice differs</h3>
      <ul class="list stack" data-testid="guidance">
        {#each m.guidance as g (g.task_id)}
          <li class="card">
            <p><a href={`#/task/${g.task_id}`}><strong>{g.task_title}</strong></a></p>
            <dl class="compare small">
              <dt>{f.brand_name}</dt><dd>{g.maker}{#if g.maker_title} · {g.maker_title}{/if}</dd>
              <dt>Usual advice</dt><dd>{g.generic} <span class="muted">({g.generic_source})</span></dd>
            </dl>
            {#if g.note}<p class="small">{g.note}</p>{/if}
            <p>{@render source(g.source, g.checked, g.unverified)}</p>
            {#if g.following === 'maker'}
              <p class="small"><strong>Following {f.brand_name}’s advice.</strong> <button type="button" class="quiet" onclick={() => choose(g.task_id, 'generic')}>Use the usual advice instead</button></p>
            {:else if g.following === 'generic'}
              <p class="small"><strong>Following the usual advice.</strong> <button type="button" class="quiet" onclick={() => choose(g.task_id, 'maker')}>Use {f.brand_name}’s instead</button></p>
            {:else}
              <div class="row">
                <button type="button" class="primary" onclick={() => choose(g.task_id, 'maker')} data-testid="follow-maker">Use {f.brand_name}’s</button>
                <button type="button" onclick={() => choose(g.task_id, 'generic')} data-testid="follow-generic">Keep the usual</button>
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}

    {#if f.facts.length}
      <h3>Good to know</h3>
      <ul class="facts" data-testid="maker-facts">
        {#each f.facts as x, i (i)}
          <li>{x.text} {@render source(x.source, x.checked, x.unverified)}</li>
        {/each}
      </ul>
    {/if}

    {#if f.consumables.length}
      <h3>Parts and supplies</h3>
      <ul class="list stack" data-testid="maker-parts">
        {#each f.consumables as c, i (i)}
          <li>
            <strong>{c.name}</strong>
            {#if c.part}<span> · part {c.part}</span>{/if}{#if c.alternates.length}<span class="small muted"> (or {c.alternates.join(', ')})</span>{/if}
            {#if c.size}<span> · size {c.size}</span>{/if}
            {#if c.replace_every}<span> · replace every {c.replace_every}</span>{/if}
            {#if c.task_title}<span class="small muted"> · for “{c.task_title}”</span>{/if}
            <div>{@render source(c.source, c.checked, c.unverified)}</div>
          </li>
        {/each}
      </ul>
    {/if}

    {#if f.warranty}
      <h3>Warranty</h3>
      <p class="small">{f.warranty.summary}{#if f.warranty.length} ({f.warranty.length}){/if} {@render source(f.warranty.source, f.warranty.checked, f.warranty.unverified)}</p>
      <p class="small muted">Your own warranty details are under Edit details.</p>
    {/if}

    {#if f.recalls.length}
      <h3>Recalls for this family</h3>
      <ul class="list stack">
        {#each f.recalls as r (r.url)}
          <li class="small">{r.title}{#if r.date} ({monthYear(r.date)}){/if} · <button type="button" class="quiet link" onclick={() => open(r.url)}>Official notice</button>
            <span class="muted">Check whether your exact model is listed.</span></li>
        {/each}
      </ul>
    {/if}

    <p class="small muted credit">
      {f.credit} Facts are written in our own words; follow the label and manual on your own appliance, and book a
      professional for gas and electrical work.
      <button type="button" class="quiet" onclick={() => setFamily(null)} data-testid="unlink-family">Not this model? Unlink it</button>
    </p>
  </section>
{/if}

<style>
  .maker h3 {
    font-size: 1rem;
    margin: 1rem 0 0.3rem;
  }
  .list {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .grow {
    flex: 1;
    text-align: left;
  }
  .link {
    min-height: 0;
    padding: 0;
    text-align: left;
    justify-content: flex-start;
  }
  .facts {
    padding-left: 1.2rem;
    margin: 0;
  }
  .facts li {
    margin-bottom: 0.4rem;
  }
  .compare {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.2rem 0.8rem;
    margin: 0.3rem 0;
  }
  .compare dt {
    color: var(--muted);
  }
  .compare dd {
    margin: 0;
  }
  .offer {
    margin: 0.5rem 0 1rem;
    background: var(--accent-soft);
  }
  .hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    opacity: 0;
  }
  .credit {
    margin-top: 1rem;
  }
</style>
