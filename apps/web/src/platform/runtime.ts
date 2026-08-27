import { BrowserPlatformAdapter } from "./browser";
import { TauriPlatformAdapter } from "./tauri";
import type { PlatformAdapter, RuntimeKind } from "./types";

export interface RuntimeAdapterFactories {
  readonly browser?: () => PlatformAdapter;
  readonly tauri?: () => PlatformAdapter;
}

export function detectRuntimeKind(scope: typeof globalThis = globalThis): RuntimeKind {
  const candidate = scope as typeof globalThis & { __TAURI_INTERNALS__?: unknown };
  return candidate.__TAURI_INTERNALS__ !== undefined ? "tauri" : "browser";
}

export function createPlatformAdapter(
  scope: typeof globalThis = globalThis,
  factories: RuntimeAdapterFactories = {},
): PlatformAdapter {
  if (detectRuntimeKind(scope) === "tauri") {
    return factories.tauri?.() ?? new TauriPlatformAdapter();
  }
  const browserScope = "window" in scope ? (scope.window as Window) : window;
  return factories.browser?.() ?? new BrowserPlatformAdapter(browserScope);
}
