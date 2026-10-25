<script lang="ts">
  import { api, type Instrument, type ScorePart } from "./api";
  import { editor } from "./editor.svelte";

  let dialog = $state<HTMLDialogElement | null>(null);
  let presets = $state<Instrument[]>([]);
  let parts = $state<ScorePart[]>([]);
  let removedPartIds = $state<number[]>([]);
  let partNames = $state<string[]>([]);
  let partShortNames = $state<string[]>([]);
  let selectedPresets = $state<string[]>([]);
  let selectedStaffCounts = $state<number[]>([]);
  let additions = $state<{ presetId: string; name: string; shortName: string; staves: number }[]>([]);
  let pendingRemoval = $state<{ partId: number; name: string } | null>(null);
  let title = $state("");
  let composer = $state("");
  let tempoBeatUnit = $state<import("./api").NoteValue>("Quarter");
  let tempoUnitsPerMinute = $state(120);
  let toAdd = $state("");
  let loaded = $state(false);

  $effect(() => {
    if (!dialog) return;
    if (editor.scorePropertiesOpen && !dialog.open) {
      dialog.showModal();
      void loadProperties();
    } else if (!editor.scorePropertiesOpen && dialog.open) {
      dialog.close();
    }
  });

  async function loadProperties() {
    loaded = false;
    editor.message = null;
    try {
      const [available, current] = await Promise.all([
        api.instrumentPresets(),
        api.scoreProperties(),
      ]);
      if (!editor.scorePropertiesOpen) return;
      presets = available;
      parts = current.parts;
      removedPartIds = [];
      pendingRemoval = null;
      partNames = current.parts.map((part) => part.name);
      partShortNames = current.parts.map((part) => part.short_name);
      title = current.title;
      composer = current.composer;
      tempoBeatUnit = current.tempo.beat_unit;
      tempoUnitsPerMinute = current.tempo.units_per_minute;
      selectedPresets = current.parts.map((part) =>
        available.find((preset) => preset.name === part.name && preset.staves >= part.staves)?.id ?? "",
      );
      selectedStaffCounts = current.parts.map((part) => part.staves);
      additions = [];
      toAdd = "";
      loaded = true;
    } catch (error) {
      editor.message = String(error);
    }
  }

  function setPreset(index: number, value: string) {
    selectedPresets = selectedPresets.map((preset, i) => (i === index ? value : preset));
    const selected = presets.find((preset) => preset.id === value);
    if (selected && selectedStaffCounts[index] > selected.staves) {
      selectedStaffCounts = selectedStaffCounts.map((count, i) => (i === index ? selected.staves : count));
    }
  }

  function setStaffCount(index: number, count: number) {
    selectedStaffCounts = selectedStaffCounts.map((value, i) => (i === index ? count : value));
  }

  function setAdditionStaffCount(index: number, count: number) {
    additions = additions.map((addition, i) => (i === index ? { ...addition, staves: count } : addition));
  }

  function moveInstrument(index: number, offset: -1 | 1) {
    const target = index + offset;
    if (target < 0 || target >= parts.length) return;

    const swap = <T,>(items: T[]) => {
      const reordered = [...items];
      [reordered[index], reordered[target]] = [reordered[target], reordered[index]];
      return reordered;
    };
    parts = swap(parts);
    partNames = swap(partNames);
    partShortNames = swap(partShortNames);
    selectedPresets = swap(selectedPresets);
    selectedStaffCounts = swap(selectedStaffCounts);
  }

  function removeInstrument(index: number) {
    pendingRemoval = { partId: parts[index].id, name: partNames[index] };
  }

  function confirmRemoveInstrument() {
    if (!pendingRemoval) return;
    const index = parts.findIndex((part) => part.id === pendingRemoval?.partId);
    if (index < 0) {
      pendingRemoval = null;
      return;
    }

    removedPartIds = [...removedPartIds, pendingRemoval.partId];
    parts = parts.filter((_, i) => i !== index);
    partNames = partNames.filter((_, i) => i !== index);
    partShortNames = partShortNames.filter((_, i) => i !== index);
    selectedPresets = selectedPresets.filter((_, i) => i !== index);
    selectedStaffCounts = selectedStaffCounts.filter((_, i) => i !== index);
    pendingRemoval = null;
  }

  function addInstrument() {
    if (!toAdd || parts.length + additions.length >= 64) return;
    const preset = presets.find((item) => item.id === toAdd);
    if (!preset) return;
    additions = [...additions, {
      presetId: preset.id,
      name: preset.name,
      shortName: preset.short_name,
      staves: preset.staves,
    }];
    toAdd = "";
  }

  async function save(event: SubmitEvent) {
    event.preventDefault();
    const saved = await editor.updateScoreProperties({
      title,
      composer,
      tempo: {
        beat_unit: tempoBeatUnit,
        units_per_minute: tempoUnitsPerMinute,
      },
      remove_part_ids: removedPartIds,
      instruments: parts.map((part, index) => ({
        part_id: part.id,
        name: partNames[index],
        short_name: partShortNames[index],
        preset_id: selectedPresets[index] || null,
        staves: selectedStaffCounts[index],
      })),
      add_instruments: additions.map((addition) => ({
        preset_id: addition.presetId,
        name: addition.name,
        short_name: addition.shortName,
        staves: addition.staves,
      })),
    });
    if (saved) editor.scorePropertiesOpen = false;
  }
</script>

<dialog bind:this={dialog} onclose={() => (editor.scorePropertiesOpen = false)}>
  <form onsubmit={save}>
    <h2>Score properties</h2>

    <label>Title <input bind:value={title} disabled={!loaded} /></label>
    <label>Composer <input bind:value={composer} disabled={!loaded} /></label>

    <fieldset>
      <legend>Default tempo</legend>
      <label class="tempo-label">
        Tempo
        <span class="tempo-controls">
          <select bind:value={tempoBeatUnit} aria-label="Tempo beat unit" disabled={!loaded}>
            <option value="Whole">1/1 note</option>
            <option value="Half">1/2 note</option>
            <option value="Quarter">1/4 note</option>
            <option value="Eighth">1/8 note</option>
            <option value="Sixteenth">1/16 note</option>
            <option value="ThirtySecond">1/32 note</option>
            <option value="SixtyFourth">1/64 note</option>
          </select>
          <input
            type="number"
            min="1"
            max="1000"
            step="1"
            bind:value={tempoUnitsPerMinute}
            aria-label="Tempo in beats per minute"
            disabled={!loaded}
            required
          />
          <span aria-hidden="true">BPM</span>
        </span>
      </label>
    </fieldset>

    <fieldset>
      <legend>Instruments</legend>
      {#if parts.length === 0 && loaded}
        <p>No instruments in this score.</p>
      {:else}
        <ol>
          {#each parts as part, index (part.id)}
            {@const selectedPreset = presets.find((preset) => preset.id === selectedPresets[index])}
            {@const maxStaves = selectedPreset?.staves ?? part.staves}
            <li class="instrument-row">
              <label class="part-name">
                Instrument name
                <input bind:value={partNames[index]} aria-label="Instrument name {index + 1}" required disabled={!loaded} />
              </label>
              <label class="part-short-name">
                Short name
                <input bind:value={partShortNames[index]} aria-label="Short name for {partNames[index]}" disabled={!loaded} />
              </label>
              <label class="part-type">
                Instrument type
                <select
                  aria-label="Change instrument type for {partNames[index]}"
                  value={selectedPresets[index]}
                  disabled={!loaded}
                  onchange={(event) => setPreset(index, event.currentTarget.value)}
                >
                  <option value="">Keep current type</option>
                  {#each presets as preset}
                    <option value={preset.id}>{preset.name}</option>
                  {/each}
                </select>
              </label>
              <label class="staff-count">
                Staves
                <select
                  aria-label="Staff count for {partNames[index]}"
                  value={selectedStaffCounts[index]}
                  disabled={!loaded}
                  onchange={(event) => setStaffCount(index, Number(event.currentTarget.value))}
                >
                  {#each Array.from({ length: maxStaves }, (_, i) => i + 1) as count}
                    <option value={count}>{count} {count === 1 ? "staff" : "staves"}</option>
                  {/each}
                </select>
              </label>
              <div class="reorder-controls" aria-label="Reorder {partNames[index]}">
                <button
                  type="button"
                  aria-label="Move {partNames[index]} up"
                  title="Move up"
                  disabled={!loaded || index === 0}
                  onclick={() => moveInstrument(index, -1)}
                >↑</button>
                <button
                  type="button"
                  aria-label="Move {partNames[index]} down"
                  title="Move down"
                  disabled={!loaded || index === parts.length - 1}
                  onclick={() => moveInstrument(index, 1)}
                >↓</button>
                <button
                  type="button"
                  aria-label="Remove {partNames[index]}"
                  title="Remove instrument"
                  disabled={!loaded || parts.length + additions.length <= 1}
                  onclick={() => removeInstrument(index)}
                >Remove</button>
              </div>
            </li>
          {/each}
        </ol>
        {#if pendingRemoval}
          <section class="removal-confirmation" role="alert" aria-live="assertive">
            <strong>Remove {pendingRemoval.name}?</strong>
            <p>This will remove the instrument and all its notes and rests when you save. You can undo the removal afterward.</p>
            <div class="confirmation-buttons">
              <button type="button" onclick={() => (pendingRemoval = null)}>Cancel</button>
              <button type="button" class="confirm-remove" onclick={confirmRemoveInstrument}>Remove instrument</button>
            </div>
          </section>
        {/if}
      {/if}
      <p class="safety">A second staff can be added or removed. A staff containing notes or rests cannot be removed.</p>
      {#if additions.length > 0}
        <ul>
          {#each additions as addition, index}
            {@const preset = presets.find((item) => item.id === addition.presetId)}
            <li class="instrument-row addition">
              <label class="part-name">
                Instrument name
                <input bind:value={additions[index].name} aria-label="New instrument name {index + 1}" required />
              </label>
              <label class="part-short-name">
                Short name
                <input bind:value={additions[index].shortName} aria-label="Short name for new {additions[index].name}" />
              </label>
              {#if (preset?.staves ?? 1) > 1}
                <label class="staff-count">
                  Staves
                  <select
                    aria-label="Staff count for new {addition.name}"
                    value={addition.staves}
                    onchange={(event) => setAdditionStaffCount(index, Number(event.currentTarget.value))}
                  >
                    <option value={1}>1 staff</option>
                    <option value={2}>2 staves</option>
                  </select>
                </label>
              {/if}
              <button
                type="button"
                aria-label="Remove added {addition.name}"
                onclick={() => (additions = additions.filter((_, i) => i !== index))}
              >
                Remove
              </button>
            </li>
          {/each}
        </ul>
      {/if}
      <div class="add">
        <select bind:value={toAdd} aria-label="New instrument" disabled={!loaded || parts.length + additions.length >= 64}>
          <option value="">Choose an instrument…</option>
          {#each presets as preset}
            <option value={preset.id}>{preset.name}</option>
          {/each}
        </select>
        <button type="button" onclick={addInstrument} disabled={!toAdd || !loaded || parts.length + additions.length >= 64}>Add</button>
      </div>
    </fieldset>

    {#if editor.message}
      <p class="error">{editor.message}</p>
    {/if}

    <div class="buttons">
      <button type="button" onclick={() => (editor.scorePropertiesOpen = false)}>Cancel</button>
      <button type="submit" disabled={!loaded}>Save properties</button>
    </div>
  </form>
</dialog>

<style>
  dialog {
    box-sizing: border-box;
    width: min(52rem, calc(100vw - 2rem));
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

  .tempo-controls {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .tempo-controls input {
    width: 6rem;
  }

  fieldset {
    display: grid;
    gap: 0.5rem;
    min-width: 0;
  }

  ol,
  ul {
    display: grid;
    gap: 0;
    margin: 0;
    padding: 0;
  }

  .instrument-row {
    display: grid;
    grid-template-columns: minmax(10rem, 1.35fr) minmax(8rem, 0.9fr) minmax(11rem, 1.3fr) minmax(7rem, 0.7fr) auto;
    align-items: end;
    gap: 0.75rem;
    padding: 0.75rem 0;
    border-bottom: 1px solid var(--line);
    list-style: none;
  }

  .part-name,
  .part-type,
  .staff-count {
    min-width: 0;
  }

  .reorder-controls {
    display: flex;
    gap: 0.25rem;
    align-self: end;
  }

  .reorder-controls button {
    min-width: 2rem;
    min-height: 2rem;
    padding: 0.25rem;
  }

  .part-name input,
  .part-short-name input,
  .part-type select,
  .staff-count select {
    width: 100%;
  }

  .addition > button {
    justify-self: start;
  }

  ul .addition {
    grid-template-columns: minmax(10rem, 1.35fr) minmax(8rem, 0.9fr) minmax(7rem, 0.7fr) auto;
  }

  .add,
  .buttons {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 0.5rem;
  }

  .add select {
    flex: 1;
    min-width: 0;
  }

  .safety,
  .error {
    margin: 0;
  }

  .removal-confirmation {
    display: grid;
    gap: 0.5rem;
    padding: 0.75rem;
    border: 1px solid var(--danger);
    border-radius: 4px;
    background: #fff2f0;
    color: var(--ink);
  }

  .removal-confirmation p {
    margin: 0;
  }

  .confirmation-buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }

  .confirm-remove {
    border-color: var(--danger);
    background: var(--danger);
    color: white;
  }

  .safety {
    color: var(--muted);
    font-size: 0.85rem;
  }

  .error {
    color: #b00020;
  }

  @media (max-width: 760px) {
    dialog {
      padding: 0.8rem;
    }

    .instrument-row,
    ul .addition {
      grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) auto;
      align-items: end;
    }

    .part-type,
    .part-name {
      min-width: 0;
    }

    .instrument-row:not(.addition) .reorder-controls {
      grid-column: 1 / -1;
      justify-self: end;
    }

    .addition > button {
      justify-self: end;
    }
  }

  @media (max-width: 480px) {
    .instrument-row,
    ul .addition {
      grid-template-columns: minmax(0, 1fr);
      gap: 0.6rem;
    }

    .instrument-row:not(.addition) .reorder-controls {
      grid-column: auto;
      justify-self: start;
    }

    .addition > button { justify-self: start; }
  }
</style>