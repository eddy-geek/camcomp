<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import type { DeepReadonly, Photo } from "../contracts";
  import type { PhotoResolutionOptions, ResolvedPhotoSource } from "../platform";
  import { NavigationController, type ViewerAdapter } from "../viewer";

  export let photo: DeepReadonly<Photo>;
  export let controller: NavigationController;
  export let resolveSource: (
    photo: DeepReadonly<Photo>,
    options?: PhotoResolutionOptions,
  ) => Promise<ResolvedPhotoSource>;
  export let releaseSource: (source: ResolvedPhotoSource) => void;
  export let alignmentStatus: string;

  let viewerElement: HTMLDivElement;
  let adapter: ViewerAdapter | undefined;
  let source: ResolvedPhotoSource | undefined;
  let error: string | undefined;
  let highMemoryWarning: string | undefined;
  let loading = true;
  let disposed = false;
  let loadSequence = 0;

  onMount(() => {
    void loadPhoto(false);
  });

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

  onDestroy(() => {
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
</script>

<article class="photo-frame" aria-labelledby={`photo-${photo.id}`}>
  <header>
    <div>
      <h3 id={`photo-${photo.id}`}>{photo.path.split("/").at(-1)}</h3>
      <span>{alignmentStatus}</span>
    </div>
    {#if source?.sourceKind === "sidecar"}<strong class="warning">Sidecar</strong>{/if}
    {#if source?.sourceKind === "decoded"}<strong class="warning">Decoded copy</strong>{/if}
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
</article>

<style>
  .photo-frame {
    display: grid;
    min-width: 0;
    min-height: 22rem;
    grid-template-rows: auto minmax(14rem, 1fr) auto auto;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 0.65rem;
    background: #0b1015;
  }
  header {
    display: flex;
    min-width: 0;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.65rem 0.8rem;
    background: var(--surface-raised);
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
    position: relative;
    min-height: 14rem;
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
  .warning {
    color: var(--orange);
    font-size: 0.72rem;
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
</style>
