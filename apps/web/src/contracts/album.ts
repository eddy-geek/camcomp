import type {
  Alignment,
  AnalysisDocument,
  Configuration,
  Group,
  PairResult,
  Photo,
  Session,
  Ungrouped,
} from "./camcomp.generated";
import { validateAnalysisDocument } from "./validation";

export interface IndexedAlbum {
  readonly document: DeepReadonly<AnalysisDocument>;
  readonly photosById: ReadonlyMap<string, DeepReadonly<Photo>>;
  readonly configurationsById: ReadonlyMap<string, DeepReadonly<Configuration>>;
  readonly sessionsById: ReadonlyMap<string, DeepReadonly<Session>>;
  readonly pairsById: ReadonlyMap<string, DeepReadonly<PairResult>>;
  readonly groupsById: ReadonlyMap<string, DeepReadonly<Group>>;
  readonly alignmentsByPhotoId: ReadonlyMap<string, DeepReadonly<Alignment>>;
  readonly ungroupedByPhotoId: ReadonlyMap<string, DeepReadonly<Ungrouped>>;
}

export type DeepReadonly<T> = T extends (...args: never[]) => unknown
  ? T
  : T extends readonly (infer U)[]
    ? readonly DeepReadonly<U>[]
    : T extends object
      ? { readonly [K in keyof T]: DeepReadonly<T[K]> }
      : T;

export function normalizeAnalysisAlbum(value: unknown): IndexedAlbum {
  const validated = validateAnalysisDocument(value);
  const document = deepFreeze(cloneJsonValue(validated)) as DeepReadonly<AnalysisDocument>;
  return Object.freeze({
    document,
    photosById: readonlyIndex(document.photos),
    configurationsById: readonlyIndex(document.configurations),
    sessionsById: readonlyIndex(document.sessions),
    pairsById: readonlyIndex(document.pairs),
    groupsById: readonlyIndex(document.groups),
    alignmentsByPhotoId: readonlyMap(
      document.groups.flatMap((group) =>
        (group.alignments ?? []).map((alignment) => [alignment.photoId, alignment] as const),
      ),
    ),
    ungroupedByPhotoId: readonlyMap(
      document.ungrouped.map((entry) => [entry.photoId, entry] as const),
    ),
  });
}

export function getPairForPhotos(
  album: IndexedAlbum,
  firstPhotoId: string,
  secondPhotoId: string,
): DeepReadonly<PairResult> | undefined {
  const [a, b] =
    firstPhotoId <= secondPhotoId ? [firstPhotoId, secondPhotoId] : [secondPhotoId, firstPhotoId];
  return album.pairsById.get(`pair-${a}-${b}`);
}

function readonlyIndex<T extends { readonly id: string }>(
  values: readonly T[],
): ReadonlyMap<string, T> {
  return readonlyMap(values.map((value) => [value.id, value] as const));
}

class ReadonlyMapSnapshot<K, V> implements ReadonlyMap<K, V> {
  readonly #map: Map<K, V>;

  constructor(entries: Iterable<readonly [K, V]>) {
    this.#map = new Map(entries);
    Object.freeze(this);
  }

  get size(): number {
    return this.#map.size;
  }

  get(key: K): V | undefined {
    return this.#map.get(key);
  }

  has(key: K): boolean {
    return this.#map.has(key);
  }

  entries(): MapIterator<[K, V]> {
    return this.#map.entries();
  }

  keys(): MapIterator<K> {
    return this.#map.keys();
  }

  values(): MapIterator<V> {
    return this.#map.values();
  }

  forEach(callbackfn: (value: V, key: K, map: ReadonlyMap<K, V>) => void, thisArg?: unknown): void {
    for (const [key, value] of this.#map) callbackfn.call(thisArg, value, key, this);
  }

  [Symbol.iterator](): MapIterator<[K, V]> {
    return this.entries();
  }
}

function readonlyMap<K, V>(entries: Iterable<readonly [K, V]>): ReadonlyMap<K, V> {
  return new ReadonlyMapSnapshot(entries);
}

function cloneJsonValue<T>(value: T): T {
  return structuredClone(value);
}

function deepFreeze<T>(value: T): T {
  if (typeof value !== "object" || value === null || Object.isFrozen(value)) return value;
  for (const child of Object.values(value)) deepFreeze(child);
  return Object.freeze(value);
}
