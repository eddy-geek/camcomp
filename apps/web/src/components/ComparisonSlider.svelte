<script lang="ts">
  import type { DeepReadonly, Photo } from "../contracts";
  import type { PhotoResolutionOptions, ResolvedPhotoSource } from "../platform";
  import type { NavigationController } from "../viewer";
  import PhotoFrame from "./PhotoFrame.svelte";

  export let photos: readonly DeepReadonly<Photo>[];
  export let controller: NavigationController;
  export let resolveSource: (
    photo: DeepReadonly<Photo>,
    options?: PhotoResolutionOptions,
  ) => Promise<ResolvedPhotoSource>;
  export let releaseSource: (source: ResolvedPhotoSource) => void;
  export let alignmentStatus: (photoId: string) => string;

  let position = 50;
</script>

<div class="comparison">
  <div class="layer">
    <PhotoFrame
      photo={photos[0]!}
      {controller}
      {resolveSource}
      {releaseSource}
      alignmentStatus={alignmentStatus(photos[0]!.id)}
    />
  </div>
  <div class="layer trailing" style={`clip-path: inset(0 0 0 ${position}%);`} aria-hidden="true">
    <PhotoFrame
      photo={photos[1]!}
      {controller}
      {resolveSource}
      {releaseSource}
      alignmentStatus={alignmentStatus(photos[1]!.id)}
    />
  </div>
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
