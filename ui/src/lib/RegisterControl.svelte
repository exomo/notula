<script lang="ts">
  import type { Register } from "./api";

  let {
    value,
    onchange,
  }: {
    value: Register | null;
    onchange: (register: Register | null) => void;
  } = $props();

  type TrebleDot = "high" | "lower_tremolo" | "middle" | "upper_tremolo" | "low";
  type BassDot = "upper" | "lower" | "master";

  // Dot centres as in the font's symbols: a circle of 3.128 units, measured from its bottom left.
  const TREBLE_DOTS: { dot: TrebleDot; x: number; y: number; label: string }[] = [
    { dot: "high", x: 1.564, y: 2.564, label: "High reed (4′)" },
    { dot: "lower_tremolo", x: 0.764, y: 1.564, label: "Tremolo below the 8′ reed" },
    { dot: "middle", x: 1.564, y: 1.564, label: "Middle reed (8′)" },
    { dot: "upper_tremolo", x: 2.364, y: 1.564, label: "Tremolo above the 8′ reed" },
    { dot: "low", x: 1.564, y: 0.564, label: "Low reed (16′)" },
  ];
  const BASS_DOTS: { dot: BassDot; x: number; y: number; label: string }[] = [
    { dot: "upper", x: 1.564, y: 2.316, label: "Upper reed" },
    { dot: "master", x: 2.27, y: 1.564, label: "Master" },
    { dot: "lower", x: 1.564, y: 0.88, label: "Lower reed" },
  ];

  // The standard treble symbols of the font, by their usual names.
  const TREBLE_PRESETS: { name: string; dots: TrebleDot[] }[] = [
    { name: "Piccolo", dots: ["high"] },
    { name: "Clarinet", dots: ["middle"] },
    { name: "Bassoon", dots: ["low"] },
    { name: "Oboe", dots: ["high", "middle"] },
    { name: "Violin", dots: ["middle", "upper_tremolo"] },
    { name: "Imitation musette", dots: ["high", "middle", "upper_tremolo"] },
    { name: "Musette", dots: ["lower_tremolo", "middle", "upper_tremolo"] },
    { name: "Organ", dots: ["high", "low"] },
    { name: "Harmonium", dots: ["high", "middle", "low"] },
    { name: "Bandoneon", dots: ["middle", "low"] },
    { name: "Accordion", dots: ["middle", "upper_tremolo", "low"] },
    { name: "Master", dots: ["high", "lower_tremolo", "upper_tremolo", "low"] },
    { name: "Two choirs", dots: ["lower_tremolo", "upper_tremolo"] },
    { name: "Tremolo, lower octave", dots: ["lower_tremolo", "upper_tremolo", "low"] },
    { name: "Tremolo, upper octave", dots: ["high", "lower_tremolo", "upper_tremolo"] },
    { name: "Double tremolo, lower octave", dots: ["lower_tremolo", "middle", "upper_tremolo", "low"] },
    { name: "Double tremolo, upper octave", dots: ["high", "lower_tremolo", "middle", "upper_tremolo"] },
    { name: "Full", dots: ["high", "lower_tremolo", "middle", "upper_tremolo", "low"] },
    { name: "Upper tremolo only", dots: ["upper_tremolo"] },
    { name: "Lower tremolo only", dots: ["lower_tremolo"] },
  ];
  const BASS_PRESETS: { name: string; dots: BassDot[] }[] = [
    { name: "Upper", dots: ["upper"] },
    { name: "Lower", dots: ["lower"] },
    { name: "Both", dots: ["upper", "lower"] },
    { name: "Master", dots: ["master"] },
    { name: "Master and lower", dots: ["master", "lower"] },
    { name: "Full master", dots: ["upper", "master", "lower"] },
  ];

  const SIZE = 3.128;
  const side = $derived(value?.side ?? "");
  const dots = $derived(value?.side === "bass" ? BASS_DOTS : TREBLE_DOTS);
  const presets = $derived(value?.side === "bass" ? BASS_PRESETS : TREBLE_PRESETS);

  function isSet(dot: string): boolean {
    return Boolean(value && (value as Record<string, unknown>)[dot]);
  }

  /** The preset whose dots are exactly the set ones, if any. */
  const preset = $derived(
    presets.find(
      (item) =>
        dots.every((spot) => isSet(spot.dot) === (item.dots as string[]).includes(spot.dot)),
    )?.name ?? "",
  );

  function withDots(registerSide: "treble" | "bass", set: string[]): Register {
    const register: Record<string, unknown> = { side: registerSide };
    for (const dot of set) register[dot] = true;
    return register as Register;
  }

  function setSide(next: string) {
    if (next === "treble") onchange(withDots("treble", ["middle"]));
    else if (next === "bass") onchange(withDots("bass", ["upper", "lower"]));
    else onchange(null);
  }

  function setPreset(name: string) {
    const chosen = presets.find((item) => item.name === name);
    if (chosen && value) onchange(withDots(value.side, chosen.dots));
  }

  function toggle(dot: string) {
    if (!value) return;
    const set = dots.map((spot) => spot.dot as string).filter((spot) => isSet(spot) !== (spot === dot));
    onchange(withDots(value.side, set));
  }
</script>

<label class="field">
  <span>Register</span>
  <select
    aria-label="Accordion register of the selected note"
    value={side}
    onchange={(event) => setSide(event.currentTarget.value)}
  >
    <option value="">None</option>
    <option value="treble">Treble (3 ranks)</option>
    <option value="bass">Bass (2 ranks)</option>
  </select>
</label>
{#if value}
  <div class="register">
    <svg viewBox="-0.1 -0.1 {SIZE + 0.2} {SIZE + 0.2}" role="group" aria-label="Reeds">
      <circle cx={SIZE / 2} cy={SIZE / 2} r={SIZE / 2 - 0.07} class="outline" />
      {#if value.side === "treble"}
        <line x1="0.13" x2={SIZE - 0.13} y1={SIZE - 1.064} y2={SIZE - 1.064} class="outline" />
        <line x1="0.13" x2={SIZE - 0.13} y1={SIZE - 2.064} y2={SIZE - 2.064} class="outline" />
      {:else}
        <line x1="0.13" x2={SIZE - 0.13} y1={SIZE / 2} y2={SIZE / 2} class="outline" />
      {/if}
      {#each dots as spot (spot.dot)}
        <circle
          cx={spot.x}
          cy={SIZE - spot.y}
          r="0.3"
          class="dot"
          class:set={isSet(spot.dot)}
          role="switch"
          tabindex="0"
          aria-checked={isSet(spot.dot)}
          aria-label={spot.label}
          onclick={() => toggle(spot.dot)}
          onkeydown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              event.stopPropagation();
              toggle(spot.dot);
            }
          }}
        ><title>{spot.label}</title></circle>
      {/each}
    </svg>
    <select
      aria-label="Register preset"
      value={preset}
      onchange={(event) => setPreset(event.currentTarget.value)}
    >
      <option value="" disabled>Custom</option>
      {#each presets as item (item.name)}
        <option value={item.name}>{item.name}</option>
      {/each}
    </select>
  </div>
  <p class="hint">Click the dots to switch reeds on or off.</p>
{/if}

<style>
  .field {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    font-size: 0.82rem;
  }

  .register {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }

  svg {
    flex: none;
    width: 4.2rem;
    height: 4.2rem;
  }

  .outline {
    fill: none;
    stroke: var(--ink);
    stroke-width: 0.1;
  }

  .dot {
    fill: #fff;
    stroke: var(--line-strong);
    stroke-width: 0.06;
    stroke-dasharray: 0.12 0.08;
    cursor: pointer;
  }

  .dot:hover,
  .dot:focus-visible {
    stroke: var(--accent);
    outline: none;
  }

  .dot.set {
    fill: var(--ink);
    stroke: var(--ink);
    stroke-dasharray: none;
  }

  select {
    flex: 1 1 auto;
    min-width: 0;
    box-sizing: border-box;
    height: 2rem;
    padding: 0 0.4rem;
    font: inherit;
    font-size: 0.82rem;
  }

  .hint {
    margin: 0;
    color: var(--muted);
    font-size: 0.8rem;
    line-height: 1.35;
  }
</style>
