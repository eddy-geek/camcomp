import { expect, test } from "@playwright/test";

test("boots the local-only app shell", async ({ page }) => {
  const privateRequests: string[] = [];
  page.on("request", (request) => {
    if (!request.url().startsWith("http://127.0.0.1:4173")) privateRequests.push(request.url());
  });
  await page.goto("./");
  await expect(page.getByRole("heading", { name: "camcomp" })).toBeVisible();
  await expect(page.getByText(/never uploaded/i)).toBeVisible();
  expect(privateRequests).toEqual([]);
});
