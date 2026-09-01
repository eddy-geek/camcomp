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

  interface GroupEntry {
    readonly group: DeepReadonly<Group>;
    readonly capturedAt?: Date;
    readonly dayKey: string;
    readonly anchorFocalLength: string;
  }

  const dateFormatter = new Intl.DateTimeFormat(undefined, { dateStyle: "full" });
  const dayKeyFormatter = new Intl.DateTimeFormat("en-CA", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  });
  const timeFormatter = new Intl.DateTimeFormat(undefined, { timeStyle: "short" });

  let groupIndex = 0;
  let dynamicRangeMode: DynamicRangeMode = "automatic";
  let chromeVisible = true;
  let chromeTimer: ReturnType<typeof setTimeout> | undefined;
  $: groupEntries = sortedGroupEntries(album);
  $: groups = groupEntries.map((entry) => entry.group);
  $: selectedGroup = groups[groupIndex] as DeepReadonly<Group> | undefined;
  $: stats = album.document.stats;

  function sortedGroupEntries(currentAlbum: IndexedAlbum): GroupEntry[] {
    return currentAlbum.document.groups
      .map((group, originalIndex) => ({
        group,
        originalIndex,
        capturedAt: groupCaptureDate(group, currentAlbum),
      }))
      .sort((left, right) => {
        if (!left.capturedAt)
          return right.capturedAt ? 1 : left.originalIndex - right.originalIndex;
        if (!right.capturedAt) return -1;
        return (
          left.capturedAt.getTime() - right.capturedAt.getTime() ||
          left.originalIndex - right.originalIndex
        );
      })
      .map(({ group, capturedAt }) => ({
        group,
        capturedAt,
        dayKey: capturedAt ? dayKeyFormatter.format(capturedAt) : "missing",
        anchorFocalLength: anchorFocalLengthLabel(group, currentAlbum),
      }));
  }

  function anchorFocalLengthLabel(group: DeepReadonly<Group>, currentAlbum: IndexedAlbum): string {
    const metadata = currentAlbum.photosById.get(group.anchorPhotoId)?.metadata;
    if (metadata?.effectiveFocalLength35mm != null)
      return `${formatNumber(metadata.effectiveFocalLength35mm)} mm eq.`;
    if (metadata?.focalLength35mm != null)
      return `${formatNumber(metadata.focalLength35mm)} mm eq.`;
    if (metadata?.focalLengthMm != null) return `${formatNumber(metadata.focalLengthMm)} mm`;
    return "Focal unavailable";
  }

  function formatNumber(value: number): string {
    return new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(value);
  }

  function groupCaptureDate(
    group: DeepReadonly<Group>,
    currentAlbum: IndexedAlbum,
  ): Date | undefined {
    const timestamps = group.photoIds.flatMap((photoId) => {
      const metadata = currentAlbum.photosById.get(photoId)?.metadata;
      for (const value of [metadata?.correctedAt, metadata?.capturedAt]) {
        if (!value) continue;
        const date = new Date(value);
        if (!Number.isNaN(date.getTime())) return [date];
      }
      return [];
    });
    return timestamps.reduce<Date | undefined>(
      (earliest, timestamp) =>
        !earliest || timestamp.getTime() < earliest.getTime() ? timestamp : earliest,
      undefined,
    );
  }

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
      {#each groupEntries as entry, index (entry.group.id)}
        {#if index === 0 || entry.dayKey !== groupEntries[index - 1]?.dayKey}
          <h3 class="group-day">
            {entry.capturedAt ? dateFormatter.format(entry.capturedAt) : "Date unavailable"}
          </h3>
        {/if}
        <button
          type="button"
          class:active={index === groupIndex}
          class="group-button"
          aria-current={index === groupIndex ? "page" : undefined}
          onclick={() => (groupIndex = index)}
        >
          <span class="group-row group-summary">
            <strong>Group {index + 1}</strong>
            <span>{entry.group.photoIds.length} photos</span>
          </span>
          <span class="group-row group-metadata">
            {#if entry.capturedAt}
              <time datetime={entry.capturedAt.toISOString()}
                >{timeFormatter.format(entry.capturedAt)}</time
              >
            {:else}
              <time>Time unavailable</time>
            {/if}
            {#if entry.group.flags?.length}<small>{entry.group.flags.join(" · ")}</small>{/if}
            <span
              class="anchor-focal"
              aria-label={`Anchor focal length: ${entry.anchorFocalLength}`}
            >
              {entry.anchorFocalLength}
            </span>
          </span>
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
    gap: 0.25rem;
  }
  .group-day {
    margin: 0.4rem 0 0;
    padding-top: 0.65rem;
    border-top: 1px solid var(--border);
    color: var(--muted);
    font-size: 0.7rem;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .group-day:first-child {
    margin-top: 0;
    padding-top: 0;
    border-top: 0;
  }
  .group-button {
    display: grid;
    gap: 0.15rem;
    width: 100%;
    padding: 0.4rem 0.55rem;
    border: 0;
    border-radius: 0.25rem;
    background: color-mix(in srgb, var(--surface-raised) 72%, var(--surface));
    color: var(--text);
    font-weight: 400;
    text-align: left;
  }
  .group-button:hover {
    background: var(--surface-raised);
  }
  .group-button.active {
    background: color-mix(in srgb, var(--teal) 14%, var(--surface-raised));
  }
  .group-row {
    display: flex;
    min-width: 0;
    align-items: baseline;
    gap: 0.45rem;
  }
  .group-summary {
    justify-content: space-between;
  }
  .group-metadata {
    color: var(--muted);
    font-size: 0.72rem;
  }
  .group-metadata small {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .anchor-focal {
    margin-left: auto;
    white-space: nowrap;
  }
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
