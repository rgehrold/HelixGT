<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { openManual } from "$lib/manual/store.svelte";
  import {
    applyTheme,
    setMode,
    setPalette,
    theme,
    toggleMode,
    togglePalette,
    wordmarkSrc,
  } from "$lib/theme.svelte";

  interface Props {
    onPrint?: () => void;
    onRestoreView?: () => void;
    onToggleLogs?: () => void;
    onToggleFilesBrowser?: () => void;
    filesBrowserCollapsed?: boolean;
    logsOpen?: boolean;
  }

  let {
    onPrint,
    onRestoreView,
    onToggleLogs,
    onToggleFilesBrowser,
    filesBrowserCollapsed = false,
    logsOpen = false,
  }: Props = $props();

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

  function openHelp(pageId: string) {
    closeMenus();
    openManual(pageId);
  }

  function printPage() {
    closeMenus();
    onPrint?.();
  }

  function restoreView() {
    closeMenus();
    onRestoreView?.();
  }

  function chooseMode(next: "dark" | "light") {
    setMode(next);
    closeMenus();
  }

  function choosePalette(next: "blue" | "orange") {
    setPalette(next);
    closeMenus();
  }

  function onLogoClick() {
    togglePalette();
  }

  $effect(() => {
    theme.palette;
    theme.mode;
    applyTheme();
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<header class="menubar" onclick={closeMenus}>
  <button
    type="button"
    class="logo-btn"
    title="Switch blue / orange theme"
    aria-label="Switch theme palette"
    onclick={(event) => {
      event.stopPropagation();
      onLogoClick();
    }}
  >
    <img class="app-logo" src={wordmarkSrc()} alt="HelixGT" />
  </button>

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
          <button
            class="dropdown-item"
            onclick={() => {
              onToggleFilesBrowser?.();
              closeMenus();
            }}
          >
            {filesBrowserCollapsed ? "Show input browser" : "Hide input browser"}
          </button>
          <div class="dropdown-sep" role="separator"></div>
          <button class="dropdown-item" class:selected={theme.mode === "dark"} onclick={() => chooseMode("dark")}>
            <span class="check">{theme.mode === "dark" ? "✓" : ""}</span>
            Dark mode
          </button>
          <button class="dropdown-item" class:selected={theme.mode === "light"} onclick={() => chooseMode("light")}>
            <span class="check">{theme.mode === "light" ? "✓" : ""}</span>
            Light mode
          </button>
          <div class="dropdown-sep" role="separator"></div>
          <button class="dropdown-item" class:selected={theme.palette === "blue"} onclick={() => choosePalette("blue")}>
            <span class="check">{theme.palette === "blue" ? "✓" : ""}</span>
            Blue theme
          </button>
          <button class="dropdown-item" class:selected={theme.palette === "orange"} onclick={() => choosePalette("orange")}>
            <span class="check">{theme.palette === "orange" ? "✓" : ""}</span>
            Orange theme
          </button>
          <div class="dropdown-sep" role="separator"></div>
          <button class="dropdown-item" onclick={restoreView}>Restore standard view</button>
        </div>
      {/if}
    </div>

    <div class="menu-group">
      <button class="menu-trigger" class:active={openMenu === "help"} onclick={() => toggleMenu("help")}>
        Help
      </button>
      {#if openMenu === "help"}
        <div class="dropdown">
          <button class="dropdown-item" onclick={() => openHelp("welcome")}>User manual</button>
          <button class="dropdown-item" onclick={() => openHelp("quick-start")}>Quick start</button>
          <button class="dropdown-item" onclick={() => openHelp("shortcuts")}>Keyboard & mouse</button>
          <button class="dropdown-item" onclick={() => openHelp("troubleshooting")}>Troubleshooting</button>
          <div class="dropdown-sep" role="separator"></div>
          <button class="dropdown-item" onclick={() => openHelp("formats")}>File formats</button>
          <button class="dropdown-item" onclick={() => openHelp("about")}>About HelixGT</button>
        </div>
      {/if}
    </div>
  </div>

  <div class="menubar-end">
    <button
      type="button"
      class="logs-btn"
      class:active={!filesBrowserCollapsed}
      title={filesBrowserCollapsed ? "Show input browser" : "Hide input browser"}
      aria-pressed={!filesBrowserCollapsed}
      onclick={(event) => {
        event.stopPropagation();
        closeMenus();
        onToggleFilesBrowser?.();
      }}
    >
      Files
    </button>
    <button
      type="button"
      class="logs-btn"
      class:active={logsOpen}
      title="Activity and tool logs"
      aria-pressed={logsOpen}
      onclick={(event) => {
        event.stopPropagation();
        closeMenus();
        onToggleLogs?.();
      }}
    >
      Logs
    </button>
  </div>
</header>

<style>
  .menubar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 2px 6px;
    flex-shrink: 0;
  }

  .menubar-end {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .logs-btn {
    cursor: pointer;
    border: 1px solid var(--chip-border);
    background: var(--chip-bg);
    color: var(--text-menu);
    padding: 3px 10px;
    border-radius: 999px;
    font: inherit;
    font-size: 0.78rem;
    font-weight: 600;
  }

  .logs-btn:hover {
    background: var(--menu-hover-bg);
    color: var(--menu-active-text);
  }

  .logs-btn.active {
    color: var(--chip-active-text);
    border-color: var(--chip-active-border);
    background: var(--chip-active-bg);
  }

  .logo-btn {
    cursor: pointer;
    border: none;
    background: transparent;
    padding: 0;
    border-radius: 8px;
    line-height: 0;
  }

  .logo-btn:hover {
    background: var(--menu-hover-bg);
  }

  .app-logo {
    height: 24px;
    width: auto;
    display: block;
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
    color: var(--text-menu);
    font: inherit;
  }

  .menu-trigger {
    padding: 4px 8px;
    border-radius: 6px;
    font-size: 0.82rem;
  }

  .menu-trigger:hover,
  .menu-trigger.active {
    background: var(--menu-hover-bg);
    color: var(--menu-active-text);
  }

  .dropdown {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    min-width: 220px;
    padding: 6px;
    border-radius: 10px;
    background: var(--dropdown-bg);
    border: 1px solid var(--dropdown-border);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.35);
    z-index: 40;
  }

  .dropdown-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    text-align: left;
    padding: 8px 10px;
    border-radius: 8px;
    font-size: 0.86rem;
  }

  .dropdown-item .check {
    width: 14px;
    flex-shrink: 0;
    color: var(--accent-highlight);
    font-size: 0.82rem;
  }

  .dropdown-item:hover:not(:disabled) {
    background: var(--dropdown-hover-bg);
    color: var(--menu-active-text);
  }

  .dropdown-item.selected {
    color: var(--menu-active-text);
  }

  .dropdown-item:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .dropdown-sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--chip-border);
  }
</style>