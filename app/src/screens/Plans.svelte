<script lang="ts">
  // Floor plans (D-25). Optional (P3): nothing else needs a plan. Stage A:
  // rooms dragged out on a grid, pins for things, several floors, and a
  // photo of an existing plan to trace over. Stage B: walls tapped point
  // by point with live length and angle, typed lengths, doors and windows,
  // areas, undo and redo. Everything on the plan is also in the lists
  // below it, for keyboards and screen readers.
  import { onMount, untrack } from 'svelte';
  import {
    call,
    upload,
    type FactInfo,
    type ItemRow,
    type OpeningKind,
    type PlanInfo,
    type PlanOpening,
    type PlanPin,
    type PlanRoom,
    type PlanWall,
    type FloorInfo,
    type Point,
    type Room,
    type Units,
  } from '../lib/api';
  import { app, go, toast } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';
  import { attachmentUrl, openFile, readFile } from '../lib/files';
  import {
    areaMm2,
    contains,
    distance,
    formatArea,
    formatLength,
    gridStep,
    labelPoint,
    lShape,
    pointAlong,
    project,
    rectangle,
    selfIntersects,
    snap,
    tryLength,
    type Pt,
  } from '../lib/plan/geometry';
  import {
    alongDirection,
    fit,
    record,
    rename,
    roomReadout,
    toPlan,
    wallReadout,
    zoomAt,
    type History,
    type View,
  } from '../lib/plan/editor';

  let { floorId = null, pinId = null, placeItem = null }: { floorId?: string | null; pinId?: string | null; placeItem?: string | null } =
    $props();

  type Mode = 'move' | 'room' | 'wall' | 'door' | 'window' | 'pin' | 'label' | 'picture';
  type Sel =
    | { kind: 'room'; id: string }
    | { kind: 'space'; index: number }
    | { kind: 'wall'; id: string }
    | { kind: 'opening'; id: string }
    | { kind: 'pin'; id: string }
    | { kind: 'label'; id: string }
    | { kind: 'new-room'; outline: Pt[] }
    | { kind: 'new-pin'; at: Pt; wall: string | null }
    | { kind: 'new-label'; at: Pt }
    | null;

  let floors = $state<FloorInfo[] | null>(null);
  let plan = $state<PlanInfo | null>(null);
  let error = $state<string | null>(null);
  // Arriving to place a thing starts in pin mode (the page is keyed on
  // the route, so the first value is the one that matters).
  // svelte-ignore state_referenced_locally
  let mode = $state<Mode>(placeItem ? 'pin' : 'move');
  let shape = $state<'rect' | 'l'>('rect');
  let sel = $state<Sel>(null);
  let view = $state<View>({ x: -500, y: -500, k: 0.05 });
  let size = $state({ w: 360, h: 420 });
  let history = $state<History>({ undo: [], redo: [] });
  let busy = $state(false);
  let bgUrl = $state<string | null>(null);
  let items = $state<ItemRow[]>([]);
  let facts = $state<FactInfo[]>([]);
  let rooms = $state<Room[]>([]);

  // Drawing state.
  let draftRoom = $state<{ a: Pt; b: Pt } | null>(null);
  let chain = $state<Pt[]>([]); // wall corners placed so far
  let pointer = $state<Pt | null>(null); // the pointer, plan mm
  // Snapped against the latest corner, so it is right as soon as a corner
  // is placed, before the pointer moves again.
  const cursor = $derived.by<Pt | null>(() => {
    if (!pointer) return null;
    if (mode === 'wall') return snapped(pointer, chain[chain.length - 1] ?? null);
    if (mode === 'room' || mode === 'pin' || mode === 'label') return snapped(pointer);
    return null;
  });
  let typed = $state(''); // typed wall length
  let drag = $state<
    | { kind: 'pan'; sx: number; sy: number; v: View }
    | { kind: 'corner'; from: Pt; to: Pt }
    | { kind: 'vertex'; room: string; index: number; outline: Pt[] }
    | { kind: 'pin'; id: string; at: Pt }
    | { kind: 'picture'; sx: number; sy: number; at: Pt }
    | null
  >(null);

  // Forms in the selection sheet.
  let nameInput = $state('');
  let lengthInput = $state('');
  let widthInput = $state('');
  let offsetInput = $state('');
  let thicknessInput = $state('');
  let pinItem = $state('');
  let pinLabel = $state('');
  let pinPrivate = $state(false);
  let pinFact = $state('');
  let pinSearch = $state('');
  let labelText = $state('');
  let labelPrivate = $state(false);
  let newFloor = $state('');
  let bgWidth = $state('');

  let svg: SVGSVGElement | undefined = $state();
  const units = $derived<Units>(plan?.units ?? app.status?.units ?? 'imperial');
  const snapOpts = $derived({
    grid: gridStep(units),
    endpointTolerance: 16 / view.k,
    angleStep: 45,
    angleTolerance: 6,
  });
  const len = (mm: number) => formatLength(mm, units);

  // -- loading --------------------------------------------------------------

  async function loadFloors() {
    try {
      floors = await call('floors');
      const target = floorId ?? floors[0]?.id ?? null;
      if (target) await load(target, true);
      else plan = null;
    } catch (e) {
      error = errorText(e);
    }
  }

  async function load(id: string, refit = false) {
    const p = await call('floor_plan', { floor_id: id });
    show(p, refit);
  }

  function show(p: PlanInfo, refit = false) {
    const floorChanged = plan?.floor.id !== p.floor.id;
    plan = p;
    floors = p.floors;
    if (refit || floorChanged) fitView();
    if (p.background) {
      attachmentUrl(p.background.attachment_id, 'image/jpeg').then((u) => (bgUrl = u));
    } else bgUrl = null;
  }

  function allPoints(p: PlanInfo): Pt[] {
    const pts: Pt[] = [];
    for (const r of p.rooms) if (r.outline) pts.push(...r.outline);
    for (const w of p.walls) pts.push(w.a, w.b);
    for (const x of p.pins) pts.push(x.at);
    for (const l of p.labels) pts.push(l.at);
    if (p.background) {
      const b = p.background;
      const h = b.pixel_width && b.pixel_height ? (b.width * b.pixel_height) / b.pixel_width : b.width;
      pts.push(b.at, { x: b.at.x + b.width, y: b.at.y + h });
    }
    return pts;
  }

  function fitView() {
    if (!plan) return;
    view = fit(allPoints(plan), size.w, size.h);
  }

  // Until someone pans or zooms, keep the whole floor in view as the
  // drawing area gets its real size (and when a phone turns).
  let userMoved = false;
  $effect(() => {
    void size.w;
    void size.h;
    // Only the size is tracked: saving a change must not move the view.
    if (!userMoved) untrack(fitView);
  });

  onMount(() => {
    loadFloors();
    call('items').then((r) => (items = r));
    call('facts').then((f) => (facts = f.groups.flatMap((g) => g.facts)));
    call('rooms').then((r) => (rooms = r));
    const onKey = (e: KeyboardEvent) => keys(e);
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  // Scroll-wheel zoom needs a listener that may cancel the page scroll.
  $effect(() => {
    const el = svg;
    if (!el) return;
    el.addEventListener('wheel', wheel, { passive: false });
    return () => el.removeEventListener('wheel', wheel);
  });

  $effect(() => {
    // Open on the pin asked for (from a thing's page).
    if (plan && pinId && !sel) {
      const p = plan.pins.find((x) => x.id === pinId);
      if (p) select({ kind: 'pin', id: p.id });
    }
  });

  // -- changes, with undo ----------------------------------------------------

  /** Run a change that returns the new plan; remember it for undo. */
  // Changes run one after another: a quick second tap waits for the
  // first to be saved instead of being dropped.
  let queue: Promise<unknown> = Promise.resolve();
  function change(what: string, run: () => Promise<PlanInfo>): Promise<PlanInfo | null> {
    const job = queue.then(() => applyChange(what, run));
    queue = job.catch(() => null);
    return job;
  }

  async function applyChange(what: string, run: () => Promise<PlanInfo>): Promise<PlanInfo | null> {
    if (!plan) return null;
    const before = plan.state;
    const floor = plan.floor.id;
    busy = true;
    try {
      const p = await run();
      history = record(history, { floor, before, after: p.state, what });
      show(p);
      return p;
    } catch (e) {
      toast({ text: errorText(e) });
      return null;
    } finally {
      busy = false;
    }
  }

  async function undo() {
    const s = history.undo[history.undo.length - 1];
    if (!s || busy || !plan) return;
    busy = true;
    try {
      const r = await call('plan_restore', { floor_id: s.floor, to: s.before, from: s.after });
      const h = rename({ undo: history.undo.slice(0, -1), redo: [...history.redo, s] }, r.renamed);
      history = h;
      sel = null;
      show(r.plan);
      toast({ text: `Undone: ${s.what}.` }, 3000);
    } catch (e) {
      toast({ text: errorText(e) });
    } finally {
      busy = false;
    }
  }

  async function redo() {
    const s = history.redo[history.redo.length - 1];
    if (!s || busy) return;
    busy = true;
    try {
      const r = await call('plan_restore', { floor_id: s.floor, to: s.after, from: s.before });
      history = rename({ undo: [...history.undo, s], redo: history.redo.slice(0, -1) }, r.renamed);
      sel = null;
      show(r.plan);
      toast({ text: `Redone: ${s.what}.` }, 3000);
    } catch (e) {
      toast({ text: errorText(e) });
    } finally {
      busy = false;
    }
  }

  // -- floors ----------------------------------------------------------------

  async function addFloor(e?: SubmitEvent) {
    e?.preventDefault();
    const name = newFloor.trim() || (floors?.length ? `Floor ${floors.length + 1}` : 'Ground floor');
    try {
      const before = new Set((floors ?? []).map((f) => f.id));
      const list = await call('save_floor', { name });
      const added = list.find((f) => !before.has(f.id));
      newFloor = '';
      floors = list;
      if (added) switchFloor(added.id);
    } catch (err) {
      toast({ text: errorText(err) });
    }
  }

  function switchFloor(id: string) {
    sel = null;
    chain = [];
    draftRoom = null;
    go(`#/plans/${id}`);
  }

  async function renameFloor(name: string) {
    if (!plan || !name.trim()) return;
    floors = await call('save_floor', { id: plan.floor.id, name });
    await load(plan.floor.id);
  }

  async function deleteFloor() {
    if (!plan) return;
    if (!confirm(`Delete the plan of "${plan.floor.name}"? Its rooms and things stay; only the drawing goes.`)) return;
    const list = await call('delete_floor', { id: plan.floor.id });
    history = { undo: [], redo: [] };
    plan = null;
    floors = list;
    go(list[0] ? `#/plans/${list[0].id}` : '#/plans');
    if (!list[0]) loadFloors();
  }

  async function setUnits(u: Units) {
    app.status = await call('set_units', { units: u });
    if (plan) await load(plan.floor.id);
  }

  // -- pointer input ---------------------------------------------------------

  const pointers = new Map<number, { x: number; y: number }>();
  let pinch: { d: number; mx: number; my: number; v: View } | null = null;
  let downAt: { x: number; y: number; t: number } | null = null;
  // What was under the pointer when it went down (after capture, the
  // pointer-up event's target is the drawing itself).
  let downHit: ReturnType<typeof hit> = null;
  let moved = false;

  function local(e: PointerEvent | WheelEvent): { x: number; y: number } {
    const r = svg!.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  }

  function planAt(e: PointerEvent): Pt {
    const s = local(e);
    const p = toPlan(view, s.x, s.y);
    return { x: Math.round(p.x), y: Math.round(p.y) };
  }

  function corners(): Pt[] {
    const c = [...(plan?.corners ?? []), ...chain];
    return c;
  }

  function snapped(p: Pt, anchor: Pt | null = null): Pt {
    return snap(p, anchor, corners(), snapOpts).point;
  }

  function hit(e: Event): { kind: string; id: string; index?: number } | null {
    const el = (e.target as Element | null)?.closest?.('[data-hit]') as HTMLElement | SVGElement | null;
    if (!el) return null;
    const [kind, id, index] = (el.getAttribute('data-hit') ?? '').split(':');
    return { kind, id, index: index === undefined ? undefined : Number(index) };
  }

  function down(e: PointerEvent) {
    if (!plan) return;
    svg?.setPointerCapture(e.pointerId);
    pointers.set(e.pointerId, local(e));
    if (pointers.size === 2) {
      // Two fingers: pinch to zoom and pan, in every mode.
      const [a, b] = [...pointers.values()];
      pinch = { d: Math.hypot(a.x - b.x, a.y - b.y), mx: (a.x + b.x) / 2, my: (a.y + b.y) / 2, v: { ...view } };
      drag = null;
      draftRoom = null;
      return;
    }
    const s = local(e);
    downAt = { ...s, t: Date.now() };
    moved = false;
    const p = planAt(e);
    const h = hit(e);
    downHit = h;
    if (!plan.can_edit && mode !== 'move') mode = 'move';
    if (mode === 'move') {
      if (h?.kind === 'corner' && plan.can_edit) {
        const c = plan.corners[Number(h.id)];
        drag = { kind: 'corner', from: c, to: c };
      } else if (h?.kind === 'vertex' && plan.can_edit && sel?.kind === 'room') {
        const r = plan.rooms.find((x) => x.id === h.id);
        if (r?.outline) drag = { kind: 'vertex', room: r.id, index: h.index ?? 0, outline: r.outline.map((q) => ({ ...q })) };
      } else if (h?.kind === 'pin' && plan.can_edit) {
        const pin = plan.pins.find((x) => x.id === h.id);
        if (pin) drag = { kind: 'pin', id: pin.id, at: pin.at };
      } else {
        drag = { kind: 'pan', sx: s.x, sy: s.y, v: { ...view } };
      }
    } else if (mode === 'picture' && plan.background) {
      drag = { kind: 'picture', sx: s.x, sy: s.y, at: plan.background.at };
    } else if (mode === 'room') {
      const a = snapped(p);
      draftRoom = { a, b: a };
    } else {
      drag = { kind: 'pan', sx: s.x, sy: s.y, v: { ...view } };
    }
  }

  function moveP(e: PointerEvent) {
    if (!plan) return;
    if (pointers.has(e.pointerId)) pointers.set(e.pointerId, local(e));
    if (pinch && pointers.size >= 2) {
      const [a, b] = [...pointers.values()];
      const d = Math.hypot(a.x - b.x, a.y - b.y);
      const mx = (a.x + b.x) / 2;
      const my = (a.y + b.y) / 2;
      let v = zoomAt(pinch.v, d / Math.max(1, pinch.d), pinch.mx, pinch.my);
      v = { ...v, x: v.x - (mx - pinch.mx) / v.k, y: v.y - (my - pinch.my) / v.k };
      view = v;
      userMoved = true;
      return;
    }
    const s = local(e);
    if (downAt && Math.hypot(s.x - downAt.x, s.y - downAt.y) > 6) moved = true;
    const p = planAt(e);
    pointer = p;
    if (draftRoom) {
      draftRoom = { a: draftRoom.a, b: snapped(p) };
      return;
    }
    if (!drag) return;
    if (drag.kind === 'pan' && moved) {
      userMoved = true;
      view = { ...drag.v, x: drag.v.x - (s.x - drag.sx) / drag.v.k, y: drag.v.y - (s.y - drag.sy) / drag.v.k };
    } else if (drag.kind === 'corner') {
      const from = drag.from;
      drag = { ...drag, to: snap(p, null, plan.corners.filter((c) => c.x !== from.x || c.y !== from.y), snapOpts).point };
    } else if (drag.kind === 'vertex') {
      const o = drag.outline.map((q) => ({ ...q }));
      o[drag.index] = snapped(p);
      drag = { ...drag, outline: o };
    } else if (drag.kind === 'pin') {
      drag = { ...drag, at: p };
    } else if (drag.kind === 'picture') {
      drag = {
        ...drag,
        at: plan.background
          ? { x: Math.round(drag.at.x + (s.x - drag.sx) / view.k), y: Math.round(drag.at.y + (s.y - drag.sy) / view.k) }
          : drag.at,
        sx: s.x,
        sy: s.y,
      };
    }
  }

  async function up(e: PointerEvent) {
    pointers.delete(e.pointerId);
    if (pinch) {
      if (pointers.size < 2) pinch = null;
      return;
    }
    if (!plan) return;
    const tap = !moved;
    const p = planAt(e);
    const h = downHit;
    downHit = null;
    const d = drag;
    drag = null;
    downAt = null;

    if (draftRoom) {
      const r = draftRoom;
      draftRoom = null;
      const outline = shape === 'l' ? lShape(r.a, r.b) : rectangle(r.a, r.b);
      if (outline.length && areaMm2(outline) >= 250_000) {
        select({ kind: 'new-room', outline });
      } else if (tap) {
        toast({ text: 'Drag from one corner of the room to the opposite corner.' }, 4000);
      }
      return;
    }
    if (d?.kind === 'corner' && moved) {
      await moveCorner(d.from, d.to);
      return;
    }
    if (d?.kind === 'vertex' && moved) {
      if (selfIntersects(d.outline)) {
        toast({ text: 'That would make the room cross itself, so it was not changed.' });
        return;
      }
      await change('reshape a room', async () => {
        const r = await call('save_room_shape', { floor_id: plan!.floor.id, room_id: d.room, outline: d.outline });
        return r.plan;
      });
      return;
    }
    if (d?.kind === 'pin' && moved) {
      const pin = plan.pins.find((x) => x.id === d.id);
      if (pin) await savePin(pin.id, snapToWall(d.at).at, pin.item_id, undefined, undefined, snapToWall(d.at).wall);
      return;
    }
    if (d?.kind === 'picture' && plan.background) {
      await change('move the picture', () =>
        call('set_background', {
          floor_id: plan!.floor.id,
          attachment_id: plan!.background!.attachment_id,
          at: d.at,
          width: plan!.background!.width,
          opacity: plan!.background!.opacity,
        }),
      );
      return;
    }
    if (!tap) return;

    switch (mode) {
      case 'move':
        tapSelect(h, p);
        break;
      case 'wall':
        await wallTap(snapped(p, chain[chain.length - 1] ?? null));
        break;
      case 'door':
      case 'window':
        await placeOpening(mode, p);
        break;
      case 'pin': {
        const s = snapToWall(p);
        select({ kind: 'new-pin', at: s.at, wall: s.wall });
        break;
      }
      case 'label':
        select({ kind: 'new-label', at: p });
        break;
    }
  }

  function wheel(e: WheelEvent) {
    e.preventDefault();
    userMoved = true;
    const s = local(e);
    view = zoomAt(view, Math.exp(-e.deltaY * 0.0015), s.x, s.y);
  }

  function zoomBy(f: number) {
    userMoved = true;
    view = zoomAt(view, f, size.w / 2, size.h / 2);
  }

  // -- selecting -------------------------------------------------------------

  function tapSelect(h: ReturnType<typeof hit>, p: Pt) {
    if (!plan) return;
    if (h?.kind === 'pin' || h?.kind === 'label' || h?.kind === 'opening' || h?.kind === 'wall' || h?.kind === 'room') {
      select({ kind: h.kind, id: h.id } as Sel);
      return;
    }
    if (h?.kind === 'space') {
      select({ kind: 'space', index: Number(h.id) });
      return;
    }
    // A tap inside a room selects it even between its outline's lines.
    const room = [...plan.rooms]
      .filter((r) => r.outline && contains(r.outline, p))
      .sort((a, b) => (a.area_mm2 ?? 0) - (b.area_mm2 ?? 0))[0];
    if (room) select({ kind: 'room', id: room.id });
    else sel = null;
  }

  function select(s: Sel) {
    sel = s;
    if (!plan || !s) return;
    if (s.kind === 'room') {
      const r = plan.rooms.find((x) => x.id === s.id);
      nameInput = r?.name ?? '';
    } else if (s.kind === 'new-room' || s.kind === 'space') {
      nameInput = '';
    } else if (s.kind === 'wall') {
      const w = plan.walls.find((x) => x.id === s.id);
      lengthInput = w ? len(w.length_mm) : '';
      thicknessInput = w ? len(w.thickness) : '';
    } else if (s.kind === 'opening') {
      const o = plan.openings.find((x) => x.id === s.id);
      widthInput = o ? len(o.width) : '';
      offsetInput = o ? len(o.offset) : '';
    } else if (s.kind === 'pin' || s.kind === 'new-pin') {
      const pin = s.kind === 'pin' ? plan.pins.find((x) => x.id === s.id) : null;
      pinItem = pin?.item_id ?? (s.kind === 'new-pin' ? (placeItem ?? '') : '');
      pinLabel = pin?.label ?? '';
      pinPrivate = pin?.sensitive ?? false;
      pinFact = pin?.fact_id ?? '';
      pinSearch = '';
    } else if (s.kind === 'label') {
      const l = plan.labels.find((x) => x.id === s.id);
      labelText = l?.text ?? '';
      labelPrivate = l?.sensitive ?? false;
    } else if (s.kind === 'new-label') {
      labelText = '';
      labelPrivate = false;
    }
    queueMicrotask(() => document.getElementById('plan-sheet')?.scrollIntoView?.({ block: 'nearest' }));
  }

  // -- rooms -----------------------------------------------------------------

  async function saveRoom(e: SubmitEvent) {
    e.preventDefault();
    if (!plan || !sel) return;
    const name = nameInput.trim();
    if (!name) return;
    const floor = plan.floor.id;
    if (sel.kind === 'new-room' || sel.kind === 'space') {
      const outline = sel.kind === 'new-room' ? sel.outline : plan.spaces[sel.index].outline;
      const p = await change(`draw ${name}`, async () => (await call('save_room_shape', { floor_id: floor, name, outline })).plan);
      if (p) {
        const r = p.rooms.find((x) => x.name.toLowerCase() === name.toLowerCase());
        sel = r ? { kind: 'room', id: r.id } : null;
        rooms = await call('rooms');
      }
    } else if (sel.kind === 'room') {
      const id = sel.id;
      await change(`rename ${name}`, async () => (await call('save_room_shape', { floor_id: floor, room_id: id, name })).plan);
    }
  }

  async function removeRoomShape(r: PlanRoom) {
    await change(`take ${r.name} off the plan`, () => call('remove_room_shape', { id: r.id }));
    sel = null;
  }

  // -- walls -----------------------------------------------------------------

  async function wallTap(p: Pt) {
    if (!plan) return;
    const last = chain[chain.length - 1];
    if (!last) {
      chain = [p];
      return;
    }
    if (p.x === last.x && p.y === last.y) {
      // Tapping the last corner again finishes.
      finishChain();
      return;
    }
    // The corner shows at once; the wall is saved in order behind it.
    const closed = chain.length > 1 && p.x === chain[0].x && p.y === chain[0].y;
    const before = chain;
    chain = closed ? [] : [...chain, p];
    typed = '';
    const floor = plan.floor.id;
    const pl = await change('draw a wall', async () => (await call('save_walls', { floor_id: floor, walls: [{ a: last, b: p }] })).plan);
    if (!pl) {
      if (!closed && chain[chain.length - 1] === p) chain = before;
      return;
    }
    if (closed) toast({ text: 'Walls closed. Tap the new space to name the room.' }, 5000);
  }

  async function typedWall(e: SubmitEvent) {
    e.preventDefault();
    const last = chain[chain.length - 1];
    const mm = tryLength(typed, units);
    if (!last || mm === null || mm <= 0) {
      toast({ text: `Type a length, like ${units === 'imperial' ? '12\' 6"' : '3.81 m'}.` }, 4000);
      return;
    }
    const dir = cursor && distance(cursor, last) > 0 ? cursor : null;
    await wallTap(alongDirection(last, dir, mm));
  }

  function finishChain() {
    chain = [];
    typed = '';
  }

  async function closeChain() {
    if (chain.length > 2) await wallTap(chain[0]);
  }

  async function moveCorner(from: Pt, to: Pt) {
    if (!plan || (from.x === to.x && from.y === to.y)) return;
    const walls = plan.walls
      .filter((w) => (w.a.x === from.x && w.a.y === from.y) || (w.b.x === from.x && w.b.y === from.y))
      .map((w) => ({
        id: w.id,
        a: w.a.x === from.x && w.a.y === from.y ? to : w.a,
        b: w.b.x === from.x && w.b.y === from.y ? to : w.b,
        thickness: w.thickness,
      }))
      .filter((w) => w.a.x !== w.b.x || w.a.y !== w.b.y);
    if (!walls.length) return;
    await change('move a corner', async () => (await call('save_walls', { floor_id: plan!.floor.id, walls })).plan);
  }

  async function saveWall(e: SubmitEvent, w: PlanWall) {
    e.preventDefault();
    const l = tryLength(lengthInput, units);
    const t = tryLength(thicknessInput, units);
    if (l === null || l <= 0) {
      toast({ text: 'Type the wall length, like 12\' 6" or 3.81 m.' });
      return;
    }
    // Keep its start and direction; move its end.
    const b = pointAlong(w.a, w.b, l);
    await change('change a wall', async () =>
      (await call('save_walls', { floor_id: plan!.floor.id, walls: [{ id: w.id, a: w.a, b, thickness: t ?? w.thickness }] })).plan,
    );
  }

  async function deleteWall(w: PlanWall) {
    await change('delete a wall', () => call('delete_wall', { id: w.id }));
    sel = null;
  }

  // -- doors and windows -------------------------------------------------------

  function nearestWall(p: Pt): { wall: PlanWall; along: number } | null {
    if (!plan) return null;
    let best: { wall: PlanWall; along: number; off: number } | null = null;
    for (const w of plan.walls) {
      const pr = project(p, w.a, w.b);
      if (!best || pr.off < best.off) best = { wall: w, along: pr.along, off: pr.off };
    }
    return best && best.off <= 24 / view.k ? best : null;
  }

  function snapToWall(p: Pt): { at: Pt; wall: string | null } {
    const n = nearestWall(p);
    if (!n || n.along <= 0) return { at: p, wall: null };
    // Close to a wall: sit on it (a thermostat, a panel).
    return { at: pointAlong(n.wall.a, n.wall.b, Math.round(n.along)), wall: n.wall.id };
  }

  async function placeOpening(kind: OpeningKind, p: Pt) {
    const n = nearestWall(p);
    if (!n || !plan) {
      toast({ text: plan?.walls.length ? 'Tap on a wall to put it there.' : 'Draw walls first (Walls), then add doors and windows.' }, 4000);
      return;
    }
    const width = Math.min(units === 'imperial' ? (kind === 'door' ? 813 : 914) : kind === 'door' ? 800 : 900, n.wall.length_mm);
    const step = units === 'imperial' ? 25.4 : 10;
    const offset = Math.max(0, Math.min(n.wall.length_mm - width, Math.round((n.along - width / 2) / step) * step));
    const pl = await change(`add a ${kind}`, async () => {
      const r = await call('save_opening', {
        wall_id: n.wall.id,
        kind,
        offset: Math.round(offset),
        width,
        swing: kind === 'door' ? { hinge: 'a', side: 'left' } : null,
      });
      return r.plan;
    });
    if (pl) {
      const o = pl.openings.find((x) => !plan!.openings.some((y) => y.id === x.id)) ?? pl.openings[pl.openings.length - 1];
      if (o) select({ kind: 'opening', id: o.id });
    }
  }

  async function saveOpening(e: SubmitEvent, o: PlanOpening, swing = o.swing) {
    e.preventDefault();
    const w = tryLength(widthInput, units);
    const off = tryLength(offsetInput, units);
    if (w === null || off === null || w <= 0) {
      toast({ text: 'Type the width and the distance from the wall\'s start.' });
      return;
    }
    await change(`change a ${o.kind}`, async () =>
      (await call('save_opening', { id: o.id, wall_id: o.wall_id, kind: o.kind, offset: off, width: w, swing })).plan,
    );
  }

  async function flip(o: PlanOpening, part: 'hinge' | 'side') {
    const s = o.swing ?? { hinge: 'a', side: 'left' };
    const swing =
      part === 'hinge'
        ? { ...s, hinge: s.hinge === 'a' ? ('b' as const) : ('a' as const) }
        : { ...s, side: s.side === 'left' ? ('right' as const) : ('left' as const) };
    await change('turn a door', async () =>
      (await call('save_opening', { id: o.id, wall_id: o.wall_id, kind: o.kind, offset: o.offset, width: o.width, swing })).plan,
    );
  }

  async function deleteOpening(o: PlanOpening) {
    await change(`delete a ${o.kind}`, () => call('delete_opening', { id: o.id }));
    sel = null;
  }

  // -- pins and labels -----------------------------------------------------------

  async function savePin(
    id: string | undefined,
    at: Pt,
    item: string | null,
    label?: string,
    sensitive?: boolean,
    wall: string | null = null,
  ): Promise<string | null> {
    if (!plan) return null;
    let saved: string | null = null;
    let movedTo: string | null = null;
    const what = item ? `place ${items.find((i) => i.id === item)?.name ?? 'a thing'}` : 'place a pin';
    const p = await change(what, async () => {
      const r = await call('save_pin', {
        id,
        floor_id: plan!.floor.id,
        at,
        item_id: item ?? undefined,
        label: label || undefined,
        sensitive,
        wall_id: wall ?? undefined,
      });
      saved = r.ids[0];
      movedTo = r.moved_to;
      return r.plan;
    });
    if (p && movedTo && item) {
      toast({ text: `${items.find((i) => i.id === item)?.name ?? 'It'} is now in ${movedTo}.` }, 5000);
      items = await call('items');
    }
    return p ? saved : null;
  }

  async function submitPin(e: SubmitEvent) {
    e.preventDefault();
    if (!plan || !sel || (sel.kind !== 'pin' && sel.kind !== 'new-pin')) return;
    const existing = sel.kind === 'pin' ? plan.pins.find((x) => x.id === (sel as { id: string }).id) : null;
    const at = sel.kind === 'new-pin' ? sel.at : existing!.at;
    const wall = sel.kind === 'new-pin' ? sel.wall : (existing?.wall_id ?? null);
    if (!pinItem && !pinLabel.trim()) {
      toast({ text: 'Choose a thing, or give the pin a name.' }, 4000);
      return;
    }
    const id = await savePin(existing?.id, at, pinItem || null, pinLabel.trim(), pinPrivate, wall);
    if (!id) return;
    const prior = facts.find((f) => f.pin_id === id);
    if (prior && prior.id !== pinFact) await call('save_fact', { id: prior.id, group: prior.group, label: prior.label, pin_id: '' });
    if (pinFact && prior?.id !== pinFact) {
      const f = facts.find((x) => x.id === pinFact);
      if (f) await call('save_fact', { id: f.id, group: f.group, label: f.label, pin_id: id });
    }
    facts = (await call('facts')).groups.flatMap((g) => g.facts);
    await load(plan.floor.id);
    if (placeItem) {
      placeItem = null;
      mode = 'move';
    }
    select({ kind: 'pin', id });
  }

  async function deletePin(p: PlanPin) {
    await change('remove a pin', () => call('delete_pin', { id: p.id }));
    sel = null;
  }

  async function submitLabel(e: SubmitEvent) {
    e.preventDefault();
    if (!plan || !sel || (sel.kind !== 'label' && sel.kind !== 'new-label')) return;
    const text = labelText.trim();
    if (!text) return;
    const existing = sel.kind === 'label' ? plan.labels.find((x) => x.id === (sel as { id: string }).id) : null;
    const at = sel.kind === 'new-label' ? sel.at : existing!.at;
    await change('write a label', async () =>
      (await call('save_label', { id: existing?.id, floor_id: plan!.floor.id, at, text, sensitive: labelPrivate })).plan,
    );
    sel = null;
  }

  async function deleteLabel(id: string) {
    await change('delete a label', () => call('delete_label', { id }));
    sel = null;
  }

  // -- the traced-over picture ------------------------------------------------------

  async function pickPicture(e: Event) {
    const input = e.target as HTMLInputElement;
    const f = input.files?.[0];
    input.value = '';
    if (!f || !plan) return;
    if (!f.type.startsWith('image/')) {
      toast({ text: 'Use a photo or a screenshot of the plan. A PDF cannot be shown behind the plan yet.' }, 7000);
      return;
    }
    try {
      busy = true;
      const att = await upload('attach', { about_kind: 'floor', about_id: plan.floor.id, kind: 'photo', name: f.name }, await readFile(f));
      busy = false;
      // Start it the size of what is on screen.
      const width = Math.round((size.w * 0.9) / view.k);
      await change('add a picture to trace', () =>
        call('set_background', { floor_id: plan!.floor.id, attachment_id: att.id, at: { x: Math.round(view.x + (size.w * 0.05) / view.k), y: Math.round(view.y + 20 / view.k) }, width, opacity: 40 }),
      );
      bgWidth = len(width);
      toast({ text: 'Picture added. Set how wide it is, then draw over it.' }, 6000);
    } catch (err) {
      busy = false;
      toast({ text: errorText(err) });
    }
  }

  async function setPicture(patch: { width?: number; opacity?: number; remove?: boolean }) {
    if (!plan?.background) return;
    const b = plan.background;
    await change(patch.remove ? 'remove the picture' : 'change the picture', () =>
      call('set_background', {
        floor_id: plan!.floor.id,
        attachment_id: patch.remove ? null : b.attachment_id,
        at: b.at,
        width: patch.width ?? b.width,
        opacity: patch.opacity ?? b.opacity,
      }),
    );
    if (patch.remove) mode = 'move';
  }

  function pictureWidth(e: SubmitEvent) {
    e.preventDefault();
    const w = tryLength(bgWidth, units);
    if (w === null || w < 500) {
      toast({ text: 'Type how wide the drawing is in real life, like 40\' or 12 m.' });
      return;
    }
    setPicture({ width: w });
  }

  // -- keys ----------------------------------------------------------------------

  function keys(e: KeyboardEvent) {
    const t = e.target as HTMLElement | null;
    if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT')) return;
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === 'z') {
      e.preventDefault();
      if (e.shiftKey) redo();
      else undo();
      return;
    }
    if (mod && e.key.toLowerCase() === 'y') {
      e.preventDefault();
      redo();
      return;
    }
    if (mod || e.altKey) return;
    const m: Record<string, Mode> = { v: 'move', r: 'room', w: 'wall', d: 'door', n: 'window', p: 'pin', l: 'label' };
    if (m[e.key] && plan?.can_edit) {
      setMode(m[e.key]);
    } else if (e.key === 'Escape') {
      if (chain.length) finishChain();
      else sel = null;
    } else if (e.key === 'Enter' && mode === 'wall' && chain.length) {
      finishChain();
    } else if (e.key === '+' || e.key === '=') zoomBy(1.25);
    else if (e.key === '-') zoomBy(0.8);
    else if (e.key === '0') fitView();
  }

  function setMode(m: Mode) {
    mode = m;
    chain = [];
    draftRoom = null;
    pointer = null;
    if (m !== 'move') sel = null;
  }

  // -- derived drawing helpers -----------------------------------------------------

  const px = (n: number) => n / view.k; // screen pixels to plan mm
  const viewBox = $derived(`${view.x} ${view.y} ${size.w / view.k} ${size.h / view.k}`);
  const gridLines = $derived.by(() => {
    // A line every foot (or 50 cm), stronger every 5 ft (or 1 m); hidden
    // when they would be too dense to read.
    const minor = units === 'imperial' ? 304.8 : 500;
    const major = units === 'imperial' ? 1524 : 1000;
    const step = minor * view.k >= 8 ? minor : major * view.k >= 8 ? major : 0;
    if (!step) return [] as { x1: number; y1: number; x2: number; y2: number; major: boolean }[];
    const x0 = view.x;
    const y0 = view.y;
    const x1 = view.x + size.w / view.k;
    const y1 = view.y + size.h / view.k;
    const out: { x1: number; y1: number; x2: number; y2: number; major: boolean }[] = [];
    for (let i = Math.ceil(x0 / step); i * step <= x1 && out.length < 400; i++) {
      const x = i * step;
      out.push({ x1: x, y1: y0, x2: x, y2: y1, major: Math.abs(x % major) < 1 || Math.abs((x % major) - major) < 1 });
    }
    for (let i = Math.ceil(y0 / step); i * step <= y1 && out.length < 800; i++) {
      const y = i * step;
      out.push({ x1: x0, y1: y, x2: x1, y2: y, major: Math.abs(y % major) < 1 || Math.abs((y % major) - major) < 1 });
    }
    return out;
  });

  function openingGeom(o: PlanOpening) {
    const w = plan?.walls.find((x) => x.id === o.wall_id);
    if (!w) return null;
    const s = pointAlong(w.a, w.b, o.offset);
    const e = pointAlong(w.a, w.b, o.offset + o.width);
    let swing: string | null = null;
    if (o.kind === 'door') {
      const sw = o.swing ?? { hinge: 'a', side: 'left' };
      const [h, f] = sw.hinge === 'a' ? [s, e] : [e, s];
      const L = Math.max(1, distance(w.a, w.b));
      const ux = (w.b.x - w.a.x) / L;
      const uy = (w.b.y - w.a.y) / L;
      const [nx, ny] = sw.side === 'left' ? [uy, -ux] : [-uy, ux];
      const leaf = { x: h.x + nx * o.width, y: h.y + ny * o.width };
      // Sweep direction: from the leaf to the closed position.
      const cross = (leaf.x - h.x) * (f.y - h.y) - (leaf.y - h.y) * (f.x - h.x);
      swing = `M ${h.x} ${h.y} L ${leaf.x} ${leaf.y} A ${o.width} ${o.width} 0 0 ${cross > 0 ? 1 : 0} ${f.x} ${f.y}`;
    }
    return { s, e, t: w.thickness, swing };
  }

  const draftOutline = $derived(draftRoom ? (shape === 'l' ? lShape(draftRoom.a, draftRoom.b) : rectangle(draftRoom.a, draftRoom.b)) : []);
  const readout = $derived.by(() => {
    if (draftRoom) return roomReadout(draftRoom.a, draftRoom.b, units);
    if (mode === 'wall' && chain.length && cursor) {
      const r = wallReadout(chain.length > 1 ? chain[chain.length - 2] : null, chain[chain.length - 1], cursor, units);
      return `${r.length} · ${r.direction}${r.corner ? ` · corner ${r.corner}` : ''}`;
    }
    if (drag?.kind === 'vertex') {
      return `${formatArea(areaMm2(drag.outline), units)}`;
    }
    return null;
  });

  const selRoom = $derived(sel?.kind === 'room' ? (plan?.rooms.find((r) => r.id === (sel as { id: string }).id) ?? null) : null);
  const selWall = $derived(sel?.kind === 'wall' ? (plan?.walls.find((r) => r.id === (sel as { id: string }).id) ?? null) : null);
  const selOpening = $derived(sel?.kind === 'opening' ? (plan?.openings.find((r) => r.id === (sel as { id: string }).id) ?? null) : null);
  const selPin = $derived(sel?.kind === 'pin' ? (plan?.pins.find((r) => r.id === (sel as { id: string }).id) ?? null) : null);
  const freeRooms = $derived(rooms.filter((r) => !plan?.rooms.some((x) => x.id === r.id && x.outline)));
  const pinChoices = $derived(
    items
      .filter((i) => !pinSearch.trim() || i.name.toLowerCase().includes(pinSearch.trim().toLowerCase()))
      .slice(0, 40),
  );
  const shutoffFacts = $derived(facts.filter((f) => f.group === 'shutoffs' || f.label.toLowerCase().includes('breaker') || f.pin_id));
  const placing = $derived(placeItem ? items.find((i) => i.id === placeItem) : null);
  const bgHeight = $derived(
    plan?.background
      ? plan.background.pixel_width && plan.background.pixel_height
        ? (plan.background.width * plan.background.pixel_height) / plan.background.pixel_width
        : plan.background.width * 0.75
      : 0,
  );
  const hint = $derived.by(() => {
    if (!plan) return '';
    if (placing) return `Tap where ${placing.name} is.`;
    switch (mode) {
      case 'move':
        return plan.can_edit ? 'Tap something to see or change it. Drag to move around; pinch or scroll to zoom.' : 'Tap something to see it. Drag to move around.';
      case 'room':
        return 'Drag from one corner of a room to the opposite corner.';
      case 'wall':
        return chain.length ? 'Tap the next corner, or type a length. Tap the last corner again to stop.' : 'Tap where the first wall starts.';
      case 'door':
        return 'Tap a wall where the door is.';
      case 'window':
        return 'Tap a wall where the window is.';
      case 'pin':
        return 'Tap where the thing is. Near a wall, the pin sits on it.';
      case 'label':
        return 'Tap where the words go.';
      case 'picture':
        return 'Drag the picture to line it up.';
    }
  });

  function fmtPin(p: PlanPin): string {
    return p.item_name ?? p.label ?? (p.sensitive ? 'Private pin' : 'Pin');
  }
</script>

<section class="plans" aria-labelledby="plans-h">
  <p><a href="#/home">← Home</a></p>
  <h1 id="plans-h">Floor plans</h1>

  {#if error}
    <p role="alert">{error}</p>
  {:else if floors === null}
    <p class="muted">Loading…</p>
  {:else if floors.length === 0}
    <div class="card intro">
      <p>
        A floor plan is optional. Draw your rooms to see their areas, put your things on the map, and print
        it in the house binder.
      </p>
      <p class="muted small">Start rough: a few rectangles are enough. You can trace over a photo of an existing plan.</p>
      <form class="row" onsubmit={addFloor}>
        <input aria-label="Floor name" bind:value={newFloor} placeholder="Ground floor" />
        <button class="primary" type="submit">Start a floor plan</button>
      </form>
    </div>
  {:else}
    <div class="floors row" role="group" aria-label="Floors">
      {#each floors as f (f.id)}
        <button class:primary={plan?.floor.id === f.id} aria-pressed={plan?.floor.id === f.id} onclick={() => switchFloor(f.id)}>{f.name}</button>
      {/each}
      <details class="addfloor">
        <summary class="button">Add a floor</summary>
        <form class="row" onsubmit={addFloor}>
          <input aria-label="New floor name" bind:value={newFloor} placeholder="Upstairs, Basement…" />
          <button type="submit">Add</button>
        </form>
      </details>
    </div>

    {#if plan}
      <p class="hint small" aria-live="polite" data-testid="plan-hint">{hint}</p>

      <div class="canvas" bind:clientWidth={size.w} bind:clientHeight={size.h}>
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <svg
          bind:this={svg}
          role="application"
          aria-label={`Plan of ${plan.floor.name}. Everything on it is also listed below.`}
          {viewBox}
          preserveAspectRatio="none"
          onpointerdown={down}
          onpointermove={moveP}
          onpointerup={up}
          onpointercancel={(e) => {
            pointers.delete(e.pointerId);
            pinch = null;
            drag = null;
            draftRoom = null;
          }}
          data-testid="plan-canvas"
        >
          {#each gridLines as g, i (i)}
            <line x1={g.x1} y1={g.y1} x2={g.x2} y2={g.y2} class="grid" class:major={g.major} vector-effect="non-scaling-stroke" />
          {/each}
          {#if plan.background && bgUrl}
            {@const at = drag?.kind === 'picture' ? drag.at : plan.background.at}
            <image href={bgUrl} x={at.x} y={at.y} width={plan.background.width} height={bgHeight} opacity={plan.background.opacity / 100} preserveAspectRatio="none" />
          {/if}
          {#each plan.spaces as s, i (i)}
            <polygon points={s.outline.map((p) => `${p.x},${p.y}`).join(' ')} class="space" class:sel={sel?.kind === 'space' && sel.index === i} data-hit={`space:${i}`} vector-effect="non-scaling-stroke" />
            <text x={s.label_at.x} y={s.label_at.y} font-size={px(13)} class="spacetext" text-anchor="middle">Tap to name · {s.area_text}</text>
          {/each}
          {#each plan.rooms as r (r.id)}
            {#if r.outline}
              {@const o = drag?.kind === 'vertex' && drag.room === r.id ? drag.outline : r.outline}
              <polygon points={o.map((p) => `${p.x},${p.y}`).join(' ')} class="room" class:sel={sel?.kind === 'room' && sel.id === r.id} data-hit={`room:${r.id}`} vector-effect="non-scaling-stroke" />
            {/if}
          {/each}
          {#each plan.walls as w (w.id)}
            {@const moving = drag?.kind === 'corner' ? drag : null}
            {@const a = moving && moving.from.x === w.a.x && moving.from.y === w.a.y ? moving.to : w.a}
            {@const b = moving && moving.from.x === w.b.x && moving.from.y === w.b.y ? moving.to : w.b}
            <line x1={a.x} y1={a.y} x2={b.x} y2={b.y} class="wall" class:sel={sel?.kind === 'wall' && sel.id === w.id} stroke-width={Math.max(w.thickness, px(3))} />
            <line x1={a.x} y1={a.y} x2={b.x} y2={b.y} class="hitline" stroke-width={px(22)} data-hit={`wall:${w.id}`} />
          {/each}
          {#each plan.openings as o (o.id)}
            {@const g = openingGeom(o)}
            {#if g}
              <line x1={g.s.x} y1={g.s.y} x2={g.e.x} y2={g.e.y} class="gap" stroke-width={Math.max(g.t, px(3)) + px(2)} />
              {#if o.kind === 'window'}
                <line x1={g.s.x} y1={g.s.y} x2={g.e.x} y2={g.e.y} class="glass" vector-effect="non-scaling-stroke" />
              {/if}
              {#if g.swing}<path d={g.swing} class="swing" vector-effect="non-scaling-stroke" />{/if}
              <line x1={g.s.x} y1={g.s.y} x2={g.e.x} y2={g.e.y} class="hitline" class:selhit={sel?.kind === 'opening' && sel.id === o.id} stroke-width={px(22)} data-hit={`opening:${o.id}`} />
            {/if}
          {/each}
          {#each plan.labels as l (l.id)}
            <text x={l.at.x} y={l.at.y} font-size={px(13)} class="label" class:sel={sel?.kind === 'label' && sel.id === l.id} text-anchor="middle" data-hit={`label:${l.id}`}>{l.text ?? 'Private label'}</text>
          {/each}
          {#if selRoom?.outline && mode === 'move' && plan.can_edit}
            {#each drag?.kind === 'vertex' && drag.room === selRoom.id ? drag.outline : selRoom.outline as v, i (i)}
              <circle cx={v.x} cy={v.y} r={px(11)} class="handle" data-hit={`vertex:${selRoom.id}:${i}`} />
            {/each}
          {/if}
          {#if mode === 'move' && plan.can_edit && plan.walls.length}
            {#each plan.corners as c, i (i)}
              {#if plan.walls.some((w) => (w.a.x === c.x && w.a.y === c.y) || (w.b.x === c.x && w.b.y === c.y))}
                {@const at = drag?.kind === 'corner' && drag.from.x === c.x && drag.from.y === c.y ? drag.to : c}
                <circle cx={at.x} cy={at.y} r={px(8)} class="corner" data-hit={`corner:${i}`} />
              {/if}
            {/each}
          {/if}
          {#each plan.pins as p (p.id)}
            {@const at = drag?.kind === 'pin' && drag.id === p.id ? drag.at : p.at}
            <g class="pin" class:sel={sel?.kind === 'pin' && sel.id === p.id} class:fact={!!p.fact_id} data-hit={`pin:${p.id}`}>
              <circle cx={at.x} cy={at.y} r={px(13)} />
              <text x={at.x} y={at.y + px(4.5)} font-size={px(12)} text-anchor="middle">{p.number}</text>
            </g>
          {/each}
          <!-- Room names above pins, with a halo, so both stay readable. -->
          {#each plan.rooms as r (r.id)}
            {#if r.outline && r.label_at}
              {@const at = drag?.kind === 'vertex' && drag.room === r.id ? labelPoint(drag.outline) : r.label_at}
              <text x={at.x} y={at.y} font-size={px(14)} class="roomname" text-anchor="middle">{r.name}</text>
              <text x={at.x} y={at.y + px(16)} font-size={px(12)} class="roomarea" text-anchor="middle">{r.area_text}</text>
            {/if}
          {/each}
          <!-- What is being drawn right now. -->
          {#if draftOutline.length}
            <polygon points={draftOutline.map((p) => `${p.x},${p.y}`).join(' ')} class="draft" vector-effect="non-scaling-stroke" />
          {/if}
          {#if sel?.kind === 'new-room'}
            <polygon points={sel.outline.map((p) => `${p.x},${p.y}`).join(' ')} class="draft" vector-effect="non-scaling-stroke" />
          {/if}
          {#if mode === 'wall' && chain.length}
            <polyline points={chain.map((p) => `${p.x},${p.y}`).join(' ')} class="chain" vector-effect="non-scaling-stroke" />
            {#if cursor}
              {@const last = chain[chain.length - 1]}
              <line x1={last.x} y1={last.y} x2={cursor.x} y2={cursor.y} class="rubber" vector-effect="non-scaling-stroke" />
            {/if}
            {#each chain as c, i (i)}<circle cx={c.x} cy={c.y} r={px(6)} class="chaindot" />{/each}
          {/if}
          {#if cursor && (mode === 'wall' || mode === 'room') && !drag}
            <circle cx={cursor.x} cy={cursor.y} r={px(5)} class="cursor" />
          {/if}
          {#if sel?.kind === 'new-pin'}
            <circle cx={sel.at.x} cy={sel.at.y} r={px(13)} class="newpin" />
          {/if}
        </svg>
        {#if readout}<div class="readout" data-testid="plan-readout" aria-live="polite">{readout}</div>{/if}
        <div class="zoom">
          <button aria-label="Zoom in" onclick={() => zoomBy(1.4)}>+</button>
          <button aria-label="Zoom out" onclick={() => zoomBy(1 / 1.4)}>−</button>
          <button aria-label="Show the whole floor" onclick={fitView}>Fit</button>
        </div>
      </div>

      {#if plan.can_edit}
        <div class="tools" role="toolbar" aria-label="Drawing tools">
          {#each [['move', 'Move', 'V'], ['room', 'Room', 'R'], ['wall', 'Walls', 'W'], ['door', 'Door', 'D'], ['window', 'Window', 'N'], ['pin', 'Pin', 'P'], ['label', 'Label', 'L']] as [m, label, key] (m)}
            <button class="tool" class:on={mode === m} aria-pressed={mode === m} title={`${label} (${key})`} onclick={() => setMode(m as Mode)}>{label}</button>
          {/each}
        </div>
      {/if}
      <div class="row under">
        {#if plan.can_edit}
          <button onclick={undo} disabled={!history.undo.length || busy} title="Undo (Ctrl+Z)">Undo</button>
          <button onclick={redo} disabled={!history.redo.length || busy} title="Redo (Ctrl+Shift+Z)">Redo</button>
        {/if}
        <span class="total" data-testid="plan-total">
          {#if plan.total_area_text}Total floor area <strong>{plan.total_area_text}</strong>{:else}<span class="muted">No rooms drawn yet</span>{/if}
        </span>
      </div>

      {#if mode === 'room'}
        <div class="row shapes" role="group" aria-label="Room shape">
          <button class:primary={shape === 'rect'} aria-pressed={shape === 'rect'} onclick={() => (shape = 'rect')}>Rectangle</button>
          <button class:primary={shape === 'l'} aria-pressed={shape === 'l'} onclick={() => (shape = 'l')}>L shape</button>
          <span class="small muted">Drag the inner corner of an L afterwards to fit.</span>
        </div>
      {/if}

      {#if mode === 'wall' && chain.length}
        <form class="card typed" onsubmit={typedWall}>
          <label for="wall-len">Next wall's length</label>
          <div class="row">
            <input id="wall-len" bind:value={typed} inputmode="text" placeholder={units === 'imperial' ? '12\' 6"' : '3.81 m'} autocomplete="off" />
            <button class="primary" type="submit">Add wall</button>
          </div>
          <p class="small muted">
            {#if typed.trim() && tryLength(typed, units) !== null}
              {len(tryLength(typed, units) ?? 0)}, towards the pointer (straight right if it has not moved).
            {:else if typed.trim()}
              Not a length yet. Try 12' 6", 150" or 3.81 m.
            {:else}
              It goes in the direction you last pointed.
            {/if}
          </p>
          <div class="row">
            <button type="button" onclick={finishChain}>Stop here</button>
            {#if chain.length > 2}<button type="button" onclick={closeChain}>Close the shape</button>{/if}
          </div>
        </form>
      {/if}

      <!-- The selection sheet. -->
      <div id="plan-sheet">
        {#if sel?.kind === 'new-room' || sel?.kind === 'space'}
          {@const outline = sel.kind === 'new-room' ? sel.outline : plan.spaces[sel.index]?.outline}
          <form class="card sheet" onsubmit={saveRoom} data-testid="room-sheet">
            <h2>Name this room</h2>
            <p class="small muted">{outline ? formatArea(areaMm2(outline), units) : ''}</p>
            <label for="room-name">Room</label>
            <input id="room-name" bind:value={nameInput} placeholder="Kitchen" autocomplete="off" />
            {#if freeRooms.length}
              <p class="small muted">Or one you already have:</p>
              <div class="row chips">
                {#each freeRooms.slice(0, 8) as r (r.id)}<button type="button" class="chip" onclick={() => (nameInput = r.name)}>{r.name}</button>{/each}
              </div>
            {/if}
            <div class="row">
              <button class="primary" type="submit" disabled={!nameInput.trim()}>Save room</button>
              <button type="button" class="quiet" onclick={() => (sel = null)}>Cancel</button>
            </div>
          </form>
        {:else if selRoom}
          <form class="card sheet" onsubmit={saveRoom} data-testid="room-sheet">
            <h2>{selRoom.name}</h2>
            <p>{selRoom.area_text} · around it {selRoom.perimeter_text}</p>
            <p class="small muted">{selRoom.things === 0 ? 'No things in this room yet.' : `${selRoom.things} ${selRoom.things === 1 ? 'thing' : 'things'} in this room.`} {plan.can_edit ? 'Drag its corners to reshape it.' : ''}</p>
            {#if plan.can_edit}
              <label for="room-rename">Name</label>
              <div class="row">
                <input id="room-rename" bind:value={nameInput} />
                <button type="submit">Rename</button>
              </div>
              <div class="row"><button type="button" class="quiet" onclick={() => removeRoomShape(selRoom)}>Take off the plan</button></div>
            {/if}
          </form>
        {:else if selWall}
          <form class="card sheet" onsubmit={(e) => saveWall(e, selWall)} data-testid="wall-sheet">
            <h2>Wall, {selWall.length_text}</h2>
            {#if plan.can_edit}
              <div class="two">
                <div>
                  <label for="wall-length">Length</label>
                  <input id="wall-length" bind:value={lengthInput} autocomplete="off" />
                </div>
                <div>
                  <label for="wall-thick">Thickness</label>
                  <input id="wall-thick" bind:value={thicknessInput} autocomplete="off" />
                </div>
              </div>
              <p class="small muted">A new length keeps the wall's start and moves its end.</p>
              <div class="row">
                <button class="primary" type="submit">Save</button>
                <button type="button" onclick={() => { setMode('door'); toast({ text: 'Tap the wall where the door is.' }, 3000); }}>Add a door</button>
                <button type="button" onclick={() => { setMode('window'); toast({ text: 'Tap the wall where the window is.' }, 3000); }}>Add a window</button>
                <button type="button" class="quiet" onclick={() => deleteWall(selWall)}>Delete wall</button>
              </div>
            {/if}
          </form>
        {:else if selOpening}
          <form class="card sheet" onsubmit={(e) => saveOpening(e, selOpening)} data-testid="opening-sheet">
            <h2>{selOpening.kind === 'door' ? 'Door' : selOpening.kind === 'window' ? 'Window' : 'Opening'}, {selOpening.width_text} wide</h2>
            {#if plan.can_edit}
              <div class="two">
                <div>
                  <label for="op-width">Width</label>
                  <input id="op-width" bind:value={widthInput} autocomplete="off" />
                </div>
                <div>
                  <label for="op-offset">From the wall's start</label>
                  <input id="op-offset" bind:value={offsetInput} autocomplete="off" />
                </div>
              </div>
              <div class="row">
                <button class="primary" type="submit">Save</button>
                {#if selOpening.kind === 'door'}
                  <button type="button" onclick={() => flip(selOpening, 'side')}>Swing other side</button>
                  <button type="button" onclick={() => flip(selOpening, 'hinge')}>Hinge other end</button>
                {/if}
                <button type="button" class="quiet" onclick={() => deleteOpening(selOpening)}>Delete</button>
              </div>
            {/if}
          </form>
        {:else if sel?.kind === 'new-pin' || selPin}
          <form class="card sheet" onsubmit={submitPin} data-testid="pin-sheet">
            {#if selPin}
              <h2>Pin {selPin.number}: {fmtPin(selPin)}</h2>
              <p class="small muted">{selPin.room_name ? `In ${selPin.room_name}. ` : ''}{selPin.fact_label ? `Linked to "${selPin.fact_label}".` : ''}</p>
              {#if selPin.item_id}
                <p><a class="button primary" href={`#/thing/${selPin.item_id}`}>Open {selPin.item_name}</a></p>
              {/if}
            {:else}
              <h2>{placing ? `Put ${placing.name} here` : 'What is here?'}</h2>
            {/if}
            {#if plan.can_edit}
              <label for="pin-search">Which thing</label>
              <input id="pin-search" bind:value={pinSearch} placeholder="Type to search your things" autocomplete="off" />
              <select aria-label="Thing" bind:value={pinItem}>
                <option value="">No thing: just a named spot</option>
                {#each pinChoices as i (i.id)}<option value={i.id}>{i.name}{i.room_name ? ` (${i.room_name})` : ''}</option>{/each}
                {#if pinItem && !pinChoices.some((i) => i.id === pinItem)}
                  <option value={pinItem}>{items.find((i) => i.id === pinItem)?.name ?? 'This thing'}</option>
                {/if}
              </select>
              <label for="pin-label">Or a name</label>
              <input id="pin-label" bind:value={pinLabel} placeholder="Water main shutoff, attic hatch…" autocomplete="off" />
              {#if shutoffFacts.length}
                <label for="pin-fact">Show it on the emergency page as</label>
                <select id="pin-fact" bind:value={pinFact}>
                  <option value="">Not a shutoff</option>
                  {#each shutoffFacts as f (f.id)}<option value={f.id}>{f.label}</option>{/each}
                </select>
              {/if}
              <label class="check"><input type="checkbox" bind:checked={pinPrivate} /> Private: leave it out of the main binder</label>
              <div class="row">
                <button class="primary" type="submit">{selPin ? 'Save' : 'Place pin'}</button>
                {#if selPin}<button type="button" class="quiet" onclick={() => deletePin(selPin)}>Remove pin</button>{/if}
                <button type="button" class="quiet" onclick={() => (sel = null)}>Close</button>
              </div>
            {/if}
          </form>
        {:else if sel?.kind === 'new-label' || sel?.kind === 'label'}
          {@const id = sel.kind === 'label' ? sel.id : null}
          <form class="card sheet" onsubmit={submitLabel} data-testid="label-sheet">
            <h2>Label</h2>
            <label for="label-text">Words</label>
            <input id="label-text" bind:value={labelText} placeholder="Stairs down, attic hatch" autocomplete="off" />
            <label class="check"><input type="checkbox" bind:checked={labelPrivate} /> Private: leave it out of the main binder</label>
            <div class="row">
              <button class="primary" type="submit" disabled={!labelText.trim()}>Save</button>
              {#if id}<button type="button" class="quiet" onclick={() => deleteLabel(id)}>Delete</button>{/if}
              <button type="button" class="quiet" onclick={() => (sel = null)}>Close</button>
            </div>
          </form>
        {/if}
      </div>

      <!-- Everything on the plan, as lists (the accessible way in). -->
      <h2>On this floor</h2>
      <details open={plan.rooms.length > 0 || plan.spaces.length > 0}>
        <summary>Rooms ({plan.rooms.filter((r) => r.outline).length})</summary>
        <ul class="list">
          {#each plan.rooms.filter((r) => r.outline) as r (r.id)}
            <li><button class="rowbtn" onclick={() => select({ kind: 'room', id: r.id })}><span>{r.name}</span><span class="muted">{r.area_text}</span></button></li>
          {/each}
          {#each plan.spaces as s, i (i)}
            <li><button class="rowbtn" onclick={() => select({ kind: 'space', index: i })}><span>Unnamed space: name it</span><span class="muted">{s.area_text}</span></button></li>
          {/each}
        </ul>
        {#if plan.total_area_text}<p class="small">Total: <strong>{plan.total_area_text}</strong></p>{/if}
      </details>
      <details open={plan.pins.length > 0}>
        <summary>Pins ({plan.pins.length})</summary>
        <ul class="list">
          {#each plan.pins as p (p.id)}
            <li><button class="rowbtn" onclick={() => select({ kind: 'pin', id: p.id })}><span>{p.number}. {fmtPin(p)}</span><span class="muted">{p.room_name ?? ''}</span></button></li>
          {/each}
        </ul>
      </details>
      <details>
        <summary>Walls ({plan.walls.length}), doors and windows ({plan.openings.length})</summary>
        <ul class="list">
          {#each plan.walls as w, i (w.id)}
            <li><button class="rowbtn" onclick={() => select({ kind: 'wall', id: w.id })}><span>Wall {i + 1}</span><span class="muted">{w.length_text}</span></button></li>
          {/each}
          {#each plan.openings as o (o.id)}
            <li><button class="rowbtn" onclick={() => select({ kind: 'opening', id: o.id })}><span>{o.kind === 'door' ? 'Door' : o.kind === 'window' ? 'Window' : 'Opening'}</span><span class="muted">{o.width_text}</span></button></li>
          {/each}
        </ul>
      </details>
      {#if plan.labels.length}
        <details>
          <summary>Labels ({plan.labels.length})</summary>
          <ul class="list">
            {#each plan.labels as l (l.id)}
              <li><button class="rowbtn" onclick={() => select({ kind: 'label', id: l.id })}><span>{l.text ?? 'Private label'}</span></button></li>
            {/each}
          </ul>
        </details>
      {/if}

      {#if plan.can_edit}
        <details data-testid="trace">
          <summary>Trace over a plan</summary>
          <p class="small muted">Use a photo or screenshot of an existing plan as a faint picture behind this one, then draw over it.</p>
          {#if plan.background}
            <form onsubmit={pictureWidth}>
              <label for="bg-width">How wide the drawing is in real life</label>
              <div class="row nowrap">
                <input id="bg-width" bind:value={bgWidth} placeholder={len(plan.background.width)} autocomplete="off" />
                <button type="submit">Set</button>
              </div>
            </form>
            <label for="bg-opacity">How strong</label>
            <input id="bg-opacity" type="range" min="10" max="90" value={plan.background.opacity} onchange={(e) => setPicture({ opacity: Number((e.target as HTMLInputElement).value) })} />
            <div class="row">
              <button class:primary={mode === 'picture'} aria-pressed={mode === 'picture'} onclick={() => setMode(mode === 'picture' ? 'move' : 'picture')}>{mode === 'picture' ? 'Done moving' : 'Move the picture'}</button>
              <button class="quiet" onclick={() => setPicture({ remove: true })}>Remove the picture</button>
            </div>
          {:else}
            <label class="button" for="bg-file">Choose a photo of a plan</label>
            <input id="bg-file" class="visually-hidden" type="file" accept="image/*" onchange={pickPicture} />
          {/if}
        </details>
      {/if}

      <details>
        <summary>This floor</summary>
        <div class="stack">
          <button onclick={() => openFile('binder_pdf', { edition: 'full', volume: 'main', sections: [`plan:${plan!.floor.id}`], record: false }, `floor-plan-${plan!.floor.name}.pdf`, 'application/pdf').catch((e) => toast({ text: errorText(e) }))} disabled={!plan.floor.drawn}>Print this floor (PDF)</button>
          <p class="small muted">Pins marked private are left out of the printout.</p>
          <div class="row" role="group" aria-label="Units">
            <span class="small">Lengths in</span>
            <button class:primary={units === 'imperial'} aria-pressed={units === 'imperial'} onclick={() => setUnits('imperial')} disabled={!plan.can_edit}>Feet and inches</button>
            <button class:primary={units === 'metric'} aria-pressed={units === 'metric'} onclick={() => setUnits('metric')} disabled={!plan.can_edit}>Metres</button>
          </div>
          {#if plan.can_edit}
            <form class="row" onsubmit={(e) => { e.preventDefault(); renameFloor((e.currentTarget.elements.namedItem('fname') as HTMLInputElement).value); }}>
              <input name="fname" aria-label="Floor name" value={plan.floor.name} />
              <button type="submit">Rename</button>
            </form>
            <button class="quiet" onclick={deleteFloor}>Delete this floor's plan</button>
          {/if}
        </div>
      </details>
    {/if}
  {/if}
</section>

<style>
  .floors {
    gap: 0.4rem;
    margin-bottom: 0.5rem;
  }
  .floors button {
    min-height: 40px;
  }
  .addfloor summary {
    list-style: none;
    min-height: 40px;
  }
  .addfloor form {
    margin-top: 0.4rem;
  }
  /* Under the drawing, where a thumb reaches; two rows on a phone. */
  .tools {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 0.25rem;
    margin-top: 0.5rem;
  }
  @media (max-width: 520px) {
    .tools {
      grid-template-columns: repeat(4, 1fr);
    }
  }
  .tool {
    padding: 0 0.2rem;
    min-width: 0;
    font-size: 0.85rem;
  }
  .tool.on {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
  }
  /* Two lines always, so the drawing does not jump when the hint changes. */
  .hint {
    margin: 0.3rem 0;
    color: var(--muted);
    height: 2.7em;
    overflow: hidden;
    line-height: 1.35;
  }
  .canvas {
    position: relative;
    height: min(55vh, 560px);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    overflow: hidden;
    background: var(--surface);
    touch-action: none;
  }
  svg {
    width: 100%;
    height: 100%;
    display: block;
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
  }
  .grid {
    stroke: var(--line);
    stroke-width: 0.5;
    opacity: 0.6;
  }
  .grid.major {
    opacity: 1;
    stroke-width: 1;
  }
  .room {
    fill: var(--accent-soft);
    fill-opacity: 0.7;
    stroke: var(--accent);
    stroke-width: 1.5;
  }
  .room.sel {
    stroke-width: 3;
    fill-opacity: 0.95;
  }
  .space {
    fill: var(--attention-soft);
    fill-opacity: 0.7;
    stroke: none;
  }
  .space.sel {
    stroke: var(--attention);
    stroke-width: 2;
  }
  .spacetext {
    fill: var(--attention);
    pointer-events: none;
  }
  .wall {
    stroke: var(--text);
    stroke-linecap: square;
  }
  .wall.sel {
    stroke: var(--focus);
  }
  .hitline {
    stroke: transparent;
    stroke-linecap: round;
  }
  .hitline.selhit {
    stroke: var(--focus);
    stroke-opacity: 0.25;
  }
  .gap {
    stroke: var(--surface);
    stroke-linecap: butt;
  }
  .glass {
    stroke: var(--text);
    stroke-width: 1.5;
  }
  .swing {
    fill: none;
    stroke: var(--muted);
    stroke-width: 1;
  }
  .roomname,
  .roomarea {
    paint-order: stroke;
    stroke: var(--surface);
    stroke-width: 3px;
    stroke-linejoin: round;
    pointer-events: none;
  }
  .roomname {
    fill: var(--text);
    font-weight: 650;
  }
  .roomarea {
    fill: var(--muted);
  }
  .label {
    fill: var(--text);
    font-style: italic;
  }
  .label.sel {
    fill: var(--focus);
  }
  .handle {
    fill: var(--surface);
    stroke: var(--focus);
    stroke-width: 2;
    vector-effect: non-scaling-stroke;
  }
  .corner {
    fill: var(--surface);
    stroke: var(--text);
    stroke-width: 1.5;
    vector-effect: non-scaling-stroke;
  }
  .pin circle {
    fill: var(--surface);
    stroke: var(--accent);
    stroke-width: 2.5;
    vector-effect: non-scaling-stroke;
  }
  .pin.fact circle {
    fill: var(--text);
    stroke: var(--text);
  }
  .pin.fact text {
    fill: var(--bg);
  }
  .pin.sel circle {
    stroke: var(--focus);
    stroke-width: 4;
  }
  .pin text {
    fill: var(--text);
    font-weight: 700;
    pointer-events: none;
  }
  .draft {
    fill: var(--accent-soft);
    fill-opacity: 0.5;
    stroke: var(--accent);
    stroke-width: 2;
    stroke-dasharray: 6 4;
  }
  .chain {
    fill: none;
    stroke: var(--focus);
    stroke-width: 3;
  }
  .rubber {
    stroke: var(--focus);
    stroke-width: 2;
    stroke-dasharray: 6 4;
  }
  .chaindot,
  .cursor {
    fill: var(--focus);
  }
  .newpin {
    fill: none;
    stroke: var(--focus);
    stroke-width: 3;
    stroke-dasharray: 4 3;
    vector-effect: non-scaling-stroke;
  }
  .readout {
    position: absolute;
    left: 0.5rem;
    top: 0.5rem;
    background: var(--text);
    color: var(--bg);
    padding: 0.25rem 0.6rem;
    border-radius: 8px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    pointer-events: none;
  }
  .zoom {
    position: absolute;
    right: 0.5rem;
    bottom: 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }
  .zoom button {
    min-height: 44px;
    min-width: 44px;
    padding: 0;
    font-size: 1.2rem;
    opacity: 0.92;
  }
  .zoom button:last-child {
    font-size: 0.85rem;
  }
  .under {
    margin: 0.5rem 0;
  }
  .total {
    margin-left: auto;
  }
  .shapes {
    margin: 0.3rem 0;
  }
  .typed label,
  .sheet label {
    margin-top: 0.5rem;
  }
  .typed .row input {
    flex: 1;
    min-width: 8rem;
    width: auto;
  }
  .sheet {
    margin: 0.6rem 0;
    border-color: var(--focus);
  }
  .sheet h2 {
    margin-top: 0;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.6rem;
  }
  .chips {
    gap: 0.35rem;
  }
  .chip {
    min-height: 36px;
    font-size: 0.9rem;
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
  .list {
    list-style: none;
    padding: 0;
    margin: 0.3rem 0;
  }
  .rowbtn {
    width: 100%;
    justify-content: space-between;
    border-radius: 8px;
    margin: 0.15rem 0;
  }
  details {
    margin: 0.5rem 0;
  }
  summary {
    cursor: pointer;
    font-weight: 600;
    min-height: 44px;
    display: flex;
    align-items: center;
  }
  .nowrap {
    flex-wrap: nowrap;
  }
  .nowrap input {
    flex: 1;
    width: auto;
    min-width: 0;
  }
  .intro form input {
    flex: 1;
    min-width: 10rem;
    width: auto;
  }
  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
</style>
