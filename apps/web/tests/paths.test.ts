import { describe, expect, it } from "vitest";

import {
  indexUniqueRelativePaths,
  normalizeRelativePath,
  RelativePathError,
  stripSingleDirectoryRoot,
} from "../src/sources";

describe("safe relative paths", () => {
  it("normalizes separators and Unicode to NFC", () => {
    expect(normalizeRelativePath("camera\\nested\\cafe\u0301.jpg")).toBe("camera/nested/café.jpg");
  });

  it.each(["../a.jpg", "a/../b.jpg", "/a.jpg", "C:\\a.jpg", "a//b.jpg", "./a.jpg"])(
    "rejects unsafe path %s",
    (path) => expect(() => normalizeRelativePath(path)).toThrow(RelativePathError),
  );

  it("rejects duplicate normalized keys", () => {
    expect(() =>
      indexUniqueRelativePaths([
        ["café.jpg", 1],
        ["cafe\u0301.jpg", 2],
      ]),
    ).toThrowError(expect.objectContaining({ code: "duplicate-path" }));
  });

  it("strips exactly one unambiguous recursive-input root", () => {
    expect(stripSingleDirectoryRoot(["album/camcomp.json", "album/sub/a.jpg"])).toEqual([
      "camcomp.json",
      "sub/a.jpg",
    ]);
    expect(() => stripSingleDirectoryRoot(["one/a.jpg", "two/b.jpg"])).toThrowError(
      expect.objectContaining({ code: "ambiguous-root" }),
    );
  });
});
