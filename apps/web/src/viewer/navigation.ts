import type {
  MappingResult,
  NavigationMode,
  NavigationState,
  Point,
  PointMapper,
  Span,
  ViewerAdapter,
  ViewerInteraction,
} from "./types";

const PROJECTIVE_EPSILON = 1e-12;
const MIN_PIXEL_RATIO = 1e-4;
const MAX_PIXEL_RATIO = 1e4;

interface ReadonlyAlignment {
  readonly photoId: string;
  readonly coordinateSystem: "normalized-oriented";
  readonly direction: "photo-to-anchor";
  readonly homography: readonly number[];
  readonly inverseHomography: readonly number[];
  readonly confidence: number;
  readonly provenance: "direct-pair" | "composed" | "manual-unverified";
}

export interface NavigationGroup {
  readonly anchorPhotoId: string;
  readonly alignments?: readonly ReadonlyAlignment[];
}

export function applyHomography(matrix: readonly number[], point: Point): Point | null {
  if (matrix.length !== 9 || matrix.some((value) => !Number.isFinite(value))) return null;
  const [x, y] = point;
  const denominator = matrix[6]! * x + matrix[7]! * y + matrix[8]!;
  if (!Number.isFinite(denominator) || Math.abs(denominator) < PROJECTIVE_EPSILON) return null;
  const mapped: Point = [
    (matrix[0]! * x + matrix[1]! * y + matrix[2]!) / denominator,
    (matrix[3]! * x + matrix[4]! * y + matrix[5]!) / denominator,
  ];
  return mapped.every(Number.isFinite) ? mapped : null;
}

export class NormalizedPointMapper implements PointMapper {
  map(_sourcePhotoId: string, _targetPhotoId: string, point: Point): MappingResult {
    return { point, fallback: false, provenance: "normalized" };
  }
}

export class GlobalHomographyPointMapper implements PointMapper {
  readonly #anchorPhotoId: string;
  readonly #alignments: ReadonlyMap<string, ReadonlyAlignment>;

  constructor(group: NavigationGroup) {
    this.#anchorPhotoId = group.anchorPhotoId;
    this.#alignments = new Map<string, ReadonlyAlignment>(
      (group.alignments ?? []).map((alignment) => [alignment.photoId, alignment]),
    );
  }

  map(sourcePhotoId: string, targetPhotoId: string, point: Point): MappingResult {
    const anchorPoint = this.#toAnchor(sourcePhotoId, point);
    if (!anchorPoint) return this.#fallback(point, "leader has no usable global alignment");
    const targetPoint = this.#fromAnchor(targetPhotoId, anchorPoint);
    if (!targetPoint) return this.#fallback(point, "target has no usable global alignment");
    if (targetPoint[0] < 0 || targetPoint[0] > 1 || targetPoint[1] < 0 || targetPoint[1] > 1) {
      return this.#fallback(point, "mapped point is outside the target image");
    }
    const alignment =
      targetPhotoId === this.#anchorPhotoId ? undefined : this.#alignments.get(targetPhotoId);
    return {
      point: targetPoint,
      fallback: false,
      provenance:
        alignment?.provenance === "composed" ? "composed" : alignment ? "direct-pair" : "anchor",
    };
  }

  #toAnchor(photoId: string, point: Point): Point | null {
    if (photoId === this.#anchorPhotoId) return point;
    const alignment = this.#eligible(photoId);
    return alignment ? applyHomography(alignment.homography, point) : null;
  }

  #fromAnchor(photoId: string, point: Point): Point | null {
    if (photoId === this.#anchorPhotoId) return point;
    const alignment = this.#eligible(photoId);
    return alignment ? applyHomography(alignment.inverseHomography, point) : null;
  }

  #eligible(photoId: string): ReadonlyAlignment | null {
    const alignment = this.#alignments.get(photoId);
    if (
      !alignment ||
      alignment.coordinateSystem !== "normalized-oriented" ||
      alignment.direction !== "photo-to-anchor" ||
      alignment.provenance === "manual-unverified" ||
      !Number.isFinite(alignment.confidence)
    ) {
      return null;
    }
    return alignment;
  }

  #fallback(point: Point, reason: string): MappingResult {
    return { point, fallback: true, provenance: "normalized", reason };
  }
}

export class NavigationController {
  readonly #adapters = new Map<string, ViewerAdapter>();
  readonly #subscriptions = new Map<string, () => void>();
  readonly #normalizedMapper = new NormalizedPointMapper();
  readonly #alignedMapper: PointMapper;
  readonly #fallbacks = new Map<string, string>();
  #state: NavigationState = {
    mode: "normalized",
    leaderPhotoId: null,
    center: [0.5, 0.5],
    scale: { kind: "normalized", span: [1, 1] },
    sequence: 0,
  };
  #frame: number | null = null;
  #applying = false;
  #applyLeader = false;

  constructor(group: NavigationGroup) {
    this.#alignedMapper = new GlobalHomographyPointMapper(group);
  }

  get state(): NavigationState {
    return this.#state;
  }

  get fallbacks(): ReadonlyMap<string, string> {
    return this.#fallbacks;
  }

  add(photoId: string, adapter: ViewerAdapter): void {
    this.remove(photoId);
    this.#adapters.set(photoId, adapter);
    this.#subscriptions.set(
      photoId,
      adapter.onInteraction((interaction) => this.#interacted(photoId, interaction)),
    );
    const leader = this.#state.leaderPhotoId ?? this.#adapters.keys().next().value;
    if (leader && this.#state.sequence > 0) {
      const mapper = this.#state.mode === "aligned" ? this.#alignedMapper : this.#normalizedMapper;
      const mapped = mapper.map(leader, photoId, this.#state.center);
      this.#applyScale(adapter, mapped.point);
    }
  }

  remove(photoId: string): void {
    this.#subscriptions.get(photoId)?.();
    this.#subscriptions.delete(photoId);
    this.#adapters.delete(photoId);
    this.#fallbacks.delete(photoId);
    if (this.#state.leaderPhotoId === photoId) {
      this.#state = { ...this.#state, leaderPhotoId: null };
    }
  }

  setMode(mode: NavigationMode): void {
    this.#state = { ...this.#state, mode, sequence: this.#state.sequence + 1 };
    this.#applyLeader = true;
    this.#schedule();
  }

  fit(): void {
    this.#state = {
      ...this.#state,
      center: [0.5, 0.5],
      scale: { kind: "normalized", span: [1, 1] },
      sequence: this.#state.sequence + 1,
    };
    this.#applyLeader = true;
    this.#schedule();
  }

  zoomToPixelRatio(screenPixelsPerImagePixel: number): void {
    if (!Number.isFinite(screenPixelsPerImagePixel)) return;
    this.#state = {
      ...this.#state,
      scale: {
        kind: "pixel",
        screenPixelsPerImagePixel: clampPixelRatio(screenPixelsPerImagePixel),
      },
      sequence: this.#state.sequence + 1,
    };
    this.#applyLeader = true;
    this.#schedule();
  }

  zoomBy(factor: number): void {
    const leader = this.#state.leaderPhotoId ?? this.#adapters.keys().next().value;
    if (leader) this.#adapters.get(leader)?.zoomBy(factor);
  }

  destroy(): void {
    if (this.#frame !== null) cancelAnimationFrame(this.#frame);
    for (const unsubscribe of this.#subscriptions.values()) unsubscribe();
    this.#subscriptions.clear();
    this.#adapters.clear();
  }

  #interacted(photoId: string, interaction: ViewerInteraction): void {
    if (this.#applying) return;
    this.#applyLeader = false;
    const scale =
      this.#state.scale.kind === "pixel" && Number.isFinite(interaction.screenPixelsPerImagePixel)
        ? {
            kind: "pixel" as const,
            screenPixelsPerImagePixel: clampPixelRatio(interaction.screenPixelsPerImagePixel),
          }
        : { kind: "normalized" as const, span: clampSpan(interaction.span) };
    this.#state = {
      ...this.#state,
      leaderPhotoId: photoId,
      center: interaction.center,
      scale,
      sequence: this.#state.sequence + 1,
    };
    this.#schedule();
  }

  #schedule(): void {
    if (this.#frame !== null) return;
    this.#frame = requestAnimationFrame(() => {
      this.#frame = null;
      this.#apply();
    });
  }

  #apply(): void {
    const leader = this.#state.leaderPhotoId ?? this.#adapters.keys().next().value;
    if (!leader) return;
    const mapper = this.#state.mode === "aligned" ? this.#alignedMapper : this.#normalizedMapper;
    this.#applying = true;
    this.#fallbacks.clear();
    try {
      for (const [photoId, adapter] of this.#adapters) {
        if (photoId === leader && !this.#applyLeader) continue;
        const mapped = mapper.map(leader, photoId, this.#state.center);
        if (mapped.fallback && mapped.reason) this.#fallbacks.set(photoId, mapped.reason);
        this.#applyScale(adapter, mapped.point);
      }
    } finally {
      this.#applyLeader = false;
      requestAnimationFrame(() => {
        this.#applying = false;
      });
    }
  }

  #applyScale(adapter: ViewerAdapter, center: Point): void {
    if (this.#state.scale.kind === "pixel") {
      adapter.applyPixelRatio(center, this.#state.scale.screenPixelsPerImagePixel);
    } else {
      adapter.applyViewport(center, this.#state.scale.span);
    }
  }
}

export function clampSpan(span: Span): Span {
  return [Math.min(4, Math.max(1e-4, span[0])), Math.min(4, Math.max(1e-4, span[1]))];
}

export function clampPixelRatio(ratio: number): number {
  return Math.min(MAX_PIXEL_RATIO, Math.max(MIN_PIXEL_RATIO, ratio));
}
