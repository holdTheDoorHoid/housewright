import type { StartingPlan } from './api';

/** The result of the last `onboard`, handed to the starting-plan screen so it
 * can show the plan in the order it was chosen and say what changed. */
let note: { mode: 'first' | 'redo'; plan: StartingPlan } | null = null;

export function setPlanNote(mode: 'first' | 'redo', plan: StartingPlan): void {
  note = { mode, plan };
}

export function takePlanNote(): { mode: 'first' | 'redo'; plan: StartingPlan } | null {
  const n = note;
  note = null;
  return n;
}
