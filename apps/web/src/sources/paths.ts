export type RelativePathErrorCode =
  | "empty-path"
  | "absolute-path"
  | "path-traversal"
  | "invalid-segment"
  | "duplicate-path"
  | "ambiguous-root";

export class RelativePathError extends Error {
  readonly code: RelativePathErrorCode;
  readonly path: string;

  constructor(code: RelativePathErrorCode, message: string, path = "") {
    super(message);
    this.name = "RelativePathError";
    this.code = code;
    this.path = path;
  }
}

export function normalizeRelativePath(path: string): string {
  if (path.length === 0)
    throw new RelativePathError("empty-path", "A relative path cannot be empty.", path);
  if (path.includes("\0"))
    throw new RelativePathError("invalid-segment", "A relative path cannot contain NUL.", path);

  const withSlashes = path.replaceAll("\\", "/");
  if (
    withSlashes.startsWith("/") ||
    withSlashes.startsWith("//") ||
    /^[A-Za-z]:/.test(withSlashes) ||
    /^[A-Za-z][A-Za-z\d+.-]*:\/\//.test(withSlashes)
  ) {
    throw new RelativePathError(
      "absolute-path",
      "Only source-root-relative paths are accepted.",
      path,
    );
  }

  const normalizedSegments = withSlashes.split("/").map((segment) => {
    if (segment === "..")
      throw new RelativePathError("path-traversal", "Parent path segments are not accepted.", path);
    if (segment === "" || segment === ".") {
      throw new RelativePathError(
        "invalid-segment",
        "Empty and current-directory path segments are not accepted.",
        path,
      );
    }
    const normalized = segment.normalize("NFC");
    if (normalized === "." || normalized === "..") {
      throw new RelativePathError(
        "path-traversal",
        "Normalized traversal segments are not accepted.",
        path,
      );
    }
    if (
      [...normalized].some((character) => {
        const code = character.charCodeAt(0);
        return code <= 0x1f || code === 0x7f;
      })
    ) {
      throw new RelativePathError(
        "invalid-segment",
        "Control-character path segments are not accepted.",
        path,
      );
    }
    return normalized;
  });
  return normalizedSegments.join("/");
}

export function indexUniqueRelativePaths<T>(
  entries: Iterable<readonly [path: string, value: T]>,
): ReadonlyMap<string, T> {
  const indexed = new Map<string, T>();
  for (const [path, value] of entries) {
    const normalized = normalizeRelativePath(path);
    if (indexed.has(normalized)) {
      throw new RelativePathError(
        "duplicate-path",
        `More than one source entry normalizes to “${normalized}”.`,
        path,
      );
    }
    indexed.set(normalized, value);
  }
  return indexed;
}

export function stripSingleDirectoryRoot(paths: readonly string[]): readonly string[] {
  if (paths.length === 0) return [];
  const split = paths.map((path) => normalizeRelativePath(path).split("/"));
  const roots = new Set(split.map((segments) => segments[0]));
  if (roots.size !== 1 || split.some((segments) => segments.length < 2)) {
    throw new RelativePathError(
      "ambiguous-root",
      "The selected files do not share one unambiguous folder root.",
    );
  }
  return split.map((segments) => segments.slice(1).join("/"));
}

export function relativePathDirectory(path: string): string {
  const normalized = normalizeRelativePath(path);
  const separator = normalized.lastIndexOf("/");
  return separator < 0 ? "" : normalized.slice(0, separator);
}

export function relativePathBasename(path: string): string {
  const normalized = normalizeRelativePath(path);
  const separator = normalized.lastIndexOf("/");
  return normalized.slice(separator + 1);
}

export function replaceRelativePathExtension(path: string, extension: string): string {
  const normalized = normalizeRelativePath(path);
  const separator = normalized.lastIndexOf("/");
  const dot = normalized.lastIndexOf(".");
  const stemEnd = dot > separator ? dot : normalized.length;
  return `${normalized.slice(0, stemEnd)}.${extension.replace(/^\./, "")}`;
}
