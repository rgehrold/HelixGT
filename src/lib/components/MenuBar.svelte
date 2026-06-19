<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";

  interface Props {
    onPrint?: () => void;
    onRestoreView?: () => void;
  }

  let { onPrint, onRestoreView }: Props = $props();

  type MenuId = "file" | "view" | "about" | "help" | null;

  let openMenu = $state<MenuId>(null);

  function toggleMenu(id: MenuId) {
    openMenu = openMenu === id ? null : id;
  }

  function closeMenus() {
    openMenu = null;
  }

  async function exitApp() {
    closeMenus();
    await getCurrentWindow().close();
  }

  function showAbout() {
    closeMenus();
    alert(
      "Universal Gene Tool\n\nConvert, merge, and align genomics files on Windows.\n\nSupported formats: FASTA, FASTQ, GenBank, GFF, BED, SAM, BAM, CRAM, and VCF.",
    );
  }

  function printPage() {
    closeMenus();
    onPrint?.();
  }

  function restoreView() {
    closeMenus();
    onRestoreView?.();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<header class="menubar" onclick={closeMenus}>
  <span class="app-title">Universal Gene Tool</span>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="menus" role="menubar" tabindex="0" onclick={(event) => event.stopPropagation()}>
    <div class="menu-group">
      <button class="menu-trigger" class:active={openMenu === "file"} onclick={() => toggleMenu("file")}>
        File
      </button>
      {#if openMenu === "file"}
        <div class="dropdown">
          <button class="dropdown-item" onclick={printPage}>Print…</button>
          <button class="dropdown-item" onclick={exitApp}>Exit</button>
        </div>
      {/if}
    </div>

    <div class="menu-group">
      <button class="menu-trigger" class:active={openMenu === "view"} onclick={() => toggleMenu("view")}>
        View
      </button>
      {#if openMenu === "view"}
        <div class="dropdown">
          <button class="dropdown-item" onclick={restoreView}>Restore standard view</button>
        </div>
      {/if}
    </div>

    <div class="menu-group">
      <button class="menu-trigger" class:active={openMenu === "about"} onclick={() => toggleMenu("about")}>
        About
      </button>
      {#if openMenu === "about"}
        <div class="dropdown">
          <button class="dropdown-item" onclick={showAbout}>About Universal Gene Tool</button>
        </div>
      {/if}
    </div>

    <div class="menu-group">
      <button class="menu-trigger" class:active={openMenu === "help"} onclick={() => toggleMenu("help")}>
        Help
      </button>
      {#if openMenu === "help"}
        <div class="dropdown">
          <button class="dropdown-item" disabled>Documentation (coming soon)</button>
        </div>
      {/if}
    </div>
  </div>
</header>

<style>
  .menubar {
    display: flex;
    align-items: center;
    gap: 18px;
    padding: 0 4px 10px;
    flex-shrink: 0;
  }

  .app-title {
    color: #cbd5e1;
    font-size: 0.88rem;
    font-weight: 600;
    letter-spacing: 0.02em;
    white-space: nowrap;
  }

  .menus {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .menu-group {
    position: relative;
  }

  .menu-trigger,
  .dropdown-item {
    cursor: pointer;
    border: none;
    background: transparent;
    color: #cbd5e1;
    font: inherit;
  }

  .menu-trigger {
    padding: 6px 12px;
    border-radius: 8px;
    font-size: 0.88rem;
  }

  .menu-trigger:hover,
  .menu-trigger.active {
    background: rgba(51, 65, 85, 0.55);
    color: #ecfeff;
  }

  .dropdown {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    min-width: 200px;
    padding: 6px;
    border-radius: 10px;
    background: rgba(15, 23, 42, 0.98);
    border: 1px solid rgba(148, 163, 184, 0.18);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.35);
    z-index: 40;
  }

  .dropdown-item {
    display: block;
    width: 100%;
    text-align: left;
    padding: 8px 10px;
    border-radius: 8px;
    font-size: 0.86rem;
  }

  .dropdown-item:hover:not(:disabled) {
    background: rgba(34, 211, 238, 0.12);
    color: #ecfeff;
  }

  .dropdown-item:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>