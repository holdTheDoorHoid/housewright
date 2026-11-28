<script lang="ts">
  // "Quirks only you know" (D-42): short notes about the house, a room or a
  // thing, printed in the house binder. A note that sounds like a hiding
  // place or a code is kept for the sealed volume (D-41); the person can
  // change that.
  import { call, type NoteAbout, type NoteInfo } from '../lib/api';
  import { toast } from '../lib/state.svelte';
  import { errorText } from '../lib/actions';

  let { aboutKind, aboutId, hint = 'The back door sticks in damp weather: lift the handle.' }: {
    aboutKind: NoteAbout;
    aboutId: string;
    hint?: string;
  } = $props();

  let notes = $state<NoteInfo[]>([]);
  let text = $state('');
  let sealed = $state(false);

  $effect(() => {
    call('notes', { about_kind: aboutKind, about_id: aboutId })
      .then((n) => (notes = n))
      .catch((e) => toast({ text: errorText(e) }));
  });

  async function add(e: SubmitEvent) {
    e.preventDefault();
    if (!text.trim()) return;
    try {
      notes = await call('add_note', {
        about_kind: aboutKind,
        about_id: aboutId,
        text,
        sensitive: sealed ? true : undefined,
      });
      const last = notes[notes.length - 1];
      if (last?.sensitive && !sealed) toast({ text: 'That sounds private, so it is kept for the sealed volume.' });
      text = '';
      sealed = false;
    } catch (err) {
      toast({ text: errorText(err) });
    }
  }

  async function remove(id: string) {
    if (!confirm('Remove this note?')) return;
    await call('delete_note', { id });
    notes = notes.filter((n) => n.id !== id);
  }
</script>

<ul class="quirks">
  {#each notes as n (n.id)}
    <li class="row">
      <span class="grow">
        {n.text ?? 'Private note (only the owner can see it)'}
        {#if n.sensitive}<span class="pill">sealed volume</span>{/if}
      </span>
      <button class="quiet" onclick={() => remove(n.id)} aria-label="Remove note">Remove</button>
    </li>
  {/each}
</ul>
<form onsubmit={add}>
  <label for={`quirk-${aboutId}`}>Add a quirk only you know</label>
  <textarea id={`quirk-${aboutId}`} bind:value={text} placeholder={hint}></textarea>
  <label class="check"><input type="checkbox" bind:checked={sealed} /> Private: print it only in the sealed volume</label>
  <button type="submit" disabled={!text.trim()}>Add note</button>
</form>

<style>
  .quirks {
    list-style: none;
    padding: 0;
    margin: 0.4rem 0;
  }
  .quirks li {
    border-bottom: 1px solid var(--line);
    padding: 0.3rem 0;
  }
  .grow {
    flex: 1;
  }
  .check {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    font-weight: normal;
  }
  .check input {
    width: auto;
    min-height: 0;
  }
</style>
