import { call, type Card } from './api';
import { refresh, toast, go } from './state.svelte';
import { resync } from './reminders';

/** One-tap Done (P1). Nothing is asked first; a note, photo or cost can be
 * added after ("Add details"), never required. */
export async function markDone(card: Pick<Card, 'id' | 'title'>): Promise<void> {
  const r = await call('complete', { id: card.id });
  refresh();
  resync();
  toast({
    text: `Done: ${card.title}.`,
    actions: [
      { label: 'Add details', run: () => go(`#/task/${card.id}/details`) },
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
  if (e instanceof Error) return e.message;
  // Tauri and plugin errors can arrive as plain objects or strings.
  if (e && typeof e === 'object' && 'message' in e) return String((e as { message: unknown }).message);
  return String(e);
}
