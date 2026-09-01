<script lang="ts">
  import { onDestroy } from "svelte";
  import { SvelteMap } from "svelte/reactivity";
  import type { DeepReadonly, Photo } from "../contracts";
  import type { PhotoResolutionOptions, ResolvedPhotoSource } from "../platform";
  import {
    presentationStatusText,
    type DynamicRangeMode,
    type HdrPresentationStatus,
    type NavigationController,
  } from "../viewer";
  import PhotoFrame from "./PhotoFrame.svelte";

  export let photos: readonly DeepReadonly<Photo>[];
  export let controller: NavigationController;
  export let resolveSource: (
    photo: DeepReadonly<Photo>,
    options?: PhotoResolutionOptions,
  ) => Promise<ResolvedPhotoSource>;
  export let releaseSource: (source: ResolvedPhotoSource) => void;
  export let alignmentStatus: (photoId: string) => string;
  export let dynamicRangeMode: DynamicRangeMode = "automatic";
  export let forcedSdrSupported = true;

  let position = 50;
  let statusByPhoto = new SvelteMap<string, HdrPresentationStatus>();
  let photoIdsKey = "";
  $: resetStatusMap(photos);
  $: statusSummary = photos
    .map((photo) => {
      const status = statusByPhoto.get(photo.id);
      const text =
        status &&
        (presentationStatusText(status) ?? (status === "sdr-source" ? "SDR source" : undefined));
      return text ? `${photo.path.split("/").at(-1)}: ${text}` : undefined;
    })
    .filter((entry): entry is string => entry !== undefined);

  function resetStatusMap(nextPhotos: readonly DeepReadonly<Photo>[]): void {
    const nextPhotoIdsKey = nextPhotos.map((photo) => photo.id).join("\0");
    if (nextPhotoIdsKey === photoIdsKey) return;
    photoIdsKey = nextPhotoIdsKey;
    statusByPhoto = new SvelteMap();
  }

  function handleStatusChange(change: { photoId: string; status: HdrPresentationStatus }): void {
    statusByPhoto = new SvelteMap(statusByPhoto).set(change.photoId, change.status);
  }

  onDestroy(() => statusByPhoto.clear());
</script>

<div class="comparison">
  <div class="layer">
    <PhotoFrame
      photo={photos[0]!}
      {controller}
      {resolveSource}
      {releaseSource}
      alignmentStatus={alignmentStatus(photos[0]!.id)}
      {dynamicRangeMode}
      {forcedSdrSupported}
      onStatusChange={handleStatusChange}
    />
  </div>
  <div class="layer trailing" style={`clip-path: inset(0 0 0 ${position}%);`} aria-hidden="true">
    <PhotoFrame
      photo={photos[1]!}
      {controller}
      {resolveSource}
      {releaseSource}
      alignmentStatus={alignmentStatus(photos[1]!.id)}
      {dynamicRangeMode}
      {forcedSdrSupported}
      onStatusChange={handleStatusChange}
    />
  </div>
  {#if statusSummary.length}
    <button
      type="button"
      class="comparison-status slider-chrome"
      aria-label={`Comparison status: ${statusSummary.join(" ")}`}
    >
      <span aria-hidden="true">!</span>
      <span class="status-popover" role="status">{statusSummary.join(" · ")}</span>
    </button>
  {/if}
  <div class="divider slider-chrome" style={`left: ${position}%`}></div>
  <label class="slider-chrome">
    <span>Comparison divider: {position}%</span>
    <input type="range" min="0" max="100" step="1" bind:value={position} />
  </label>
</div>

<style>
  .comparison {
    position: relative;
    width: 100%;
    height: 100%;
    min-height: 0;
  }
  .slider-chrome {
    opacity: 1;
    transition: opacity 160ms ease;
  }
  :global(.has-group:not(.chrome-visible)) .slider-chrome {
    opacity: 0;
    pointer-events: none;
  }
  :global(.has-group:not(.chrome-visible)) .slider-chrome:focus-within {
    opacity: 1;
    pointer-events: auto;
  }
  .layer,
  .layer :global(.photo-frame) {
    position: absolute;
    inset: 0;
  }
  .trailing {
    pointer-events: none;
  }
  .comparison-status {
    position: absolute;
    z-index: 5;
    top: 0.4rem;
    right: 0.4rem;
    display: grid;
    width: 1.4rem;
    min-width: 1.4rem;
    height: 1.4rem;
    min-height: 1.4rem;
    padding: 0;
    place-items: center;
    border-color: var(--orange);
    border-radius: 50%;
    background: color-mix(in srgb, var(--surface) 72%, transparent);
    color: var(--orange);
    font-size: 0.75rem;
    font-weight: 800;
  }
  .status-popover {
    position: absolute;
    top: calc(100% + 0.4rem);
    right: 0;
    width: max-content;
    max-width: min(32rem, 80vw);
    padding: 0.45rem 0.55rem;
    border: 1px solid var(--border);
    border-radius: 0.35rem;
    background: color-mix(in srgb, var(--surface) 92%, transparent);
    box-shadow: 0 0.4rem 1.2rem #0008;
    color: var(--text);
    font-size: 0.7rem;
    font-weight: 400;
    line-height: 1.3;
    opacity: 0;
    pointer-events: none;
    text-align: left;
    transform: translateY(-0.2rem);
    transition:
      opacity 120ms ease,
      transform 120ms ease;
  }
  .comparison-status:hover .status-popover,
  .comparison-status:focus-visible .status-popover {
    opacity: 1;
    transform: none;
  }
  .divider {
    position: absolute;
    z-index: 4;
    top: 0;
    bottom: 2.5rem;
    width: 2px;
    background: white;
    box-shadow: 0 0 0 1px #111;
    pointer-events: none;
  }
  label {
    position: absolute;
    z-index: 5;
    right: 0.4rem;
    bottom: 0.35rem;
    left: 0.4rem;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.3rem 0.45rem;
    border: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
    border-radius: 0.35rem;
    background: color-mix(in srgb, var(--surface) 72%, transparent);
  }
  label span {
    min-width: 10rem;
    font-size: 0.78rem;
  }
  input {
    width: 100%;
    accent-color: var(--orange);
  }
  @media (prefers-reduced-motion: reduce) {
    .slider-chrome {
      transition: none;
    }
  }
</style>
