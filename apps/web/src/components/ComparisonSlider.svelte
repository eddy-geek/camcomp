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
    <p class="comparison-status" role="status">{statusSummary.join(" · ")}</p>
  {/if}
  <div class="divider" style={`left: ${position}%`}></div>
  <label>
    <span>Comparison divider: {position}%</span>
    <input type="range" min="0" max="100" step="1" bind:value={position} />
  </label>
</div>

<style>
  .comparison {
    position: relative;
    min-height: 34rem;
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
    top: 0.75rem;
    right: 1rem;
    left: 1rem;
    margin: 0;
    padding: 0.45rem 0.7rem;
    border-radius: 0.4rem;
    background: color-mix(in srgb, var(--surface) 94%, transparent);
    color: var(--orange);
    font-size: 0.76rem;
  }
  .divider {
    position: absolute;
    z-index: 4;
    top: 0;
    bottom: 3.5rem;
    width: 2px;
    background: white;
    box-shadow: 0 0 0 1px #111;
    pointer-events: none;
  }
  label {
    position: absolute;
    z-index: 5;
    right: 1rem;
    bottom: 0.6rem;
    left: 1rem;
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.45rem 0.7rem;
    border-radius: 0.5rem;
    background: color-mix(in srgb, var(--surface) 94%, transparent);
  }
  label span {
    min-width: 10rem;
    font-size: 0.78rem;
  }
  input {
    width: 100%;
    accent-color: var(--orange);
  }
</style>
