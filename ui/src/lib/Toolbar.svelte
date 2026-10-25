<script lang="ts">
  import type { NoteValue } from "./api";
  import { NOTE_VALUES, editor } from "./editor.svelte";

  // SMuFL "individual notes" glyphs, named by the note value they show.
  const NOTE_GLYPHS: Record<NoteValue, string> = {
    Whole: "\uE1D2",
    Half: "\uE1D3",
    Quarter: "\uE1D5",
    Eighth: "\uE1D7",
    Sixteenth: "\uE1D9",
    ThirtySecond: "\uE1DB",
    SixtyFourth: "\uE1DD",
  };
  const ACCIDENTALS = [
    { alter: -1, glyph: "\uE260", label: "Flat" },
    { alter: 0, glyph: "\uE261", label: "Natural" },
    { alter: 1, glyph: "\uE262", label: "Sharp" },
  ];

  const hasSelection = $derived(editor.selected !== null);
  let fileMenuOpen = $state(false);
  let fileMenuTrigger = $state<HTMLButtonElement | null>(null);
  let fileMenuPanel = $state<HTMLDivElement | null>(null);
  let fileMenuPosition = $state({ left: 0, top: 0 });
  let measureDeleteDialog = $state<HTMLDialogElement | null>(null);
  let pendingDeleteMeasure = $state<number | null>(null);

  function toggleFileMenu() {
    if (fileMenuOpen) {
      fileMenuOpen = false;
      return;
    }
    const bounds = fileMenuTrigger?.getBoundingClientRect();
    if (bounds) {
      fileMenuPosition = {
        left: Math.max(8, Math.min(bounds.left, window.innerWidth - 224)),
        top: bounds.bottom + 4,
      };
    }
    fileMenuOpen = true;
  }

  function onWindowClick(event: MouseEvent) {
    if (
      fileMenuOpen &&
      !fileMenuPanel?.contains(event.target as Node) &&
      !fileMenuTrigger?.contains(event.target as Node)
    ) {
      fileMenuOpen = false;
    }
  }

  function onFileKeydown(event: KeyboardEvent) {
    if (fileMenuOpen && event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      fileMenuOpen = false;
      fileMenuTrigger?.focus();
    }
  }

  function runFileAction(action: () => void) {
    fileMenuOpen = false;
    action();
  }

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

  function exportChanged(event: Event) {
    const select = event.currentTarget as HTMLSelectElement;
    const format = select.value as "pdf" | "png" | "svg" | "";
    select.value = "";
    if (format) {
      fileMenuOpen = false;
      void editor.exportScore(format);
    }
  }
</script>

<svelte:window onclick={onWindowClick} />

<header role="toolbar" aria-label="Score editor toolbar" tabindex="0" onkeydown={onFileKeydown}>
  <div class="group file-menu">
    <button
      bind:this={fileMenuTrigger}
      aria-haspopup="true"
      aria-expanded={fileMenuOpen}
      aria-controls="file-actions"
      onclick={toggleFileMenu}
    >
      File
    </button>
  </div>

  <div class="group">
    <button onclick={() => editor.undo()} disabled={!editor.status.can_undo} title="Undo (Ctrl+Z)">Undo</button>
    <button onclick={() => editor.redo()} disabled={!editor.status.can_redo} title="Redo (Ctrl+Y)">Redo</button>
  </div>

  <div class="group playback">
    <button onclick={() => editor.playScore()} disabled={editor.playbackPlaying || !editor.playbackInstrument} title="Play score">
      Play
    </button>
    <button onclick={() => editor.stopPlayback()} disabled={!editor.playbackPlaying} title="Stop playback">
      Stop
    </button>
    <button onclick={() => editor.selectSoundfont()} title="Choose a playback instrument (SF2, SF3, or SFZ)">
      Instrument
    </button>
    {#if editor.playbackInstrument}
      <span title={editor.playbackInstrument}>{editor.playbackInstrument}</span>
    {/if}
  </div>

  <div class="group">
    <button
      class:active={editor.inputMode}
      aria-pressed={editor.inputMode}
      onclick={() => editor.toggleInputMode()}
      title="Note input (N)"
    >
      Note input
    </button>
    <button onclick={() => editor.enterRest()} disabled={!editor.inputMode} title="Enter a rest (0)">Rest</button>
  </div>

  <div class="group">
    {#each NOTE_VALUES as value, index}
      <button
        class="smufl"
        class:active={editor.duration.value === value}
        aria-pressed={editor.duration.value === value}
        aria-label={value}
        title="{value} ({index + 1})"
        onclick={() => editor.setNoteValue(value)}>{NOTE_GLYPHS[value]}</button
      >
    {/each}
    <button
      class:active={editor.duration.dots > 0}
      onclick={() => editor.toggleDot()}
      title="Dot (.)"
    >
      {editor.duration.dots === 2 ? ".." : "."}
    </button>
  </div>

  <div class="group">
    {#each ACCIDENTALS as accidental}
      <button
        class="smufl"
        aria-label={accidental.label}
        title={accidental.label}
        disabled={!hasSelection}
        onclick={() => editor.setAlter(accidental.alter)}>{accidental.glyph}</button
      >
    {/each}
  </div>

  <div class="group">
    <button onclick={() => editor.insertMeasure(true)} disabled={!editor.cursor} title="Insert a measure before the current measure">
      Insert before
    </button>
    <button onclick={() => editor.insertMeasure(false)} disabled={!editor.cursor} title="Insert a measure after the current measure">
      Insert after
    </button>
    <button onclick={askDeleteMeasure} disabled={!editor.canDeleteMeasure} title="Delete the current measure">
      Delete measure
    </button>
    {#if editor.cursor}
      <label class="voice-control">
        Voice
        <select
          aria-label="Voice being edited"
          value={editor.cursor.voice}
          onchange={(event) => editor.selectVoice(Number(event.currentTarget.value))}
        >
          {#each Array.from({ length: editor.voiceCount }, (_, index) => index) as voice}
            <option value={voice}>{voice + 1}</option>
          {/each}
        </select>
      </label>
    {/if}
    <button
      onclick={() => editor.addVoice()}
      disabled={!editor.cursor}
      title="Start a new voice at the beginning of this measure"
    >
      + Voice
    </button>
    <button
      onclick={() => editor.removeVoice()}
      disabled={!editor.cursor || editor.cursor.voice === 0}
      title="Delete the selected additional voice across this staff"
    >
      Delete voice
    </button>
    <button
      onclick={() => editor.endVoice()}
      disabled={!editor.canEndVoice}
      title="End this voice here and continue in voice 1"
    >
      End here
    </button>
    <button
      onclick={() => {
        if (editor.cursor) editor.timeSignatureOpen = true;
        else editor.message = "Click on a staff first.";
      }}
      title="Change the time signature from the cursor's measure (T)"
    >
      Time signature
    </button>
    <button
      onclick={() => {
        if (editor.cursor) editor.keySignatureOpen = true;
        else editor.message = "Click on a measure first.";
      }}
      title="Change the key signature from the current measure until the next key change"
    >
      Key signature
    </button>
    <button
      onclick={() => editor.togglePickup()}
      title="Pickup (anacrusis): shorten the current measure to the notes in it, or restore its full length"
    >
      Pickup
    </button>
  </div>

  <div class="group zoom">
    <button onclick={() => editor.zoomOut()} disabled={!editor.canZoomOut} aria-label="Zoom out">−</button>
    <span>{Math.round((editor.zoom / 8) * 100)}%</span>
    <button onclick={() => editor.zoomIn()} disabled={!editor.canZoomIn} aria-label="Zoom in">+</button>
  </div>
</header>

<dialog bind:this={measureDeleteDialog} class="measure-delete-dialog" onclose={() => (pendingDeleteMeasure = null)}>
  <h2>Delete measure {pendingDeleteMeasure === null ? "" : pendingDeleteMeasure + 1}?</h2>
  <p>This removes the entire measure, including all notes and rests in every staff. You can undo this change.</p>
  <div class="measure-delete-actions">
    <button type="button" onclick={() => measureDeleteDialog?.close()}>Cancel</button>
    <button type="button" class="confirm-delete" onclick={confirmDeleteMeasure}>Delete measure</button>
  </div>
</dialog>

{#if fileMenuOpen}
  <div
    bind:this={fileMenuPanel}
    id="file-actions"
    class="file-panel"
    role="toolbar"
    aria-label="File actions"
    tabindex="-1"
    onkeydown={onFileKeydown}
    style:left="{fileMenuPosition.left}px"
    style:top="{fileMenuPosition.top}px"
  >
    <button onclick={() => runFileAction(() => (editor.newScoreOpen = true))} title="New score (Ctrl+N)">New score</button>
    <button onclick={() => runFileAction(() => void editor.open())} title="Open (Ctrl+O)">Open…</button>
    <button onclick={() => runFileAction(() => void editor.save())} title="Save (Ctrl+S)">Save</button>
    <button onclick={() => runFileAction(() => void editor.save(true))} title="Save as (Ctrl+Shift+S)">Save as…</button>
    <button onclick={() => runFileAction(() => (editor.scorePropertiesOpen = true))}>Score properties…</button>
    <label for="score-export">Export</label>
    <select id="score-export" aria-label="Export score" onchange={exportChanged}>
      <option value="">Choose format…</option>
      <option value="pdf">PDF</option>
      <option value="png">PNG image</option>
      <option value="svg">SVG image</option>
    </select>
  </div>
{/if}

<style>
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.35rem 0;
    padding: 0.45rem 0.6rem;
    border-bottom: 1px solid var(--line);
    background: var(--toolbar);
    box-shadow: 0 1px 3px rgb(28 38 34 / 7%);
  }

  .group {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0 0.55rem;
    border-right: 1px solid var(--line);
  }

  .group:first-child {
    padding-left: 0;
  }

  .group:last-child {
    border-right: 0;
  }

  .file-panel {
    position: fixed;
    z-index: 30;
    display: grid;
    width: 13rem;
    gap: 0.2rem;
    padding: 0.45rem;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    background: var(--toolbar);
    box-shadow: 0 10px 28px rgb(20 32 26 / 20%);
  }

  .file-panel button {
    width: 100%;
    text-align: left;
  }

  .file-panel label {
    padding: 0.25rem 0.55rem 0;
    color: var(--muted);
    font-size: 0.74rem;
    font-weight: 600;
  }

  .file-panel select {
    width: 100%;
    border-color: var(--line);
    background: #fff;
  }

  .group.playback span {
    max-width: 10rem;
    overflow: hidden;
    color: var(--muted);
    font-size: 0.78rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  button,
  select {
    min-height: 2rem;
    border: 1px solid transparent;
    border-radius: 4px;
    padding: 0.3rem 0.55rem;
    background: transparent;
    color: var(--ink);
    font: inherit;
    font-size: 0.82rem;
    cursor: pointer;
  }

  button:hover:not(:disabled),
  select:hover {
    border-color: var(--line-strong);
    background: #fff;
  }

  button:focus-visible,
  select:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
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

  .smufl {
    font-family: "Bravura";
    font-size: 1.4rem;
    line-height: 1;
    min-width: 2.1rem;
    padding: 0.2rem 0.35rem 0.1rem;
  }

  .zoom span {
    min-width: 3.2em;
    text-align: center;
    color: var(--muted);
    font-size: 0.82rem;
  }

  @media (max-width: 760px) {
    header {
      flex-wrap: nowrap;
      gap: 0;
      padding: 0.4rem;
      overflow-x: auto;
      overscroll-behavior-x: contain;
      scrollbar-width: thin;
    }

    .group {
      flex: 0 0 auto;
      flex-wrap: nowrap;
      padding: 0 0.35rem;
    }

    .group.playback span {
      display: none;
    }
  }

  @media (max-width: 480px) {
    .group {
      border-right: 0;
      padding: 0 0.2rem;
    }

    button,
    select {
      padding-inline: 0.4rem;
    }
  }

  .measure-delete-dialog {
    width: min(28rem, calc(100vw - 2rem));
    padding: 1rem 1.25rem;
  }

  .measure-delete-dialog h2,
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
</style>
