<script lang="ts">
  import { openManual } from "$lib/manual/store.svelte";

  interface Props {
    /** Manual page id to open (see src/lib/manual/content.ts). */
    section: string;
    label?: string;
  }

  let { section, label = "Open manual" }: Props = $props();

  function open(event: MouseEvent) {
    event.preventDefault();
    event.stopPropagation();
    openManual(section);
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      event.stopPropagation();
      openManual(section);
    }
  }
</script>

<!--
  span[role=button] (not <button>) so this is never a second labelable control
  inside a parent <label> — that pattern makes browsers inflate the hit target
  to the whole label.
-->
<span
  class="info-link"
  role="button"
  tabindex="0"
  aria-label={label}
  title={label}
  onclick={open}
  onkeydown={onKeydown}
>
  i
</span>

<style>
  /* Higher specificity than parent pane rules like `.toggle span`. */
  span.info-link[role="button"] {
    box-sizing: border-box;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    align-self: center;
    width: 18px;
    height: 18px;
    min-width: 18px;
    min-height: 18px;
    max-width: 18px;
    max-height: 18px;
    margin: 0 0 0 2px;
    padding: 0;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: var(--text-muted);
    opacity: 0.45;
    font-size: 0.72rem;
    font-weight: 700;
    font-style: italic;
    font-family: Georgia, "Times New Roman", serif;
    line-height: 1;
    letter-spacing: 0;
    cursor: pointer;
    vertical-align: middle;
    user-select: none;
    -webkit-user-select: none;
  }

  span.info-link[role="button"]:hover,
  span.info-link[role="button"]:focus-visible {
    opacity: 0.95;
    color: var(--link-color);
    background: color-mix(in srgb, var(--link-color) 12%, transparent);
    outline: none;
  }
</style>
