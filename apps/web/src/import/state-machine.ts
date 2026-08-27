import type { IndexedAlbum } from "../contracts";
import type { AnalysisProgressEvent, SourceSelection } from "../platform";

export type ImportPhase =
  | "idle"
  | "acquiring-source"
  | "validating-analysis"
  | "resolving-photos"
  | "ready"
  | "analyzing"
  | "cancelling"
  | "cancelled"
  | "error";

interface ImportStateBase {
  readonly phase: ImportPhase;
  readonly runId: number;
}

export type ImportState =
  | (ImportStateBase & { readonly phase: "idle" })
  | (ImportStateBase & { readonly phase: "acquiring-source" })
  | (ImportStateBase & {
      readonly phase: "validating-analysis";
      readonly selection: SourceSelection;
    })
  | (ImportStateBase & {
      readonly phase: "resolving-photos";
      readonly selection: SourceSelection;
      readonly album: IndexedAlbum;
    })
  | (ImportStateBase & {
      readonly phase: "ready";
      readonly selection: SourceSelection;
      readonly album: IndexedAlbum;
    })
  | (ImportStateBase & {
      readonly phase: "analyzing";
      readonly selection: SourceSelection;
      readonly progress?: AnalysisProgressEvent;
      readonly warnings: readonly string[];
    })
  | (ImportStateBase & {
      readonly phase: "cancelling";
      readonly selection?: SourceSelection;
    })
  | (ImportStateBase & { readonly phase: "cancelled" })
  | (ImportStateBase & {
      readonly phase: "error";
      readonly selection?: SourceSelection;
      readonly code: string;
      readonly message: string;
    });

export type ImportEvent =
  | { readonly type: "acquisition-started"; readonly runId: number }
  | { readonly type: "acquisition-cancelled"; readonly runId: number }
  | {
      readonly type: "analysis-validation-started";
      readonly runId: number;
      readonly selection: SourceSelection;
    }
  | {
      readonly type: "photo-resolution-started";
      readonly runId: number;
      readonly selection: SourceSelection;
      readonly album: IndexedAlbum;
    }
  | {
      readonly type: "analysis-started";
      readonly runId: number;
      readonly selection: SourceSelection;
    }
  | {
      readonly type: "analysis-progressed";
      readonly runId: number;
      readonly progress: AnalysisProgressEvent;
    }
  | {
      readonly type: "ready";
      readonly runId: number;
      readonly selection: SourceSelection;
      readonly album: IndexedAlbum;
    }
  | { readonly type: "cancellation-requested"; readonly runId: number }
  | { readonly type: "cancelled"; readonly runId: number }
  | {
      readonly type: "failed";
      readonly runId: number;
      readonly selection?: SourceSelection;
      readonly code: string;
      readonly message: string;
    }
  | { readonly type: "reset"; readonly runId: number };

export const initialImportState: ImportState = Object.freeze({ phase: "idle", runId: 0 });

export function reduceImportState(state: ImportState, event: ImportEvent): ImportState {
  if (event.type === "acquisition-started") {
    return event.runId > state.runId
      ? Object.freeze({ phase: "acquiring-source", runId: event.runId })
      : state;
  }
  if (event.type === "reset") {
    return event.runId >= state.runId
      ? Object.freeze({ phase: "idle", runId: event.runId })
      : state;
  }
  if (event.runId !== state.runId) return state;
  if (state.phase === "cancelled" || state.phase === "error") return state;

  switch (event.type) {
    case "acquisition-cancelled":
      return Object.freeze({ phase: "idle", runId: event.runId });
    case "analysis-validation-started":
      return Object.freeze({
        phase: "validating-analysis",
        runId: event.runId,
        selection: event.selection,
      });
    case "photo-resolution-started":
      return Object.freeze({
        phase: "resolving-photos",
        runId: event.runId,
        selection: event.selection,
        album: event.album,
      });
    case "analysis-started":
      return Object.freeze({
        phase: "analyzing",
        runId: event.runId,
        selection: event.selection,
        warnings: Object.freeze([]),
      });
    case "analysis-progressed":
      return state.phase === "analyzing"
        ? Object.freeze({
            ...state,
            progress: event.progress,
            warnings:
              event.progress.kind === "warning"
                ? Object.freeze([...state.warnings, event.progress.message])
                : state.warnings,
          })
        : state;
    case "ready":
      return Object.freeze({
        phase: "ready",
        runId: event.runId,
        selection: event.selection,
        album: event.album,
      });
    case "cancellation-requested":
      return Object.freeze({
        phase: "cancelling",
        runId: event.runId,
        selection: "selection" in state ? state.selection : undefined,
      });
    case "cancelled":
      return Object.freeze({ phase: "cancelled", runId: event.runId });
    case "failed":
      return Object.freeze({
        phase: "error",
        runId: event.runId,
        selection: event.selection,
        code: event.code,
        message: event.message,
      });
  }
}

export class ImportStateStore {
  #state: ImportState = initialImportState;
  readonly #listeners = new Set<(state: ImportState) => void>();

  get state(): ImportState {
    return this.#state;
  }

  dispatch(event: ImportEvent): ImportState {
    const next = reduceImportState(this.#state, event);
    if (next === this.#state) return next;
    this.#state = next;
    for (const listener of this.#listeners) listener(next);
    return next;
  }

  subscribe(listener: (state: ImportState) => void): () => void {
    this.#listeners.add(listener);
    listener(this.#state);
    return () => this.#listeners.delete(listener);
  }
}
