<script lang="ts">
  import { onMount } from "svelte";
  import { applyTheme } from "$lib/theme.svelte";
  import "$lib/theme.css";

  let { children } = $props();

  onMount(() => {
    applyTheme();
  });
</script>

{@render children()}

<style>
  :global(*) {
    box-sizing: border-box;
  }

  :global(html, body) {
    margin: 0;
    height: 100%;
    overflow: hidden;
    background: var(--bg-base);
    color: var(--text-primary);
    font-family: "DM Sans", system-ui, -apple-system, "Segoe UI", sans-serif;
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
  }

  :global(button, input, select) {
    font: inherit;
  }

  :global(button:focus-visible),
  :global(input:focus-visible),
  :global(select:focus-visible) {
    outline: 2px solid var(--focus-ring, var(--accent-highlight));
    outline-offset: 1px;
  }

  :global(::selection) {
    background: var(--selection-bg);
  }

  :global(*) {
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) var(--scrollbar-track);
  }

  :global(*::-webkit-scrollbar) {
    width: 8px;
    height: 8px;
  }

  :global(*::-webkit-scrollbar-track) {
    background: var(--scrollbar-track);
    border-radius: 999px;
  }

  :global(*::-webkit-scrollbar-thumb) {
    background: var(--scrollbar-thumb);
    border-radius: 999px;
  }

  :global(*::-webkit-scrollbar-thumb:hover) {
    background: var(--scrollbar-thumb-hover);
  }
</style>