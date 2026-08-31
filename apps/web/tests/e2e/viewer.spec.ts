import { expect, test } from "@playwright/test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import type { AnalysisDocument } from "../../src/contracts";
import { validAnalysisDocument, validPhoto } from "../fixtures";

const tinyPng = readFileSync(resolve(process.cwd(), "../desktop/src-tauri/icons/32x32.png"));

function twoPhotoDocument(): AnalysisDocument {
  const document = validAnalysisDocument();
  document.photos[0]!.path = "a.jpg";
  document.photos[0]!.dimensions = { original: [32, 32], oriented: [32, 32] };
  const second = structuredClone(validPhoto());
  second.id = "photo-b";
  second.path = "b.jpg";
  second.dimensions = { original: [32, 32], oriented: [32, 32] };
  second.configurationId = "configuration-2";
  document.configurations.push({
    id: "configuration-2",
    deviceKey: "device-2",
    displayName: "Second test camera",
    photoCount: 1,
  });
  document.photos.push(second);
  document.groups = [
    {
      id: "group-1",
      anchorPhotoId: "photo-a",
      photoIds: ["photo-a", "photo-b"],
      evidence: {
        compatibleMemberPairs: 1,
        incompatibleMemberPairs: 0,
        unverifiedMemberPairs: 0,
      },
    },
  ];
  document.ungrouped = [];
  return document;
}

function labeledTstgrp2Document(): AnalysisDocument {
  const document = validAnalysisDocument();
  const baseMetadata = validPhoto().metadata;
  document.configurations = [
    {
      id: "oppo-find-x9-pro",
      deviceKey: "oppo-find-x9-pro",
      displayName: "OPPO Find X9 Pro",
      make: "OPPO",
      model: "OPPO Find X9 Pro",
      photoCount: 1,
    },
    {
      id: "pixel-9-pro-xl",
      deviceKey: "pixel-9-pro-xl",
      displayName: "Pixel 9 Pro XL",
      make: "Google",
      model: "Pixel 9 Pro XL",
      photoCount: 1,
    },
  ];
  document.photos = [
    {
      ...validPhoto(),
      id: "photo-9ef0aa041e40",
      path: "g3-20260822_115745_45.20000_7.20000.jpg",
      configurationId: "oppo-find-x9-pro",
      fingerprint: {
        size: 4_393_261,
        sha256: "830f029fbf0cfafbabbad7d983154a03d5c52184229a25a401faf4edd7176b83",
      },
      dimensions: { original: [4096, 3072], oriented: [4096, 3072] },
      metadata: {
        ...baseMetadata,
        cameraMake: "OPPO",
        cameraModel: "OPPO Find X9 Pro",
        focalLengthMm: 6.75,
        focalLength35mm: 23,
        effectiveFocalLength35mm: 23,
        focalSource: "exif35mm",
      },
    },
    {
      ...validPhoto(),
      id: "photo-5f450eab77ff",
      path: "g3-20260822_115835_45.20000_7.20000.jpg",
      configurationId: "pixel-9-pro-xl",
      fingerprint: {
        size: 2_905_922,
        sha256: "ee8235af02cb5a3241c4a7dba366590590e2b9853c37f40c148516ee9761e162",
      },
      dimensions: { original: [4080, 3072], oriented: [4080, 3072] },
      metadata: {
        ...baseMetadata,
        cameraMake: "Google",
        cameraModel: "Pixel 9 Pro XL",
        focalLengthMm: 6.9,
      },
    },
  ];
  document.groups = [
    {
      id: "group-5f450eab77ff",
      anchorPhotoId: "photo-5f450eab77ff",
      photoIds: ["photo-5f450eab77ff", "photo-9ef0aa041e40"],
      alignments: [
        {
          photoId: "photo-9ef0aa041e40",
          coordinateSystem: "normalized-oriented",
          direction: "photo-to-anchor",
          homography: [
            1.2667245008668513, -0.014563258878243251, -0.15071144061738606, 0.03998339260243537,
            1.216624112438394, -0.10954833968600704, 0.07630862688894346, -0.023453692034280372,
            0.9928648395976629,
          ],
          inverseHomography: [
            0.7818423053812829, 0.011671513473780858, 0.1199671600321492, -0.031171616208031508,
            0.8232332388412266, 0.08610025443034833, -0.060826408434967214, 0.018549575888625956,
            1.0000000000000002,
          ],
          confidence: 0.9323843772384418,
          provenance: "direct-pair",
        },
      ],
      evidence: {
        compatibleMemberPairs: 0,
        incompatibleMemberPairs: 0,
        unverifiedMemberPairs: 0,
        meanAnchorScore: 0.9323843772384418,
        minAnchorScore: 0.9323843772384418,
      },
    },
  ];
  document.ungrouped = [];
  return document;
}

test("two-photo groups preserve controls and expose an accessible comparison slider", async ({
  page,
}) => {
  await page.goto("./");
  const chooser = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: "Select files" }).click();
  await (
    await chooser
  ).setFiles([
    {
      name: "camcomp.json",
      mimeType: "application/json",
      buffer: Buffer.from(JSON.stringify(twoPhotoDocument())),
    },
    { name: "a.jpg", mimeType: "image/jpeg", buffer: tinyPng },
    { name: "b.jpg", mimeType: "image/jpeg", buffer: tinyPng },
  ]);

  await expect(page.getByRole("heading", { name: "Comparison group" })).toBeVisible();
  await expect(page.getByText("32 × 32")).toHaveCount(2);

  const workspace = await page.locator(".viewer-shell").boundingBox();
  const viewport = page.viewportSize();
  expect(workspace).not.toBeNull();
  expect(viewport).not.toBeNull();
  expect(workspace!.x).toBe(0);
  expect(workspace!.y).toBe(0);
  expect(workspace!.width).toBe(viewport!.width);
  expect(workspace!.height).toBe(viewport!.height);
  const frames = await page.locator(".photo-frame").evaluateAll((elements) =>
    elements.map((element) => {
      const rect = element.getBoundingClientRect();
      return { left: rect.left, right: rect.right };
    }),
  );
  expect(frames[0]!.right).toBe(frames[1]!.left);

  await expect(page.locator(".viewer-toolbar")).toHaveCSS("opacity", "0", { timeout: 3_000 });
  await expect(page.locator(".viewer-toolbar")).toHaveCSS("pointer-events", "none");
  const collapsedSidebar = await page.locator(".album-sidebar").boundingBox();
  expect(collapsedSidebar).not.toBeNull();
  expect(collapsedSidebar!.x + collapsedSidebar!.width).toBeLessThanOrEqual(12);
  await page.mouse.move(2, viewport!.height / 2);
  await expect(page.locator(".album-sidebar")).toHaveCSS("transform", "none");
  await page.mouse.move(viewport!.width / 2, viewport!.height / 2);
  await expect(page.locator(".viewer-toolbar")).toHaveCSS("opacity", "1");

  await page.getByRole("button", { name: "100%" }).click();
  await expect
    .poll(() =>
      page
        .locator(".photo-frame img")
        .first()
        .evaluate((image) =>
          Math.abs(
            (image.getBoundingClientRect().width * window.devicePixelRatio) /
              (image as HTMLImageElement).naturalWidth -
              1,
          ),
        ),
    )
    .toBeLessThanOrEqual(2 / 32 + 0.001);
  const scale100 = await page
    .locator(".photo-frame img")
    .evaluateAll((images) =>
      images.map(
        (image) =>
          (image.getBoundingClientRect().width * window.devicePixelRatio) /
          (image as HTMLImageElement).naturalWidth,
      ),
    );
  expect(scale100).toHaveLength(2);
  for (const scale of scale100) expect(Math.abs(scale - 1)).toBeLessThanOrEqual(2 / 32 + 0.001);

  await page.getByRole("button", { name: "200%" }).click();
  await expect
    .poll(() =>
      page
        .locator(".photo-frame img")
        .first()
        .evaluate((image) =>
          Math.abs(
            (image.getBoundingClientRect().width * window.devicePixelRatio) /
              (image as HTMLImageElement).naturalWidth -
              2,
          ),
        ),
    )
    .toBeLessThanOrEqual(2 / 32 + 0.001);
  const scale200 = await page
    .locator(".photo-frame img")
    .evaluateAll((images) =>
      images.map(
        (image) =>
          (image.getBoundingClientRect().width * window.devicePixelRatio) /
          (image as HTMLImageElement).naturalWidth,
      ),
    );
  for (const scale of scale200) expect(Math.abs(scale - 2)).toBeLessThanOrEqual(2 / 32 + 0.001);
  const stableWidths: number[] = [];
  for (let sample = 0; sample < 3; sample += 1) {
    stableWidths.push(
      await page
        .locator(".photo-frame img")
        .first()
        .evaluate((image) => image.getBoundingClientRect().width),
    );
    await page.waitForTimeout(200);
  }
  expect(new Set(stableWidths.map((width) => width.toFixed(3))).size).toBe(1);

  await page.getByRole("button", { name: "Comparison slider" }).click();
  const divider = page.getByRole("slider", { name: /comparison divider/i });
  await expect(divider).toBeVisible();
  await divider.press("ArrowRight");
  await expect(divider).toHaveValue("51");
  await page.getByRole("button", { name: "Grid view" }).click();
  await expect(page.getByRole("button", { name: "Comparison slider" })).toBeVisible();

  await page.getByLabel("Navigation").selectOption("aligned");
  await expect(page.locator(".mode-note")).toContainText("no local correspondence refinement");
  await expect(page.getByText(/normalized fallback/i)).toBeVisible();
});

test("renders the labeled tstgrp2 g3 comparison with real photos and alignment", async ({
  page,
  browserName,
}) => {
  test.skip(browserName !== "chromium", "The full-resolution reference smoke test runs once.");
  await page.goto("./");
  const chooser = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: "Select files" }).click();
  await (
    await chooser
  ).setFiles([
    {
      name: "camcomp.json",
      mimeType: "application/json",
      buffer: Buffer.from(JSON.stringify(labeledTstgrp2Document())),
    },
    {
      name: "g3-20260822_115745_45.20000_7.20000.jpg",
      mimeType: "image/jpeg",
      buffer: readFileSync(
        resolve(process.cwd(), "../../test/tstgrp2/g3-20260822_115745_45.20000_7.20000.jpg"),
      ),
    },
    {
      name: "g3-20260822_115835_45.20000_7.20000.jpg",
      mimeType: "image/jpeg",
      buffer: readFileSync(
        resolve(process.cwd(), "../../test/tstgrp2/g3-20260822_115835_45.20000_7.20000.jpg"),
      ),
    },
  ]);

  await expect(page.getByRole("heading", { name: "Comparison group" })).toBeVisible();
  await expect(page.locator(".photo-frame img")).toHaveCount(2);
  await expect(page.locator(".photo-frame canvas")).toHaveCount(0);
  await expect(page.getByText("4096 × 3072")).toBeVisible();
  await expect(page.getByText("4080 × 3072")).toBeVisible();
  await expect(page.getByText("23 mm equivalent")).toBeVisible();
  await expect(page.getByText(/direct-pair · 93%/i)).toBeVisible();
  await expect(page.getByText(/missing source/i)).toHaveCount(0);

  await page.getByLabel("Navigation").selectOption("aligned");
  await expect(page.getByText(/direct-pair · 93%/i)).toBeVisible();
  await page.getByRole("button", { name: "Comparison slider" }).click();
  await expect(page.getByRole("slider", { name: /comparison divider/i })).toBeVisible();
});
