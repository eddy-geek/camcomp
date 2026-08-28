import type { AnalysisDocument, Photo } from "../src/contracts";

export function validAnalysisDocument(): AnalysisDocument {
  return {
    schemaVersion: "1.0",
    application: { name: "camcomp", version: "0.1.0" },
    algorithm: {
      name: "test",
      version: "1.0.0",
      thresholdsProvisional: true,
      parameters: {
        sessionGapSeconds: 60,
        fieldOfViewTolerance: 0.15,
        compatibilityThreshold: 0.8,
        analysisLongestEdge: 1200,
        maxFeaturesPerPhoto: 4000,
        loweRatio: 0.75,
        mutualMatch: true,
        ransacThresholdPx: 3,
        ransacMaxIterations: 2000,
        ransacSeed: 42,
        maxCorrespondences: 200,
        minKeypointsForDecision: 150,
        minMatchesForDecision: 12,
        maxMegapixels: 120,
      },
      weights: {
        geometricOverlap: 0.35,
        featureCoverage: 0.25,
        inlierConfidence: 0.2,
        alignedAppearanceSimilarity: 0.2,
      },
      gates: {
        minInliers: 25,
        maxMedianReprojectionErrorPx: 3,
        gridSize: 4,
        minOccupiedCells: 5,
        minGeometricOverlap: 0.6,
        minScale: 0.2,
        maxScale: 5,
        maxPerspective: 0.6,
      },
    },
    source: {},
    configurations: [
      {
        id: "configuration-1",
        deviceKey: "device-1",
        displayName: "Test camera",
        photoCount: 1,
      },
    ],
    photos: [validPhoto()],
    sessions: [],
    pairs: [],
    groups: [],
    ungrouped: [{ photoId: "photo-a", reason: "no-session-peer" }],
  };
}

export function validPhoto(): Photo {
  return {
    id: "photo-a",
    path: "photos/a.jpg",
    configurationId: "configuration-1",
    fingerprint: { size: 3, sha256: "0".repeat(64) },
    dimensions: { original: [4, 3], oriented: [4, 3] },
    metadata: {
      captureTimeSource: "missing",
      timezoneAssumed: false,
      appliedTimeOffsetSeconds: 0,
      exifOrientation: 1,
      orientationAppliedByDecoder: false,
      focalSource: "unknown",
    },
  };
}
