import OpenSeadragon from "openseadragon";
import type { Point, Span, ViewerAdapter, ViewerInteraction } from "./types";

export class OpenSeadragonViewerAdapter implements ViewerAdapter {
  readonly #viewer: OpenSeadragon.Viewer;
  readonly #handlers = new Set<(interaction: ViewerInteraction) => void>();
  #destroyed = false;
  #interactionFrame: number | null = null;

  constructor(container: HTMLElement) {
    this.#viewer = OpenSeadragon({
      element: container,
      drawer: "html",
      showNavigationControl: false,
      showNavigator: false,
      showSequenceControl: false,
      visibilityRatio: 0.2,
      maxZoomPixelRatio: 16,
      constrainDuringPan: false,
      animationTime: 0.12,
      blendTime: 0,
      gestureSettingsMouse: { clickToZoom: false, dblClickToZoom: true },
      gestureSettingsTouch: { pinchToZoom: true },
    });
    this.#viewer.addHandler("canvas-drag", () => this.#scheduleEmit());
    this.#viewer.addHandler("canvas-drag-end", () => this.#scheduleEmit());
    this.#viewer.addHandler("canvas-scroll", () => this.#scheduleEmit());
    this.#viewer.addHandler("canvas-pinch", () => this.#scheduleEmit());
    this.#viewer.addHandler("canvas-double-click", () => this.#scheduleEmit());
  }

  open(url: string): Promise<void> {
    return new Promise((resolve, reject) => {
      const onOpen = () => {
        this.#viewer.removeHandler("open-failed", onFailure);
        resolve();
      };
      const onFailure = (event: OpenSeadragon.OpenFailedEvent) => {
        this.#viewer.removeHandler("open", onOpen);
        reject(new Error(event.message || "The photo could not be opened"));
      };
      this.#viewer.addOnceHandler("open", onOpen);
      this.#viewer.addOnceHandler("open-failed", onFailure);
      const tileSource = new OpenSeadragon.ImageTileSource({ url, buildPyramid: false });
      this.#viewer.open({ tileSource });
    });
  }

  viewport(): ViewerInteraction | null {
    const image = this.#viewer.world.getItemAt(0);
    if (!image) return null;
    const bounds = image.viewportToImageRectangle(this.#viewer.viewport.getBounds(false));
    const size = image.getContentSize();
    const container = this.#viewer.viewport.getContainerSize();
    if (!size.x || !size.y || !bounds.width || !container.x) return null;
    return {
      center: [(bounds.x + bounds.width / 2) / size.x, (bounds.y + bounds.height / 2) / size.y],
      span: [bounds.width / size.x, bounds.height / size.y],
      screenPixelsPerImagePixel: (container.x * devicePixelRatio()) / bounds.width,
    };
  }

  applyViewport(center: Point, span: Span): void {
    const image = this.#viewer.world.getItemAt(0);
    if (!image) return;
    const size = image.getContentSize();
    this.#fitImageBounds(
      image,
      new OpenSeadragon.Rect(
        (center[0] - span[0] / 2) * size.x,
        (center[1] - span[1] / 2) * size.y,
        span[0] * size.x,
        span[1] * size.y,
      ),
    );
  }

  applyPixelRatio(center: Point, screenPixelsPerImagePixel: number): void {
    const image = this.#viewer.world.getItemAt(0);
    if (!image || !Number.isFinite(screenPixelsPerImagePixel) || screenPixelsPerImagePixel <= 0)
      return;
    const size = image.getContentSize();
    const container = this.#viewer.viewport.getContainerSize();
    const imagePixelsPerCssPixel = devicePixelRatio() / screenPixelsPerImagePixel;
    const width = container.x * imagePixelsPerCssPixel;
    const height = container.y * imagePixelsPerCssPixel;
    this.#fitImageBounds(
      image,
      new OpenSeadragon.Rect(
        center[0] * size.x - width / 2,
        center[1] * size.y - height / 2,
        width,
        height,
      ),
    );
  }

  fit(): void {
    this.#viewer.viewport.goHome(true);
  }

  zoomBy(factor: number): void {
    this.#viewer.viewport.zoomBy(factor, undefined, true);
    this.#viewer.viewport.applyConstraints(true);
    this.#scheduleEmit();
  }

  onInteraction(handler: (interaction: ViewerInteraction) => void): () => void {
    this.#handlers.add(handler);
    return () => this.#handlers.delete(handler);
  }

  destroy(): void {
    if (this.#destroyed) return;
    this.#destroyed = true;
    if (this.#interactionFrame !== null) cancelAnimationFrame(this.#interactionFrame);
    this.#handlers.clear();
    this.#viewer.destroy();
  }

  #fitImageBounds(image: OpenSeadragon.TiledImage, bounds: OpenSeadragon.Rect): void {
    this.#viewer.viewport.fitBounds(image.imageToViewportRectangle(bounds), true);
  }

  #scheduleEmit(): void {
    if (this.#destroyed || this.#interactionFrame !== null) return;
    this.#interactionFrame = requestAnimationFrame(() => {
      this.#interactionFrame = null;
      this.#emit();
    });
  }

  #emit(): void {
    if (this.#destroyed) return;
    const viewport = this.viewport();
    if (viewport) for (const handler of this.#handlers) handler(viewport);
  }
}

function devicePixelRatio(): number {
  const ratio = typeof window === "undefined" ? 1 : window.devicePixelRatio;
  return Number.isFinite(ratio) && ratio > 0 ? ratio : 1;
}
