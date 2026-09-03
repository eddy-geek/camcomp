import type { DeepReadonly, Photo } from "../contracts";
import type {
  AnalysisOptions,
  PhotoResolutionOptions,
  PlatformAdapter,
  ResolvedPhotoSource,
  SourceSelection,
} from "../platform";
import { ImportStateStore, type ImportState } from "./state-machine";

export class ImportController {
  readonly #platform: PlatformAdapter;
  readonly #store: ImportStateStore;
  readonly #resolvedSources = new Set<ResolvedPhotoSource>();
  #runId = 0;
  #abortController?: AbortController;

  constructor(platform: PlatformAdapter, store = new ImportStateStore()) {
    this.#platform = platform;
    this.#store = store;
  }

  get state(): ImportState {
    return this.#store.state;
  }

  subscribe(listener: (state: ImportState) => void): () => void {
    return this.#store.subscribe(listener);
  }

  async chooseFolder(): Promise<void> {
    if (this.state.phase !== "ready") {
      await this.acquire(() => this.#platform.chooseFolder());
      return;
    }
    try {
      const selection = await this.#platform.chooseFolder();
      if (selection) await this.openSelection(selection);
    } catch (error) {
      this.reportError(error);
    }
  }

  reportError(error: unknown): void {
    const run = this.#beginRun();
    this.#store.dispatch({ type: "acquisition-started", runId: run.id });
    this.#handleRunError(run, error);
  }

  async chooseFiles(): Promise<void> {
    await this.acquire(() => this.#platform.chooseFiles());
  }

  async acceptDrop(dataTransfer: DataTransfer): Promise<void> {
    await this.acquire(() => this.#platform.acceptDrop(dataTransfer));
  }

  async acquire(acquireSelection: () => Promise<SourceSelection | undefined>): Promise<void> {
    const run = this.#beginRun();
    this.#store.dispatch({ type: "acquisition-started", runId: run.id });
    try {
      const selection = await acquireSelection();
      if (!this.#isCurrent(run)) {
        if (selection) this.#platform.releaseSelection(selection);
        return;
      }
      if (!selection) {
        this.#store.dispatch({ type: "acquisition-cancelled", runId: run.id });
        return;
      }
      await this.#loadOrAnalyze(run, selection);
    } catch (error) {
      this.#handleRunError(run, error);
    }
  }

  async openSelection(selection: SourceSelection): Promise<void> {
    const run = this.#beginRun();
    this.#store.dispatch({ type: "acquisition-started", runId: run.id });
    try {
      await this.#loadOrAnalyze(run, selection);
    } catch (error) {
      this.#handleRunError(run, error, selection);
    }
  }

  async reauthorize(): Promise<void> {
    const state = this.state;
    if (!("selection" in state) || !state.selection) return;
    const currentSelection = state.selection;
    const run = this.#beginRun(false);
    this.#store.dispatch({ type: "acquisition-started", runId: run.id });
    try {
      const selection = await this.#platform.reauthorize(currentSelection);
      if (!this.#isCurrent(run)) return;
      await this.#loadOrAnalyze(run, selection);
    } catch (error) {
      this.#handleRunError(run, error, currentSelection);
    }
  }

  async analyze(options: AnalysisOptions = {}): Promise<void> {
    const state = this.state;
    if (!("selection" in state) || !state.selection) {
      throw new Error("No source selection is available for analysis.");
    }
    const selection = state.selection;
    const run = this.#beginRun(false);
    this.#store.dispatch({ type: "acquisition-started", runId: run.id });
    try {
      await this.#runAnalysis(run, selection, options);
    } catch (error) {
      this.#handleRunError(run, error, selection);
    }
  }

  async refresh(options: AnalysisOptions = {}): Promise<void> {
    await this.analyze({ ...options, reuseCache: true });
  }

  async cancel(): Promise<void> {
    const runId = this.state.runId;
    if (this.state.phase !== "analyzing" && this.state.phase !== "resolving-photos") return;
    this.#store.dispatch({ type: "cancellation-requested", runId });
    this.#abortController?.abort();
    await this.#platform.cancelAnalysis().catch(() => false);
    this.#store.dispatch({ type: "cancelled", runId });
  }

  async resolvePhoto(
    photo: DeepReadonly<Photo>,
    options: PhotoResolutionOptions = {},
  ): Promise<ResolvedPhotoSource> {
    const state = this.state;
    if (state.phase !== "ready") throw new Error("Photos can only be resolved for a ready album.");
    const runId = state.runId;
    const source = await this.#platform.resolvePhoto(
      state.selection,
      photo,
      this.#abortController?.signal,
      options,
    );
    if (this.state.runId !== runId || this.state.phase !== "ready") {
      source.release();
      throw new DOMException("The import was superseded.", "AbortError");
    }
    this.#resolvedSources.add(source);
    return source;
  }

  releasePhoto(source: ResolvedPhotoSource): void {
    if (!this.#resolvedSources.delete(source)) return;
    source.release();
  }

  reset(): void {
    const previous = this.state;
    this.#abortController?.abort();
    this.#releaseResolvedSources();
    if ("selection" in previous && previous.selection) {
      this.#platform.releaseSelection(previous.selection);
    }
    const runId = ++this.#runId;
    this.#store.dispatch({ type: "reset", runId });
  }

  dispose(): void {
    this.reset();
  }

  async #loadOrAnalyze(
    run: { readonly id: number; readonly controller: AbortController },
    selection: SourceSelection,
  ): Promise<void> {
    if (selection.needsReauthorization) {
      this.#store.dispatch({
        type: "failed",
        runId: run.id,
        selection,
        code: "reauthorization-required",
        message: "Folder access must be reauthorized before the analysis can be loaded.",
      });
      return;
    }
    if (selection.hasAnalysis) {
      this.#store.dispatch({
        type: "analysis-validation-started",
        runId: run.id,
        selection,
      });
      const album = await this.#platform.loadAnalysis(selection, run.controller.signal);
      if (!this.#isCurrent(run)) return;
      if (!album) throw new Error("The selected analysis is no longer available.");
      this.#store.dispatch({
        type: "photo-resolution-started",
        runId: run.id,
        selection,
        album,
      });
      run.controller.signal.throwIfAborted();
      this.#store.dispatch({ type: "ready", runId: run.id, selection, album });
      return;
    }
    if (!this.#platform.capabilities.analysis) {
      this.#store.dispatch({
        type: "failed",
        runId: run.id,
        selection,
        code: "native-analysis-required",
        message: "These photos need new analysis in the camcomp desktop application.",
      });
      return;
    }
    this.#store.dispatch({
      type: "failed",
      runId: run.id,
      selection,
      code: "analysis-confirmation-required",
      message: `New analysis will be saved to ${selection.writeTargetLabel ?? "camcomp.json in the selected source folder"}.`,
    });
  }

  async #runAnalysis(
    run: { readonly id: number; readonly controller: AbortController },
    selection: SourceSelection,
    options: AnalysisOptions,
  ): Promise<void> {
    this.#store.dispatch({ type: "analysis-started", runId: run.id, selection });
    const album = await this.#platform.analyze(selection, options, {
      signal: run.controller.signal,
      onProgress: (progress) => {
        if (this.#isCurrent(run)) {
          this.#store.dispatch({ type: "analysis-progressed", runId: run.id, progress });
        }
      },
    });
    if (!this.#isCurrent(run)) return;
    run.controller.signal.throwIfAborted();
    this.#store.dispatch({
      type: "photo-resolution-started",
      runId: run.id,
      selection,
      album,
    });
    this.#store.dispatch({ type: "ready", runId: run.id, selection, album });
  }

  #beginRun(releaseSelection = true): {
    readonly id: number;
    readonly controller: AbortController;
  } {
    const previous = this.state;
    this.#abortController?.abort();
    this.#releaseResolvedSources();
    if (releaseSelection && "selection" in previous && previous.selection) {
      this.#platform.releaseSelection(previous.selection);
    }
    const controller = new AbortController();
    const id = ++this.#runId;
    this.#abortController = controller;
    return { id, controller };
  }

  #isCurrent(run: { readonly id: number; readonly controller: AbortController }): boolean {
    return run.id === this.#runId && !run.controller.signal.aborted;
  }

  #handleRunError(
    run: { readonly id: number; readonly controller: AbortController },
    error: unknown,
    selection?: SourceSelection,
  ): void {
    if (run.id !== this.#runId) return;
    if (run.controller.signal.aborted || isAbortError(error)) {
      this.#store.dispatch({ type: "cancelled", runId: run.id });
      return;
    }
    const details = errorDetails(error);
    this.#store.dispatch({
      type: "failed",
      runId: run.id,
      selection,
      code: details.code,
      message: details.message,
    });
  }

  #releaseResolvedSources(): void {
    for (const source of this.#resolvedSources) source.release();
    this.#resolvedSources.clear();
  }
}

function errorDetails(error: unknown): { readonly code: string; readonly message: string } {
  if (typeof error === "object" && error !== null) {
    const candidate = error as { code?: unknown; message?: unknown };
    return {
      code: typeof candidate.code === "string" ? candidate.code : "import-failed",
      message: typeof candidate.message === "string" ? candidate.message : "The import failed.",
    };
  }
  return { code: "import-failed", message: String(error) };
}

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}
