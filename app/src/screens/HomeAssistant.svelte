<script lang="ts">
  // "Connect Home Assistant" (D-117): plain steps, and what Home Assistant
  // can see. The work is done by the bridge, a Housewright device that runs
  // next to Home Assistant.
</script>

<section aria-labelledby="ha-h">
  <h1 id="ha-h">Connect Home Assistant</h1>
  <p>
    Home Assistant can show your maintenance tasks (a to-do list, a calendar, a sensor for each task), mark them
    done, and tell Housewright when something happens: a leak sensor goes off, a smoke alarm's battery runs low, the
    washer finishes another load.
  </p>

  <h2>What you need</h2>
  <ul>
    <li>Sync turned on in <a href="#/sync">Sync and devices</a>. Home Assistant joins your household the same way a
      phone does, so it needs a sync server. The Housewright add-on for Home Assistant can be that server.</li>
    <li>The Housewright bridge running next to Home Assistant. With Home Assistant OS, install the Housewright
      add-on, which runs it for you. Otherwise run <code>housewright-bridge</code> on the same computer.</li>
  </ul>

  <h2>Steps</h2>
  <ol class="steps">
    <li>Start the bridge (or the add-on). Its log shows an address and a token.</li>
    <li>In Home Assistant, go to Settings, Devices and services, Add integration, and choose Housewright. Enter the
      address and the token.</li>
    <li>Here, open <a href="#/sync">Sync and devices</a>, choose Add a device or person, then
      <strong>Home Assistant</strong>. Type the code it shows into Home Assistant.</li>
    <li>Both show a six-digit number. If they are the same, say so in Home Assistant, then tap
      <strong>They match</strong> here.</li>
  </ol>
  <p class="muted small">
    The bridge then appears in your device list as “Home Assistant bridge”. Things it records show in History as
    “via Home Assistant”.
  </p>

  <h2>Linking sensors</h2>
  <p>
    In the integration's options you can send a sensor's number to a meter (runtime hours, washer cycles), so tasks
    like “every 40 loads” come due by themselves, and report an event when a sensor turns on (a leak under the water
    heater, a low battery). You can also add Home Assistant's devices to Housewright: their tasks wait as suggestions
    until you switch them on.
  </p>

  <h2>What Home Assistant can see</h2>
  <p>
    The bridge is a full member of your household: like a family member's phone, it holds the household's keys and a
    copy of everything, on the computer that runs Home Assistant. It only passes task names, due dates, things,
    rooms and people's names to Home Assistant. It never passes on anything marked sensitive (codes, serial numbers,
    notes, costs, where valuables are), nor things marked private. Anyone who controls that computer could still read
    the bridge's copy, so keep it as safe as your phone. You can remove the bridge at any time in
    <a href="#/sync">Sync and devices</a>.
  </p>
</section>

<style>
  .steps li {
    margin-bottom: 0.5rem;
  }
</style>
