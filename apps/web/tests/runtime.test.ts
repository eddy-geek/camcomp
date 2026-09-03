import { describe, expect, it, vi } from "vitest";

const eventMocks = vi.hoisted(() => ({ listen: vi.fn() }));

vi.mock("@tauri-apps/api/event", () => eventMocks);

import {
  createPlatformAdapter,
  detectBrowserCapabilities,
  detectRuntimeKind,
  TauriPlatformAdapter,
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

  it("forwards valid native menu actions and ignores unknown payloads", async () => {
    const unlisten = vi.fn();
    const onAction = vi.fn();
    eventMocks.listen.mockResolvedValue(unlisten);
    const dispose = await new TauriPlatformAdapter().listenForApplicationMenu(onAction);
    expect(eventMocks.listen).toHaveBeenCalledWith("camcomp://menu-action", expect.any(Function));
    const handler = eventMocks.listen.mock.calls[0]?.[1] as
      ((event: { payload: string }) => void) | undefined;
    handler?.({ payload: "refresh" });
    handler?.({ payload: "unknown" });
    expect(onAction).toHaveBeenCalledOnce();
    expect(onAction).toHaveBeenCalledWith("refresh");
    dispose();
    expect(unlisten).toHaveBeenCalledOnce();
  });
});
