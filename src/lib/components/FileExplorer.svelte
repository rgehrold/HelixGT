<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { onMount, tick } from "svelte";
  import type { DirEntry } from "$lib/types";

  interface Props {
    selectedPaths?: string[];
    disabled?: boolean;
    showSetAsReference?: boolean;
    onchange?: (paths: string[]) => void;
    onSetOutputFolder?: (path: string) => void;
    onSetAsReference?: (path: string) => void;
  }

  let {
    selectedPaths = $bindable([]),
    disabled = false,
    showSetAsReference = false,
    onchange,
    onSetOutputFolder,
    onSetAsReference,
  }: Props = $props();

  let rootPath = $state("");
  let childrenByDir = $state<Record<string, DirEntry[]>>({});
  let expandedDirs = $state<Set<string>>(new Set());
  let folderFilesCache = $state<Record<string, string[]>>({});
  let focusedPath = $state<string | null>(null);
  let renamePath = $state<string | null>(null);
  let renameValue = $state("");
  let errorMessage = $state("");
  let loadingDirs = $state<Set<string>>(new Set());
  let treeBodyEl = $state<HTMLDivElement | null>(null);
  let contextMenu = $state<{
    x: number;
    y: number;
    path: string;
    isDir: boolean;
  } | null>(null);

  const selectedSet = $derived(new Set(selectedPaths));
  const selectedCount = $derived(selectedPaths.length);

  onMount(async () => {
    rootPath = await invoke<string>("get_default_browse_root");
    expandedDirs = new Set([rootPath]);
    await loadDirectory(rootPath);
  });

  function parentDir(path: string): string | null {
    const normalized = path.replace(/[\\/]+$/, "");
    const slash = Math.max(normalized.lastIndexOf("\\"), normalized.lastIndexOf("/"));
    if (slash <= 0) return null;
    return normalized.slice(0, slash);
  }

  function pathPrefix(dirPath: string) {
    const normalized = dirPath.replace(/[\\/]+$/, "");
    return `${normalized}\\`;
  }

  function pathVariants(path: string) {
    const normalized = path.replace(/[\\/]+$/, "");
    return [normalized, normalized.replace(/\//g, "\\"), normalized.replace(/\\/g, "/")];
  }

  function isUnderRoot(path: string, root: string) {
    const variants = pathVariants(root);
    return variants.some((base) => {
      const prefix = `${base}\\`;
      const alt = `${base}/`;
      return path === base || path.startsWith(prefix) || path.startsWith(alt);
    });
  }

  function ancestors(path: string): string[] {
    const dirs: string[] = [];
    let dir = parentDir(path);
    while (dir) {
      dirs.push(dir);
      dir = parentDir(dir);
    }
    return dirs;
  }

  function containsAll(root: string, paths: string[]) {
    return paths.every((path) => isUnderRoot(path, root));
  }

  function findMinimalRoot(paths: string[], currentRoot: string) {
    const unique = [...new Set(paths.filter(Boolean))];
    if (unique.length === 0) return currentRoot;
    if (containsAll(currentRoot, unique)) return currentRoot;

    let candidate = parentDir(unique[0]) ?? currentRoot;
    while (candidate) {
      if (containsAll(candidate, unique)) return candidate;
      const parent = parentDir(candidate);
      if (!parent || parent === candidate) break;
      candidate = parent;
    }
    return currentRoot;
  }

  function notifyChange(paths: string[]) {
    selectedPaths = [...paths].sort();
    onchange?.(selectedPaths);
  }

  function checkboxState(entry: DirEntry): "checked" | "unchecked" | "indeterminate" {
    if (!entry.isDir) {
      return selectedSet.has(entry.path) ? "checked" : "unchecked";
    }

    const cached = folderFilesCache[entry.path];
    if (cached) {
      const selected = cached.filter((file) => selectedSet.has(file)).length;
      if (selected === 0) return "unchecked";
      if (selected === cached.length) return "checked";
      return "indeterminate";
    }

    const prefix = pathPrefix(entry.path);
    const altPrefix = prefix.replace(/\\/g, "/");
    const selectedUnder = selectedPaths.filter(
      (file) => file.startsWith(prefix) || file.startsWith(altPrefix),
    );
    if (selectedUnder.length === 0) return "unchecked";
    return "indeterminate";
  }

  function setIndeterminate(node: HTMLInputElement, state: "checked" | "unchecked" | "indeterminate") {
    node.indeterminate = state === "indeterminate";
    return {
      update(nextState: "checked" | "unchecked" | "indeterminate") {
        node.indeterminate = nextState === "indeterminate";
      },
    };
  }

  async function cacheFolderFiles(path: string) {
    const files = await invoke<string[]>("collect_compatible_files_under", { path });
    folderFilesCache = { ...folderFilesCache, [path]: files };
    return files;
  }

  async function loadDirectory(path: string, force = false) {
    if (!force && childrenByDir[path]) return;
    loadingDirs = new Set([...loadingDirs, path]);
    try {
      const entries = await invoke<DirEntry[]>("list_browse_directory", { path });
      childrenByDir = { ...childrenByDir, [path]: entries };
      errorMessage = "";
    } catch (error) {
      errorMessage = String(error);
    } finally {
      const next = new Set(loadingDirs);
      next.delete(path);
      loadingDirs = next;
    }
  }

  async function reloadExpandedTree(force = true) {
    const dirs = [...expandedDirs].sort((left, right) => left.length - right.length);
    for (const dir of dirs) {
      await loadDirectory(dir, force);
      if (folderFilesCache[dir]) {
        try {
          await cacheFolderFiles(dir);
        } catch {
          // Ignore stale folder cache refresh errors.
        }
      }
    }
  }

  function filterExpandedForRoot(expanded: Set<string>, root: string) {
    return new Set([root, ...[...expanded].filter((dir) => isUnderRoot(dir, root))]);
  }

  function remapPathKey(key: string, oldPath: string, newPath: string) {
    const oldPrefix = pathPrefix(oldPath);
    const newPrefix = pathPrefix(newPath);
    const altOld = oldPrefix.replace(/\\/g, "/");
    if (key === oldPath) return newPath;
    if (key.startsWith(oldPrefix)) return `${newPrefix}${key.slice(oldPrefix.length)}`;
    if (key.startsWith(altOld)) return `${newPrefix.replace(/\\/g, "/")}${key.slice(altOld.length)}`;
    return key;
  }

  function remapStatePaths(oldPath: string, newPath: string) {
    const nextChildren: Record<string, DirEntry[]> = {};
    for (const [dir, children] of Object.entries(childrenByDir)) {
      const mappedDir = remapPathKey(dir, oldPath, newPath);
      nextChildren[mappedDir] = children.map((entry) => ({
        ...entry,
        path: remapPathKey(entry.path, oldPath, newPath),
      }));
    }
    childrenByDir = nextChildren;

    folderFilesCache = Object.fromEntries(
      Object.entries(folderFilesCache).map(([dir, files]) => [
        remapPathKey(dir, oldPath, newPath),
        files.map((file) => remapPathKey(file, oldPath, newPath)),
      ]),
    );

    expandedDirs = new Set([...expandedDirs].map((dir) => remapPathKey(dir, oldPath, newPath)));
    if (focusedPath) focusedPath = remapPathKey(focusedPath, oldPath, newPath);
    if (renamePath) renamePath = remapPathKey(renamePath, oldPath, newPath);
    if (rootPath) rootPath = remapPathKey(rootPath, oldPath, newPath);
  }

  function removeStateUnder(path: string) {
    const prefix = pathPrefix(path);
    const altPrefix = prefix.replace(/\\/g, "/");

    childrenByDir = Object.fromEntries(
      Object.entries(childrenByDir).filter(([dir]) => {
        return dir !== path && !dir.startsWith(prefix) && !dir.startsWith(altPrefix);
      }),
    );

    folderFilesCache = Object.fromEntries(
      Object.entries(folderFilesCache).filter(([dir]) => {
        return dir !== path && !dir.startsWith(prefix) && !dir.startsWith(altPrefix);
      }),
    );

    expandedDirs = new Set(
      [...expandedDirs].filter((dir) => dir !== path && !dir.startsWith(prefix) && !dir.startsWith(altPrefix)),
    );
  }

  async function ensureExpanded(path: string, force = false) {
    expandedDirs = new Set([...expandedDirs, path]);
    await loadDirectory(path, force);
  }

  async function toggleExpand(path: string) {
    if (expandedDirs.has(path)) {
      const next = new Set(expandedDirs);
      next.delete(path);
      expandedDirs = next;
      return;
    }
    await ensureExpanded(path, true);
  }

  function isFastaPath(path: string) {
    return /\.(fasta|fa|fna|ffn)(\.gz)?$/i.test(path);
  }

  async function toggleCheck(entry: DirEntry) {
    if (disabled) return;

    const next = new Set(selectedPaths);
    if (entry.isDir) {
      const state = checkboxState(entry);
      const shouldSelect = state !== "checked";
      try {
        const files = await cacheFolderFiles(entry.path);
        if (shouldSelect) {
          for (const file of files) next.add(file);
          await expandPathsToFiles(entry.path, files);
        } else {
          for (const file of files) next.delete(file);
        }
      } catch (error) {
        errorMessage = String(error);
        return;
      }
    } else if (next.has(entry.path)) {
      next.delete(entry.path);
    } else {
      next.add(entry.path);
    }

    notifyChange([...next]);
  }

  async function expandPathsToFiles(folderPath: string, files: string[]) {
    const folderPrefix = pathPrefix(folderPath);
    const altPrefix = folderPrefix.replace(/\\/g, "/");
    const dirsToExpand = new Set<string>([folderPath]);

    for (const file of files) {
      for (const dir of ancestors(file)) {
        if (
          dir === folderPath ||
          dir.startsWith(folderPrefix) ||
          dir.startsWith(altPrefix)
        ) {
          dirsToExpand.add(dir);
        }
      }
    }

    const sorted = [...dirsToExpand].sort((left, right) => left.length - right.length);
    for (const dir of sorted) {
      await ensureExpanded(dir);
    }
  }

  export async function refreshTree() {
    const staleDirs = new Set([...expandedDirs, rootPath]);
    childrenByDir = Object.fromEntries(
      Object.entries(childrenByDir).filter(([dir]) => !staleDirs.has(dir)),
    );
    folderFilesCache = Object.fromEntries(
      Object.entries(folderFilesCache).filter(([dir]) => !staleDirs.has(dir)),
    );
    await reloadExpandedTree();
  }

  export async function revealPaths(paths: string[]) {
    if (paths.length === 0) return;

    const keepVisible = selectedPaths.filter((path) => isUnderRoot(path, rootPath));
    const relevant = [...new Set([...paths, ...keepVisible])];
    const nextRoot = findMinimalRoot(relevant, rootPath);

    const dirsToExpand = new Set<string>([nextRoot]);
    for (const path of relevant) {
      for (const dir of ancestors(path)) {
        dirsToExpand.add(dir);
      }
    }

    rootPath = nextRoot;
    expandedDirs = filterExpandedForRoot(dirsToExpand, nextRoot);
    await reloadExpandedTree();
    await tick();

    const target = paths[0];
    focusedPath = target;
    treeBodyEl?.querySelector(`[data-path="${cssEscape(target)}"]`)?.scrollIntoView({
      block: "nearest",
      inline: "nearest",
    });
  }

  function cssEscape(value: string) {
    if (typeof CSS !== "undefined" && "escape" in CSS) {
      return CSS.escape(value);
    }
    return value.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  }

  async function changeRoot() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: "Choose root folder",
      defaultPath: rootPath,
    });
    if (!picked) return;

    const savedExpanded = filterExpandedForRoot(expandedDirs, String(picked));
    rootPath = String(picked);
    expandedDirs = new Set([rootPath, ...savedExpanded]);
    focusedPath = null;
    renamePath = null;
    await reloadExpandedTree();
  }

  async function goUp() {
    const parent = parentDir(rootPath);
    if (!parent) return;

    const savedExpanded = expandedDirs;
    rootPath = parent;
    expandedDirs = filterExpandedForRoot(savedExpanded, parent);
    focusedPath = null;
    renamePath = null;
    await reloadExpandedTree();
  }

  async function setRootFromFolder(path: string) {
    const savedExpanded = filterExpandedForRoot(expandedDirs, path);
    rootPath = path;
    expandedDirs = new Set([path, ...savedExpanded]);
    focusedPath = path;
    renamePath = null;
    await reloadExpandedTree();
  }

  function findEntry(path: string): DirEntry | undefined {
    for (const children of Object.values(childrenByDir)) {
      const match = children.find((entry) => entry.path === path);
      if (match) return match;
    }
    return undefined;
  }

  function startRename(path: string, currentName: string) {
    if (disabled) return;
    focusedPath = path;
    renamePath = path;
    renameValue = currentName;
  }

  async function commitRename() {
    if (!renamePath) return;
    const target = renamePath;
    try {
      const newPath = await invoke<string>("rename_browse_path", {
        path: renamePath,
        newName: renameValue.trim(),
      });

      const oldPrefix = pathPrefix(renamePath);
      const newPrefix = pathPrefix(newPath);
      const altOld = oldPrefix.replace(/\\/g, "/");
      const next = selectedPaths.map((item) => {
        if (item === renamePath) return newPath;
        if (item.startsWith(oldPrefix)) return `${newPrefix}${item.slice(oldPrefix.length)}`;
        if (item.startsWith(altOld)) return `${newPrefix.replace(/\\/g, "/")}${item.slice(altOld.length)}`;
        return item;
      });
      notifyChange(next);

      remapStatePaths(renamePath, newPath);
      const parent = parentDir(newPath);
      if (parent) {
        expandedDirs = new Set([...expandedDirs, parent]);
        await loadDirectory(parent, true);
      }
      if (focusedPath === target) focusedPath = newPath;
      renamePath = null;
    } catch (error) {
      errorMessage = String(error);
    }
  }

  function cancelRename() {
    renamePath = null;
  }

  function closeContextMenu() {
    contextMenu = null;
  }

  function openContextMenu(event: MouseEvent, path: string, isDir: boolean) {
    event.preventDefault();
    event.stopPropagation();
    focusedPath = path;
    contextMenu = { x: event.clientX, y: event.clientY, path, isDir };
  }

  function handleTreeContextMenu(event: MouseEvent) {
    const row = (event.target as HTMLElement).closest(".tree-row");
    if (row) {
      const path = row.getAttribute("data-path");
      const isDir = row.getAttribute("data-is-dir") === "true";
      if (path) {
        openContextMenu(event, path, isDir);
        return;
      }
    }
    openContextMenu(event, rootPath, true);
  }

  async function openInExplorer(path: string, isDir: boolean) {
    closeContextMenu();
    try {
      if (isDir) {
        await openPath(path);
      } else {
        await revealItemInDir(path);
      }
    } catch (error) {
      errorMessage = String(error);
    }
  }

  async function createNewFolder(parentPath: string) {
    closeContextMenu();
    if (disabled) return;
    try {
      const newPath = await invoke<string>("create_browse_folder", {
        parentPath,
        name: null,
      });
      const parent = parentDir(newPath) ?? parentPath;
      expandedDirs = new Set([...expandedDirs, parent]);
      await loadDirectory(parent, true);
      focusedPath = newPath;
    } catch (error) {
      errorMessage = String(error);
    }
  }

  function setAsOutputFolder(path: string) {
    closeContextMenu();
    onSetOutputFolder?.(path);
  }

  function setAsReference(path: string) {
    closeContextMenu();
    onSetAsReference?.(path);
  }

  async function refreshCurrentView() {
    closeContextMenu();
    await refreshTree();
  }

  async function deleteItem(path: string, isDirHint?: boolean) {
    if (disabled) return;
    closeContextMenu();

    const entry = findEntry(path);
    const isDir = entry?.isDir ?? isDirHint ?? false;
    const name = path.split(/[\\/]/).pop() ?? path;

    let message = `Move "${name}" to the Recycle Bin?`;
    if (isDir) {
      try {
        const childCount = await invoke<number>("browse_folder_child_count", { path });
        if (childCount > 0) {
          message =
            `The folder "${name}" contains ${childCount} item(s), including files that may not be shown in the browser.\n\n` +
            "Delete it and move everything to the Recycle Bin?";
        }
      } catch (error) {
        errorMessage = String(error);
        return;
      }
    }

    if (!confirm(message)) return;

    try {
      await invoke("delete_browse_path", { path });
      const prefix = pathPrefix(path);
      const altPrefix = prefix.replace(/\\/g, "/");
      const next = selectedPaths.filter(
        (item) => item !== path && !item.startsWith(prefix) && !item.startsWith(altPrefix),
      );
      notifyChange(next);

      removeStateUnder(path);
      const parent = parentDir(path);
      if (parent) {
        expandedDirs = new Set([...expandedDirs, parent]);
        await loadDirectory(parent, true);
      }
      if (focusedPath === path) focusedPath = null;
      renamePath = null;
    } catch (error) {
      errorMessage = String(error);
    }
  }

  function clearSelection() {
    notifyChange([]);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!focusedPath || renamePath) return;
    if (event.key === "F2") {
      event.preventDefault();
      const entry = findEntry(focusedPath);
      if (entry) startRename(entry.path, entry.name);
    } else if (event.key === "Delete") {
      event.preventDefault();
      void deleteItem(focusedPath);
    }
  }

  type VisibleNode = { entry: DirEntry; depth: number };

  function pathBreadcrumbs(path: string): { label: string; path: string }[] {
    const normalized = path.replace(/[\\/]+$/, "");
    if (!normalized) return [];

    const crumbs: { label: string; path: string }[] = [];
    const driveMatch = normalized.match(/^([A-Za-z]:)(?:[\\/]|$)/);
    if (driveMatch) {
      let current = `${driveMatch[1]}\\`;
      crumbs.push({ label: driveMatch[1], path: current });
      const rest = normalized
        .slice(driveMatch[0].length)
        .split(/[\\/]/)
        .filter(Boolean);
      for (const part of rest) {
        current = `${current.replace(/[\\/]+$/, "")}\\${part}`;
        crumbs.push({ label: part, path: current });
      }
      return crumbs;
    }

    const parts = normalized.split(/[\\/]/).filter(Boolean);
    let current = parts[0].startsWith("/") ? parts[0] : `/${parts[0]}`;
    crumbs.push({ label: parts[0], path: current });
    for (const part of parts.slice(1)) {
      current = `${current.replace(/\/+$/, "")}/${part}`;
      crumbs.push({ label: part, path: current });
    }
    return crumbs;
  }

  const breadcrumbs = $derived(pathBreadcrumbs(rootPath));

  function visibleNodes(): VisibleNode[] {
    const nodes: VisibleNode[] = [];
    const walk = (dirPath: string, depth: number) => {
      const children = childrenByDir[dirPath] ?? [];
      for (const entry of children) {
        nodes.push({ entry, depth });
        if (entry.isDir && expandedDirs.has(entry.path)) {
          walk(entry.path, depth + 1);
        }
      }
    };
    walk(rootPath, 0);
    return nodes;
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="explorer"
  role="tree"
  tabindex="0"
  onkeydown={handleKeydown}
  onclick={closeContextMenu}
>
  <div class="toolbar">
    <button class="ghost" onclick={goUp} disabled={disabled || !rootPath}>↑ Up</button>
    <button
      class="ghost icon-btn"
      onclick={() => void refreshCurrentView()}
      disabled={disabled}
      title="Refresh"
      aria-label="Refresh"
    >
      ↻
    </button>
    <button class="ghost" onclick={changeRoot} disabled={disabled}>Change root</button>
    <button class="ghost" onclick={clearSelection} disabled={disabled || selectedCount === 0}>
      Clear selection
    </button>
    <span class="selection-count">{selectedCount} file{selectedCount === 1 ? "" : "s"} selected</span>
  </div>

  {#if errorMessage}
    <p class="explorer-error">{errorMessage}</p>
  {/if}

  <div class="tree-wrap">
    <div class="tree-header">
      <span class="col-expand"></span>
      <span class="col-check"></span>
      <nav class="col-name breadcrumb" aria-label="Current root folder">
        {#each breadcrumbs as crumb, index}
          {#if index > 0}
            <span class="crumb-sep">\</span>
          {/if}
          <button
            class="crumb-btn"
            class:current={crumb.path === rootPath}
            title={crumb.path}
            onclick={() => void setRootFromFolder(crumb.path)}
          >
            {crumb.label}
          </button>
        {/each}
      </nav>
    </div>

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="tree-body" bind:this={treeBodyEl} oncontextmenu={handleTreeContextMenu}>
      {#if loadingDirs.has(rootPath) && !childrenByDir[rootPath]}
        <p class="tree-empty">Loading…</p>
      {:else if (childrenByDir[rootPath] ?? []).length === 0}
        <p class="tree-empty">No compatible files in this folder.</p>
      {:else}
        <div class="tree-scroll-content">
          {#each visibleNodes() as node (node.entry.path)}
            {@const entry = node.entry}
            {@const expanded = entry.isDir && expandedDirs.has(entry.path)}
            {@const state = checkboxState(entry)}
            <div
              class="tree-row"
              class:focused={focusedPath === entry.path}
              class:folder={entry.isDir}
              data-path={entry.path}
              data-is-dir={entry.isDir ? "true" : "false"}
              style={`--depth:${node.depth}`}
              oncontextmenu={(event) => openContextMenu(event, entry.path, entry.isDir)}
            >
              <div class="col-expand">
                {#if entry.isDir}
                  <button
                    class="expand-btn"
                    aria-label={expanded ? "Collapse folder" : "Expand folder"}
                    onclick={() => void toggleExpand(entry.path)}
                  >
                    {expanded ? "▾" : "▸"}
                  </button>
                {/if}
              </div>

              <label class="col-check">
                <input
                  type="checkbox"
                  checked={state === "checked"}
                  use:setIndeterminate={state}
                  disabled={disabled}
                  onchange={() => void toggleCheck(entry)}
                />
              </label>

              <div class="col-name">
                {#if renamePath === entry.path}
                  <!-- svelte-ignore a11y_autofocus -->
                  <input
                    class="rename-input"
                    bind:value={renameValue}
                    autofocus
                    onkeydown={(event) => {
                      if (event.key === "Enter") void commitRename();
                      if (event.key === "Escape") cancelRename();
                    }}
                    onblur={() => void commitRename()}
                  />
                {:else}
                  <button
                    class="name-btn"
                    onclick={() => (focusedPath = entry.path)}
                    ondblclick={() => {
                      if (entry.isDir) {
                        void setRootFromFolder(entry.path);
                      } else {
                        startRename(entry.path, entry.name);
                      }
                    }}
                  >
                    <span class="icon">{entry.isDir ? "📁" : "📄"}</span>
                    <span class="name-text">{entry.name}</span>
                  </button>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  {#if contextMenu}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="context-menu"
      style={`left:${contextMenu.x}px;top:${contextMenu.y}px`}
      onclick={(event) => event.stopPropagation()}
    >
      <button
        class="context-item"
        disabled={disabled}
        onclick={() => {
          const entry = findEntry(contextMenu!.path);
          if (entry) startRename(entry.path, entry.name);
          else closeContextMenu();
        }}
      >
        Rename
      </button>
      <button
        class="context-item danger"
        disabled={disabled}
        onclick={() => void deleteItem(contextMenu!.path, contextMenu!.isDir)}
      >
        Delete
      </button>
      <button
        class="context-item"
        onclick={() => void openInExplorer(contextMenu!.path, contextMenu!.isDir)}
      >
        Open in Explorer
      </button>
      <button
        class="context-item"
        disabled={disabled}
        onclick={() =>
          void createNewFolder(
            contextMenu!.isDir ? contextMenu!.path : (parentDir(contextMenu!.path) ?? rootPath),
          )}
      >
        New folder
      </button>
      <button class="context-item" disabled={disabled} onclick={() => void refreshCurrentView()}>
        Refresh
      </button>
      {#if showSetAsReference && !contextMenu.isDir && isFastaPath(contextMenu.path)}
        <button class="context-item" disabled={disabled} onclick={() => setAsReference(contextMenu!.path)}>
          Set as reference
        </button>
      {/if}
      {#if contextMenu.isDir}
        <button
          class="context-item"
          disabled={disabled}
          onclick={() => setAsOutputFolder(contextMenu!.path)}
        >
          Set as output folder
        </button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .explorer {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
    flex: 1;
    overflow: hidden;
  }

  .toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    flex-shrink: 0;
  }

  .selection-count {
    margin-left: auto;
    color: #86efac;
    font-size: 0.86rem;
    font-weight: 600;
  }

  .icon-btn {
    min-width: 38px;
    padding-inline: 10px;
    font-size: 1.05rem;
    line-height: 1;
  }

  .explorer-error {
    margin: 0;
    color: #fca5a5;
    font-size: 0.86rem;
    white-space: pre-wrap;
  }

  .tree-wrap {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
    border: 1px solid rgba(148, 163, 184, 0.14);
    border-radius: 14px;
    overflow: hidden;
    background: rgba(2, 6, 23, 0.45);
  }

  .tree-header,
  .tree-row {
    display: grid;
    grid-template-columns: 34px 38px minmax(240px, 1fr);
    gap: 6px;
    align-items: center;
  }

  .tree-header {
    padding: 8px 12px;
    border-bottom: 1px solid rgba(148, 163, 184, 0.12);
    color: #94a3b8;
    font-size: 0.82rem;
    flex-shrink: 0;
  }

  .breadcrumb {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 2px;
    min-width: 0;
  }

  .crumb-sep {
    color: #64748b;
    user-select: none;
  }

  .crumb-btn {
    border: none;
    background: none;
    color: #94a3b8;
    cursor: pointer;
    padding: 2px 4px;
    border-radius: 6px;
    font: inherit;
    max-width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .crumb-btn:hover {
    color: #ecfeff;
    background: rgba(51, 65, 85, 0.45);
  }

  .crumb-btn.current {
    color: #e2e8f0;
    font-weight: 600;
  }

  .tree-body {
    overflow-x: auto;
    overflow-y: auto;
    flex: 1 1 auto;
    min-height: 0;
    height: 0;
    scrollbar-gutter: stable both-edges;
  }

  .tree-scroll-content {
    min-width: max-content;
    padding: 4px 0;
  }

  .tree-row {
    padding: 1px 12px 1px calc(12px + var(--depth) * 16px);
    min-width: max-content;
  }

  .tree-row.focused {
    background: rgba(34, 211, 238, 0.08);
  }

  .tree-row:hover {
    background: rgba(51, 65, 85, 0.35);
  }

  .tree-empty {
    margin: 0;
    padding: 18px 14px;
    color: #94a3b8;
    font-size: 0.9rem;
  }

  .expand-btn,
  .name-btn,
  .ghost,
  .context-item {
    cursor: pointer;
    border: none;
    background: transparent;
    color: inherit;
  }

  .expand-btn {
    width: 30px;
    height: 30px;
    border-radius: 6px;
    color: #cbd5e1;
    font-size: 1.05rem;
    line-height: 1;
  }

  .col-check input[type="checkbox"] {
    width: 18px;
    height: 18px;
    accent-color: #22d3ee;
    cursor: pointer;
  }

  .name-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    width: 100%;
    text-align: left;
    padding: 2px 0;
  }

  .name-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 48vw;
  }

  .rename-input {
    width: 100%;
    min-width: 180px;
    padding: 6px 8px;
    border-radius: 8px;
    border: 1px solid rgba(103, 232, 249, 0.35);
    background: rgba(15, 23, 42, 0.95);
    color: #e8eef8;
  }

  .context-menu {
    position: fixed;
    z-index: 50;
    min-width: 190px;
    padding: 6px;
    border-radius: 10px;
    background: rgba(15, 23, 42, 0.98);
    border: 1px solid rgba(148, 163, 184, 0.18);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.4);
  }

  .context-item {
    display: block;
    width: 100%;
    text-align: left;
    padding: 8px 10px;
    border-radius: 8px;
    color: #e2e8f0;
    font-size: 0.86rem;
  }

  .context-item:hover:not(:disabled) {
    background: rgba(34, 211, 238, 0.12);
    color: #ecfeff;
  }

  .context-item.danger {
    color: #fecaca;
  }

  .context-item:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .ghost {
    padding: 8px 12px;
    border-radius: 10px;
    background: rgba(30, 41, 59, 0.9);
    color: #e2e8f0;
    border: 1px solid rgba(148, 163, 184, 0.16);
    font-weight: 600;
  }

  .ghost:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
</style>