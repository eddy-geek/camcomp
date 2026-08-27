import type { ErrorObject, ValidateFunction } from "ajv";

import { normalizeRelativePath, RelativePathError } from "../sources/paths";
import type { Alignment, AnalysisDocument, Group, PairResult } from "./camcomp.generated";
import generatedValidator from "./camcomp.validator.generated";

export const SUPPORTED_SCHEMA_MAJOR = 1;

export type AnalysisValidationErrorCode =
  | "malformed-document"
  | "missing-schema-version"
  | "invalid-schema-version"
  | "unsupported-schema-major"
  | "schema-validation"
  | "semantic-validation";

export interface AnalysisValidationIssue {
  readonly path: string;
  readonly message: string;
}

export class AnalysisValidationError extends Error {
  readonly code: AnalysisValidationErrorCode;
  readonly issues: readonly AnalysisValidationIssue[];

  constructor(
    code: AnalysisValidationErrorCode,
    message: string,
    issues: readonly AnalysisValidationIssue[] = [],
  ) {
    super(message);
    this.name = "AnalysisValidationError";
    this.code = code;
    this.issues = Object.freeze([...issues]);
  }
}

const validateSchema = generatedValidator as unknown as ValidateFunction<AnalysisDocument>;

export function parseAnalysisDocumentJson(json: string): AnalysisDocument {
  let parsed: unknown;
  try {
    parsed = JSON.parse(json) as unknown;
  } catch {
    throw new AnalysisValidationError("malformed-document", "The analysis is not valid JSON.");
  }
  return validateAnalysisDocument(parsed);
}

export function validateAnalysisDocument(value: unknown): AnalysisDocument {
  validateSchemaVersion(value);
  if (!validateSchema(value)) {
    const issues = schemaIssues(validateSchema.errors);
    throw new AnalysisValidationError(
      "schema-validation",
      `The analysis does not match the camcomp schema${issueSuffix(issues)}.`,
      issues,
    );
  }
  const issues = collectSemanticIssues(value);
  if (issues.length > 0) {
    throw new AnalysisValidationError(
      "semantic-validation",
      `The analysis is internally inconsistent${issueSuffix(issues)}.`,
      issues,
    );
  }
  return value;
}

function validateSchemaVersion(value: unknown): void {
  if (!isRecord(value) || !("schemaVersion" in value)) {
    throw new AnalysisValidationError(
      "missing-schema-version",
      "The analysis does not declare schemaVersion.",
    );
  }
  if (typeof value.schemaVersion !== "string") {
    throw new AnalysisValidationError(
      "invalid-schema-version",
      "The analysis schemaVersion must use major.minor form.",
    );
  }
  const match = /^(0|[1-9]\d*)\.(0|[1-9]\d*)$/.exec(value.schemaVersion);
  if (!match) {
    throw new AnalysisValidationError(
      "invalid-schema-version",
      `The analysis schemaVersion “${value.schemaVersion}” must use major.minor form.`,
    );
  }
  const major = Number(match[1]);
  if (major !== SUPPORTED_SCHEMA_MAJOR) {
    throw new AnalysisValidationError(
      "unsupported-schema-major",
      `The analysis uses schema major ${major}; this build supports major ${SUPPORTED_SCHEMA_MAJOR}.`,
    );
  }
}

function schemaIssues(errors: ErrorObject[] | null | undefined): AnalysisValidationIssue[] {
  return (errors ?? []).map((error) => ({
    path: error.instancePath || "/",
    message: error.message ?? error.keyword,
  }));
}

function issueSuffix(issues: readonly AnalysisValidationIssue[]): string {
  const first = issues[0];
  return first ? ` at ${first.path}: ${first.message}` : "";
}

function collectSemanticIssues(document: AnalysisDocument): AnalysisValidationIssue[] {
  const issues: AnalysisValidationIssue[] = [];
  findNonFinite(document, "", issues);

  const configurations = uniqueIndex(document.configurations, "configuration", issues);
  const photos = uniqueIndex(document.photos, "photo", issues);
  const sessions = uniqueIndex(document.sessions, "session", issues);
  uniqueIndex(document.groups, "group", issues);
  uniqueIndex(document.pairs, "pair", issues);
  uniqueIndex(document.manualOverrides ?? [], "manual override", issues);

  const normalizedPaths = new Map<string, string>();
  for (const [index, photo] of document.photos.entries()) {
    if (!configurations.has(photo.configurationId)) {
      addIssue(
        issues,
        `/photos/${index}/configurationId`,
        `unknown configuration “${photo.configurationId}”`,
      );
    }
    const normalized = validateRelativePath(photo.path);
    if (typeof normalized === "string") {
      const previous = normalizedPaths.get(normalized);
      if (previous !== undefined) {
        addIssue(
          issues,
          `/photos/${index}/path`,
          `duplicates normalized path “${normalized}” used by photo “${previous}”`,
        );
      } else {
        normalizedPaths.set(normalized, photo.id);
      }
    } else {
      addIssue(issues, `/photos/${index}/path`, normalized.message);
    }
  }

  const sessionMembership = new Map<string, string>();
  for (const [sessionIndex, session] of document.sessions.entries()) {
    const local = new Set<string>();
    for (const [photoIndex, photoId] of session.photoIds.entries()) {
      const path = `/sessions/${sessionIndex}/photoIds/${photoIndex}`;
      if (!photos.has(photoId)) addIssue(issues, path, `unknown photo “${photoId}”`);
      if (local.has(photoId)) addIssue(issues, path, `duplicate session member “${photoId}”`);
      local.add(photoId);
      const previous = sessionMembership.get(photoId);
      if (previous !== undefined && previous !== session.id) {
        addIssue(issues, path, `photo “${photoId}” also belongs to session “${previous}”`);
      } else {
        sessionMembership.set(photoId, session.id);
      }
    }
  }

  for (const [pairIndex, pair] of document.pairs.entries()) {
    validatePair(pair, pairIndex, photos, sessions, sessionMembership, issues);
  }

  const partitionCounts = new Map<string, number>();
  for (const photoId of photos.keys()) partitionCounts.set(photoId, 0);
  for (const [groupIndex, group] of document.groups.entries()) {
    validateGroup(group, groupIndex, photos, partitionCounts, issues);
  }
  for (const [index, entry] of document.ungrouped.entries()) {
    const path = `/ungrouped/${index}/photoId`;
    if (!photos.has(entry.photoId)) {
      addIssue(issues, path, `unknown photo “${entry.photoId}”`);
    } else {
      partitionCounts.set(entry.photoId, (partitionCounts.get(entry.photoId) ?? 0) + 1);
    }
  }
  for (const [photoId, count] of partitionCounts) {
    if (count === 0) addIssue(issues, "/photos", `photo “${photoId}” is not grouped or ungrouped`);
    if (count > 1)
      addIssue(
        issues,
        "/photos",
        `photo “${photoId}” appears in the group partition ${count} times`,
      );
  }

  return issues;
}

function uniqueIndex<T extends { id: string }>(
  values: readonly T[],
  label: string,
  issues: AnalysisValidationIssue[],
): Map<string, T> {
  const result = new Map<string, T>();
  for (const [index, value] of values.entries()) {
    if (value.id.length === 0)
      addIssue(issues, `/${collectionName(label)}/${index}/id`, `${label} ID is empty`);
    if (result.has(value.id)) {
      addIssue(
        issues,
        `/${collectionName(label)}/${index}/id`,
        `duplicate ${label} ID “${value.id}”`,
      );
    } else {
      result.set(value.id, value);
    }
  }
  return result;
}

function collectionName(label: string): string {
  if (label === "manual override") return "manualOverrides";
  if (label === "configuration") return "configurations";
  return `${label}s`;
}

function validatePair(
  pair: PairResult,
  index: number,
  photos: ReadonlyMap<string, unknown>,
  sessions: ReadonlyMap<string, unknown>,
  sessionMembership: ReadonlyMap<string, string>,
  issues: AnalysisValidationIssue[],
): void {
  const base = `/pairs/${index}`;
  if (!photos.has(pair.a)) addIssue(issues, `${base}/a`, `unknown photo “${pair.a}”`);
  if (!photos.has(pair.b)) addIssue(issues, `${base}/b`, `unknown photo “${pair.b}”`);
  if (pair.a === pair.b) addIssue(issues, base, "a pair must contain two different photos");
  if (pair.a > pair.b) addIssue(issues, base, "pair photo IDs are not in canonical order");
  const expectedId = `pair-${pair.a}-${pair.b}`;
  if (pair.id !== expectedId)
    addIssue(issues, `${base}/id`, `expected canonical ID “${expectedId}”`);
  if (!sessions.has(pair.sessionId)) {
    addIssue(issues, `${base}/sessionId`, `unknown session “${pair.sessionId}”`);
  }
  for (const photoId of [pair.a, pair.b]) {
    const actual = sessionMembership.get(photoId);
    if (actual === undefined) {
      addIssue(issues, `${base}/sessionId`, `photo “${photoId}” is not a member of any session`);
    } else if (actual !== pair.sessionId) {
      addIssue(issues, `${base}/sessionId`, `photo “${photoId}” belongs to session “${actual}”`);
    }
  }
  if (pair.homographyAToB != null)
    validateMatrix(pair.homographyAToB, `${base}/homographyAToB`, issues);
}

function validateGroup(
  group: Group,
  index: number,
  photos: ReadonlyMap<string, unknown>,
  partitionCounts: Map<string, number>,
  issues: AnalysisValidationIssue[],
): void {
  const base = `/groups/${index}`;
  if (group.photoIds.length < 2)
    addIssue(issues, `${base}/photoIds`, "a group must contain at least two photos");
  const members = new Set<string>();
  for (const [memberIndex, photoId] of group.photoIds.entries()) {
    const path = `${base}/photoIds/${memberIndex}`;
    if (!photos.has(photoId)) {
      addIssue(issues, path, `unknown photo “${photoId}”`);
    } else {
      partitionCounts.set(photoId, (partitionCounts.get(photoId) ?? 0) + 1);
    }
    if (members.has(photoId)) addIssue(issues, path, `duplicate group member “${photoId}”`);
    members.add(photoId);
  }
  if (!members.has(group.anchorPhotoId)) {
    addIssue(issues, `${base}/anchorPhotoId`, `anchor “${group.anchorPhotoId}” is not a member`);
  } else if (group.photoIds[0] !== group.anchorPhotoId) {
    addIssue(issues, `${base}/photoIds/0`, "the anchor must be the first group member");
  }

  const aligned = new Set<string>();
  for (const [alignmentIndex, alignment] of (group.alignments ?? []).entries()) {
    const path = `${base}/alignments/${alignmentIndex}`;
    if (!members.has(alignment.photoId))
      addIssue(issues, `${path}/photoId`, `alignment photo “${alignment.photoId}” is not a member`);
    if (alignment.photoId === group.anchorPhotoId)
      addIssue(issues, `${path}/photoId`, "the anchor must not have an alignment");
    if (aligned.has(alignment.photoId))
      addIssue(issues, `${path}/photoId`, `duplicate alignment for “${alignment.photoId}”`);
    aligned.add(alignment.photoId);
    validateAlignment(alignment, path, issues);
  }
}

function validateAlignment(
  alignment: Alignment,
  path: string,
  issues: AnalysisValidationIssue[],
): void {
  const h = validateMatrix(alignment.homography, `${path}/homography`, issues);
  const inverse = validateMatrix(alignment.inverseHomography, `${path}/inverseHomography`, issues);
  if (!h || !inverse) return;
  const probe: readonly [number, number] = [0.37, 0.61];
  const mapped = applyHomography(h, probe);
  const roundTrip = mapped && applyHomography(inverse, mapped);
  if (!roundTrip) {
    addIssue(issues, path, `alignment for “${alignment.photoId}” maps to infinity`);
    return;
  }
  const residual = Math.hypot(roundTrip[0] - probe[0], roundTrip[1] - probe[1]);
  if (residual > 1e-6) {
    addIssue(
      issues,
      `${path}/inverseHomography`,
      `does not invert the homography (residual ${residual.toExponential(3)})`,
    );
  }
}

function validateMatrix(
  matrix: readonly number[],
  path: string,
  issues: AnalysisValidationIssue[],
): readonly number[] | undefined {
  if (matrix.length !== 9) {
    addIssue(issues, path, "must contain exactly 9 row-major values");
    return undefined;
  }
  if (!matrix.every(Number.isFinite)) {
    addIssue(issues, path, "contains a non-finite value");
    return undefined;
  }
  return matrix;
}

function applyHomography(
  matrix: readonly number[],
  point: readonly [number, number],
): readonly [number, number] | undefined {
  const w = matrix[6]! * point[0] + matrix[7]! * point[1] + matrix[8]!;
  if (!Number.isFinite(w) || Math.abs(w) < 1e-12) return undefined;
  const x = (matrix[0]! * point[0] + matrix[1]! * point[1] + matrix[2]!) / w;
  const y = (matrix[3]! * point[0] + matrix[4]! * point[1] + matrix[5]!) / w;
  return Number.isFinite(x) && Number.isFinite(y) ? [x, y] : undefined;
}

function findNonFinite(value: unknown, path: string, issues: AnalysisValidationIssue[]): void {
  if (typeof value === "number" && !Number.isFinite(value)) {
    addIssue(issues, path || "/", "number is not finite");
    return;
  }
  if (Array.isArray(value)) {
    value.forEach((item, index) => findNonFinite(item, `${path}/${index}`, issues));
    return;
  }
  if (isRecord(value)) {
    for (const [key, item] of Object.entries(value))
      findNonFinite(item, `${path}/${escapePointer(key)}`, issues);
  }
}

function validateRelativePath(path: string): string | AnalysisValidationIssue {
  try {
    return normalizeRelativePath(path);
  } catch (error) {
    if (error instanceof RelativePathError) return { path: "", message: error.message };
    throw error;
  }
}

function addIssue(issues: AnalysisValidationIssue[], path: string, message: string): void {
  issues.push({ path, message });
}

function escapePointer(value: string): string {
  return value.replaceAll("~", "~0").replaceAll("/", "~1");
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
