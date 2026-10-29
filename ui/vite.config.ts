import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig(({ command }) => ({
  // In development, components inject their own styles. Separate style modules only work once
  // the dev server has compiled the component, so after a restart or a cached page load they can
  // come back as the raw component source and break the layout.
  plugins: [svelte({ emitCss: command === "build" })],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
}));
