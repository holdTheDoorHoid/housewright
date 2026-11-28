<script lang="ts">
  import { call, type StartingPlan } from '../lib/api';
  import { takePlanNote } from '../lib/plan-note';
  import { errorText } from '../lib/actions';
  import { dueText, placeText } from '../lib/format';

  const note = takePlanNote();
  let plan = $state<StartingPlan | null>(note?.plan ?? null);
  let error = $state<string | null>(null);
  const mode = note?.mode ?? 'view';

  $effect(() => {
    if (plan) return;
    call('starting_plan')
      .then((p) => (plan = p))
      .catch((e) => (error = errorText(e)));
  });

  function climateLine(p: StartingPlan): string | null {
    const c = p.climate;
    if (!c) return null;
    if (!c.first_frost) return `Frost is rare at ${c.lookup.zip}, so frost jobs are left out.`;
    const base = `Seasonal jobs follow ${c.lookup.zip}: first frost around ${c.first_frost}, last frost around ${c.last_frost}.`;
    return c.freezes ? base : `${base} Hard freezes are not normal there, so pipe winterising is left out.`;
  }
</script>

<section aria-labelledby="plan-h">
  {#if error}
    <p role="alert">{error}</p>
  {:else if !plan}
    <p class="muted">Loading…</p>
  {:else}
    <h1 id="plan-h">{mode === 'redo' ? 'Your answers are saved' : mode === 'view' ? 'Your plan' : 'Your starting plan'}</h1>
    {#if mode === 'redo'}
      <p>
        {plan.new_things === 0
          ? 'Nothing new came up, and nothing you already had was changed.'
          : `${plan.new_things} new ${plan.new_things === 1 ? 'thing was' : 'things were'} added, with suggested tasks switched off. Nothing you already had was changed or removed.`}
      </p>
    {:else if mode === 'view' || plan.switched_on.length === 0}
      <p>These are the tasks that are switched on. More will come one at a time, when the season makes them useful.</p>
    {:else}
      <p>
        We switched on {plan.switched_on.length === 1 ? 'one task' : `${plan.switched_on.length} tasks`} to begin with,
        the ones that matter most: safety first, then the most likely water or fire damage. More will come
        one at a time, when the season makes them useful.
      </p>
    {/if}

    <p class="card progress" data-testid="recorded">
      Your home, so far: <strong>{plan.things_recorded} {plan.things_recorded === 1 ? 'thing' : 'things'} recorded</strong>.
    </p>
    {#if climateLine(plan)}<p class="muted small">{climateLine(plan)}</p>{/if}

    <h2>Switched on</h2>
    <ul class="stack list" aria-label="Switched on">
      {#each plan.switched_on as t (t.card.id)}
        <li class="card" data-testid="plan-task">
          <a href={`#/task/${t.card.id}`} class="title">{t.card.title}</a>
          {#if placeText(t.card)}<span class="muted small block">{placeText(t.card)}</span>{/if}
          {#if t.why}<p class="small why">{t.why}</p>{/if}
          <p class="muted small">
            {#if t.source}Source: {t.source}. {/if}{dueText(t.card)}{t.card.minutes ? `. About ${t.card.minutes} min.` : ''}
          </p>
        </li>
      {/each}
    </ul>

    <p class="actions"><a class="button primary" href="#/">Go to Today</a></p>
    {#if plan.suggested_count > 0}
      <p class="muted small">
        {plan.suggested_count} more suggested tasks are switched off for now.
        <a href="#/suggested">See everything we suggested</a>
      </p>
    {/if}
  {/if}
</section>

<style>
  .list {
    padding: 0;
    margin: 0;
    list-style: none;
  }
  .title {
    font-weight: 600;
    text-decoration: none;
    color: var(--text);
  }
  .block {
    display: block;
  }
  .why {
    margin-top: 0.35rem;
  }
  .progress {
    margin: 1rem 0 0.4rem;
  }
  .actions {
    margin: 1.4rem 0 0.6rem;
  }
</style>
