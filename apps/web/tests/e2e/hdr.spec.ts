import { expect, test } from "@playwright/test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import type { AnalysisDocument } from "../../src/contracts";
import { validAnalysisDocument, validPhoto } from "../fixtures";

test.describe.configure({ mode: "serial" });

function hdrComparisonDocument(): AnalysisDocument {
  const document = validAnalysisDocument();
  document.photos[0]!.path = "g1-PXL_20260822_125342320.jpg";
  document.photos[0]!.dimensions = { original: [4080, 3072], oriented: [4080, 3072] };
  const second = structuredClone(validPhoto());
  second.id = "photo-b";
  second.path = "g3-20260822_115745_45.20000_7.20000.jpg";
  second.configurationId = "configuration-2";
  second.dimensions = { original: [4096, 3072], oriented: [4096, 3072] };
  document.photos.push(second);
  document.configurations.push({
    id: "configuration-2",
    deviceKey: "device-2",
    displayName: "Second test camera",
    photoCount: 1,
  });
  document.groups = [
    {
      id: "group-1",
      anchorPhotoId: "photo-a",
      photoIds: ["photo-a", "photo-b"],
      evidence: { compatibleMemberPairs: 1, incompatibleMemberPairs: 0, unverifiedMemberPairs: 0 },
    },
  ];
  document.ungrouped = [];
  return document;
}

test("local Blob URLs return exact bounded ranges in every browser engine", async ({ page }) => {
  await page.goto("./");
  const result = await page.evaluate(async () => {
    const bytes = new Uint8Array(1024 * 1024);
    bytes[0] = 0xff;
    bytes[1] = 0xd8;
    bytes[bytes.length - 2] = 0xff;
    bytes[bytes.length - 1] = 0xd9;
    const url = URL.createObjectURL(new Blob([bytes], { type: "image/jpeg" }));
    try {
      const response = await fetch(url, { headers: { Range: "bytes=0-3" } });
      return {
        status: response.status,
        length: (await response.arrayBuffer()).byteLength,
        contentRange: response.headers.get("Content-Range"),
      };
    } finally {
      URL.revokeObjectURL(url);
    }
  });
  expect(result).toEqual({ status: 206, length: 4, contentRange: "bytes 0-3/1048576" });
});

test("keeps a real gain-map JPEG on the HTML image path with an explicit SDR mode", async ({
  page,
  browserName,
}) => {
  test.skip(browserName === "firefox", "Real Firefox gain-map viewer flow remains pending M2.");
  test.setTimeout(90_000);
  await page.goto("./");
  const chooser = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: "Select files" }).click();
  await (
    await chooser
  ).setFiles([
    {
      name: "camcomp.json",
      mimeType: "application/json",
      buffer: Buffer.from(JSON.stringify(hdrComparisonDocument())),
    },
    {
      name: "g1-PXL_20260822_125342320.jpg",
      mimeType: "image/jpeg",
      buffer: readFileSync(
        resolve(process.cwd(), "../../test/tstgrp5/g1-PXL_20260822_125342320.jpg"),
      ),
    },
    {
      name: "g3-20260822_115745_45.20000_7.20000.jpg",
      mimeType: "image/jpeg",
      buffer: readFileSync(
        resolve(process.cwd(), "../../test/tstgrp2/g3-20260822_115745_45.20000_7.20000.jpg"),
      ),
    },
  ]);

  await expect(page.getByRole("heading", { name: "Comparison group" })).toBeVisible();
  await expect(page.locator(".photo-frame img")).toHaveCount(2, { timeout: 30_000 });
  await expect(page.locator(".photo-frame canvas")).toHaveCount(0);
  await expect(
    page
      .getByText(/HDR path active|HDR source · SDR display|SDR fallback|HDR metadata unverified/i)
      .first(),
  ).toBeVisible();

  await page.getByLabel("Dynamic range").selectOption("sdr");
  await expect(
    page
      .getByText(/HDR source · comparing in SDR|HDR output unverified|HDR metadata unverified/i)
      .first(),
  ).toBeVisible();
});
