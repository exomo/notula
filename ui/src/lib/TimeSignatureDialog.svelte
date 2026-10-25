<script lang="ts">
  import type { TimeSignatureDisplay } from "./api";
  import { editor } from "./editor.svelte";

  const DENOMINATORS = [1, 2, 4, 8, 16, 32];

  let dialog = $state<HTMLDialogElement | null>(null);
  let numerator = $state(4);
  let denominator = $state(4);
  let display = $state<TimeSignatureDisplay>("numeric");

  const measure = $derived(editor.cursor?.measure ?? 0);

  $effect(() => {
    if (!dialog) return;
    if (editor.timeSignatureOpen && !dialog.open) {
      const current = editor.areas.find((a) => a.measure === measure)?.time_signature;
      const area = editor.areas.find((a) => a.measure === measure);
      if (current) {
        numerator = current.numerator;
        denominator = current.denominator;
      }
      display = area?.time_signature_display ?? "numeric";
      editor.message = null;
      dialog.showModal();
    } else if (!editor.timeSignatureOpen && dialog.open) {
      dialog.close();
    }
  });

  const valid = $derived(Number.isInteger(numerator) && numerator >= 1 && numerator <= 32);

  function changeDisplay(value: TimeSignatureDisplay) {
    display = value;
    if (value === "common") {
      numerator = 4;
      denominator = 4;
    } else if (value === "alla_breve") {
      numerator = 2;
      denominator = 2;
    }
  }

  async function apply(event: SubmitEvent) {
    event.preventDefault();
    if (await editor.setTimeSignature(numerator, denominator, display)) editor.timeSignatureOpen = false;
  }
</script>

<dialog bind:this={dialog} onclose={() => (editor.timeSignatureOpen = false)}>
  <form onsubmit={apply}>
    <h2>Time signature</h2>
    <p>
      Applies from measure {measure + 1} until the next time signature change.
    </p>

    <label>
      Display
      <select value={display} onchange={(event) => changeDisplay(event.currentTarget.value as TimeSignatureDisplay)}>
        <option value="numeric">Numeric</option>
        <option value="common">Common time (4/4)</option>
        <option value="alla_breve">Alla breve (2/2)</option>
      </select>
    </label>

    {#if display === "numeric"}
      <div class="time">
        <input type="number" min="1" max="32" bind:value={numerator} aria-label="Beats per measure" />
        /
        <select bind:value={denominator} aria-label="Beat unit">
          {#each DENOMINATORS as d}
            <option value={d}>{d}</option>
          {/each}
        </select>
      </div>
    {/if}

    {#if editor.message}
      <p class="error">{editor.message}</p>
    {/if}

    <div class="buttons">
      <button type="button" onclick={() => (editor.timeSignatureOpen = false)}>Cancel</button>
      <button type="submit" disabled={!valid}>Apply</button>
    </div>
  </form>
</dialog>

<style>
  dialog {
    min-width: 20rem;
    border: 1px solid #888;
    border-radius: 6px;
    padding: 1rem 1.25rem;
  }

  form {
    display: grid;
    gap: 0.75rem;
  }

  h2,
  p {
    margin: 0;
  }

  .time {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  input[type="number"] {
    width: 4.5rem;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }

  .error {
    color: #b00020;
  }
</style>
