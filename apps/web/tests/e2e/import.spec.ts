import { expect, test } from "@playwright/test";
import { validAnalysisDocument } from "../fixtures";

const tinyPng = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Y9ZlqkAAAAASUVORK5CYII=",
  "base64",
);

test("browser photos remain selected and explain that native analysis is required", async ({
  page,
}) => {
  await page.goto("./");
  const chooser = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: "Select files" }).click();
  await (await chooser).setFiles({ name: "photo.png", mimeType: "image/png", buffer: tinyPng });
  await expect(
    page.getByRole("heading", { name: "Open these photos in camcomp desktop" }),
  ).toBeVisible();
  await expect(page.getByText(/need new analysis/i)).toBeVisible();
});

test("loads a selected cached analysis without invoking browser analysis", async ({ page }) => {
  await page.goto("./");
  const chooser = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: "Select files" }).click();
  await (
    await chooser
  ).setFiles([
    {
      name: "camcomp.json",
      mimeType: "application/json",
      buffer: Buffer.from(JSON.stringify(validAnalysisDocument())),
    },
    { name: "a.jpg", mimeType: "image/jpeg", buffer: tinyPng },
  ]);
  await expect(page.getByRole("heading", { name: "Analysis result" })).toBeVisible();
  await expect(page.getByText("photos/a.jpg")).toBeVisible();
  await expect(page.getByText("no-session-peer")).toBeVisible();
});

test("the generated app shell is available offline without external requests", async ({
  page,
  context,
  browserName,
}) => {
  await page.goto("./");
  await page.evaluate(async () => {
    if ("serviceWorker" in navigator) await navigator.serviceWorker.ready;
  });
  await page.reload();
  await page.waitForFunction(() => navigator.serviceWorker?.controller !== null);
  const cachedUrls = await page.evaluate(async () => {
    const urls: string[] = [];
    for (const cacheName of await caches.keys()) {
      urls.push(...(await (await caches.open(cacheName)).keys()).map((request) => request.url));
    }
    return urls;
  });
  expect(cachedUrls.length).toBeGreaterThan(0);
  expect(cachedUrls.every((url) => url.startsWith("http://127.0.0.1:4173/"))).toBe(true);
  expect(cachedUrls.some((url) => /camcomp\.json|blob:|photo\.(?:jpe?g|png|heic)/i.test(url))).toBe(
    false,
  );
  if (browserName === "webkit") {
    const cachedShell = await page.evaluate(async () => {
      for (const cacheName of await caches.keys()) {
        const cache = await caches.open(cacheName);
        const request = (await cache.keys()).find(({ url }) =>
          new URL(url).pathname.endsWith("/index.html"),
        );
        const response = request ? await cache.match(request) : undefined;
        if (response) return response.text();
      }
      return "";
    });
    expect(cachedShell).toContain('<div id="app"></div>');
  } else {
    await context.setOffline(true);
    await page.goto("./", { waitUntil: "domcontentloaded" });
  }
  await expect(page.getByRole("heading", { name: "camcomp" })).toBeVisible();
  await expect(page.getByText(/never uploaded/i)).toBeVisible();
});
