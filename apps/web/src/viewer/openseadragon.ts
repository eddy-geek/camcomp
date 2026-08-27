import OpenSeadragon from "openseadragon";
import type { Point, Span, ViewerAdapter, ViewerInteraction } from "./types";

export class OpenSeadragonViewerAdapter implements ViewerAdapter {
  readonly #viewer: OpenSeadragon.Viewer;
  readonly #handlers = new Set<(interaction: ViewerInteraction) => void>();
  #destroyed = false;
  #suppress = false;
  #applicationSequence = 0;

  constructor(container: HTMLElement) {
    this.#viewer = OpenSeadragon({
      element: container,
      showNavigationControl: false,
      showNavigator: false,
      showSequenceControl: false,
      visibilityRatio: 0.2,
      constrainDuringPan: false,
      animationTime: 0.12,
      blendTime: 0,
      gestureSettingsMouse: { clickToZoom: false, dblClickToZoom: true },
      gestureSettingsTouch: { pinchToZoom: true },
    });
    this.#viewer.addHandler("animation", () => this.#emit());
    this.#viewer.addHandler("animation-finish", () => this.#emit());
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
    const bounds = image.viewportToImageRectangle(this.#viewer.viewport.getBounds(true));
    const size = image.getContentSize();
    if (!size.x || !size.y) return null;
    return {
      center: [(bounds.x + bounds.width / 2) / size.x, (bounds.y + bounds.height / 2) / size.y],
      span: [bounds.width / size.x, bounds.height / size.y],
    };
  }

  applyViewport(center: Point, span: Span): void {
    const image = this.#viewer.world.getItemAt(0);
    if (!image) return;
    const size = image.getContentSize();
    const imageBounds = new OpenSeadragon.Rect(
      (center[0] - span[0] / 2) * size.x,
      (center[1] - span[1] / 2) * size.y,
      span[0] * size.x,
      span[1] * size.y,
    );
    this.#suppress = true;
    const sequence = ++this.#applicationSequence;
    this.#viewer.viewport.fitBounds(image.imageToViewportRectangle(imageBounds), true);
    requestAnimationFrame(() => {
      if (this.#applicationSequence === sequence) this.#suppress = false;
    });
  }

  fit(): void {
    this.#viewer.viewport.goHome(false);
  }

  zoomBy(factor: number): void {
    this.#viewer.viewport.zoomBy(factor);
    this.#viewer.viewport.applyConstraints();
  }

  onInteraction(handler: (interaction: ViewerInteraction) => void): () => void {
    this.#handlers.add(handler);
    return () => this.#handlers.delete(handler);
  }

  destroy(): void {
    if (this.#destroyed) return;
    this.#destroyed = true;
    this.#handlers.clear();
    this.#viewer.destroy();
  }

  #emit(): void {
    if (this.#suppress || this.#destroyed) return;
    const viewport = this.viewport();
    if (viewport) for (const handler of this.#handlers) handler(viewport);
  }
}
