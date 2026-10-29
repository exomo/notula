<script lang="ts">
  import type { Dynamic, TempoDirection } from "./api";
  import { TEMPO_BEATS, editor, tempoBeatKey, tempoFromBeatKey } from "./editor.svelte";
  import { returnFocusAfterChoice, returnFocusAfterClick } from "./focus";
  import Icon from "./Icon.svelte";

  const DYNAMICS: { dynamic: Dynamic; label: string }[] = [
    { dynamic: "Ppp", label: "ppp" },
    { dynamic: "Pp", label: "pp" },
    { dynamic: "P", label: "p" },
    { dynamic: "Mp", label: "mp" },
    { dynamic: "Mf", label: "mf" },
    { dynamic: "F", label: "f" },
    { dynamic: "Ff", label: "ff" },
    { dynamic: "Fff", label: "fff" },
    { dynamic: "Fp", label: "fp" },
    { dynamic: "Sf", label: "sf" },
    { dynamic: "Sfz", label: "sfz" },
    { dynamic: "Rfz", label: "rfz" },
  ];

  let measureDeleteDialog = $state<HTMLDialogElement | null>(null);
  let pendingDeleteMeasure = $state<number | null>(null);
  let tempoDirectionChoice = $state<TempoDirection>("ritardando");
  let tempoBeatChoice = $state("Quarter:0");
  let tempoUnitsChoice = $state(120);
  const tempoChoiceValid = $derived(
    Number.isInteger(tempoUnitsChoice) && tempoUnitsChoice >= 1 && tempoUnitsChoice <= 1000,
  );
  // Single-note controls stay off while a range selection is active.
  const selectedNote = $derived(
    editor.selected !== null && !editor.hasRangeSelection ? editor.eventArticulations : null,
  );
  const range = $derived(editor.rangeSelection);
  const rangeSummary = $derived.by(() => {
    if (!range) return "";
    if (!editor.hasRangeSelection) {
      return `Cursor across ${range.resolved.staves.length} staves: notes you enter go into each.`;
    }
    const { events, measures, staves } = range.resolved;
    const first = measures[0]?.measure ?? 0;
    const last = measures[measures.length - 1]?.measure ?? first;
    const measureText = first === last ? `measure ${first + 1}` : `measures ${first + 1}–${last + 1}`;
    const staffText = staves.length === 1 ? "1 staff" : `${staves.length} staves`;
    const itemText = events.length === 1 ? "1 note or rest" : `${events.length} notes and rests`;
    return `${itemText} · ${measureText} · ${staffText}`;
  });
  // Voices that occur in the selected staves and measures.
  const rangeVoices = $derived.by(() => {
    if (!range) return 1;
    const staves = new Set(range.resolved.staves);
    const measures = new Set(range.resolved.measures.map((span) => span.measure));
    const counts = editor.areas
      .filter((area) => staves.has(area.staff) && measures.has(area.measure))
      .map((area) => area.voice_count);
    return Math.max(1, (range.selection.voice ?? 0) + 1, ...counts);
  });
  // A new hairpin or slur can start where none is, or replace one starting at the selection.
  const hairpinPosition = $derived(selectedNote?.hairpin ?? null);
  const slurPosition = $derived(selectedNote?.slur ?? null);
  const phrasePosition = $derived(selectedNote?.phrase_slur ?? null);
  const canStartHairpin = $derived(hairpinPosition === null || hairpinPosition === "start");
  const canStartSlur = $derived(slurPosition === null || slurPosition === "start");
  const canStartPhrase = $derived(phrasePosition === null || phrasePosition === "start");

  $effect(() => {
    const cursor = editor.cursor;
    editor.revision;
    void editor.refreshTempoDirection(cursor);
    void editor.refreshTempoChange(cursor);
  });

  // Show the marks at the cursor so they can be adjusted in place.
  $effect(() => {
    const direction = editor.tempoDirectionAt;
    if (direction) tempoDirectionChoice = direction;
  });

  $effect(() => {
    const mark = editor.tempoChangeAt;
    if (!mark) return;
    tempoBeatChoice = tempoBeatKey(mark.tempo);
    tempoUnitsChoice = mark.tempo.units_per_minute;
  });

  function askDeleteMeasure() {
    const cursor = editor.cursor;
    if (!cursor || !editor.canDeleteMeasure) return;
    pendingDeleteMeasure = cursor.measure;
    measureDeleteDialog?.showModal();
  }

  function confirmDeleteMeasure() {
    const measure = pendingDeleteMeasure;
    measureDeleteDialog?.close();
    pendingDeleteMeasure = null;
    if (measure !== null) void editor.removeMeasure(measure);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<aside
  class="sidebar"
  aria-label="Score properties"
  onclick={returnFocusAfterClick}
  onchange={returnFocusAfterChoice}
>
  {#if range}
    <section class="selection">
      <h2>Selection</h2>
      <p class="hint">{rangeSummary}</p>
      {#if editor.hasRangeSelection}
        <label class="field">
          <span>Voices</span>
          <select
            aria-label="Voices in the selection"
            value={range.selection.voice === null ? "" : String(range.selection.voice)}
            onchange={(event) => {
              const value = event.currentTarget.value;
              void editor.setSelectionVoice(value === "" ? null : Number(value));
            }}
          >
            <option value="">All voices</option>
            {#each Array.from({ length: rangeVoices }, (_, index) => index) as voice}
              <option value={String(voice)}>Voice {voice + 1}</option>
            {/each}
          </select>
        </label>
      {/if}
      <div class="row">
        <button onclick={() => editor.clearSelection()} title="Clear the selection (Esc)">Clear</button>
      </div>
    </section>
  {/if}

  <section>
    <h2>Dynamic</h2>
    {#if selectedNote}
      <label class="field">
        <span>Selected note</span>
        <select
          aria-label="Dynamic of the selected note"
          value={selectedNote.dynamic ?? ""}
          onchange={(event) => editor.setDynamic((event.currentTarget.value || null) as Dynamic | null)}
        >
          <option value="">None</option>
          {#each DYNAMICS as item (item.dynamic)}
            <option value={item.dynamic}>{item.label}</option>
          {/each}
        </select>
      </label>
    {:else if editor.hasRangeSelection}
      <p class="hint">Dynamics are set on one note; click a note to edit its dynamic.</p>
    {:else}
      <p class="hint">Select a note or rest to set its dynamic.</p>
    {/if}
    <div class="row">
      <button
        class="with-icon"
        class:active={editor.hairpinMode === "Crescendo"}
        aria-pressed={editor.hairpinMode === "Crescendo"}
        disabled={!canStartHairpin && editor.hairpinMode !== "Crescendo"}
        title="Select a start note, then an end note to add or remove a crescendo; the same note twice puts it on one note"
        onclick={() => editor.toggleHairpinMode("Crescendo")}
      >
        <Icon name="crescendo" />{editor.hairpinMode === "Crescendo" ? "Cancel" : "Cresc."}
      </button>
      <button
        class="with-icon"
        class:active={editor.hairpinMode === "Diminuendo"}
        aria-pressed={editor.hairpinMode === "Diminuendo"}
        disabled={!canStartHairpin && editor.hairpinMode !== "Diminuendo"}
        title="Select a start note, then an end note to add or remove a diminuendo; the same note twice puts it on one note"
        onclick={() => editor.toggleHairpinMode("Diminuendo")}
      >
        <Icon name="diminuendo" />{editor.hairpinMode === "Diminuendo" ? "Cancel" : "Dim."}
      </button>
      <button
        onclick={() => editor.removeHairpin()}
        disabled={hairpinPosition === null}
        title="Remove the hairpin the selected note belongs to"
      >Remove</button>
    </div>
  </section>

  <section>
    <h2>Lines</h2>
    <div class="row">
      <button
        class:active={editor.slurMode}
        aria-pressed={editor.slurMode}
        disabled={!canStartSlur && !editor.slurMode}
        title="Select a start note, then an end note to add or remove a slur"
        onclick={() => editor.toggleSlurMode()}>{editor.slurMode ? "Cancel slur" : "Slur"}</button
      >
      <button
        class:active={editor.phraseSlurMode}
        aria-pressed={editor.phraseSlurMode}
        disabled={!canStartPhrase && !editor.phraseSlurMode}
        title="Select a start note, then an end note to add or remove an overlapping phrase slur"
        onclick={() => editor.togglePhraseSlurMode()}>{editor.phraseSlurMode ? "Cancel phrase" : "Phrase"}</button
      >
      <select aria-label="Slur placement" title="Where new slurs are placed" bind:value={editor.slurPlacement}>
        <option value="auto">Auto</option>
        <option value="above">Above</option>
        <option value="below">Below</option>
      </select>
    </div>
    <div class="row">
      <button
        onclick={() => editor.removeSlur(false)}
        disabled={slurPosition === null}
        title="Remove the slur the selected note belongs to"
      >Remove slur</button>
      <button
        onclick={() => editor.removeSlur(true)}
        disabled={phrasePosition === null}
        title="Remove the phrase slur the selected note belongs to"
      >Remove phrase</button>
    </div>
  </section>

  {#if editor.cursor}
    {@const cursor = editor.cursor}
    <section>
      <h2>Tempo</h2>
      <div class="part">
        <h3>Metronome mark</h3>
        <div class="row single-line">
          <select bind:value={tempoBeatChoice} aria-label="Metronome mark beat unit">
            {#each TEMPO_BEATS as beat (beat.key)}
              <option value={beat.key}>{beat.label}</option>
            {/each}
          </select>
          <span aria-hidden="true">=</span>
          <input
            class="bpm"
            type="number"
            min="1"
            max="1000"
            step="1"
            bind:value={tempoUnitsChoice}
            aria-label="Metronome mark in beats per minute"
          />
        </div>
        <div class="row">
          <button
            onclick={() => editor.setTempoChange(tempoFromBeatKey(tempoBeatChoice, tempoUnitsChoice))}
            disabled={!tempoChoiceValid}
            title="Add or change the metronome mark at the cursor; playback follows it from there"
          >{editor.tempoChangeAt === null ? "Add" : "Change"}</button>
          <button
            onclick={() => editor.setTempoChange(null)}
            disabled={editor.tempoChangeAt === null}
            title="Remove the metronome mark at the cursor"
          >Remove</button>
        </div>
      </div>
      <div class="part">
        <h3>Tempo direction</h3>
        <div class="row">
          <select bind:value={tempoDirectionChoice} aria-label="Tempo direction">
            <option value="accelerando">accel.</option>
            <option value="ritardando">rit.</option>
            <option value="rallentando">rall.</option>
            <option value="a_tempo">a tempo</option>
            <option value="rubato">rubato</option>
          </select>
          <button
            onclick={() => editor.setTempoDirection(tempoDirectionChoice)}
            title="Add or change the tempo direction at the cursor for the whole score"
          >{editor.tempoDirectionAt === null ? "Add" : "Change"}</button>
          <button
            onclick={() => editor.setTempoDirection(null)}
            disabled={editor.tempoDirectionAt === null}
            title="Remove the tempo direction at the cursor from the whole score"
          >Remove</button>
        </div>
      </div>
    </section>

    <section>
      <h2>Voice</h2>
      <div class="row">
        <select
          aria-label="Voice being edited"
          value={cursor.voice}
          onchange={(event) => editor.selectVoice(Number(event.currentTarget.value))}
        >
          {#each Array.from({ length: editor.voiceCount }, (_, index) => index) as voice}
            <option value={voice}>Voice {voice + 1}</option>
          {/each}
        </select>
        <button
          class="with-icon"
          onclick={() => editor.addVoice()}
          title="Start a new voice at the beginning of this measure"
        ><Icon name="plus" />Add</button>
      </div>
      <div class="row">
        <button
          onclick={() => editor.endVoice()}
          disabled={!editor.canEndVoice}
          title="End this voice here and continue in voice 1"
        >End here</button>
        <button
          onclick={() => editor.removeVoice()}
          disabled={cursor.voice === 0}
          title="Delete the selected additional voice across this staff"
        >Delete</button>
      </div>
    </section>

    <section>
      <h2>Measure {cursor.measure + 1}</h2>
      <div class="row">
        <button onclick={() => editor.insertMeasure(true)} title="Insert a measure before this one">Insert before</button>
        <button onclick={() => editor.insertMeasure(false)} title="Insert a measure after this one">Insert after</button>
      </div>
      <div class="row">
        <button
          onclick={() => (editor.timeSignatureOpen = true)}
          title="Change the time signature from this measure (T)"
        >Time…</button>
        <button
          onclick={() => (editor.keySignatureOpen = true)}
          title="Change the key signature from this measure until the next key change"
        >Key…</button>
        <button
          onclick={() => editor.togglePickup()}
          title="Pickup (anacrusis): shorten this measure to the notes in it, or restore its full length"
        >Pickup</button>
      </div>
      <div class="row">
        <button
          class="danger"
          onclick={askDeleteMeasure}
          disabled={!editor.canDeleteMeasure}
          title="Delete this measure in every staff"
        >Delete measure</button>
      </div>
    </section>
  {:else}
    <p class="hint">Click in the score to place the cursor. Tempo, voice, and measure options apply there.</p>
  {/if}
</aside>

<dialog bind:this={measureDeleteDialog} class="measure-delete-dialog" onclose={() => (pendingDeleteMeasure = null)}>
  <h2>Delete measure {pendingDeleteMeasure === null ? "" : pendingDeleteMeasure + 1}?</h2>
  <p>This removes the entire measure, including all notes and rests in every staff. You can undo this change.</p>
  <div class="measure-delete-actions">
    <button type="button" onclick={() => measureDeleteDialog?.close()}>Cancel</button>
    <button type="button" class="confirm-delete" onclick={confirmDeleteMeasure}>Delete measure</button>
  </div>
</dialog>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    box-sizing: border-box;
    width: 19rem;
    min-height: 0;
    padding: 0.6rem;
    overflow-y: auto;
    border-left: 1px solid var(--line);
    background: var(--toolbar);
  }

  section.selection {
    border-color: rgba(26, 115, 232, 0.45);
    background: rgba(26, 115, 232, 0.04);
  }

  section {
    display: grid;
    gap: 0.4rem;
    padding: 0.55rem 0.6rem 0.65rem;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: #fff;
  }

  h2 {
    margin: 0;
    color: var(--muted);
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  /* Parts of one section, such as the two kinds of tempo marks. */
  .part {
    display: grid;
    gap: 0.35rem;
  }

  .part + .part {
    padding-top: 0.45rem;
    border-top: 1px solid var(--line);
  }

  h3 {
    margin: 0;
    color: var(--ink);
    font-size: 0.78rem;
    font-weight: 600;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.3rem;
  }

  .field {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    font-size: 0.82rem;
  }

  .hint {
    margin: 0;
    color: var(--muted);
    font-size: 0.8rem;
    line-height: 1.35;
  }

  button,
  select,
  input {
    box-sizing: border-box;
    height: 2rem;
    min-height: 2rem;
    font: inherit;
    font-size: 0.82rem;
  }

  select,
  input {
    padding: 0 0.4rem;
  }

  /* Keeps "beat = number" together; the dropdown gives way. */
  .single-line {
    flex-wrap: nowrap;
  }

  .single-line select {
    flex: 1 1 auto;
    min-width: 0;
  }

  button {
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 0.3rem 0.6rem;
    background: var(--toolbar);
    color: var(--ink);
    cursor: pointer;
  }

  button:hover:not(:disabled) {
    border-color: var(--line-strong);
    background: #fff;
  }

  button:disabled {
    color: #9aa19e;
    cursor: default;
  }

  button.active {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent-strong);
  }

  button.with-icon {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
  }

  button.danger:not(:disabled) {
    color: var(--danger);
  }

  .bpm {
    width: 4.5rem;
  }

  .measure-delete-dialog {
    width: min(28rem, calc(100vw - 2rem));
    padding: 1rem 1.25rem;
  }

  .measure-delete-dialog h2 {
    margin: 0 0 0.75rem;
    color: var(--ink);
    font-size: 1.2rem;
    letter-spacing: normal;
    text-transform: none;
  }

  .measure-delete-dialog p {
    margin: 0 0 0.75rem;
  }

  .measure-delete-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }

  .confirm-delete {
    border-color: var(--danger);
    background: var(--danger);
    color: white;
  }

  .confirm-delete:hover:not(:disabled) {
    border-color: var(--danger);
    background: var(--danger);
  }
</style>
