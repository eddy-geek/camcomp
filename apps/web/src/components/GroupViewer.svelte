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
  export let canSelectPreviousGroup = false;
  export let canSelectNextGroup = false;
  export let onSelectPreviousGroup: () => void = () => undefined;
  export let onSelectNextGroup: () => void = () => undefined;

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

  function handleKeydown(event: KeyboardEvent): void {
    if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) return;
    const target = event.target;
    if (
      target instanceof Element &&
      target.closest('input, select, textarea, button, a, [contenteditable="true"]')
    )
      return;

    let handled = true;
    switch (event.key) {
      case "ArrowLeft":
        controller.panBy(-0.1, 0);
        break;
      case "ArrowRight":
        controller.panBy(0.1, 0);
        break;
      case "ArrowUp":
        controller.panBy(0, -0.1);
        break;
      case "ArrowDown":
        controller.panBy(0, 0.1);
        break;
      case "+":
      case "=":
        controller.zoomBy(1.25);
        break;
      case "-":
        controller.zoomBy(0.8);
        break;
      case "PageUp":
        if (canSelectPreviousGroup) onSelectPreviousGroup();
        else handled = false;
        break;
      case "PageDown":
      case " ":
        if (canSelectNextGroup) onSelectNextGroup();
        else handled = false;
        break;
      default:
        handled = false;
    }
    if (handled) event.preventDefault();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<section class="viewer-shell" aria-labelledby="viewer-heading">
  <header class="toolbar viewer-toolbar">
    <h2 id="viewer-heading">
      Comparison group <span>· {group.photoIds.length} photos</span>
    </h2>
    <div class="controls" role="toolbar" aria-label="Viewer controls">
      <label>
        <span class="control-label">Dynamic range</span>
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
        <span class="control-label">Navigation</span>
        <select
          value={mode}
          aria-label="Navigation"
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
    top: 3.1rem;
    right: 0.4rem;
    left: 0.4rem;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.3rem 0.4rem;
    border: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
    border-radius: 0.4rem;
    background: color-mix(in srgb, var(--surface) 72%, transparent);
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
  h2 {
    margin: 0;
    font-size: 0.85rem;
    white-space: nowrap;
  }
  h2 span,
  .controls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.3rem;
  }
  .controls label {
    display: block;
    color: var(--muted);
    font-size: 0.72rem;
  }
  .control-label {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }
  select {
    min-height: 1.8rem;
    max-width: 12rem;
    border: 1px solid var(--border);
    border-radius: 0.3rem;
    background: color-mix(in srgb, var(--surface-raised) 82%, transparent);
    color: var(--text);
    font-size: 0.72rem;
  }
  .controls button {
    min-height: 1.8rem;
    padding: 0.25rem 0.55rem;
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
</style>
