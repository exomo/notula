<script lang="ts">
  import type { StaffText, TextPlacement } from "./api";

  let {
    placement,
    value,
    onchange,
  }: {
    placement: TextPlacement;
    /** The selected note's text on this side of the staff. */
    value: StaffText | null;
    onchange: (text: StaffText | null) => void;
  } = $props();

  // Common texts with their usual look: playing directions upright above the staff,
  // expressions in italics below it.
  const PRESETS: Record<TextPlacement, string[]> = {
    above: ["div.", "unis.", "pizz.", "arco", "con sord.", "senza sord.", "sul pont.", "sul tasto", "solo", "tutti", "simile"],
    below: ["dolce", "espr.", "cantabile", "legato", "leggiero", "marcato", "sempre", "sotto voce", "tranquillo", "grazioso"],
  };

  let text = $state("");
  // Expressions below the staff are usually italic.
  let italic = $state(false);
  let styled = false;
  const listId = $derived(`staff-texts-${placement}`);
  const changed = $derived(text.trim() !== (value?.text ?? "") || italic !== (value?.italic ?? false));

  // Show the note's text so it can be edited in place; a new text keeps the last style.
  $effect(() => {
    text = value?.text ?? "";
    if (value) {
      italic = value.italic ?? false;
      styled = true;
    } else if (!styled) {
      italic = placement === "below";
    }
  });

  /** Picking a common text gives it its usual look. */
  function input(next: string) {
    text = next;
    for (const side of ["above", "below"] as const) {
      if (PRESETS[side].includes(next.trim())) italic = side === "below";
    }
  }

  function apply() {
    const trimmed = text.trim();
    onchange(trimmed ? { text: trimmed, placement, italic } : null);
  }
</script>

<div class="text-control">
  <div class="row">
    <span class="side">{placement === "above" ? "Above" : "Below"}</span>
    <input
      type="text"
      list={listId}
      placeholder={PRESETS[placement][0]}
      value={text}
      oninput={(event) => input(event.currentTarget.value)}
      onkeydown={(event) => {
        if (event.key === "Enter" && changed) apply();
      }}
      aria-label={`Text ${placement} the staff at the selected note`}
      title={placement === "above"
        ? "Text above the staff at the selected note, such as a playing direction; playback ignores it"
        : "Text on the dynamics line below the staff, after a dynamic; playback ignores it"}
    />
    <datalist id={listId}>
      {#each PRESETS[placement] as preset}
        <option value={preset}></option>
      {/each}
    </datalist>
  </div>
  <div class="row">
    <label class="check">
      <input type="checkbox" bind:checked={italic} />
      <span>Italic</span>
    </label>
    <button onclick={apply} disabled={!text.trim() || !changed} title={`Add or change the text ${placement} the staff`}
      >{value === null ? "Add" : "Change"}</button
    >
    <button onclick={() => onchange(null)} disabled={value === null} title={`Remove the text ${placement} the staff`}
      >Remove</button
    >
  </div>
</div>

<style>
  .text-control {
    display: grid;
    gap: 0.3rem;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.3rem;
  }

  .side {
    width: 3.2rem;
    font-size: 0.82rem;
  }

  input[type="text"] {
    flex: 1 1 auto;
    min-width: 0;
    box-sizing: border-box;
    height: 2rem;
    padding: 0 0.4rem;
    font: inherit;
    font-size: 0.82rem;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin-left: 3.5rem;
    margin-right: auto;
    font-size: 0.82rem;
  }

  button {
    box-sizing: border-box;
    height: 2rem;
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 0.3rem 0.6rem;
    background: var(--toolbar);
    color: var(--ink);
    font: inherit;
    font-size: 0.82rem;
    cursor: pointer;
  }

  button:disabled {
    cursor: default;
    opacity: 0.5;
  }
</style>
