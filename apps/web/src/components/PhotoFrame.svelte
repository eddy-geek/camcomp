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
  let statusIndicator: HTMLButtonElement | undefined;
  let statusPopover: HTMLSpanElement | undefined;
  let statusPopoverLeft = 0;
  let statusPopoverTop = 0;

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
  $: hdrMetadataMessages =
    source?.displayDynamicRange.kind === "jpeg-gain-map"
      ? [
          "GM",
          source.displayDynamicRange.hdrStops === undefined
            ? undefined
            : `≈${formatNumber(source.displayDynamicRange.hdrStops)} HDR stops`,
        ]
      : [];
  $: statusMessages = [
    presentationStatus
      ? presentationStatusText(presentationStatus, source?.jpegGainMapRuntimeCapability)
      : undefined,
    ...hdrMetadataMessages,
    ...(source?.warnings ?? []),
  ].filter((message): message is string => Boolean(message));

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
        error = errorMessage(reason);
      }
    } finally {
      if (sequence === loadSequence) loading = false;
    }
  }

  function errorMessage(reason: unknown): string {
    if (reason == null) return "The photo source could not be opened.";
    if (reason instanceof Error && reason.message) return reason.message;
    if (typeof reason === "object" && reason !== null) {
      const message = (reason as { message?: unknown }).message;
      if (typeof message === "string" && message) return message;
      return "The photo source could not be opened.";
    }
    const message = String(reason);
    return message || "The photo source could not be opened.";
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

  function placeStatusPopover(): void {
    if (!statusIndicator || !statusPopover || typeof window === "undefined") return;
    const indicator = statusIndicator.getBoundingClientRect();
    const width = statusPopover.offsetWidth;
    const height = statusPopover.offsetHeight;
    statusPopoverLeft = Math.min(
      Math.max(indicator.right - width, 8),
      Math.max(8, window.innerWidth - width - 8),
    );
    const above = indicator.top - height - 6;
    statusPopoverTop =
      above >= 8
        ? above
        : Math.min(indicator.bottom + 6, Math.max(8, window.innerHeight - height - 8));
  }

  function formatNumber(value: number): string {
    return new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(value);
  }

  function presentationStatusBadge(status: HdrPresentationStatus): string {
    return status === "unverified-runtime" || status === "unverified-source" ? "HDR?" : "HDR";
  }
</script>

<svelte:window onresize={placeStatusPopover} />

<article class="photo-frame" aria-labelledby={`photo-${photo.id}`}>
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
    <header class="photo-heading">
      <div>
        <h3 id={`photo-${photo.id}`} title={photo.path.split("/").at(-1)}>
          {photo.path.split("/").at(-1)}
        </h3>
        <span title={alignmentStatus}>{alignmentStatus}</span>
      </div>
      <div class="badges">
        {#if source?.sourceKind === "sidecar"}<strong class="warning">Sidecar</strong>{/if}
        {#if source?.sourceKind === "decoded"}<strong class="warning">Decoded copy</strong>{/if}
        {#if presentationStatus && presentationStatus !== "sdr-source"}
          <strong class="hdr-badge">{presentationStatusBadge(presentationStatus)}</strong>
        {/if}
        {#if statusMessages.length}
          <button
            bind:this={statusIndicator}
            type="button"
            class="status-indicator"
            aria-label={`Photo status: ${statusMessages.join(" ")}`}
            aria-describedby={`photo-status-${photo.id}`}
            onmouseenter={placeStatusPopover}
            onfocus={placeStatusPopover}
          >
            <span aria-hidden="true">!</span>
            <span
              bind:this={statusPopover}
              id={`photo-status-${photo.id}`}
              class="status-popover"
              role="status"
              style={`position: fixed; left: ${statusPopoverLeft}px; top: ${statusPopoverTop}px;`}
            >
              {#each statusMessages as message (message)}<span>{message}</span>{/each}
            </span>
          </button>
        {/if}
      </div>
    </header>
    <dl>
      <div>
        <dt>Focal length</dt>
        <dd title={focalLabel()}>{focalLabel()}</dd>
      </div>
      <div>
        <dt>Resolution</dt>
        <dd title={`${photo.dimensions.oriented[0]} × ${photo.dimensions.oriented[1]}`}>
          {photo.dimensions.oriented[0]} × {photo.dimensions.oriented[1]}
        </dd>
      </div>
      <div>
        <dt>Camera</dt>
        <dd title={cameraLabel()}>{cameraLabel()}</dd>
      </div>
      <div>
        <dt>Mode</dt>
        <dd title={photo.metadata.modeLabel ?? "Unavailable"}>
          {photo.metadata.modeLabel ?? "Unavailable"}
        </dd>
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
  .photo-heading {
    display: flex;
    min-width: 0;
    align-items: center;
    justify-content: space-between;
    gap: 0.4rem;
    padding: 0.35rem 0.5rem;
  }
  .photo-heading > div:first-child {
    min-width: 0;
  }
  .badges {
    display: flex;
    flex: none;
    align-items: center;
    gap: 0.3rem;
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
  .photo-heading span {
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
    right: 0.25rem;
    bottom: 0.25rem;
    left: 0.25rem;
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(16rem, 1.7fr);
    align-items: center;
    border: 1px solid color-mix(in srgb, var(--border) 55%, transparent);
    border-radius: 0.35rem;
    background: color-mix(in srgb, var(--surface) 68%, transparent);
  }
  dl {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 0.35rem;
    margin: 0;
    padding: 0.35rem 0.5rem;
  }
  dt {
    color: var(--muted);
    font-size: 0.58rem;
    line-height: 1;
    text-transform: uppercase;
  }
  dd {
    margin: 0.1rem 0 0;
    overflow: hidden;
    font-size: 0.68rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .warning,
  .hdr-badge {
    color: var(--orange);
    font-size: 0.65rem;
  }
  .status-indicator {
    position: relative;
    display: grid;
    width: 1.3rem;
    min-width: 1.3rem;
    height: 1.3rem;
    min-height: 1.3rem;
    padding: 0;
    place-items: center;
    border-color: var(--orange);
    border-radius: 50%;
    background: color-mix(in srgb, var(--orange) 14%, transparent);
    color: var(--orange);
    font-size: 0.75rem;
    font-weight: 800;
  }
  .status-popover {
    position: fixed;
    z-index: 60;
    display: grid;
    width: max-content;
    max-width: min(24rem, 70vw);
    gap: 0.25rem;
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
    overflow-wrap: anywhere;
    pointer-events: none;
    text-align: left;
    transform: translateY(0.2rem);
    transition:
      opacity 120ms ease,
      transform 120ms ease;
  }
  .status-indicator:hover .status-popover,
  .status-indicator:focus-visible .status-popover {
    opacity: 1;
    transform: none;
  }
  @media (max-width: 650px) {
    .photo-details {
      grid-template-columns: 1fr;
    }
    dl {
      border-top: 1px solid var(--border);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .photo-chrome {
      transition: none;
    }
  }
</style>
