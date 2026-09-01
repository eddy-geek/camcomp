import { describe, expect, it, vi } from "vitest";
import type { Group } from "../src/contracts/camcomp.generated";
import {
  applyHomography,
  GlobalHomographyPointMapper,
  NavigationController,
} from "../src/viewer/navigation";
import type { ViewerAdapter, ViewerInteraction } from "../src/viewer/types";

const identity = [1, 0, 0, 0, 1, 0, 0, 0, 1];
const group: Group = {
  id: "g",
  anchorPhotoId: "a",
  photoIds: ["a", "b", "c", "missing"],
  evidence: {
    compatibleMemberPairs: 3,
    incompatibleMemberPairs: 0,
    unverifiedMemberPairs: 1,
  },
  alignments: [
    {
      photoId: "b",
      coordinateSystem: "normalized-oriented",
      direction: "photo-to-anchor",
      homography: [1, 0, 0.1, 0, 1, 0, 0, 0, 1],
      inverseHomography: [1, 0, -0.1, 0, 1, 0, 0, 0, 1],
      confidence: 0.9,
      provenance: "direct-pair",
    },
    {
      photoId: "c",
      coordinateSystem: "normalized-oriented",
      direction: "photo-to-anchor",
      homography: [1, 0, -0.2, 0, 1, 0, 0, 0, 1],
      inverseHomography: [1, 0, 0.2, 0, 1, 0, 0, 0, 1],
      confidence: 0.8,
      provenance: "composed",
    },
  ],
};

describe("homography navigation", () => {
  it("applies row-major projective matrices", () => {
    expect(applyHomography(identity, [0.3, 0.7])).toEqual([0.3, 0.7]);
    expect(applyHomography([1, 0, 0.2, 0, 1, -0.1, 0, 0, 1], [0.3, 0.7])).toEqual([0.5, 0.6]);
    expect(applyHomography([1, 0, 0, 0, 1, 0, 0, 0, 0], [0.3, 0.7])).toBeNull();
  });

  it("maps non-anchor members through anchor space", () => {
    const mapper = new GlobalHomographyPointMapper(group);
    const mapped = mapper.map("b", "c", [0.2, 0.4]);
    expect(mapped.point[0]).toBeCloseTo(0.5);
    expect(mapped.point[1]).toBeCloseTo(0.4);
    expect(mapped.provenance).toBe("composed");
  });

  it("falls back per target when an alignment is unavailable", () => {
    const mapper = new GlobalHomographyPointMapper(group);
    expect(mapper.map("a", "missing", [0.4, 0.6])).toMatchObject({
      point: [0.4, 0.6],
      fallback: true,
      provenance: "normalized",
    });
  });
});

class FakeAdapter implements ViewerAdapter {
  readonly applied = vi.fn();
  #handler?: (interaction: ViewerInteraction) => void;
  open = vi.fn(async () => undefined);
  viewport = vi.fn<() => ViewerInteraction | null>(() => null);
  applyViewport = this.applied;
  applyPixelRatio = vi.fn();
  fit = vi.fn();
  zoomBy = vi.fn();
  onInteraction(handler: (interaction: ViewerInteraction) => void): () => void {
    this.#handler = handler;
    return () => {
      this.#handler = undefined;
    };
  }
  destroy = vi.fn();
  interact(interaction: ViewerInteraction): void {
    this.#handler?.(interaction);
  }
}

describe("navigation controller", () => {
  it("coalesces one leader interaction and does not write back to the leader", async () => {
    const controller = new NavigationController(group);
    const leader = new FakeAdapter();
    const peer = new FakeAdapter();
    controller.add("a", leader);
    controller.add("b", peer);
    leader.interact({
      center: [0.3, 0.4],
      span: [0.5, 0.6],
      screenPixelsPerImagePixel: 0.5,
    });
    await new Promise((resolve) => requestAnimationFrame(resolve));
    expect(leader.applied).not.toHaveBeenCalled();
    expect(peer.applied).toHaveBeenCalledTimes(1);
    expect(peer.applied).toHaveBeenCalledWith([0.3, 0.4], [0.5, 0.6]);
    controller.destroy();
  });

  it("restores canonical navigation when viewers remount and fits the leader", async () => {
    const controller = new NavigationController(group);
    const leader = new FakeAdapter();
    const peer = new FakeAdapter();
    controller.add("a", leader);
    controller.add("b", peer);
    leader.interact({
      center: [0.25, 0.75],
      span: [0.4, 0.6],
      screenPixelsPerImagePixel: 0.5,
    });
    await new Promise((resolve) => requestAnimationFrame(resolve));

    controller.remove("b");
    const remountedPeer = new FakeAdapter();
    controller.add("b", remountedPeer);
    expect(remountedPeer.applied).toHaveBeenCalledWith([0.25, 0.75], [0.4, 0.6]);

    controller.fit();
    await new Promise((resolve) => requestAnimationFrame(resolve));
    expect(leader.applied).toHaveBeenCalledWith([0.5, 0.5], [1, 1]);
    expect(remountedPeer.applied).toHaveBeenLastCalledWith([0.5, 0.5], [1, 1]);
    controller.destroy();
  });

  it("pans every viewer by a fraction of the visible leader viewport", async () => {
    const controller = new NavigationController(group);
    const leader = new FakeAdapter();
    const peer = new FakeAdapter();
    controller.add("a", leader);
    controller.add("b", peer);
    leader.interact({
      center: [0.25, 0.75],
      span: [0.4, 0.6],
      screenPixelsPerImagePixel: 0.5,
    });
    await new Promise((resolve) => requestAnimationFrame(resolve));
    leader.viewport.mockReturnValue({
      center: [0.25, 0.75],
      span: [0.4, 0.6],
      screenPixelsPerImagePixel: 0.5,
    });
    leader.applied.mockClear();
    peer.applied.mockClear();

    controller.panBy(0.1, -0.2);
    await new Promise((resolve) => requestAnimationFrame(resolve));
    expect(controller.state.center[0]).toBeCloseTo(0.29);
    expect(controller.state.center[1]).toBeCloseTo(0.63);
    const expectedCenter = [expect.closeTo(0.29), expect.closeTo(0.63)];
    expect(leader.applied).toHaveBeenCalledWith(expectedCenter, [0.4, 0.6]);
    expect(peer.applied).toHaveBeenCalledWith(expectedCenter, [0.4, 0.6]);
    controller.destroy();
  });

  it("clears a removed leader before synchronizing remaining viewers", async () => {
    const controller = new NavigationController(group);
    const leader = new FakeAdapter();
    const peer = new FakeAdapter();
    controller.add("a", leader);
    controller.add("b", peer);
    leader.interact({
      center: [0.25, 0.75],
      span: [0.4, 0.6],
      screenPixelsPerImagePixel: 0.5,
    });
    await new Promise((resolve) => requestAnimationFrame(resolve));

    controller.remove("a");
    expect(controller.state.leaderPhotoId).toBeNull();
    controller.zoomToPixelRatio(1);
    await new Promise((resolve) => requestAnimationFrame(resolve));
    expect(peer.applyPixelRatio).toHaveBeenLastCalledWith([0.25, 0.75], 1);
    controller.destroy();
  });

  it("preserves the center and applies exact pixel ratios across remounts", async () => {
    const controller = new NavigationController(group);
    const leader = new FakeAdapter();
    const peer = new FakeAdapter();
    controller.add("a", leader);
    controller.add("b", peer);
    leader.interact({
      center: [0.25, 0.75],
      span: [0.4, 0.6],
      screenPixelsPerImagePixel: 0.5,
    });
    await new Promise((resolve) => requestAnimationFrame(resolve));

    controller.zoomToPixelRatio(1);
    await new Promise((resolve) => requestAnimationFrame(resolve));
    expect(controller.state.center).toEqual([0.25, 0.75]);
    expect(leader.applyPixelRatio).toHaveBeenLastCalledWith([0.25, 0.75], 1);
    expect(peer.applyPixelRatio).toHaveBeenLastCalledWith([0.25, 0.75], 1);

    controller.remove("b");
    const remountedPeer = new FakeAdapter();
    controller.add("b", remountedPeer);
    expect(remountedPeer.applyPixelRatio).toHaveBeenCalledWith([0.25, 0.75], 1);
    controller.destroy();
  });

  it("keeps pixel mode while synchronizing a user zoom", async () => {
    const controller = new NavigationController(group);
    const leader = new FakeAdapter();
    const peer = new FakeAdapter();
    controller.add("a", leader);
    controller.add("b", peer);
    controller.zoomToPixelRatio(2);
    await new Promise((resolve) => requestAnimationFrame(resolve));
    await new Promise((resolve) => requestAnimationFrame(resolve));

    leader.interact({
      center: [0.4, 0.6],
      span: [0.2, 0.3],
      screenPixelsPerImagePixel: 2.5,
    });
    await new Promise((resolve) => requestAnimationFrame(resolve));
    expect(peer.applyPixelRatio).toHaveBeenLastCalledWith([0.4, 0.6], 2.5);
    expect(controller.state.scale).toEqual({
      kind: "pixel",
      screenPixelsPerImagePixel: 2.5,
    });
    controller.destroy();
  });
});
