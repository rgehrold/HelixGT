<script lang="ts">
  let { children } = $props();
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
    background: #070b14;
    color: #e8eef8;
    font-family: "DM Sans", system-ui, sans-serif;
    -webkit-font-smoothing: antialiased;
  }

  :global(button, input, select) {
    font: inherit;
  }

  :global(::selection) {
    background: rgba(56, 189, 248, 0.35);
  }

  :global(*) {
    scrollbar-width: thin;
    scrollbar-color: rgba(71, 85, 105, 0.9) rgba(15, 23, 42, 0.95);
  }

  :global(*::-webkit-scrollbar) {
    width: 10px;
    height: 10px;
  }

  :global(*::-webkit-scrollbar-track) {
    background: rgba(15, 23, 42, 0.95);
    border-radius: 999px;
  }

  :global(*::-webkit-scrollbar-thumb) {
    background: linear-gradient(180deg, rgba(71, 85, 105, 0.95), rgba(51, 65, 85, 0.95));
    border: 2px solid rgba(15, 23, 42, 0.95);
    border-radius: 999px;
  }

  :global(*::-webkit-scrollbar-thumb:hover) {
    background: linear-gradient(180deg, rgba(100, 116, 139, 0.95), rgba(71, 85, 105, 0.95));
  }
</style>