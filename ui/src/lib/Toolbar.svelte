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
</script>

<header>
  <div class="group">
    <button onclick={() => (editor.newScoreOpen = true)} title="New score (Ctrl+N)">New</button>
    <button onclick={() => editor.open()} title="Open (Ctrl+O)">Open</button>
    <button onclick={() => editor.save()} title="Save (Ctrl+S)">Save</button>
    <button onclick={() => editor.save(true)} title="Save as (Ctrl+Shift+S)">Save as</button>
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
    <button onclick={() => editor.insertMeasure()} title="Insert a measure after the cursor">
      + Measure
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

<style>
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.25rem 1rem;
    padding: 0.4rem 0.75rem;
    border-bottom: 1px solid #bbb;
    background: #f4f4f4;
  }

  .group {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }

  button {
    min-width: 2rem;
    padding: 0.25rem 0.55rem;
  }

  button.active {
    background: #cfe0fc;
    border-color: #1a73e8;
  }

  .smufl {
    font-family: "Bravura";
    font-size: 1.4rem;
    line-height: 1;
    padding: 0.2rem 0.45rem 0.1rem;
  }

  .zoom span {
    min-width: 3.2em;
    text-align: center;
  }
</style>
