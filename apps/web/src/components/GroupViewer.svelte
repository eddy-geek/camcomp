<script lang="ts">
  import { onDestroy } from "svelte";
  import type { DeepReadonly, Group, IndexedAlbum, Photo } from "../contracts";
  import type { PhotoResolutionOptions, ResolvedPhotoSource } from "../platform";
  import {
    chooseLayout,
    dynamicRangeLimitValue,
    gridColumns,
    NavigationController,
    supportsDynamicRangeLimit,
    type DynamicRangeMode,
    type NavigationMode,
  } from "../viewer";
  import ComparisonSlider from "./ComparisonSlider.svelte";
  import PhotoFrame from "./PhotoFrame.svelte";

  export let album: IndexedAlbum;
  export let group: DeepReadonly<Group>;
  export let resolveSource: (
    photo: DeepReadonly<Photo>,
    options?: PhotoResolutionOptions,
  ) => Promise<ResolvedPhotoSource>;
  export let releaseSource: (source: ResolvedPhotoSource) => void;
  export let dynamicRangeMode: DynamicRangeMode = "automatic";
  export let onDynamicRangeModeChange: (mode: DynamicRangeMode) => void = () => undefined;

  let container: HTMLElement;
  let width = 1200;
  let height = 700;
  let resizeObserver: ResizeObserver | undefined;
  let mode: NavigationMode = "normalized";
  let presentation: "grid" | "slider" = "grid";
  let forcedSdrSupported = supportsDynamicRangeLimit();
  let controller = new NavigationController(group);

  $: photos = group.photoIds.flatMap((id) => {
    const photo = album.photosById.get(id);
    return photo ? [photo] : [];
  });
  $: layout = chooseLayout(photos.length, width, height);
  $: columns = gridColumns(photos.length);
  $: alignmentStatuses = new Map(
    photos.map((photo) => [photo.id, alignmentStatus(photo.id, mode)]),
  );
  $: if (container) {
    resizeObserver?.disconnect();
    resizeObserver = new ResizeObserver(([entry]) => {
      if (!entry) return;
      width = entry.contentRect.width;
      height = entry.contentRect.height;
    });
    resizeObserver.observe(container);
  }

  onDestroy(() => {
    resizeObserver?.disconnect();
    controller.destroy();
  });

  function setMode(next: NavigationMode): void {
    mode = next;
    controller.setMode(next);
  }

  function alignmentStatus(photoId: string, currentMode: NavigationMode): string {
    if (photoId === group.anchorPhotoId) return "Anchor";
    const alignment = group.alignments?.find((candidate) => candidate.photoId === photoId);
    if (!alignment)
      return currentMode === "aligned" ? "Normalized fallback · no alignment" : "No alignment";
    return `${alignment.provenance} · ${Math.round(alignment.confidence * 100)}%`;
  }
</script>

<section class="viewer-shell" aria-labelledby="viewer-heading">
  <header class="toolbar viewer-toolbar">
    <div>
      <p class="eyebrow">{group.photoIds.length} photos</p>
      <h2 id="viewer-heading">Comparison group</h2>
    </div>
    <div class="controls" role="toolbar" aria-label="Viewer controls">
      <label>
        Dynamic range
        <select
          value={dynamicRangeMode}
          aria-label="Dynamic range"
          onchange={(event) =>
            onDynamicRangeModeChange(event.currentTarget.value as DynamicRangeMode)}
        >
          <option value="automatic">Automatic / HDR when available</option>
          <option value="sdr">Compare in SDR</option>
        </select>
      </label>
      <label>
        Navigation
        <select
          value={mode}
          onchange={(event) => setMode(event.currentTarget.value as NavigationMode)}
        >
          <option value="normalized">Normalized</option>
          <option value="aligned">Experimental global aligned</option>
        </select>
      </label>
      {#if photos.length === 2}
        <button
          type="button"
          class="secondary"
          onclick={() => (presentation = presentation === "grid" ? "slider" : "grid")}
        >
          {presentation === "grid" ? "Comparison slider" : "Grid view"}
        </button>
      {/if}
      <button type="button" class="secondary" onclick={() => controller.fit()}>Fit all</button>
      <button type="button" class="secondary" onclick={() => controller.zoomToPixelRatio(1)}
        >100%</button
      >
      <button type="button" class="secondary" onclick={() => controller.zoomToPixelRatio(2)}
        >200%</button
      >
      <button
        type="button"
        class="secondary"
        aria-label="Zoom in"
        onclick={() => controller.zoomBy(1.25)}>+</button
      >
      <button
        type="button"
        class="secondary"
        aria-label="Zoom out"
        onclick={() => controller.zoomBy(0.8)}>−</button
      >
    </div>
  </header>

  {#if mode === "aligned"}
    <p class="mode-note viewer-note">
      Experimental until the real-landmark gate is recorded. Global homographies map centers through
      the anchor. Unsupported members fall back to normalized coordinates; no local correspondence
      refinement is applied.
    </p>
  {/if}

  <div
    class="viewer-container"
    bind:this={container}
    style={`dynamic-range-limit: ${dynamicRangeLimitValue(dynamicRangeMode)};`}
  >
    {#if presentation === "slider" && photos.length === 2}
      <ComparisonSlider
        {photos}
        {controller}
        {resolveSource}
        {releaseSource}
        alignmentStatus={(photoId) => alignmentStatuses.get(photoId) ?? "No alignment"}
        {dynamicRangeMode}
        {forcedSdrSupported}
      />
    {:else}
      <div
        class:columns={layout === "columns"}
        class:rows={layout === "rows"}
        class:quad={layout === "quad"}
        class:generic={layout === "grid"}
        class="photo-grid"
        style={`--grid-columns: ${columns}`}
      >
        {#each photos as photo (photo.id)}
          <PhotoFrame
            {photo}
            {controller}
            {resolveSource}
            {releaseSource}
            alignmentStatus={alignmentStatuses.get(photo.id) ?? "No alignment"}
            {dynamicRangeMode}
            {forcedSdrSupported}
          />
        {/each}
      </div>
    {/if}
  </div>
</section>

<style>
  .viewer-shell {
    position: relative;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  .toolbar {
    position: absolute;
    z-index: 25;
    top: 4.4rem;
    right: 0.6rem;
    left: 0.6rem;
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.5rem;
    border-radius: 0.55rem;
    background: color-mix(in srgb, var(--surface) 92%, transparent);
    opacity: 1;
    transition:
      opacity 160ms ease,
      transform 160ms ease;
  }
  :global(.has-group:not(.chrome-visible)) .toolbar {
    opacity: 0;
    pointer-events: none;
    transform: translateY(-0.5rem);
  }
  :global(.has-group:not(.chrome-visible)) .toolbar:focus-within {
    opacity: 1;
    pointer-events: auto;
    transform: none;
  }
  h2,
  .eyebrow {
    margin: 0;
  }
  .eyebrow,
  .mode-note {
    color: var(--muted);
  }
  .controls {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: 0.5rem;
  }
  .controls label {
    display: grid;
    gap: 0.2rem;
    color: var(--muted);
    font-size: 0.72rem;
  }
  select {
    min-height: 2.2rem;
    border: 1px solid var(--border);
    border-radius: 0.4rem;
    background: var(--surface-raised);
    color: var(--text);
  }
  .mode-note {
    position: absolute;
    z-index: 24;
    right: 0.6rem;
    bottom: 0.6rem;
    left: 0.6rem;
    margin: 0;
    padding: 0.5rem 0.7rem;
    border-radius: 0.45rem;
    background: color-mix(in srgb, var(--surface) 92%, transparent);
    font-size: 0.78rem;
    opacity: 0;
    pointer-events: none;
    transition: opacity 160ms ease;
  }
  :global(.chrome-visible) .mode-note {
    opacity: 1;
  }
  .viewer-container,
  .photo-grid {
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
  }
  .photo-grid {
    display: grid;
    grid-auto-rows: minmax(0, 1fr);
    gap: 0;
  }
  .columns {
    grid-template-columns: repeat(auto-fit, minmax(0, 1fr));
  }
  .rows {
    grid-template-columns: 1fr;
  }
  .quad {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .generic {
    grid-template-columns: repeat(var(--grid-columns), minmax(0, 1fr));
  }
  @media (max-width: 800px) {
    .toolbar {
      top: 3.8rem;
      align-items: stretch;
      flex-direction: column;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .toolbar,
    .mode-note {
      transition: none;
    }
  }
</style>
