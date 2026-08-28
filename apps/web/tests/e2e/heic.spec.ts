import { expect, test } from "@playwright/test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import type { AnalysisDocument } from "../../src/contracts";
import { validAnalysisDocument, validPhoto } from "../fixtures";

function heicComparisonDocument(): AnalysisDocument {
  const document = validAnalysisDocument();
  const metadata = validPhoto().metadata;
  document.configurations = [
    {
      id: "oppo-find-x9-pro--auto",
      deviceKey: "oppo-find-x9-pro",
      displayName: "OPPO Find X9 Pro (auto)",
      photoCount: 1,
    },
    {
      id: "pixel-9-pro-xl--hdr-plus",
      deviceKey: "pixel-9-pro-xl",
      displayName: "Pixel 9 Pro XL (hdr-plus)",
      photoCount: 1,
    },
  ];
  document.photos = [
    {
      ...validPhoto(),
      id: "photo-636956fac038",
      path: "g1-oppo-ai-IMG20260822115355.heic",
      configurationId: "oppo-find-x9-pro--auto",
      fingerprint: { size: 0, sha256: "0".repeat(64) },
      dimensions: { original: [4096, 3072], oriented: [4096, 3072] },
      metadata: {
        ...metadata,
        cameraMake: "OPPO",
        cameraModel: "OPPO Find X9 Pro",
        focalLengthMm: 16.53,
        focalLength35mm: 140,
        effectiveFocalLength35mm: 140,
        focalSource: "exif35mm",
        orientationAppliedByDecoder: true,
      },
    },
    {
      ...validPhoto(),
      id: "photo-cbec07d0231e",
      path: "g1-pxl-PXL_20260822_095351447.jpg",
      configurationId: "pixel-9-pro-xl--hdr-plus",
      fingerprint: { size: 0, sha256: "0".repeat(64) },
      dimensions: { original: [4080, 3072], oriented: [4080, 3072] },
      metadata: {
        ...metadata,
        cameraMake: "Google",
        cameraModel: "Pixel 9 Pro XL",
        focalLengthMm: 17.906,
        focalLength35mm: 110,
        effectiveFocalLength35mm: 122.1,
        focalSource: "exif35mm-zoom-applied",
      },
    },
  ];
  document.groups = [
    {
      id: "group-heic-worker",
      anchorPhotoId: "photo-cbec07d0231e",
      photoIds: ["photo-cbec07d0231e", "photo-636956fac038"],
      evidence: {
        compatibleMemberPairs: 0,
        incompatibleMemberPairs: 0,
        unverifiedMemberPairs: 1,
      },
    },
  ];
  document.ungrouped = [];
  return document;
}

test("warns before decoding a real tstgrp3 HEIC with the high-memory worker", async ({
  page,
  browserName,
}) => {
  test.skip(browserName !== "chromium", "The real HEIC worker smoke test runs once.");
  test.setTimeout(120_000);
  const workerRequests: string[] = [];
  page.on("request", (request) => {
    if (/heic-decoder\.worker-.*\.js/.test(request.url())) workerRequests.push(request.url());
  });
  await page.goto("./");
  const chooser = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: "Select files" }).click();
  await (
    await chooser
  ).setFiles([
    {
      name: "camcomp.json",
      mimeType: "application/json",
      buffer: Buffer.from(JSON.stringify(heicComparisonDocument())),
    },
    {
      name: "g1-oppo-ai-IMG20260822115355.heic",
      mimeType: "image/heic",
      buffer: readFileSync(
        resolve(process.cwd(), "../../test/tstgrp3/g1-oppo-ai-IMG20260822115355.heic"),
      ),
    },
    {
      name: "g1-pxl-PXL_20260822_095351447.jpg",
      mimeType: "image/jpeg",
      buffer: readFileSync(
        resolve(process.cwd(), "../../test/tstgrp3/g1-pxl-PXL_20260822_095351447.jpg"),
      ),
    },
  ]);

  await expect(page.getByText(/2,120 MiB peak RAM/i)).toBeVisible();
  expect(workerRequests).toEqual([]);
  await page.getByRole("button", { name: "Decode HEIC anyway" }).click();
  await expect(page.getByText("Decoded copy")).toBeVisible({ timeout: 90_000 });
  await expect(page.getByText(/high-memory HEIC worker enabled/i)).toBeVisible();
  await expect(page.locator(".photo-frame img")).toHaveCount(2);
  await expect(page.locator(".photo-frame canvas")).toHaveCount(0);
  await expect(page.getByText("4096 × 3072")).toBeVisible();
  expect(workerRequests).toHaveLength(1);

  await page.getByRole("button", { name: "Open another set" }).click();
  await expect(page.getByRole("heading", { name: "Open a comparison set" })).toBeVisible();
});
