<script lang="ts">
  import type { Articulation, NoteValue } from "./api";
  import { NOTE_VALUES, editor } from "./editor.svelte";
  import { returnFocusAfterChoice, returnFocusAfterClick } from "./focus";
  import Icon from "./Icon.svelte";
  import MenuButton from "./MenuButton.svelte";

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
  const QUARTER_REST = "\uE4E5";
  const ACCIDENTALS = [
    { alter: -1, glyph: "\uE260", label: "Flat" },
    { alter: 0, glyph: "\uE261", label: "Natural" },
    { alter: 1, glyph: "\uE262", label: "Sharp" },
  ];
  const ARTICULATIONS: { articulation: Articulation; glyph: string; label: string }[] = [
    { articulation: "Staccato", glyph: "\uE4A2", label: "Staccato" },
    { articulation: "Staccatissimo", glyph: "\uE4A6", label: "Staccatissimo" },
    { articulation: "Tenuto", glyph: "\uE4A4", label: "Tenuto" },
    { articulation: "Accent", glyph: "\uE4A0", label: "Accent" },
    { articulation: "Marcato", glyph: "\uE4AC", label: "Marcato" },
    { articulation: "Fermata", glyph: "\uE4C0", label: "Fermata" },
  ];

  const hasSelection = $derived(editor.selected !== null);

  $effect(() => {
    editor.revision;
    void editor.refreshArticulations(editor.selected);
  });

  function exportChanged(event: Event, close: () => void) {
    const select = event.currentTarget as HTMLSelectElement;
    const format = select.value as "pdf" | "png" | "svg" | "musicxml" | "";
    select.value = "";
    if (format) {
      close();
      void editor.exportScore(format);
    }
  }
</script>

<!-- Clicks and choices only bubble up here to hand focus back to the score. -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<header
  role="toolbar"
  aria-label="Score editor toolbar"
  tabindex="0"
  onclick={returnFocusAfterClick}
  onchange={returnFocusAfterChoice}
>
  <div class="bar">
    <div class="group">
      <MenuButton label="File" title="File">
        {#snippet trigger()}File<Icon name="chevron" />{/snippet}
        {#snippet children(close)}
          <button onclick={() => { close(); editor.newScoreOpen = true; }} title="New score (Ctrl+N)">New score…</button>
          <button onclick={() => { close(); void editor.open(); }} title="Open (Ctrl+O)">Open…</button>
          <button onclick={() => { close(); void editor.save(); }} title="Save (Ctrl+S)">Save</button>
          <button onclick={() => { close(); void editor.save(true); }} title="Save as (Ctrl+Shift+S)">Save as…</button>
          <button onclick={() => { close(); editor.scorePropertiesOpen = true; }}>Score properties…</button>
          <label class="menu-label" for="score-export">Export</label>
          <select id="score-export" aria-label="Export score" onchange={(event) => exportChanged(event, close)}>
            <option value="">Choose format…</option>
            <option value="pdf">PDF</option>
            <option value="png">PNG image</option>
            <option value="svg">SVG image</option>
            <option value="musicxml">MusicXML score</option>
          </select>
        {/snippet}
      </MenuButton>
    </div>

    <div class="group">
      <button class="icon-button" onclick={() => editor.undo()} disabled={!editor.status.can_undo} aria-label="Undo" title="Undo (Ctrl+Z)">
        <Icon name="undo" />
      </button>
      <button class="icon-button" onclick={() => editor.redo()} disabled={!editor.status.can_redo} aria-label="Redo" title="Redo (Ctrl+Y)">
        <Icon name="redo" />
      </button>
    </div>

    <div class="group">
      {#if editor.playbackPlaying}
        <button class="icon-button active" onclick={() => editor.stopPlayback()} aria-label="Stop playback" title="Stop playback">
          <Icon name="stop" />
        </button>
      {:else}
        <button
          class="icon-button"
          onclick={() => editor.playScore()}
          disabled={!editor.playbackInstrument}
          aria-label="Play score"
          title={editor.playbackInstrument ? "Play score" : "Choose a playback instrument first"}
        >
          <Icon name="play" />
        </button>
      {/if}
      <MenuButton label="Playback options" title="Playback options">
        {#snippet trigger()}<Icon name="speaker" /><Icon name="chevron" />{/snippet}
        {#snippet children(close)}
          <span class="menu-label">Playback instrument</span>
          <span class="menu-note" title={editor.playbackInstrument ?? undefined}>
            {editor.playbackInstrument ?? "None chosen"}
          </span>
          <button
            onclick={() => { close(); void editor.selectSoundfont(); }}
            title="Choose a playback instrument (SF2, SF3, or SFZ)"
          >Choose instrument…</button>
        {/snippet}
      </MenuButton>
    </div>

    <div class="group">
      <button
        class="with-icon"
        class:active={editor.inputMode}
        aria-pressed={editor.inputMode}
        onclick={() => editor.toggleInputMode()}
        title="Note input (N)"
      >
        <Icon name="pencil" />Note input
      </button>
    </div>

    <div class="spacer"></div>

    <div class="group zoom">
      <button class="icon-button" onclick={() => editor.zoomOut()} disabled={!editor.canZoomOut} aria-label="Zoom out" title="Zoom out (Ctrl+wheel)">
        <Icon name="minus" />
      </button>
      <span>{Math.round((editor.zoom / 8) * 100)}%</span>
      <button class="icon-button" onclick={() => editor.zoomIn()} disabled={!editor.canZoomIn} aria-label="Zoom in" title="Zoom in (Ctrl+wheel)">
        <Icon name="plus" />
      </button>
    </div>
    <div class="group">
      <button
        class="icon-button"
        class:active={editor.sidebarOpen}
        aria-pressed={editor.sidebarOpen}
        onclick={() => editor.toggleSidebar()}
        aria-label="Show score properties panel"
        title={editor.sidebarOpen ? "Hide the side panel" : "Show the side panel"}
      >
        <Icon name="sidebar" />
      </button>
    </div>
  </div>

  <div class="bar input-bar">
    <div class="group">
      {#each NOTE_VALUES as value, index}
        <button
          class="smufl"
          class:active={editor.duration.value === value}
          aria-pressed={editor.duration.value === value}
          aria-label={value}
          title="{value} ({index + 1})"
          onclick={() => editor.setNoteValue(value)}><span class="glyph">{NOTE_GLYPHS[value]}</span></button
        >
      {/each}
      <button
        class="dot"
        class:active={editor.duration.dots > 0}
        aria-pressed={editor.duration.dots > 0}
        aria-label="Dot"
        onclick={() => editor.toggleDot()}
        title="Dot (.)"
      >
        {editor.duration.dots === 2 ? "••" : "•"}
      </button>
      <button
        class="smufl"
        onclick={() => editor.enterRest()}
        disabled={!editor.inputMode}
        aria-label="Rest"
        title="Enter a rest (0)"><span class="glyph">{QUARTER_REST}</span></button
      >
    </div>

    <div class="group">
      {#each ACCIDENTALS as accidental}
        <button
          class="smufl"
          class:active={editor.inputMode && editor.entryAlter === accidental.alter}
          aria-pressed={editor.inputMode && editor.entryAlter === accidental.alter}
          aria-label={accidental.label}
          title={accidental.label}
          disabled={!editor.inputMode && (!hasSelection || editor.hasRangeSelection)}
          onclick={() => editor.setAlter(accidental.alter)}><span class="glyph">{accidental.glyph}</span></button
        >
      {/each}
    </div>

    <div class="group">
      {#each ARTICULATIONS as item}
        <button
          class="smufl"
          class:active={editor.hasArticulation(item.articulation)}
          aria-pressed={editor.hasArticulation(item.articulation)}
          aria-label={item.label}
          title="Toggle {item.label}"
          disabled={!editor.canToggleArticulation(item.articulation)}
          onclick={() => editor.toggleArticulation(item.articulation)}><span class="glyph">{item.glyph}</span></button
        >
      {/each}
      <button
        class="icon-button"
        class:active={editor.hasTie()}
        aria-pressed={editor.hasTie()}
        aria-label="Tie to next note"
        title="Tie matching pitches to the next note"
        disabled={!editor.canToggleTie()}
        onclick={() => editor.toggleTie()}
      >
        <Icon name="tie" />
      </button>
    </div>
  </div>
</header>

<style>
  header {
    display: grid;
    border-bottom: 1px solid var(--line);
    background: var(--toolbar);
    box-shadow: 0 1px 3px rgb(28 38 34 / 7%);
  }

  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.35rem 0;
    padding: 0.35rem 0.6rem;
  }

  /* Note entry controls stay in one place, right above the score. */
  .input-bar {
    border-top: 1px solid var(--line);
    background: #fbfcfb;
  }

  .spacer {
    flex: 1 1 0;
  }

  .group {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 0.25rem;
    padding: 0 0.5rem;
    border-right: 1px solid var(--line);
  }

  .bar > .group:first-child {
    padding-left: 0;
  }

  .bar > .group:last-child,
  .bar > .group:has(+ .spacer) {
    border-right: 0;
  }

  header :global(button),
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

  header :global(button:hover:not(:disabled)),
  select:hover {
    border-color: var(--line-strong);
    background: #fff;
  }

  header :global(button:disabled) {
    color: #9aa19e;
    cursor: default;
  }

  header :global(button.active) {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent-strong);
  }

  .icon-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 2.1rem;
    padding: 0.3rem 0.45rem;
  }

  .with-icon {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
  }

  .dot {
    min-width: 2.1rem;
    font-size: 1rem;
    letter-spacing: -0.1em;
  }

  .smufl {
    line-height: 1;
    min-width: 2.1rem;
    padding: 0.2rem 0.35rem 0.1rem;
  }

  /* Bravura's ascent and descent are 2 em each. Hit testing uses those boxes, not the ink,
     so they would catch the pointer over neighbouring controls. The button keeps the UI font
     for its own line, and the glyph sits in a one-line box that ignores the pointer. */
  .smufl .glyph {
    display: inline-block;
    font-family: "Bravura";
    font-size: 1.4rem;
    line-height: 1;
    pointer-events: none;
  }

  .zoom span {
    min-width: 3.2em;
    text-align: center;
    color: var(--muted);
    font-size: 0.82rem;
  }

  @media (max-width: 760px) {
    .bar {
      flex-wrap: nowrap;
      gap: 0;
      padding: 0.35rem 0.4rem;
      overflow-x: auto;
      overscroll-behavior-x: contain;
      scrollbar-width: thin;
    }

    .group {
      padding: 0 0.3rem;
    }
  }
</style>
