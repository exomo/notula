<script lang="ts">
  import { editor } from "./editor.svelte";

  const KEY_NAMES = ["C♭", "G♭", "D♭", "A♭", "E♭", "B♭", "F", "C", "G", "D", "A", "E", "B", "F♯", "C♯"];
  const MINOR_NAMES = ["A♭", "E♭", "B♭", "F", "C", "G", "D", "A", "E", "B", "F♯", "C♯", "G♯", "D♯", "A♯"];

  let dialog = $state<HTMLDialogElement | null>(null);
  let fifths = $state(0);

  const measure = $derived(editor.cursor?.measure ?? 0);

  $effect(() => {
    if (!dialog) return;
    if (editor.keySignatureOpen && !dialog.open) {
      fifths = editor.areas.find((area) => area.measure === measure)?.key_fifths ?? 0;
      editor.message = null;
      dialog.showModal();
    } else if (!editor.keySignatureOpen && dialog.open) {
      dialog.close();
    }
  });

  async function apply(event: SubmitEvent) {
    event.preventDefault();
    if (await editor.setKeySignature(fifths)) editor.keySignatureOpen = false;
  }
</script>

<dialog bind:this={dialog} onclose={() => (editor.keySignatureOpen = false)}>
  <form onsubmit={apply}>
    <h2>Key signature</h2>
    <p>Applies from measure {measure + 1} until the next key change.</p>

    <label>
      Key
      <select bind:value={fifths}>
        {#each KEY_NAMES as name, index}
          <option value={index - 7}>{name} major / {MINOR_NAMES[index]} minor</option>
        {/each}
      </select>
    </label>

    {#if editor.message}
      <p class="error">{editor.message}</p>
    {/if}

    <div class="buttons">
      <button type="button" onclick={() => (editor.keySignatureOpen = false)}>Cancel</button>
      <button type="submit">Apply</button>
    </div>
  </form>
</dialog>

<style>
  dialog {
    box-sizing: border-box;
    width: min(28rem, calc(100vw - 2rem));
    border: 1px solid var(--line-strong);
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

  label {
    display: grid;
    gap: 0.2rem;
  }

  select {
    width: 100%;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }

  .error {
    color: var(--danger);
  }
</style>