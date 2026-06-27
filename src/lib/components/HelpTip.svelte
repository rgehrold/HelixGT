<script lang="ts">
  interface Props {
    text: string;
    label?: string;
  }

  let { text, label = "Help" }: Props = $props();
</script>

<span class="help-tip">
  <button type="button" class="help-btn" aria-label={label}>?</button>
  <span class="help-tooltip" role="tooltip">{text}</span>
</span>

<style>
  .help-tip {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    width: 18px;
    height: 18px;
    vertical-align: middle;
  }

  .help-btn {
    width: 18px;
    height: 18px;
    min-width: 18px;
    min-height: 18px;
    padding: 0;
    margin: 0;
    border-radius: 999px;
    border: 1px solid var(--help-border);
    background: var(--help-bg);
    color: var(--text-muted);
    font-size: 0.72rem;
    font-weight: 700;
    cursor: help;
    line-height: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .help-btn:hover,
  .help-btn:focus-visible {
    color: var(--link-color);
    border-color: var(--chip-active-border);
  }

  .help-tip:hover .help-tooltip,
  .help-btn:focus-visible + .help-tooltip {
    opacity: 1;
    visibility: visible;
    transform: translateY(-50%);
  }

  .help-tooltip {
    position: absolute;
    left: calc(100% + 8px);
    top: 50%;
    transform: translateY(calc(-50% + 4px));
    z-index: 30;
    width: max-content;
    max-width: 280px;
    padding: 8px 10px;
    border-radius: 10px;
    border: 1px solid var(--help-tooltip-border);
    background: var(--help-tooltip-bg);
    color: var(--text-menu);
    font-size: 0.78rem;
    font-weight: 400;
    line-height: 1.45;
    box-shadow: 0 10px 24px rgba(0, 0, 0, 0.35);
    opacity: 0;
    visibility: hidden;
    pointer-events: none;
    transition: opacity 0.15s ease, transform 0.15s ease, visibility 0.15s ease;
  }
</style>