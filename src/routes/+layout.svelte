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
    font-family: "DM Sans", system-ui, sans-serif;
    -webkit-font-smoothing: antialiased;
  }

  :global(button, input, select) {
    font: inherit;
  }

  :global(::selection) {
    background: var(--selection-bg);
  }

  :global(*) {
    scrollbar-width: thin;
    scrollbar-color: rgba(71, 85, 105, 0.9) var(--scrollbar-track);
  }

  :global(*::-webkit-scrollbar) {
    width: 10px;
    height: 10px;
  }

  :global(*::-webkit-scrollbar-track) {
    background: var(--scrollbar-track);
    border-radius: 999px;
  }

  :global(*::-webkit-scrollbar-thumb) {
    background: var(--scrollbar-thumb);
    border: 2px solid var(--scrollbar-track);
    border-radius: 999px;
  }

  :global(*::-webkit-scrollbar-thumb:hover) {
    background: var(--scrollbar-thumb-hover);
  }
</style>