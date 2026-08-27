import { describe, expect, it } from "vitest";
import { chooseLayout, gridColumns } from "../src/viewer/layout";

describe("adaptive group layout", () => {
  it.each([
    [2, 1200, 700, "columns"],
    [3, 700, 1200, "rows"],
    [4, 1200, 700, "quad"],
    [5, 1200, 700, "grid"],
  ] as const)("chooses %s-photo layout", (count, width, height, expected) => {
    expect(chooseLayout(count, width, height)).toBe(expected);
  });

  it("uses deterministic near-square generic grids", () => {
    expect(gridColumns(5)).toBe(3);
    expect(gridColumns(9)).toBe(3);
  });
});
