<script lang="ts">
  import { api } from "./api";
  import { editor } from "./editor.svelte";
  import type { RenderList } from "./render";

  let clientWidth = $state(0);
  let list = $state<RenderList | null>(null);
  let svg = $state<SVGSVGElement | null>(null);
  let caretElement = $state<SVGLineElement | null>(null);
  let playbackCaretElement = $state<SVGLineElement | null>(null);
  let error = $state<string | null>(null);
  let request = 0;

  $effect(() => {
    editor.layoutWidth = clientWidth / editor.zoom;
  });

  $effect(() => {
    // Reading these makes the effect rerun when the score, zoom or window changes.
    editor.revision;
    const width = editor.layoutWidth;
    if (width <= 0) return;
    const id = ++request;
    api
      .renderScore(width)
      .then((result) => {
        if (id !== request) return;
        list = result;
        editor.areas = result.areas;
        error = null;
      })
      .catch((e) => {
        if (id === request) error = String(e);
      });
  });

  const caret = $derived.by(() => {
    const cursor = editor.cursor;
    if (!cursor || !list) return null;
    const area = list.areas.find((a) => a.staff === cursor.staff && a.measure === cursor.measure);
    if (!area) return null;
    const column = area.columns.find(([tick]) => tick >= cursor.tick);
    const x = column
      ? column[1] - 0.6
      : area.columns.length > 0
        ? area.x_end - 1.0
        : area.x_start + 1.0;
    return { x, area, voice: cursor.voice };
  });

  const renderedItems = $derived.by(() => {
    if (!list) return [];
    const cursor = editor.cursor;
    if (!cursor) return list.items;
    return [
      ...list.items.filter(
        (item) => item.staff !== cursor.staff || item.voice !== cursor.voice,
      ),
      ...list.items.filter(
        (item) => item.staff === cursor.staff && item.voice === cursor.voice,
      ),
    ];
  });

  const playbackCaret = $derived.by(() => {
    const cursor = editor.playbackCursor;
    if (!cursor || !list) return null;
    const area = list.areas.find((a) => a.staff === cursor.staff && a.measure === cursor.measure);
    if (!area) return null;
    const column = area.columns.find(([tick]) => tick >= cursor.tick);
    const x = column ? column[1] - 0.6 : area.x_end - 1.0;
    return { x, area };
  });

  $effect(() => {
    // Keep the insertion point visible while entering notes.
    if (editor.inputMode && caret) caretElement?.scrollIntoView({ block: "nearest", inline: "nearest" });
  });

  $effect(() => {
    if (editor.playbackPlaying && playbackCaret) {
      playbackCaretElement?.scrollIntoView({ block: "nearest", inline: "nearest" });
    }
  });

  function onclick(event: MouseEvent) {
    if (!svg) return;
    const box = svg.getBoundingClientRect();
    const x = (event.clientX - box.left) / editor.zoom;
    const y = (event.clientY - box.top) / editor.zoom;
    const target = (event.target as Element).closest("[data-event]");
    if (target) {
      void editor.clickEvent(Number(target.getAttribute("data-event")), x, y, event.shiftKey);
      return;
    }
    void editor.clickStaff(x, y, event.shiftKey);
  }
</script>

<div class="view" bind:clientWidth>
  {#if error}
    <p class="error">{error}</p>
  {:else if list}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <svg
      bind:this={svg}
      class="paper"
      class:input={editor.inputMode}
      width={list.width * editor.zoom}
      height={list.height * editor.zoom}
      viewBox="0 0 {list.width} {list.height}"
      {onclick}
    >
      {#if caret}
        {#if editor.inputMode}
          <rect
            class="staff-highlight"
            x={caret.area.x_start}
            y={caret.area.y_top - 2}
            width={caret.area.x_end - caret.area.x_start}
            height="8"
          />
        {/if}
        {#if caret.area.voice_count > 1}
          <text
            class="voice-indicator"
            class:additional={caret.voice > 0}
            x={caret.area.x_start}
            y={caret.area.y_top - 0.8}
          >Voice {caret.voice + 1}</text>
        {/if}
        <line
          bind:this={caretElement}
          class="caret"
          x1={caret.x}
          y1={caret.area.y_top - 1}
          x2={caret.x}
          y2={caret.area.y_top + 5}
          stroke-width="0.2"
        />
      {/if}
      {#each renderedItems as item}
        {@const selected = editor.inputMode && item.event !== undefined && item.event === editor.selected}
        {#if item.type === "line"}
          <line
            class:selected
            class:active-voice={editor.cursor !== null && item.staff === editor.cursor.staff && item.voice === editor.cursor.voice}
            data-event={item.event}
            x1={item.x1}
            y1={item.y1}
            x2={item.x2}
            y2={item.y2}
            stroke-width={item.thickness}
          />
        {:else if item.type === "polygon"}
          <polygon
            class:selected
            class:active-voice={editor.cursor !== null && item.staff === editor.cursor.staff && item.voice === editor.cursor.voice}
            data-event={item.event}
            points={item.points.map((p) => p.join(",")).join(" ")}
          />
        {:else if item.type === "glyph"}
          <text
            class="glyph"
            class:selected
            class:active-voice={editor.cursor !== null && item.staff === editor.cursor.staff && item.voice === editor.cursor.voice}
            data-event={item.event}
            x={item.x}
            y={item.y}
            font-size={item.size}
            >{item.glyph}</text
          >
        {:else}
          <text x={item.x} y={item.y} font-size={item.size} text-anchor={item.anchor}>{item.text}</text>
        {/if}
      {/each}
      {#if editor.playbackPlaying && playbackCaret}
        <line
          bind:this={playbackCaretElement}
          class="playback-caret"
          x1={playbackCaret.x}
          y1={playbackCaret.area.y_top - 1}
          x2={playbackCaret.x}
          y2={playbackCaret.area.y_top + 5}
          stroke-width="0.3"
        />
      {/if}
    </svg>
  {/if}
</div>

<style>
  .view {
    height: 100%;
    overflow: auto;
    scrollbar-gutter: stable;
    background: #d8d8d8;
  }

  .paper {
    display: block;
    background: white;
    cursor: default;
    -webkit-user-select: none;
    user-select: none;
  }

  .paper.input {
    cursor: crosshair;
  }

  line {
    stroke: black;
  }

  polygon,
  text {
    fill: black;
  }

  line.selected {
    stroke: #1a73e8;
  }

  line.active-voice {
    stroke: #087f70;
  }

  polygon.active-voice,
  text.active-voice {
    fill: #087f70;
  }

  polygon.selected,
  text.selected {
    fill: #1a73e8;
  }

  .glyph {
    font-family: "Bravura";
    user-select: none;
  }

  .staff-highlight {
    fill: rgba(26, 115, 232, 0.08);
  }

  .caret {
    stroke: #1a73e8;
  }

  .voice-indicator {
    fill: #1a73e8;
    font: 600 0.8px sans-serif;
    pointer-events: none;
  }

  .voice-indicator.additional {
    fill: #087f70;
  }

  .playback-caret {
    stroke: #cf3e36;
    pointer-events: none;
  }

  .error {
    margin: 1rem;
    color: #b00020;
  }
</style>
