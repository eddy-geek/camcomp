<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import type { DeepReadonly, Group, IndexedAlbum, Photo } from "../contracts";
  import type { PhotoResolutionOptions, ResolvedPhotoSource, SourceSelection } from "../platform";
  import type { DynamicRangeMode } from "../viewer";
  import GroupViewer from "./GroupViewer.svelte";

  export let album: IndexedAlbum;
  export let selection: SourceSelection;
  export let canRefresh: boolean;
  export let onRefresh: () => void;
  export let onReset: () => void;
  export let onReauthorize: () => void;
  export let resolveSource: (
    photo: DeepReadonly<Photo>,
    options?: PhotoResolutionOptions,
  ) => Promise<ResolvedPhotoSource>;
  export let releaseSource: (source: ResolvedPhotoSource) => void;

  let groupIndex = 0;
  let dynamicRangeMode: DynamicRangeMode = "automatic";
  let chromeVisible = true;
  let chromeTimer: ReturnType<typeof setTimeout> | undefined;
  $: groups = album.document.groups;
  $: selectedGroup = groups[groupIndex] as DeepReadonly<Group> | undefined;
  $: stats = album.document.stats;

  onMount(() => scheduleChromeHide());
  onDestroy(() => clearTimeout(chromeTimer));

  function revealChrome(): void {
    chromeVisible = true;
    scheduleChromeHide();
  }

  function scheduleChromeHide(): void {
    clearTimeout(chromeTimer);
    chromeTimer = setTimeout(() => (chromeVisible = false), 1400);
  }
</script>

<div
  class:chrome-visible={chromeVisible}
  class:has-group={selectedGroup !== undefined}
  class="album-shell"
>
  <aside class="album-sidebar">
    <div class="album-title">
      <p class="eyebrow">{selection.label}</p>
      <h2>Analysis result</h2>
      <p>
        {album.document.photos.length} photos · {groups.length} groups ·
        {album.document.ungrouped.length} ungrouped
      </p>
      {#if stats}
        <p class="cache-stats">{stats.analyzedPairs} analyzed · {stats.reusedPairs} reused</p>
      {/if}
    </div>
    <nav aria-label="Comparison groups">
      {#each groups as group, index (group.id)}
        <button
          type="button"
          class:active={index === groupIndex}
          class="group-button"
          aria-current={index === groupIndex ? "page" : undefined}
          onclick={() => (groupIndex = index)}
        >
          <strong>Group {index + 1}</strong>
          <span>{group.photoIds.length} photos</span>
          {#if group.flags?.length}<small>{group.flags.join(" · ")}</small>{/if}
        </button>
      {/each}
    </nav>
    <details open={groups.length === 0}>
      <summary>Ungrouped ({album.document.ungrouped.length})</summary>
      <ul>
        {#each album.document.ungrouped as entry (entry.photoId)}
          <li>
            <strong>{album.photosById.get(entry.photoId)?.path ?? entry.photoId}</strong>
            <span>{entry.reason}</span>
            {#if entry.detail}<small>{entry.detail}</small>{/if}
          </li>
        {/each}
      </ul>
    </details>
    {#if selection.kind === "analysis-only"}
      <button type="button" class="secondary full" onclick={onReauthorize}
        >Choose photo source</button
      >
    {/if}
    {#if canRefresh}
      <button type="button" class="secondary full" onclick={onRefresh}>Refresh analysis</button>
    {/if}
    <button type="button" class="secondary full" onclick={onReset}>Open another set</button>
  </aside>

  <main onmousemove={revealChrome} onpointerdown={revealChrome}>
    {#if selectedGroup}
      <div class="group-nav chrome-overlay">
        <button
          type="button"
          class="secondary"
          disabled={groupIndex === 0}
          onclick={() => (groupIndex -= 1)}>Previous</button
        >
        <span>Group {groupIndex + 1} of {groups.length}</span>
        <button
          type="button"
          class="secondary"
          disabled={groupIndex >= groups.length - 1}
          onclick={() => (groupIndex += 1)}>Next</button
        >
      </div>
      {#key selectedGroup.id}
        <GroupViewer
          {album}
          group={selectedGroup}
          {resolveSource}
          {releaseSource}
          {dynamicRangeMode}
          onDynamicRangeModeChange={(mode) => (dynamicRangeMode = mode)}
        />
      {/key}
    {:else}
      <section class="empty">
        <h2>No comparison groups</h2>
        <p>Every photo remains listed under Ungrouped with its analysis reason.</p>
      </section>
    {/if}
  </main>
</div>

<style>
  .album-shell {
    position: relative;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: #050708;
  }
  .album-shell:not(.has-group) {
    display: grid;
    grid-template-columns: minmax(15rem, 19rem) minmax(0, 1fr);
    gap: 1rem;
    padding: 1rem;
  }
  aside {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    overflow: auto;
    padding: 1rem;
    border-right: 1px solid var(--border);
    background: color-mix(in srgb, var(--surface) 97%, transparent);
  }
  .has-group aside {
    position: absolute;
    z-index: 35;
    inset: 0 auto 0 0;
    width: min(19rem, calc(100vw - 2rem));
    opacity: 0.35;
    transform: translateX(calc(-100% + 0.7rem));
    transition:
      opacity 160ms ease,
      transform 160ms ease;
  }
  .has-group aside:hover,
  .has-group aside:focus-within {
    opacity: 1;
    transform: none;
  }
  .album-title h2,
  .album-title p {
    margin: 0.2rem 0;
  }
  .eyebrow,
  .cache-stats {
    color: var(--muted);
    font-size: 0.75rem;
  }
  nav {
    display: grid;
    gap: 0.4rem;
  }
  .group-button {
    display: grid;
    gap: 0.1rem;
    width: 100%;
    border-color: var(--border);
    background: var(--surface-raised);
    color: var(--text);
    text-align: left;
  }
  .group-button.active {
    border-color: var(--teal);
    background: color-mix(in srgb, var(--teal) 18%, var(--surface-raised));
  }
  .group-button span,
  .group-button small,
  details span,
  details small {
    display: block;
    color: var(--muted);
  }
  details ul {
    display: grid;
    gap: 0.6rem;
    padding-left: 1.2rem;
    font-size: 0.78rem;
  }
  .full {
    width: 100%;
  }
  main {
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
  }
  .group-nav {
    position: absolute;
    z-index: 45;
    top: 0.6rem;
    left: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.75rem;
    padding: 0.35rem;
    border-radius: 0.55rem;
    background: color-mix(in srgb, var(--surface) 92%, transparent);
    opacity: 0;
    transform: translate(-50%, -0.5rem);
    transition:
      opacity 160ms ease,
      transform 160ms ease;
  }
  .has-group:not(.chrome-visible) .group-nav {
    pointer-events: none;
  }
  .chrome-visible .group-nav,
  .group-nav:focus-within {
    opacity: 1;
    pointer-events: auto;
    transform: translateX(-50%);
  }
  .empty {
    padding: 2rem;
    border: 1px solid var(--border);
    border-radius: 0.75rem;
    background: var(--surface);
  }
  @media (prefers-reduced-motion: reduce) {
    .has-group aside,
    .group-nav {
      transition: none;
    }
  }
</style>
