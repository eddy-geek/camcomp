export type Point = readonly [number, number];
export type Span = readonly [number, number];
export type NavigationMode = "normalized" | "aligned";

export type NavigationScale =
  | { readonly kind: "normalized"; readonly span: Span }
  | { readonly kind: "pixel"; readonly screenPixelsPerImagePixel: number };

export interface NavigationState {
  mode: NavigationMode;
  leaderPhotoId: string | null;
  center: Point;
  scale: NavigationScale;
  sequence: number;
}

export interface MappingResult {
  point: Point;
  fallback: boolean;
  provenance: "anchor" | "direct-pair" | "composed" | "normalized";
  reason?: string;
}

export interface PointMapper {
  map(sourcePhotoId: string, targetPhotoId: string, point: Point): MappingResult;
}

export interface ViewerInteraction {
  center: Point;
  span: Span;
  screenPixelsPerImagePixel: number;
}

export interface ViewerAdapter {
  open(url: string): Promise<void>;
  viewport(): ViewerInteraction | null;
  applyViewport(center: Point, span: Span): void;
  applyPixelRatio(center: Point, screenPixelsPerImagePixel: number): void;
  fit(): void;
  zoomBy(factor: number): void;
  onInteraction(handler: (interaction: ViewerInteraction) => void): () => void;
  destroy(): void;
}
