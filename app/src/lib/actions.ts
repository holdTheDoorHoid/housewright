import { call, type Card } from './api';
import { refresh, toast, go } from './state.svelte';
import { resync } from './reminders';

/** One-tap Done (P1). Nothing is asked first; a note can be added after. */
export async function markDone(card: Pick<Card, 'id' | 'title'>): Promise<void> {
  const r = await call('complete', { id: card.id });
  refresh();
  resync();
  toast({
    text: `Done: ${card.title}.`,
    actions: [
      { label: 'Add a note', run: () => go(`#/task/${card.id}`) },
      {
        label: 'Undo',
        run: async () => {
          await call('void_completion', { id: r.completion_id });
          refresh();
          resync();
          toast({ text: 'Undone. The record stays in History.' }, 4000);
        },
      },
    ],
  });
}

export function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
