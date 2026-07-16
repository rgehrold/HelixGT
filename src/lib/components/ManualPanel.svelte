<script lang="ts">
  import { tick } from "svelte";
  import {
    MANUAL_CATEGORIES,
    getManualPage,
    pagesInCategory,
    parseWikiText,
    searchManualPages,
    type ManualBlock,
    type ManualPage,
  } from "$lib/manual/content";
  import { closeManual, manualState, setManualPage } from "$lib/manual/store.svelte";

  let search = $state("");
  let contentEl = $state<HTMLElement | null>(null);

  const activePage = $derived(getManualPage(manualState.pageId) ?? getManualPage("welcome")!);

  const filteredPages = $derived(searchManualPages(search));

  const navGroups = $derived.by(() => {
    const q = search.trim();
    if (q) {
      return [
        {
          id: "search",
          title: `Results (${filteredPages.length})`,
          pages: filteredPages,
        },
      ];
    }
    return MANUAL_CATEGORIES.map((cat) => ({
      id: cat.id,
      title: cat.title,
      pages: pagesInCategory(cat.id),
    })).filter((g) => g.pages.length > 0);
  });

  $effect(() => {
    if (!manualState.open) return;
    const id = manualState.pageId;
    void tick().then(() => {
      if (contentEl) contentEl.scrollTop = 0;
      const navItem = document.querySelector(`[data-manual-nav="${id}"]`);
      navItem?.scrollIntoView({ block: "nearest" });
    });
  });

  function selectPage(page: ManualPage) {
    setManualPage(page.id);
  }

  function goToPage(pageId: string) {
    if (!getManualPage(pageId)) return;
    setManualPage(pageId);
  }

  function linkLabel(pageId: string, label?: string) {
    if (label) return label;
    return getManualPage(pageId)?.title ?? pageId;
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Escape" && manualState.open) {
      event.preventDefault();
      closeManual();
    }
  }

  function onBackdropClick(event: MouseEvent) {
    if (event.target === event.currentTarget) closeManual();
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if manualState.open}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="manual-backdrop" onclick={onBackdropClick} role="presentation">
    <section class="manual-panel" role="dialog" aria-modal="true" aria-label="User manual">
      <header class="manual-head">
        <div class="head-titles">
          <h2>User manual</h2>
          <p class="subtitle">HelixGT guide — click a topic or use the faint i next to controls</p>
        </div>
        <button type="button" class="close-btn" onclick={closeManual} aria-label="Close manual">
          ✕
        </button>
      </header>

      <div class="manual-body">
        <aside class="manual-nav">
          <input
            class="search"
            type="search"
            placeholder="Search manual…"
            bind:value={search}
            aria-label="Search manual"
          />
          <nav class="nav-scroll" aria-label="Manual topics">
            {#each navGroups as group (group.id)}
              <div class="nav-group">
                <div class="nav-group-title">{group.title}</div>
                {#if group.pages.length === 0}
                  <p class="nav-empty">No matching topics.</p>
                {:else}
                  {#each group.pages as page (page.id)}
                    <button
                      type="button"
                      class="nav-item"
                      class:active={page.id === activePage.id}
                      data-manual-nav={page.id}
                      onclick={() => selectPage(page)}
                    >
                      {page.title}
                    </button>
                  {/each}
                {/if}
              </div>
            {/each}
          </nav>
        </aside>

        <article class="manual-content" bind:this={contentEl} aria-label={activePage.title}>
          <h1>{activePage.title}</h1>
          {#each activePage.blocks as block, i (i)}
            {@render blockView(block)}
          {/each}
        </article>
      </div>
    </section>
  </div>
{/if}

{#snippet wikiText(text: string)}
  {#each parseWikiText(text) as seg, i (i)}
    {#if seg.type === "text"}
      {seg.text}
    {:else}
      <button
        type="button"
        class="wiki-link"
        class:missing={!getManualPage(seg.pageId)}
        title={getManualPage(seg.pageId) ? `Open “${linkLabel(seg.pageId)}”` : `Missing page: ${seg.pageId}`}
        onclick={() => goToPage(seg.pageId)}
      >
        {linkLabel(seg.pageId, seg.label)}
      </button>
    {/if}
  {/each}
{/snippet}

{#snippet blockView(block: ManualBlock)}
  {#if block.type === "p"}
    <p>{@render wikiText(block.text)}</p>
  {:else if block.type === "h3"}
    <h3>{@render wikiText(block.text)}</h3>
  {:else if block.type === "ul"}
    <ul>
      {#each block.items as item}
        <li>{@render wikiText(item)}</li>
      {/each}
    </ul>
  {:else if block.type === "ol"}
    <ol>
      {#each block.items as item}
        <li>{@render wikiText(item)}</li>
      {/each}
    </ol>
  {:else if block.type === "note"}
    <div class="callout note">
      <span class="callout-label">Note</span>
      <p>{@render wikiText(block.text)}</p>
    </div>
  {:else if block.type === "tip"}
    <div class="callout tip">
      <span class="callout-label">Tip</span>
      <p>{@render wikiText(block.text)}</p>
    </div>
  {:else if block.type === "table"}
    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            {#each block.headers as h}
              <th>{@render wikiText(h)}</th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each block.rows as row}
            <tr>
              {#each row as cell}
                <td>{@render wikiText(cell)}</td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else if block.type === "kbd"}
    <ul class="kbd-list">
      {#each block.items as item}
        <li>
          <kbd>{@render wikiText(item.keys)}</kbd>
          <span>{@render wikiText(item.desc)}</span>
        </li>
      {/each}
    </ul>
  {/if}
{/snippet}

<style>
  .manual-backdrop {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 12px;
    background: rgba(0, 0, 0, 0.5);
    backdrop-filter: blur(4px);
  }

  .manual-panel {
    display: flex;
    flex-direction: column;
    width: min(960px, 100%);
    height: min(640px, calc(100vh - 24px));
    background: var(--panel-bg);
    border: 1px solid var(--panel-border);
    border-radius: 12px;
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.45);
    overflow: hidden;
    backdrop-filter: blur(14px);
  }

  .manual-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 10px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--panel-border);
    flex-shrink: 0;
  }

  .head-titles h2 {
    margin: 0;
    font-size: 0.95rem;
  }

  .subtitle {
    margin: 2px 0 0;
    font-size: 0.72rem;
    color: var(--text-muted);
  }

  .close-btn {
    width: 34px;
    height: 34px;
    padding: 0;
    border-radius: 10px;
    border: 1px solid var(--chip-border);
    background: var(--chip-bg);
    color: var(--text-menu);
    cursor: pointer;
    font-weight: 600;
    flex-shrink: 0;
  }

  .close-btn:hover {
    border-color: var(--chip-active-border);
    color: var(--menu-active-text);
  }

  .manual-body {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .manual-nav {
    width: 220px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--panel-border);
    background: color-mix(in srgb, var(--panel-bg) 88%, transparent);
  }

  .search {
    margin: 8px 8px 6px;
    padding: 6px 8px;
    border-radius: 8px;
    border: 1px solid var(--input-border);
    background: var(--input-bg);
    color: var(--text-primary);
    font-size: 0.78rem;
  }

  .nav-scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 4px 8px 12px;
  }

  .nav-group {
    margin-bottom: 12px;
  }

  .nav-group-title {
    font-size: 0.68rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
    padding: 6px 8px 4px;
  }

  .nav-empty {
    margin: 0;
    padding: 8px;
    font-size: 0.8rem;
    color: var(--text-muted);
  }

  .nav-item {
    display: block;
    width: 100%;
    text-align: left;
    padding: 8px 10px;
    margin-bottom: 2px;
    border: 1px solid transparent;
    border-radius: 10px;
    background: transparent;
    color: var(--text-menu);
    cursor: pointer;
    font: inherit;
    font-size: 0.84rem;
    font-weight: 600;
  }

  .nav-item:hover {
    background: var(--menu-hover-bg);
  }

  .nav-item.active {
    background: var(--chip-active-bg);
    border-color: var(--chip-active-border);
    color: var(--chip-active-text);
  }

  .manual-content {
    flex: 1;
    min-width: 0;
    overflow: auto;
    padding: 14px 18px 20px;
  }

  .manual-content h1 {
    margin: 0 0 10px;
    font-size: 1.15rem;
    font-weight: 700;
    color: var(--text-primary);
  }

  .manual-content h3 {
    margin: 14px 0 6px;
    font-size: 0.9rem;
    font-weight: 650;
    color: var(--text-primary);
  }

  .manual-content p {
    margin: 0 0 8px;
    font-size: 0.84rem;
    line-height: 1.5;
    color: var(--text-menu);
  }

  .wiki-link {
    display: inline;
    margin: 0;
    padding: 0;
    border: none;
    background: none;
    color: var(--link-color);
    font: inherit;
    font-weight: 600;
    text-decoration: underline;
    text-decoration-color: color-mix(in srgb, var(--link-color) 45%, transparent);
    text-underline-offset: 2px;
    cursor: pointer;
  }

  .wiki-link:hover,
  .wiki-link:focus-visible {
    color: var(--accent-highlight, var(--link-color));
    text-decoration-color: currentColor;
    outline: none;
  }

  .wiki-link.missing {
    color: var(--error, #f87171);
    text-decoration-style: wavy;
  }

  .manual-content ul,
  .manual-content ol {
    margin: 0 0 14px;
    padding-left: 1.25rem;
    font-size: 0.9rem;
    line-height: 1.55;
    color: var(--text-menu);
  }

  .manual-content li {
    margin-bottom: 6px;
  }

  .callout {
    margin: 0 0 14px;
    padding: 10px 12px;
    border-radius: 12px;
    border: 1px solid var(--chip-border);
    background: var(--chip-bg);
  }

  .callout p {
    margin: 0;
  }

  .callout-label {
    display: block;
    font-size: 0.7rem;
    font-weight: 700;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    margin-bottom: 4px;
    color: var(--link-color);
  }

  .callout.note {
    border-color: color-mix(in srgb, var(--link-color) 25%, var(--chip-border));
  }

  .callout.tip {
    border-color: color-mix(in srgb, var(--accent-highlight, var(--link-color)) 30%, var(--chip-border));
  }

  .table-wrap {
    overflow-x: auto;
    margin: 0 0 16px;
    border-radius: 12px;
    border: 1px solid var(--chip-border);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.84rem;
  }

  th,
  td {
    padding: 8px 12px;
    text-align: left;
    border-bottom: 1px solid var(--chip-border);
    color: var(--text-menu);
    vertical-align: top;
  }

  th {
    font-weight: 650;
    background: var(--chip-bg);
    color: var(--text-primary);
  }

  tr:last-child td {
    border-bottom: none;
  }

  .kbd-list {
    list-style: none;
    padding: 0;
    margin: 0 0 14px;
  }

  .kbd-list li {
    display: flex;
    align-items: baseline;
    gap: 12px;
    margin-bottom: 8px;
  }

  kbd {
    display: inline-block;
    min-width: 7.5rem;
    padding: 3px 8px;
    border-radius: 6px;
    border: 1px solid var(--chip-border);
    background: var(--chip-bg);
    font-family: ui-monospace, Consolas, monospace;
    font-size: 0.78rem;
    font-weight: 600;
    color: var(--text-primary);
    text-align: center;
  }

  @media (max-width: 720px) {
    .manual-body {
      flex-direction: column;
    }

    .manual-nav {
      width: 100%;
      max-height: 38%;
      border-right: none;
      border-bottom: 1px solid var(--panel-border);
    }
  }
</style>
