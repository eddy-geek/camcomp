import { describe, expect, it } from "vitest";

import {
  AnalysisValidationError,
  normalizeAnalysisAlbum,
  parseAnalysisDocumentJson,
  validateAnalysisDocument,
} from "../src/contracts";
import { validAnalysisDocument } from "./fixtures";

function expectValidationCode(action: () => unknown, code: AnalysisValidationError["code"]): void {
  try {
    action();
    expect.fail("expected validation to fail");
  } catch (error) {
    expect(error).toBeInstanceOf(AnalysisValidationError);
    expect((error as AnalysisValidationError).code).toBe(code);
  }
}

describe("analysis contract validation", () => {
  it("returns stable parse and version errors", () => {
    expectValidationCode(() => parseAnalysisDocumentJson("{"), "malformed-document");
    expectValidationCode(() => validateAnalysisDocument({}), "missing-schema-version");
    expectValidationCode(
      () => validateAnalysisDocument({ schemaVersion: "one" }),
      "invalid-schema-version",
    );
    expectValidationCode(
      () => validateAnalysisDocument({ schemaVersion: "2.0" }),
      "unsupported-schema-major",
    );
  });

  it("accepts a future compatible minor and unknown fields", () => {
    const document = validAnalysisDocument();
    document.schemaVersion = "1.27";
    document.futureField = { kept: true };
    document.photos[0]!.futurePhotoField = "future";
    expect(validateAnalysisDocument(document)).toBe(document);
  });

  it("rejects duplicate IDs, invalid partitions, and bad inverses", () => {
    const duplicate = validAnalysisDocument();
    duplicate.photos.push({ ...duplicate.photos[0]!, path: "photos/b.jpg" });
    expectValidationCode(() => validateAnalysisDocument(duplicate), "semantic-validation");

    const missing = validAnalysisDocument();
    missing.ungrouped = [];
    expectValidationCode(() => validateAnalysisDocument(missing), "semantic-validation");

    const badInverse = validAnalysisDocument();
    const second = { ...badInverse.photos[0]!, id: "photo-b", path: "photos/b.jpg" };
    badInverse.photos.push(second);
    badInverse.configurations[0]!.photoCount = 2;
    badInverse.ungrouped = [];
    badInverse.groups = [
      {
        id: "group-1",
        anchorPhotoId: "photo-a",
        photoIds: ["photo-a", "photo-b"],
        evidence: {
          compatibleMemberPairs: 1,
          incompatibleMemberPairs: 0,
          unverifiedMemberPairs: 0,
        },
        alignments: [
          {
            photoId: "photo-b",
            coordinateSystem: "normalized-oriented",
            direction: "photo-to-anchor",
            homography: [1, 0, 0, 0, 1, 0, 0, 0, 1],
            inverseHomography: [2, 0, 0, 0, 2, 0, 0, 0, 1],
            confidence: 1,
            provenance: "direct-pair",
          },
        ],
      },
    ];
    expectValidationCode(() => validateAnalysisDocument(badInverse), "semantic-validation");
  });

  it("clones, freezes, and indexes without mutating parsed data", () => {
    const document = validAnalysisDocument();
    const album = normalizeAnalysisAlbum(document);
    expect(album.document).not.toBe(document);
    expect(Object.isFrozen(album.document.photos)).toBe(true);
    expect(album.photosById.get("photo-a")?.path).toBe("photos/a.jpg");
    document.photos[0]!.path = "changed.jpg";
    expect(album.photosById.get("photo-a")?.path).toBe("photos/a.jpg");
    expect("set" in album.photosById).toBe(false);
  });
});
