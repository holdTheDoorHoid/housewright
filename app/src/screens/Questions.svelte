<script lang="ts">
  import { call, type ClimateInfo, type ProfileAnswers } from '../lib/api';
  import { QUESTIONS, answered, clear, fromProfile, toSend, type Part } from '../lib/questions';
  import { go, refresh } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import { setPlanNote } from '../lib/plan-note';

  let { redo = false }: { redo?: boolean } = $props();

  let step = $state(0);
  let a = $state<ProfileAnswers>({});
  let busy = $state(false);
  let error = $state<string | null>(null);
  let loaded = $state(false);
  let climate = $state<ClimateInfo | null>(null);
  let zipNote = $state<string | null>(null);

  $effect(() => {
    if (!redo) {
      loaded = true;
      return;
    }
    call('profile')
      .then((p) => {
        a = fromProfile(p.profile);
        climate = p.climate;
      })
      .catch((e) => (error = errorText(e)))
      .finally(() => (loaded = true));
  });

  const PETS: [boolean, string][] = [
    [true, 'Yes'],
    [false, 'No'],
  ];
  const total = QUESTIONS.length;
  const q = $derived(QUESTIONS[step]);
  const lone = $derived(q.parts.length === 1 && q.parts[0].kind === 'single');

  function pick(p: Part & { kind: 'single' }, value: string) {
    const cur = (a as Record<string, unknown>)[p.key];
    a = { ...a, [p.key]: cur === value ? undefined : value };
    if (lone && cur !== value) next();
  }

  function toggle(p: Part & { kind: 'multi' }, value: string) {
    const cur = (a[p.key] ?? []) as string[];
    const nextVals = cur.includes(value) ? cur.filter((v) => v !== value) : [...cur, value];
    a = { ...a, [p.key]: nextVals };
  }

  function tick(key: string) {
    const f = { ...(a.features ?? {}) };
    f[key] = !f[key];
    a = { ...a, features: f };
  }

  function count(key: 'adults' | 'children' | 'older_adults', delta: number) {
    const people = { ...(a.people ?? {}) };
    const v = Math.max(0, Math.min(20, (people[key] ?? 0) + delta));
    people[key] = v;
    a = { ...a, people };
  }

  async function lookupZip() {
    zipNote = null;
    climate = null;
    const z = (a.zip ?? '').trim();
    if (!z) return;
    if (!/^\d{5}(-\d{4})?$/.test(z)) {
      zipNote = 'A ZIP code is five digits.';
      return;
    }
    try {
      climate = await call('climate_lookup', { zip: z });
      if (!climate) zipNote = 'We do not have climate data for that ZIP code. Seasonal jobs will use general dates.';
    } catch (e) {
      zipNote = errorText(e);
    }
  }

  function climateText(c: ClimateInfo): string {
    if (!c.first_frost) return 'Frost is rare here, so there is no frost date to plan around.';
    const base = `Average first frost around ${c.first_frost}, last frost around ${c.last_frost}.`;
    return c.freezes ? base : `${base} Hard freezes are not normal here, so pipes rarely need winterising.`;
  }

  function back() {
    error = null;
    if (step > 0) step--;
    else go(redo ? '#/settings' : '#/');
  }

  function skip() {
    a = clear(q, a);
    if (q.parts.some((p) => p.kind === 'zip')) climate = null;
    next();
  }

  function next() {
    error = null;
    if (step < total - 1) step++;
    else finish();
  }

  async function finish() {
    busy = true;
    try {
      const plan = await call('onboard', { profile: toSend(a) });
      setPlanNote(redo ? 'redo' : 'first', plan);
      refresh();
      go('#/plan');
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }
</script>

<section class="questions" aria-labelledby="q-h">
  {#if !loaded}
    <p class="muted">Loading…</p>
  {:else}
    <p class="muted small progress" aria-live="polite">
      Question {step + 1} of {total}{step === 0 && !redo ? ' · about 3 minutes in all' : ''}
    </p>
    <div class="bar" aria-hidden="true"><span style={`width: ${((step + 1) / total) * 100}%`}></span></div>
    <h1 id="q-h">{q.title}</h1>
    {#if q.help}<p class="muted small">{q.help}</p>{/if}

    {#each q.parts as p, i (i)}
      {#if p.kind === 'single'}
        {#if p.label}<h2>{p.label}</h2>{/if}
        <div class="options" role="group" aria-label={p.label ?? q.title}>
          {#each p.options as [value, label] (value)}
            <button
              type="button"
              class="option"
              aria-pressed={(a as Record<string, unknown>)[p.key] === value}
              onclick={() => pick(p, value)}>{label}</button
            >
          {/each}
        </div>
      {:else if p.kind === 'multi'}
        <div class="options" role="group" aria-label={q.title}>
          {#each p.options as [value, label] (value)}
            <button
              type="button"
              class="option"
              aria-pressed={((a[p.key] ?? []) as string[]).includes(value)}
              onclick={() => toggle(p, value)}>{label}</button
            >
          {/each}
        </div>
      {:else if p.kind === 'checklist'}
        <div class="options grid" role="group" aria-label={q.title}>
          {#each p.options as [value, label] (value)}
            <button type="button" class="option" aria-pressed={!!a.features?.[value]} onclick={() => tick(value)}
              >{label}</button
            >
          {/each}
        </div>
      {:else if p.kind === 'zip'}
        <label for="zip">ZIP code</label>
        <input
          id="zip"
          inputmode="numeric"
          autocomplete="postal-code"
          maxlength="10"
          value={a.zip ?? ''}
          oninput={(e) => (a = { ...a, zip: (e.target as HTMLInputElement).value })}
          onchange={lookupZip}
          onblur={lookupZip}
        />
        {#if climate}<p class="small" data-testid="climate">{climateText(climate)}</p>{/if}
        {#if zipNote}<p class="small muted">{zipNote}</p>{/if}
      {:else if p.kind === 'count'}
        <div class="row count">
          <span class="count-label" id={`c-${p.key}`}>{p.label}</span>
          <button type="button" aria-label={`Fewer ${p.label.toLowerCase()}`} onclick={() => count(p.key, -1)}>−</button>
          <span class="count-value" aria-labelledby={`c-${p.key}`}>{a.people?.[p.key] ?? '–'}</span>
          <button type="button" aria-label={`More ${p.label.toLowerCase()}`} onclick={() => count(p.key, 1)}>+</button>
        </div>
      {:else if p.kind === 'pets'}
        <h2>Pets?</h2>
        <div class="options row" role="group" aria-label="Pets">
          {#each PETS as [v, label] (label)}
            <button
              type="button"
              class="option"
              aria-pressed={a.people?.pets === v}
              onclick={() => (a = { ...a, people: { ...(a.people ?? {}), pets: a.people?.pets === v ? undefined : v } })}
              >{label}</button
            >
          {/each}
        </div>
      {/if}
    {/each}

    {#if error}<p role="alert">{error}</p>{/if}
    <div class="nav">
      <button type="button" onclick={back} disabled={busy}>Back</button>
      <button type="button" class="quiet" onclick={skip} disabled={busy}>Not sure</button>
      {#if !lone || answered(q, a) || step === total - 1}
        <button type="button" class="primary" onclick={next} disabled={busy}>
          {step === total - 1 ? 'Show my plan' : 'Next'}
        </button>
      {/if}
    </div>
  {/if}
</section>

<style>
  .questions {
    max-width: 32rem;
    margin: 0 auto;
  }
  .progress {
    margin: 0;
  }
  .bar {
    height: 6px;
    background: var(--surface-2);
    border-radius: 3px;
    margin: 0.3rem 0 1rem;
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--accent);
  }
  .options {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .options.grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(9.5rem, 1fr));
  }
  .options.row {
    flex-direction: row;
  }
  .option {
    justify-content: flex-start;
    text-align: left;
    padding: 0.55rem 1rem;
    line-height: 1.25;
  }
  .option[aria-pressed='true'] {
    background: var(--accent-soft);
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .count {
    margin: 0.5rem 0;
  }
  .count-label {
    flex: 1;
    font-weight: 550;
  }
  .count-value {
    min-width: 2rem;
    text-align: center;
  }
  .nav {
    display: flex;
    gap: 0.5rem;
    margin-top: 1.5rem;
    position: sticky;
    bottom: env(safe-area-inset-bottom);
    background: var(--bg);
    padding: 0.5rem 0;
  }
  .nav .primary {
    margin-left: auto;
  }
</style>
