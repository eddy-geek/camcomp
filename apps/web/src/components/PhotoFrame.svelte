<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import type { DeepReadonly, Photo } from "../contracts";
  import type { PhotoResolutionOptions, ResolvedPhotoSource } from "../platform";
  import {
    evaluateHdrPresentationStatus,
    presentationStatusText,
    NavigationController,
    type DynamicRangeMode,
    type HdrPresentationStatus,
    type ViewerAdapter,
  } from "../viewer";

  export let photo: DeepReadonly<Photo>;
  export let controller: NavigationController;
  export let resolveSource: (
    photo: DeepReadonly<Photo>,
    options?: PhotoResolutionOptions,
  ) => Promise<ResolvedPhotoSource>;
  export let releaseSource: (source: ResolvedPhotoSource) => void;
  export let alignmentStatus: string;
  export let dynamicRangeMode: DynamicRangeMode = "automatic";
  export let forcedSdrSupported = true;
  interface PhotoStatusChange {
    readonly photoId: string;
    readonly status: HdrPresentationStatus;
  }
  export let onStatusChange: (change: PhotoStatusChange) => void = () => undefined;

  let viewerElement: HTMLDivElement;
  let adapter: ViewerAdapter | undefined;
  let source: ResolvedPhotoSource | undefined;
  let error: string | undefined;
  let highMemoryWarning: string | undefined;
  let loading = true;
  let disposed = false;
  let loadSequence = 0;
  let displayHeadroom = false;
  let mediaQuery: MediaQueryList | undefined;
  let presentationStatus: HdrPresentationStatus | undefined;
  let reportedStatus: HdrPresentationStatus | undefined;

  onMount(() => {
    if (typeof window !== "undefined" && typeof window.matchMedia === "function") {
      mediaQuery = window.matchMedia("(dynamic-range: high)");
      displayHeadroom = mediaQuery.matches;
      mediaQuery.addEventListener("change", handleDisplayHeadroomChange);
    }
    void loadPhoto(false);
  });

  $: presentationStatus = source
    ? evaluateHdrPresentationStatus(
        source.displayDynamicRange,
        source.jpegGainMapRuntimeCapability,
        dynamicRangeMode,
        displayHeadroom,
        forcedSdrSupported,
      )
    : undefined;
  $: if (presentationStatus) notifyStatusChange(presentationStatus);

  function notifyStatusChange(status: HdrPresentationStatus): void {
    if (status === reportedStatus) return;
    reportedStatus = status;
    onStatusChange({ photoId: photo.id, status });
  }

  async function loadPhoto(allowHighMemoryHeic: boolean): Promise<void> {
    const sequence = ++loadSequence;
    loading = true;
    error = undefined;
    highMemoryWarning = undefined;
    try {
      const [resolvedSource, viewerModule] = await Promise.all([
        resolveSource(photo, { allowHighMemoryHeic }),
        import("../viewer/openseadragon"),
      ]);
      source = resolvedSource;
      if (disposed || sequence !== loadSequence) {
        releaseSource(source);
        return;
      }
      adapter = new viewerModule.OpenSeadragonViewerAdapter(viewerElement);
      await adapter.open(source.displayUrl);
      if (disposed || sequence !== loadSequence) return;
      controller.add(photo.id, adapter);
    } catch (reason) {
      adapter?.destroy();
      adapter = undefined;
      if (source) {
        releaseSource(source);
        source = undefined;
      }
      if (isHighMemoryConfirmation(reason)) {
        highMemoryWarning = reason.message;
      } else {
        error = reason instanceof Error ? reason.message : String(reason);
      }
    } finally {
      if (sequence === loadSequence) loading = false;
    }
  }

  function isHighMemoryConfirmation(
    reason: unknown,
  ): reason is Error & { code: "high-memory-heic-confirmation" } {
    return (
      reason instanceof Error && "code" in reason && reason.code === "high-memory-heic-confirmation"
    );
  }

  function handleDisplayHeadroomChange(event: MediaQueryListEvent): void {
    displayHeadroom = event.matches;
  }

  onDestroy(() => {
    mediaQuery?.removeEventListener("change", handleDisplayHeadroomChange);
    mediaQuery = undefined;
    disposed = true;
    controller.remove(photo.id);
    adapter?.destroy();
    if (source) releaseSource(source);
  });

  function focalLabel(): string {
    const metadata = photo.metadata;
    if (metadata.effectiveFocalLength35mm != null) {
      const corrected = metadata.focalSource === "exif35mm-zoom-applied" ? " (zoom-corrected)" : "";
      return `${formatNumber(metadata.effectiveFocalLength35mm)} mm equivalent${corrected}`;
    }
    if (metadata.focalLength35mm != null)
      return `${formatNumber(metadata.focalLength35mm)} mm equivalent (reported)`;
    if (metadata.focalLengthMm != null)
      return `${formatNumber(metadata.focalLengthMm)} mm physical`;
    return "Unavailable";
  }

  function cameraLabel(): string {
    const value = [photo.metadata.cameraMake, photo.metadata.cameraModel].filter(Boolean).join(" ");
    return value || "Unavailable";
  }

  function formatNumber(value: number): string {
    return new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(value);
  }

  function presentationStatusBadge(status: HdrPresentationStatus): string {
    return status === "unverified-runtime" || status === "unverified-source" ? "HDR?" : "HDR";
  }
</script>

<article class="photo-frame" aria-labelledby={`photo-${photo.id}`}>
  <header class="photo-chrome">
    <div>
      <h3 id={`photo-${photo.id}`}>{photo.path.split("/").at(-1)}</h3>
      <span>{alignmentStatus}</span>
    </div>
    {#if source?.sourceKind === "sidecar"}<strong class="warning">Sidecar</strong>{/if}
    {#if source?.sourceKind === "decoded"}<strong class="warning">Decoded copy</strong>{/if}
    {#if presentationStatus && presentationStatus !== "sdr-source"}
      <strong class="hdr-badge">{presentationStatusBadge(presentationStatus)}</strong>
    {/if}
  </header>
  <div class="viewer" bind:this={viewerElement}>
    {#if loading}<p role="status">Loading full-detail photo…</p>{/if}
    {#if highMemoryWarning}
      <div class="high-memory-warning" role="alert">
        <strong>High-memory HEIC decoding</strong>
        <span>{highMemoryWarning} Other applications may become less responsive.</span>
        <button type="button" onclick={() => void loadPhoto(true)}>Decode HEIC anyway</button>
      </div>
    {/if}
    {#if error}<p class="source-error" role="alert">Missing source: {error}</p>{/if}
  </div>
  <div class="photo-details photo-chrome">
    {#if presentationStatus && presentationStatusText(presentationStatus, source?.jpegGainMapRuntimeCapability)}
      <p class="hdr-status" role="status">
        {presentationStatusText(presentationStatus, source?.jpegGainMapRuntimeCapability)}
      </p>
    {/if}
    {#if source?.warnings.length}
      <ul class="warnings">
        {#each source.warnings as warning (warning)}<li>{warning}</li>{/each}
      </ul>
    {/if}
    <dl>
      <div>
        <dt>Focal length</dt>
        <dd>{focalLabel()}</dd>
      </div>
      <div>
        <dt>Resolution</dt>
        <dd>{photo.dimensions.oriented[0]} × {photo.dimensions.oriented[1]}</dd>
      </div>
      <div>
        <dt>Camera</dt>
        <dd>{cameraLabel()}</dd>
      </div>
    </dl>
  </div>
</article>

<style>
  .photo-frame {
    position: relative;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: #050708;
  }
  header {
    position: absolute;
    z-index: 4;
    top: 0;
    right: 0;
    left: 0;
    display: flex;
    min-width: 0;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.65rem 0.8rem;
    background: color-mix(in srgb, var(--surface-raised) 92%, transparent);
  }
  .photo-chrome {
    opacity: 1;
    transition: opacity 160ms ease;
  }
  :global(.has-group:not(.chrome-visible)) .photo-chrome {
    opacity: 0;
    pointer-events: none;
  }
  h3 {
    max-width: 22rem;
    margin: 0;
    overflow: hidden;
    font-size: 0.9rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  header span {
    color: var(--muted);
    font-size: 0.72rem;
  }
  .viewer {
    position: absolute;
    inset: 0;
    min-width: 0;
    min-height: 0;
    background: #050708;
  }
  .viewer > p {
    position: absolute;
    z-index: 2;
    inset: 50% auto auto 50%;
    max-width: 80%;
    margin: 0;
    transform: translate(-50%, -50%);
    color: var(--muted);
    text-align: center;
  }
  .source-error {
    color: var(--danger) !important;
  }
  .high-memory-warning {
    position: absolute;
    z-index: 3;
    inset: 50% auto auto 50%;
    display: grid;
    width: min(28rem, 90%);
    gap: 0.65rem;
    padding: 1rem;
    border: 1px solid var(--orange);
    border-radius: 0.65rem;
    background: color-mix(in srgb, var(--surface) 96%, transparent);
    transform: translate(-50%, -50%);
    text-align: left;
  }
  .high-memory-warning span {
    color: var(--muted);
    font-size: 0.78rem;
  }
  .photo-details {
    position: absolute;
    z-index: 4;
    right: 0;
    bottom: 0;
    left: 0;
    max-height: 50%;
    overflow: auto;
    background: color-mix(in srgb, var(--surface) 92%, transparent);
  }
  dl {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0.5rem;
    margin: 0;
    padding: 0.7rem 0.8rem;
  }
  dt {
    color: var(--muted);
    font-size: 0.68rem;
    text-transform: uppercase;
  }
  dd {
    margin: 0.15rem 0 0;
    font-size: 0.78rem;
  }
  .warning,
  .hdr-badge {
    color: var(--orange);
    font-size: 0.72rem;
  }
  .hdr-status {
    margin: 0;
    padding: 0.45rem 0.8rem;
    color: var(--orange);
    font-size: 0.76rem;
  }
  .warnings {
    margin: 0;
    padding: 0.45rem 1.5rem;
    color: var(--orange);
    font-size: 0.72rem;
  }
  @media (max-width: 650px) {
    dl {
      grid-template-columns: 1fr;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .photo-chrome {
      transition: none;
    }
  }
</style>
