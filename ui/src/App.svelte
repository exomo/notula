<script lang="ts">
  import { onMount } from "svelte";
  import { editor } from "./lib/editor.svelte";
  import NewScoreDialog from "./lib/NewScoreDialog.svelte";
  import KeySignatureDialog from "./lib/KeySignatureDialog.svelte";
  import ScorePropertiesDialog from "./lib/ScorePropertiesDialog.svelte";
  import ScoreView from "./lib/ScoreView.svelte";
  import Sidebar from "./lib/Sidebar.svelte";
  import TimeSignatureDialog from "./lib/TimeSignatureDialog.svelte";
  import Toolbar from "./lib/Toolbar.svelte";

  onMount(() => {
    void editor.init();
  });

  function isTyping(target: EventTarget | null) {
    return target instanceof HTMLElement && target.closest("input, select, textarea, dialog") !== null;
  }

  function onkeydown(event: KeyboardEvent) {
    if (isTyping(event.target) || editor.newScoreOpen || editor.scorePropertiesOpen || editor.timeSignatureOpen || editor.keySignatureOpen) return;
    if (editor.handleKey(event)) event.preventDefault();
  }
</script>

<svelte:window {onkeydown} />

<main class:with-sidebar={editor.sidebarOpen}>
  <Toolbar />
  <ScoreView />
  {#if editor.sidebarOpen}
    <Sidebar />
  {/if}
  <footer>
    <span class="mode">{editor.inputMode ? "Note input" : "Select"}</span>
    {#if editor.hairpinMode}
      <span>{editor.hairpinMode === "Crescendo" ? "Crescendo" : "Diminuendo"}: click a start note, then an end note (the same note for one note) · Esc cancels</span>
    {:else if editor.slurMode || editor.phraseSlurMode}
      <span>{editor.phraseSlurMode ? "Phrase slur" : "Slur"}: click a start note, then an end note · Esc cancels</span>
    {:else if editor.message}
      <span class="error">{editor.message}</span>
    {:else if editor.inputMode}
      <span>A–G enter notes · 1–7 duration · . dot · 0 rest · ←/→ move cursor · Alt+↑/↓ changes voice, then staff · Ctrl+wheel zooms · click a chord tone to target it · ↑/↓ moves target tone or chord · Ctrl+↑/↓ moves an octave · Shift+click adds to chord · Esc leaves</span>
    {:else}
      <span>Click a note to select it · ←/→ move cursor · Alt+↑/↓ changes voice, then staff · Ctrl+wheel zooms · Delete removes it</span>
    {/if}
  </footer>
</main>

<NewScoreDialog />
<ScorePropertiesDialog />
<TimeSignatureDialog />
<KeySignatureDialog />

<style>
  main {
    position: fixed;
    inset: 0;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    grid-template-columns: minmax(0, 1fr);
    width: 100%;
    min-height: 0;
    overflow: hidden;
  }

  main.with-sidebar {
    grid-template-columns: minmax(0, 1fr) auto;
  }

  main > :global(header),
  footer {
    grid-column: 1 / -1;
  }

  footer {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem;
    padding: 0.3rem 0.75rem;
    border-top: 1px solid #bbb;
    background: #f4f4f4;
    font-size: 0.85rem;
  }

  .mode {
    font-weight: 600;
  }

  .error {
    color: #b00020;
  }
</style>
