<script lang="ts">
  // Add or edit a contractor (D-28). Everything but the name is optional.
  // The "Just for you" part is the household's own opinion: it is never
  // printed, never sent to Home Assistant or any server extra, and travels
  // only inside end-to-end encrypted sync.
  import { call, type ContractorInfo, type ContractorIn } from '../lib/api';
  import { toast } from '../lib/state.svelte';
  import { resync } from '../lib/reminders';
  import { errorText } from '../lib/actions';

  let {
    start = null,
    onsaved,
    oncancel,
  }: {
    start?: ContractorInfo | null;
    onsaved: (list: ContractorInfo[], name: string) => void;
    oncancel: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  const c = start;
  let name = $state(c?.name ?? '');
  let company = $state(c?.company ?? '');
  let trades = $state((c?.trades ?? []).join(', '));
  let phone = $state(c?.phone ?? '');
  let email = $state(c?.email ?? '');
  let website = $state(c?.website ?? '');
  let area = $state(c?.service_area ?? '');
  let licenceNumber = $state(c?.licence?.number ?? '');
  let licenceExpires = $state(c?.licence?.expires ?? '');
  let insuranceExpires = $state(c?.insurance?.expires ?? '');
  let insuranceNumber = $state(c?.insurance?.number ?? '');
  let notes = $state(c?.notes ?? '');
  let notesPrivate = $state(c?.notes_private ?? false);
  let emergency = $state(c?.emergency ?? false);
  let rating = $state<number>(c?.private?.rating ?? 0);
  let again = $state<'yes' | 'no' | ''>(
    c?.private?.would_hire_again === true ? 'yes' : c?.private?.would_hire_again === false ? 'no' : '',
  );
  let privateNotes = $state(c?.private?.notes ?? '');
  const showPrivate = $derived(!c || c.private !== null);
  let busy = $state(false);

  async function save(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    try {
      const args: ContractorIn = {
        id: c?.id,
        name,
        company,
        trades: trades
          .split(',')
          .map((t) => t.trim())
          .filter(Boolean),
        phone,
        email,
        website,
        service_area: area,
        notes,
        notes_private: notesPrivate,
        emergency,
        // A number hidden from this person is left as it was.
        ...(c?.licence && c.licence.number === null ? {} : { licence_number: licenceNumber }),
        licence_expires: licenceExpires,
        ...(c?.insurance && c.insurance.number === null ? {} : { insurance_number: insuranceNumber }),
        insurance_expires: insuranceExpires,
      };
      if (showPrivate) {
        args.private_rating = rating;
        args.would_hire_again = again;
        args.private_notes = privateNotes;
      }
      const list = await call('save_contractor', args);
      // Licence and insurance dates decide their notices (D-138).
      resync();
      onsaved(list, name.trim());
    } catch (err) {
      toast({ text: errorText(err) });
    } finally {
      busy = false;
    }
  }
</script>

<form class="card stack" onsubmit={save} data-testid="contractor-form">
  <div>
    <label for="k-name">Name</label>
    <input id="k-name" bind:value={name} required placeholder="Who you deal with" autocomplete="off" />
  </div>
  <div>
    <label for="k-company">Company (optional)</label>
    <input id="k-company" bind:value={company} autocomplete="off" />
  </div>
  <div>
    <label for="k-trades">What they do</label>
    <input id="k-trades" bind:value={trades} placeholder="Plumbing, water heaters" autocomplete="off" />
    <p class="small muted">Separate several with commas.</p>
  </div>
  <div class="row two">
    <div>
      <label for="k-phone">Phone</label>
      <input id="k-phone" type="tel" bind:value={phone} autocomplete="off" />
    </div>
    <div>
      <label for="k-email">Email</label>
      <input id="k-email" type="email" bind:value={email} autocomplete="off" />
    </div>
  </div>
  <div class="row two">
    <div>
      <label for="k-web">Website</label>
      <input id="k-web" bind:value={website} placeholder="example.com" autocomplete="off" />
    </div>
    <div>
      <label for="k-area">Where they work</label>
      <input id="k-area" bind:value={area} placeholder="The towns they cover" autocomplete="off" />
    </div>
  </div>

  <fieldset>
    <legend>Licence and insurance</legend>
    <p class="small muted">Worth checking before work starts. With an end date, you get one reminder a month before, if you would hire them again.</p>
    <div class="row two">
      <div>
        <label for="k-lic">Licence number</label>
        <input
          id="k-lic"
          bind:value={licenceNumber}
          placeholder={c?.licence && c.licence.number === null ? 'Set, hidden' : ''}
          autocomplete="off"
        />
      </div>
      <div>
        <label for="k-lic-exp">Licence ends</label>
        <input id="k-lic-exp" type="date" bind:value={licenceExpires} />
      </div>
    </div>
    <div class="row two">
      <div>
        <label for="k-ins">Insurance policy (optional)</label>
        <input
          id="k-ins"
          bind:value={insuranceNumber}
          placeholder={c?.insurance && c.insurance.number === null ? 'Set, hidden' : ''}
          autocomplete="off"
        />
      </div>
      <div>
        <label for="k-ins-exp">Insurance ends</label>
        <input id="k-ins-exp" type="date" bind:value={insuranceExpires} />
      </div>
    </div>
    <p class="small muted">Attach the insurance certificate on their page after saving.</p>
  </fieldset>

  <div>
    <label for="k-notes">Notes</label>
    <textarea id="k-notes" bind:value={notes} placeholder="Ask for Dana; they need a day's notice"></textarea>
    <label class="check"><input type="checkbox" bind:checked={notesPrivate} /> Keep these notes out of the main binder</label>
  </div>
  <label class="check"><input type="checkbox" bind:checked={emergency} /> Call them in an emergency (listed on the binder's first page)</label>

  {#if showPrivate}
    <fieldset class="private" data-testid="private-fields">
      <legend>Just for you <span class="pill">Private</span></legend>
      <p class="small muted">
        Your own opinion. Never printed in the binder or the inventory, never sent to Home Assistant, and only
        shared with your household's devices, encrypted.
      </p>
      <label for="k-rating">Your rating</label>
      <select id="k-rating" bind:value={rating}>
        <option value={0}>Not rated</option>
        <option value={5}>5: excellent</option>
        <option value={4}>4: good</option>
        <option value={3}>3: fine</option>
        <option value={2}>2: poor</option>
        <option value={1}>1: never again</option>
      </select>
      <label for="k-again">Would you hire them again?</label>
      <select id="k-again" bind:value={again}>
        <option value="">Not sure yet</option>
        <option value="yes">Yes</option>
        <option value="no">No</option>
      </select>
      <label for="k-private">Private notes</label>
      <textarea id="k-private" bind:value={privateNotes} placeholder="Quotes, how it went"></textarea>
    </fieldset>
  {/if}

  <div class="row">
    <button class="primary" type="submit" disabled={busy || !name.trim()}>Save</button>
    <button type="button" class="quiet" onclick={oncancel}>Cancel</button>
  </div>
</form>

<style>
  .two > div {
    flex: 1;
    min-width: 10rem;
  }
  .two {
    flex-wrap: wrap;
    align-items: flex-start;
  }
  fieldset {
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 0.5rem 0.75rem 0.75rem;
    margin: 0;
  }
  legend {
    font-weight: 600;
    padding: 0 0.3rem;
  }
  .private {
    background: var(--surface-2);
  }
  .check {
    display: flex;
    gap: 0.6rem;
    align-items: center;
    font-weight: normal;
  }
  .check input {
    width: 1.4rem;
    min-height: 1.4rem;
  }
</style>
