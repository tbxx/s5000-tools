import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri expects a fixed port and must not have the screen cleared.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  // macOS's own WebKit runs the app: Safari 13 is what Catalina (10.15)
  // shipped with, and it can't be updated past 15.6. Newer syntax is lowered.
  build: { target: "safari13" },
});
