<script lang="ts">
  import type { Snippet } from "svelte";

  // A toolbar button that opens a panel of further actions below it.
  let {
    label,
    title,
    trigger,
    children,
  }: {
    /** Accessible name of the trigger button. */
    label: string;
    title?: string;
    trigger: Snippet;
    /** Panel content; call `close` after running an action. */
    children: Snippet<[close: () => void]>;
  } = $props();

  const PANEL_WIDTH = 224;
  const id = `menu-${Math.random().toString(36).slice(2, 9)}`;
  let open = $state(false);
  let button = $state<HTMLButtonElement | null>(null);
  let panel = $state<HTMLDivElement | null>(null);
  let position = $state({ left: 0, top: 0 });

  function toggle() {
    if (open) {
      open = false;
      return;
    }
    const bounds = button?.getBoundingClientRect();
    if (bounds) {
      position = {
        left: Math.max(8, Math.min(bounds.left, window.innerWidth - PANEL_WIDTH - 8)),
        top: bounds.bottom + 4,
      };
    }
    open = true;
  }

  function close() {
    open = false;
  }

  function onWindowClick(event: MouseEvent) {
    const target = event.target as Node;
    if (open && !panel?.contains(target) && !button?.contains(target)) open = false;
  }

  function onKeydown(event: KeyboardEvent) {
    if (open && event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      open = false;
      button?.focus();
    }
  }
</script>

<svelte:window onclick={onWindowClick} />

<button
  bind:this={button}
  class="menu-trigger"
  class:open
  aria-label={label}
  {title}
  aria-haspopup="true"
  aria-expanded={open}
  aria-controls={id}
  onclick={toggle}
  onkeydown={onKeydown}
>
  {@render trigger()}
</button>

{#if open}
  <!-- Escape inside the panel closes it; its controls handle everything else. -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    bind:this={panel}
    {id}
    class="menu-panel"
    role="group"
    aria-label={label}
    tabindex="-1"
    onkeydown={onKeydown}
    style:left="{position.left}px"
    style:top="{position.top}px"
    style:width="{PANEL_WIDTH}px"
  >
    {@render children(close)}
  </div>
{/if}

<style>
  .menu-trigger {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
  }

  .menu-trigger.open {
    border-color: var(--line-strong);
    background: #fff;
  }

  .menu-panel {
    position: fixed;
    z-index: 30;
    display: grid;
    gap: 0.2rem;
    box-sizing: border-box;
    padding: 0.45rem;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    background: var(--toolbar);
    box-shadow: 0 10px 28px rgb(20 32 26 / 20%);
  }

  .menu-panel :global(button) {
    width: 100%;
    text-align: left;
  }

  .menu-panel :global(.menu-label) {
    padding: 0.25rem 0.55rem 0;
    color: var(--muted);
    font-size: 0.74rem;
    font-weight: 600;
  }

  .menu-panel :global(.menu-note) {
    padding: 0 0.55rem 0.25rem;
    overflow: hidden;
    color: var(--muted);
    font-size: 0.78rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .menu-panel :global(select) {
    width: 100%;
  }
</style>
