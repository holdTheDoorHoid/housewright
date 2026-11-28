<script lang="ts">
  import { call, type TaskDetail } from '../lib/api';
  import { app, refresh, toast } from '../lib/state.svelte';
  import { dueText, placeText, when } from '../lib/format';
  import { markDone, errorText } from '../lib/actions';
  import { resync } from '../lib/reminders';
  import Reschedule from './Reschedule.svelte';

  let { id, sheet }: { id: string; sheet?: 'reschedule' } = $props();
  let t = $state<TaskDetail | null>(null);
  let error = $state<string | null>(null);
  let rescheduling = $state(false);
  let noteFor = $state<string | null>(null);
  let note = $state('');

  $effect(() => {
    void app.tick;
    call('task', { id })
      .then((d) => {
        t = d;
        error = null;
      })
      .catch((e) => (error = errorText(e)));
  });
  $effect(() => {
    if (sheet === 'reschedule') rescheduling = true;
  });

  async function saveNote(completionId: string) {
    try {
      await call('annotate', { completion_id: completionId, text: note });
      note = '';
      noteFor = null;
      refresh();
    } catch (e) {
      toast({ text: errorText(e) });
    }
  }

  async function setActive(active: boolean) {
    await call('set_active', { id, active });
    refresh();
    resync();
  }
</script>

{#if error}
  <p role="alert">{error}</p>
{:else if t}
  {@const c = t.card}
  <article>
    <p><a href="#/">← Today</a></p>
    <h1>{c.title}</h1>
    <p class="muted">
      {#if c.item_id}<a href={`#/thing/${c.item_id}`}>{placeText(c)}</a>{:else}{placeText(c)}{/if}
    </p>
    <p class="row">
      <span>{dueText(c)}</span>
      <span class="pill">{c.priority_label}</span>
      {#if c.minutes}<span class="pill">about {c.minutes} min</span>{/if}
    </p>
    <p class="muted small">Repeats: {c.schedule_text}. Owner: {c.owner_name}.</p>

    {#if !c.active}
      <div class="card">
        <p>This is a suggestion and is switched off.</p>
        <button class="primary" onclick={() => setActive(true)}>Switch on</button>
      </div>
    {:else if c.state === 'paused' || c.state === 'not_applicable'}
      <div class="card">
        <p>This is {c.state === 'paused' ? 'paused' : 'marked not applicable'}.</p>
        <button
          onclick={async () => {
            await call('review', { id, action: 'resume' });
            refresh();
            resync();
          }}>Bring it back</button
        >
      </div>
    {:else}
      <div class="row actions">
        <button class="primary big" onclick={() => markDone(c).catch((e) => toast({ text: errorText(e) }))}
          >Done</button
        >
        <button class="big" onclick={() => (rescheduling = !rescheduling)}>Reschedule</button>
      </div>
      {#if rescheduling}
        <Reschedule card={c} interval={t.interval} onclose={() => (rescheduling = false)} />
      {:else if c.needs_review}
        <p class="pill attention">Moved {c.snooze_count} times. Tap Reschedule for other options.</p>
      {/if}
    {/if}

    {#if t.why}
      <h2>Why it matters</h2>
      <p>{t.why}</p>
      {#if t.source}<p class="muted small">Source: {t.source}</p>{/if}
    {/if}

    {#if t.steps.length}
      <h2>Steps</h2>
      <ol class="steps">
        {#each t.steps as s, i (i)}<li>{s}</li>{/each}
      </ol>
    {/if}

    <h2>Done before</h2>
    {#if t.completions.length === 0}
      <p class="muted">Not recorded yet.</p>
    {:else}
      <ul class="stack history">
        {#each t.completions as h (h.id)}
          <li class="card" class:voided={h.voided}>
            <div>{when(h.at)} · {h.by_name}{h.voided ? ' · undone' : ''}</div>
            {#each h.notes as n, i (i)}<div class="muted">{n}</div>{/each}
            {#if !h.voided}
              {#if noteFor === h.id}
                <form
                  onsubmit={(e) => {
                    e.preventDefault();
                    saveNote(h.id);
                  }}
                >
                  <label for={`note-${h.id}`}>Note</label>
                  <textarea id={`note-${h.id}`} bind:value={note} placeholder="Anything worth remembering"></textarea>
                  <div class="row">
                    <button class="primary" type="submit">Save note</button>
                    <button type="button" class="quiet" onclick={() => (noteFor = null)}>Cancel</button>
                  </div>
                  <p class="muted small">Photos can be added once attachments arrive (Phase 4).</p>
                </form>
              {:else}
                <button class="quiet" onclick={() => (noteFor = h.id)}>Add a note</button>
              {/if}
            {/if}
          </li>
        {/each}
      </ul>
    {/if}

    {#if c.active}
      <p><button class="quiet" onclick={() => setActive(false)}>Switch this task off</button></p>
    {/if}
  </article>
{:else}
  <p class="muted">Loading…</p>
{/if}

<style>
  .actions {
    margin: 1rem 0;
  }
  .big {
    flex: 1;
    min-height: 56px;
    font-size: 1.05rem;
  }
  .steps li {
    margin: 0.35rem 0;
  }
  .history {
    padding: 0;
    list-style: none;
  }
  .voided {
    opacity: 0.6;
  }
</style>
