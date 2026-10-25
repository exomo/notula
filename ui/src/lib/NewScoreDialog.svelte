<script lang="ts">
  import { api, type Instrument, type ScoreSetup } from "./api";
  import { editor } from "./editor.svelte";

  // Major keys by number of accidentals, -7 (seven flats) to 7 (seven sharps).
  const KEY_NAMES = ["C♭", "G♭", "D♭", "A♭", "E♭", "B♭", "F", "C", "G", "D", "A", "E", "B", "F♯", "C♯"];
  const DENOMINATORS = [2, 4, 8, 16];

  let dialog = $state<HTMLDialogElement | null>(null);
  let presets = $state<Instrument[]>([]);
  let title = $state("");
  let composer = $state("");
  let chosen = $state<{ presetId: string; staves: number }[]>([{ presetId: "piano", staves: 2 }]);
  let numerator = $state(4);
  let denominator = $state(4);
  let fifths = $state(0);
  let measures = $state(16);
  let toAdd = $state("");
  let pickupChoice = $state("");

  const TICKS_PER_WHOLE = 3840;
  const PICKUP_LENGTHS = [
    { label: "1/16", ticks: 240 },
    { label: "1/8", ticks: 480 },
    { label: "3/16", ticks: 720 },
    { label: "1/4", ticks: 960 },
    { label: "3/8", ticks: 1440 },
    { label: "1/2", ticks: 1920 },
    { label: "3/4", ticks: 2880 },
  ];
  // Only lengths shorter than the bar make sense for a pickup.
  const pickupOptions = $derived(
    PICKUP_LENGTHS.filter((p) => p.ticks < (numerator * TICKS_PER_WHOLE) / denominator),
  );

  $effect(() => {
    if (!dialog) return;
    if (editor.newScoreOpen && !dialog.open) {
      if (presets.length === 0) api.instrumentPresets().then((p) => (presets = p));
      dialog.showModal();
    } else if (!editor.newScoreOpen && dialog.open) {
      dialog.close();
    }
  });

  function nameOf(id: string) {
    return presets.find((p) => p.id === id)?.name ?? id;
  }

  function add() {
    const preset = presets.find((item) => item.id === toAdd);
    if (preset) chosen = [...chosen, { presetId: preset.id, staves: preset.staves }];
  }

  function setStaffCount(index: number, staves: number) {
    chosen = chosen.map((instrument, i) => (i === index ? { ...instrument, staves } : instrument));
  }

  function move(index: number, by: number) {
    const target = index + by;
    if (target < 0 || target >= chosen.length) return;
    const next = [...chosen];
    [next[index], next[target]] = [next[target], next[index]];
    chosen = next;
  }

  const valid = $derived(
    chosen.length > 0 && numerator >= 1 && numerator <= 32 && measures >= 1 && measures <= 999,
  );

  async function create(event: SubmitEvent) {
    event.preventDefault();
    const setup: ScoreSetup = {
      title,
      composer,
      instruments: chosen.map(({ presetId, staves }) => ({ preset_id: presetId, staves })),
      time_numerator: numerator,
      time_denominator: denominator,
      key_fifths: fifths,
      measures,
      pickup: pickupOptions.some((p) => String(p.ticks) === pickupChoice)
        ? Number(pickupChoice)
        : null,
    };
    if (await editor.newScore(setup)) editor.newScoreOpen = false;
  }
</script>

<dialog bind:this={dialog} onclose={() => (editor.newScoreOpen = false)}>
  <form onsubmit={create}>
    <h2>New score</h2>

    <label>Title <input bind:value={title} /></label>
    <label>Composer <input bind:value={composer} /></label>

    <fieldset>
      <legend>Instruments</legend>
      <ol>
        {#each chosen as instrument, index}
          <li>
            <span>{nameOf(instrument.presetId)}</span>
            {#if (presets.find((preset) => preset.id === instrument.presetId)?.staves ?? 1) > 1}
              <select
                aria-label="Staff count for {nameOf(instrument.presetId)} {index + 1}"
                value={instrument.staves}
                onchange={(event) => setStaffCount(index, Number(event.currentTarget.value))}
              >
                <option value={1}>One staff</option>
                <option value={2}>Two staves</option>
              </select>
            {/if}
            <button type="button" onclick={() => move(index, -1)} disabled={index === 0} aria-label="Move up">↑</button>
            <button type="button" onclick={() => move(index, 1)} disabled={index === chosen.length - 1} aria-label="Move down">↓</button>
            <button type="button" onclick={() => (chosen = chosen.filter((_, i) => i !== index))} aria-label="Remove">✕</button>
          </li>
        {/each}
      </ol>
      <div class="add">
        <select bind:value={toAdd}>
          <option value="" disabled>Choose an instrument…</option>
          {#each presets as preset}
            <option value={preset.id}>{preset.name}</option>
          {/each}
        </select>
        <button type="button" onclick={add} disabled={!toAdd}>Add</button>
      </div>
    </fieldset>

    <div class="row">
      <label>
        Time signature
        <span class="time">
          <input type="number" min="1" max="32" bind:value={numerator} />
          /
          <select bind:value={denominator}>
            {#each DENOMINATORS as d}
              <option value={d}>{d}</option>
            {/each}
          </select>
        </span>
      </label>
      <label>
        Key
        <select bind:value={fifths}>
          {#each KEY_NAMES as name, index}
            <option value={index - 7}>{name} major</option>
          {/each}
        </select>
      </label>
      <label>Measures <input type="number" min="1" max="999" bind:value={measures} /></label>
      <label>
        Pickup
        <select bind:value={pickupChoice}>
          <option value="">None</option>
          {#each pickupOptions as option}
            <option value={String(option.ticks)}>{option.label}</option>
          {/each}
        </select>
      </label>
    </div>

    {#if editor.message}
      <p class="error">{editor.message}</p>
    {/if}

    <div class="buttons">
      <button type="button" onclick={() => (editor.newScoreOpen = false)}>Cancel</button>
      <button type="submit" disabled={!valid}>Create</button>
    </div>
  </form>
</dialog>

<style>
  dialog {
    box-sizing: border-box;
    width: min(36rem, calc(100vw - 2rem));
    max-height: calc(100vh - 2rem);
    overflow: auto;
    border: 1px solid #888;
    border-radius: 6px;
    padding: 1rem 1.25rem;
  }

  form {
    display: grid;
    gap: 0.75rem;
  }

  h2 {
    margin: 0;
  }

  label {
    display: grid;
    gap: 0.2rem;
  }

  fieldset {
    display: grid;
    gap: 0.5rem;
    min-width: 0;
  }

  ol {
    margin: 0;
    padding-left: 1.2rem;
  }

  li {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    margin-bottom: 0.2rem;
  }

  li span {
    flex: 1;
  }

  .add,
  .time {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem;
  }

  input[type="number"] {
    width: 4.5rem;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }

  .error {
    margin: 0;
    color: #b00020;
  }

  @media (max-width: 520px) {
    dialog {
      padding: 0.8rem;
    }

    li {
      flex-wrap: wrap;
    }

    li span {
      flex: 1 0 100%;
    }
  }
</style>
