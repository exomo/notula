<script lang="ts" module>
  export type IconName =
    | "undo"
    | "redo"
    | "play"
    | "stop"
    | "speaker"
    | "chevron"
    | "sidebar"
    | "pencil"
    | "plus"
    | "minus"
    | "tie"
    | "crescendo"
    | "diminuendo";
</script>

<script lang="ts">
  // Simple line icons on a 24 unit grid, drawn in the current text colour.
  let { name }: { name: IconName } = $props();
</script>

<svg class="icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
  {#if name === "undo"}
    <path d="M4 9h11a5 5 0 0 1 0 10h-4" />
    <polyline points="8 5 4 9 8 13" />
  {:else if name === "redo"}
    <path d="M20 9H9a5 5 0 0 0 0 10h4" />
    <polyline points="16 5 20 9 16 13" />
  {:else if name === "play"}
    <polygon class="filled" points="7 4.5 19.5 12 7 19.5" />
  {:else if name === "stop"}
    <rect class="filled" x="6" y="6" width="12" height="12" rx="1.5" />
  {:else if name === "speaker"}
    <polygon points="4 9.5 8 9.5 12.5 5.5 12.5 18.5 8 14.5 4 14.5" />
    <path d="M16 9a4.5 4.5 0 0 1 0 6" />
    <path d="M18.5 6.5a8 8 0 0 1 0 11" />
  {:else if name === "chevron"}
    <polyline points="7 10 12 15 17 10" />
  {:else if name === "sidebar"}
    <rect x="3.5" y="4.5" width="17" height="15" rx="2" />
    <line x1="14.5" y1="4.5" x2="14.5" y2="19.5" />
  {:else if name === "pencil"}
    <path d="M4.5 19.5l1-4.5L15.5 5l3.5 3.5-10 10z" />
    <line x1="13" y1="7.5" x2="16.5" y2="11" />
  {:else if name === "plus"}
    <line x1="12" y1="5.5" x2="12" y2="18.5" />
    <line x1="5.5" y1="12" x2="18.5" y2="12" />
  {:else if name === "minus"}
    <line x1="5.5" y1="12" x2="18.5" y2="12" />
  {:else if name === "tie"}
    <path d="M3.5 10c4 6 13 6 17 0" />
  {:else if name === "crescendo"}
    <polyline points="20 7 4 12 20 17" />
  {:else if name === "diminuendo"}
    <polyline points="4 7 20 12 4 17" />
  {/if}
</svg>

<style>
  .icon {
    display: block;
    width: 1.15rem;
    height: 1.15rem;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
    /* The button around the icon takes the pointer. */
    pointer-events: none;
  }

  .filled {
    fill: currentColor;
    stroke: none;
  }
</style>
