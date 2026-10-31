<script lang="ts">
  // The main window, or one of the windows opened from it (#window=…).
  import Floating from "./Floating.svelte";
  import Main from "./Main.svelte";
  import { applyZoom } from "./lib/prefs.svelte";
  import { applyScheme } from "./lib/schemes";
  import { app } from "./lib/state.svelte";
  import { isMain } from "./lib/windows.svelte";

  // Every window follows the size and colour scheme, also when another
  // window changes them.
  $effect(() => {
    if (app.loaded) applyScheme(app.settings.colorScheme);
  });
  $effect(() => {
    if (app.loaded) void applyZoom(app.settings.zoom ?? 1);
  });
</script>

{#if isMain}<Main />{:else}<Floating />{/if}
