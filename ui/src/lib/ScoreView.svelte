<script lang="ts">
  import { api } from "./api";
  import { editor } from "./editor.svelte";
  import { glyphInkBox, type RenderList } from "./render";

  // Room around a glyph's ink that still counts as clicking it, in staff spaces.
  const GLYPH_HIT_MARGIN = 0.2;

  let clientWidth = $state(0);
  let list = $state<RenderList | null>(null);
  let view = $state<HTMLDivElement | null>(null);
  let svg = $state<SVGSVGElement | null>(null);
  let caretElement = $state<SVGLineElement | null>(null);
  let playbackCaretElement = $state<SVGLineElement | null>(null);
  let error = $state<string | null>(null);
  // Glyph hit boxes are measured from the font, so they wait until it has loaded.
  let fontReady = $state(false);
  let request = 0;
  let caretScrollFrame = 0;
  let expectedScroll: { left: number; top: number; expires: number } | null = null;

  $effect(() => {
    editor.setNoteEditor(svg);
  });

  $effect(() => {
    void document.fonts.load("10px Bravura").then(() => (fontReady = true));
  });

  $effect(() => {
    const element = view;
    if (!element) return;
    const onWheel = (event: WheelEvent) => {
      if (!event.ctrlKey) return;
      event.preventDefault();
      if (event.deltaY < 0) editor.zoomIn();
      else if (event.deltaY > 0) editor.zoomOut();
    };
    element.addEventListener("wheel", onWheel, { passive: false });
    return () => element.removeEventListener("wheel", onWheel);
  });

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

  function keepCaretVisible(element: SVGLineElement | null) {
    if (!element || !view) return;
    const viewport = view.getBoundingClientRect();
    const caretBounds = element.getBoundingClientRect();
    const margin = 12;
    let scrollTop = view.scrollTop;
    let scrollLeft = view.scrollLeft;
    if (caretBounds.top < viewport.top + margin) {
      scrollTop -= viewport.top + margin - caretBounds.top;
    } else if (caretBounds.bottom > viewport.bottom - margin) {
      scrollTop += caretBounds.bottom - (viewport.bottom - margin);
    }
    if (caretBounds.left < viewport.left + margin) {
      scrollLeft -= viewport.left + margin - caretBounds.left;
    } else if (caretBounds.right > viewport.right - margin) {
      scrollLeft += caretBounds.right - (viewport.right - margin);
    }
    if (scrollTop !== view.scrollTop || scrollLeft !== view.scrollLeft) {
      view.scrollTop = scrollTop;
      view.scrollLeft = scrollLeft;
      expectedScroll = {
        top: view.scrollTop,
        left: view.scrollLeft,
        expires: performance.now() + 250,
      };
    }
  }

  function scheduleCaretVisibility() {
    if (caretScrollFrame) cancelAnimationFrame(caretScrollFrame);
    caretScrollFrame = requestAnimationFrame(() => {
      caretScrollFrame = 0;
      keepCaretVisible(caretElement);
    });
  }

  function onViewScroll() {
    if (!caret || !view || !caretElement) return;
    const isExpected =
      expectedScroll !== null &&
      performance.now() <= expectedScroll.expires &&
      Math.abs(view.scrollTop - expectedScroll.top) < 1 &&
      Math.abs(view.scrollLeft - expectedScroll.left) < 1;
    expectedScroll = null;
    if (isExpected) return;
    if (!editor.inputMode) return;

    const viewport = view.getBoundingClientRect();
    const caretBounds = caretElement.getBoundingClientRect();
    const margin = 12;
    if (
      caretBounds.top < viewport.top + margin ||
      caretBounds.bottom > viewport.bottom - margin ||
      caretBounds.left < viewport.left + margin ||
      caretBounds.right > viewport.right - margin
    ) {
      editor.disableInputMode();
    }
  }

  $effect(() => {
    if (!caret) return;
    caret.x;
    caret.area;
    editor.cursor?.tick;
    scheduleCaretVisibility();
  });

  $effect(() => {
    if (editor.inputMode && caret) scheduleCaretVisibility();
  });

  $effect(() => {
    if (editor.playbackPlaying && playbackCaret)
      requestAnimationFrame(() => keepCaretVisible(playbackCaretElement));
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

<div class="view" bind:this={view} bind:clientWidth onscroll={onViewScroll}>
  {#if error}
    <p class="error">{error}</p>
  {:else if list}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <svg
      bind:this={svg}
      class="paper"
      class:input={editor.inputMode}
      tabindex="-1"
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
        {:else if item.type === "white_polygon"}
          <polygon class="white-underlay" points={item.points.map((p) => p.join(",")).join(" ")} />
        {:else if item.type === "tie" || item.type === "slur" || item.type === "phrase_slur"}
          <polygon
            class:tie={item.type === "tie"}
            class:slur={item.type === "slur"}
            class:phrase-slur={item.type === "phrase_slur"}
            class:active-staff={editor.cursor !== null && item.staff === editor.cursor.staff}
            class:selected
            data-event={item.event}
            points={item.points.map((p) => p.join(",")).join(" ")}
          />
        {:else if item.type === "knockout"}
          <rect x={item.x} y={item.y} width={item.width} height={item.height} fill="white" />
        {:else if item.type === "glyph"}
          <text
            class="glyph"
            class:selected
            class:active-voice={editor.cursor !== null && item.staff === editor.cursor.staff && item.voice === editor.cursor.voice}
            class:selected-tone={editor.inputMode && editor.selectedTone !== null && editor.selectedTone.event === item.event && Math.abs(item.y - editor.selectedTone.y) < 0.01}
            data-event={item.event}
            x={item.x}
            y={item.y}
            font-size={item.size}
            text-anchor={item.centered ? "middle" : undefined}
            >{item.glyph}</text
          >
          {#if item.event !== undefined && fontReady}
            {@const box = glyphInkBox(item, GLYPH_HIT_MARGIN)}
            <rect
              class="hit-box"
              data-event={item.event}
              x={box.x}
              y={box.y}
              width={box.width}
              height={box.height}
            />
          {/if}
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
    min-height: 0;
    min-width: 0;
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

  polygon.white-underlay {
    fill: white;
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

  polygon.active-staff {
    fill: #087f70;
  }

  polygon.selected,
  polygon.tie.selected,
  text.selected {
    fill: #1a73e8;
  }

  .glyph {
    font-family: "Bravura";
    user-select: none;
  }

  /* SVG hit-tests text by character cell, and Bravura's cells reach about 8 staff spaces
     above and below a glyph. Clicks on glyphs go to the hit boxes drawn over their ink. */
  .paper text {
    pointer-events: none;
  }

  .hit-box {
    fill: transparent;
  }

  .glyph.selected-tone {
    fill: #c24d2c;
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
