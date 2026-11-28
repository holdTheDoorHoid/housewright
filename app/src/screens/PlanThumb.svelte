<script lang="ts">
  // A small picture of a floor with one pin highlighted, on a thing's page
  // (D-25). Tapping it opens the plan at that pin.
  import { call, type PlanInfo } from '../lib/api';
  import type { Pt } from '../lib/plan/geometry';

  let { floorId, pinId, name }: { floorId: string; pinId: string; name: string } = $props();
  let plan = $state<PlanInfo | null>(null);

  $effect(() => {
    call('floor_plan', { floor_id: floorId })
      .then((p) => (plan = p))
      .catch(() => (plan = null));
  });

  const pin = $derived(plan?.pins.find((p) => p.id === pinId) ?? null);
  const box = $derived.by(() => {
    if (!plan) return null;
    const pts: Pt[] = [];
    for (const r of plan.rooms) if (r.outline) pts.push(...r.outline);
    for (const w of plan.walls) pts.push(w.a, w.b);
    if (pin) pts.push(pin.at);
    if (!pts.length) return null;
    const xs = pts.map((p) => p.x);
    const ys = pts.map((p) => p.y);
    const pad = 600;
    const x = Math.min(...xs) - pad;
    const y = Math.min(...ys) - pad;
    return { x, y, w: Math.max(...xs) + pad - x, h: Math.max(...ys) + pad - y };
  });
  const r = $derived(box ? Math.max(box.w, box.h) / 28 : 100);
</script>

{#if plan && box && pin}
  <a class="thumb card" href={`#/plans/${floorId}/pin/${pinId}`} aria-label={`${name} on the ${plan.floor.name} plan${pin.room_name ? `, in ${pin.room_name}` : ''}. Open the plan.`} data-testid="plan-thumb">
    <svg viewBox={`${box.x} ${box.y} ${box.w} ${box.h}`} aria-hidden="true">
      {#each plan.rooms as room (room.id)}
        {#if room.outline}
          <polygon points={room.outline.map((p) => `${p.x},${p.y}`).join(' ')} class="room" vector-effect="non-scaling-stroke" />
        {/if}
      {/each}
      {#each plan.walls as w (w.id)}
        <line x1={w.a.x} y1={w.a.y} x2={w.b.x} y2={w.b.y} class="wall" stroke-width={Math.max(w.thickness, r / 3)} />
      {/each}
      {#each plan.pins as p (p.id)}
        {#if p.id !== pinId}<circle cx={p.at.x} cy={p.at.y} r={r * 0.45} class="other" />{/if}
      {/each}
      <circle cx={pin.at.x} cy={pin.at.y} r={r * 1.6} class="halo" />
      <circle cx={pin.at.x} cy={pin.at.y} r={r} class="me" />
    </svg>
    <span class="small">{plan.floor.name}{pin.room_name ? ` · ${pin.room_name}` : ''} · pin {pin.number}</span>
  </a>
{/if}

<style>
  .thumb {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    text-decoration: none;
    color: var(--text);
    padding: 0.5rem;
    max-width: 18rem;
  }
  svg {
    width: 100%;
    height: 9rem;
  }
  .room {
    fill: var(--accent-soft);
    stroke: var(--accent);
    stroke-width: 1;
  }
  .wall {
    stroke: var(--text);
  }
  .other {
    fill: var(--muted);
  }
  .halo {
    fill: var(--focus);
    opacity: 0.25;
  }
  .me {
    fill: var(--focus);
  }
</style>
