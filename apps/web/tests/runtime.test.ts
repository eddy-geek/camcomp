import { describe, expect, it, vi } from "vitest";

import {
  createPlatformAdapter,
  detectBrowserCapabilities,
  detectRuntimeKind,
} from "../src/platform";
import type { PlatformAdapter } from "../src/platform";

describe("runtime capability detection", () => {
  it("detects Tauri from its runtime capability and never reads a user agent", () => {
    const browser = {} as typeof globalThis;
    const tauri = { __TAURI_INTERNALS__: {} } as unknown as typeof globalThis;
    expect(detectRuntimeKind(browser)).toBe("browser");
    expect(detectRuntimeKind(tauri)).toBe("tauri");
  });

  it("selects injected adapters by detected runtime", () => {
    const browserAdapter = { name: "browser" } as unknown as PlatformAdapter;
    const tauriAdapter = { name: "tauri" } as unknown as PlatformAdapter;
    expect(
      createPlatformAdapter({ window } as typeof globalThis, {
        browser: () => browserAdapter,
        tauri: () => tauriAdapter,
      }),
    ).toBe(browserAdapter);
    expect(
      createPlatformAdapter({ __TAURI_INTERNALS__: {} } as unknown as typeof globalThis, {
        browser: () => browserAdapter,
        tauri: () => tauriAdapter,
      }),
    ).toBe(tauriAdapter);
  });

  it("reports directory handles only when the picker capability exists", () => {
    const fallback = detectBrowserCapabilities({} as Window);
    const chromium = detectBrowserCapabilities({
      showDirectoryPicker: vi.fn(),
    } as unknown as Window);
    expect(fallback.folderAccess).toBe("recursive-file-input");
    expect(chromium.folderAccess).toBe("directory-handle");
  });
});
