<script lang="ts">
  import { TEMPO_BEATS } from "./editor.svelte";

  // `beat` is a TEMPO_BEATS key ("value:dots").
  let {
    beat = $bindable(),
    unitsPerMinute = $bindable(),
    visible = $bindable(),
    disabled = false,
  }: { beat: string; unitsPerMinute: number; visible: boolean; disabled?: boolean } = $props();
</script>

<fieldset>
  <legend>Starting tempo</legend>
  <label>
    Tempo
    <span class="controls">
      <select bind:value={beat} aria-label="Tempo beat unit" {disabled}>
        {#each TEMPO_BEATS as option (option.key)}
          <option value={option.key}>{option.label}</option>
        {/each}
      </select>
      <input
        type="number"
        min="1"
        max="1000"
        step="1"
        bind:value={unitsPerMinute}
        aria-label="Tempo in beats per minute"
        {disabled}
        required
      />
      <span aria-hidden="true">BPM</span>
    </span>
  </label>
  <label class="visible">
    <input type="checkbox" bind:checked={visible} {disabled} />
    Show the tempo mark at the start of the score
  </label>
</fieldset>

<style>
  fieldset {
    display: grid;
    gap: 0.5rem;
    min-width: 0;
  }

  label {
    display: grid;
    gap: 0.2rem;
  }

  .visible,
  .controls {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .controls input {
    width: 6rem;
  }
</style>
