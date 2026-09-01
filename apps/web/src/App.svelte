<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import AlbumShell from "./components/AlbumShell.svelte";
  import ErrorPanel from "./components/ErrorPanel.svelte";
  import ImportPanel from "./components/ImportPanel.svelte";
  import ProgressPanel from "./components/ProgressPanel.svelte";
  import { ImportController, initialImportState, type ImportState } from "./import";
  import {
    createPlatformAdapter,
    type PlatformAdapter,
    type ResolvedPhotoSource,
    type SourceSelection,
  } from "./platform";

  const platform = createPlatformAdapter();
  const controller = new ImportController(platform);
  let state: ImportState = initialImportState;
  let unlistenNativeDrop: (() => void) | undefined;
  const unsubscribe = controller.subscribe((next) => (state = next));

  onMount(async () => {
    const native = platform as PlatformAdapter & {
      listenForNativeDrops?: (
        handler: (selection: SourceSelection) => void,
        onError: (error: unknown) => void,
      ) => Promise<() => void>;
    };
    if (native.listenForNativeDrops) {
      try {
        unlistenNativeDrop = await native.listenForNativeDrops(
          (selection) => void controller.openSelection(selection),
          (error) => controller.reportError(error),
        );
      } catch (error) {
        controller.reportError(error);
      }
    }
  });

  onDestroy(() => {
    unlistenNativeDrop?.();
    unsubscribe();
    controller.dispose();
  });

  async function restoreFolder(): Promise<void> {
    const selection = await platform.restorePersistedFolder();
    if (selection) await controller.openSelection(selection);
  }

  function primaryErrorAction(): (() => void) | undefined {
    if (state.phase !== "error") return undefined;
    if (state.code === "reauthorization-required") return () => void controller.reauthorize();
    if (state.code === "analysis-confirmation-required") return () => void controller.analyze();
    const selection = state.selection;
    return selection ? () => void controller.openSelection(selection) : undefined;
  }

  function writeTarget(selection: SourceSelection): string | undefined {
    return selection.writeTargetLabel;
  }

  function interfaceErrorMessage(error: unknown): string {
    return error instanceof Error && error.message
      ? error.message
      : "The interface could not render this album.";
  }
</script>

<svelte:head>
  <title>camcomp · local photo comparison</title>
</svelte:head>

{#if state.phase !== "ready"}
  <header class="app-header">
    <a
      href="./"
      aria-label="camcomp home"
      onclick={(event) => {
        event.preventDefault();
        controller.reset();
      }}
    >
      <span class="mark" aria-hidden="true"></span>
      <h1>camcomp</h1>
    </a>
    <p>Local photo comparison</p>
    <span class="privacy">No uploads</span>
  </header>
{/if}

<div class="status sr-only" role="status" aria-live="polite">{state.phase}</div>

<svelte:boundary>
  <div class:comparison={state.phase === "ready"} class="app-content">
    {#if state.phase === "idle" || state.phase === "acquiring-source"}
      <ImportPanel
        capabilities={platform.capabilities}
        busy={state.phase === "acquiring-source"}
        onChooseFolder={() => void controller.chooseFolder()}
        onChooseFiles={() => void controller.chooseFiles()}
        onRestore={() => void restoreFolder()}
        onClearRetained={() =>
          void platform.clearRetainedData().catch((error) => controller.reportError(error))}
        onDrop={(dataTransfer) => void controller.acceptDrop(dataTransfer)}
      />
    {:else if state.phase === "analyzing" || state.phase === "cancelling"}
      <ProgressPanel
        phase={state.phase}
        progress={state.phase === "analyzing" ? state.progress : undefined}
        selectionLabel={state.selection?.label ?? "Selected photos"}
        writeTargetLabel={state.selection ? writeTarget(state.selection) : undefined}
        warnings={state.phase === "analyzing" ? state.warnings : []}
        onCancel={() => void controller.cancel()}
      />
    {:else if state.phase === "validating-analysis" || state.phase === "resolving-photos"}
      <section class="working" aria-live="polite">
        <h2>
          {state.phase === "validating-analysis" ? "Validating camcomp.json" : "Preparing album"}
        </h2>
        <p>Cached analysis is loaded without rescanning or rewriting source photos.</p>
      </section>
    {:else if state.phase === "error"}
      <ErrorPanel
        code={state.code}
        message={state.message}
        onReset={() => controller.reset()}
        onPrimary={primaryErrorAction()}
      />
    {:else if state.phase === "cancelled"}
      <ErrorPanel
        code="cancelled"
        message="Analysis was cancelled. Any previous valid camcomp.json remains untouched."
        onReset={() => controller.reset()}
      />
    {:else if state.phase === "ready"}
      <AlbumShell
        album={state.album}
        selection={state.selection}
        canRefresh={platform.capabilities.analysis}
        onRefresh={() => void controller.refresh()}
        onReset={() => controller.reset()}
        onReauthorize={() => void controller.reauthorize()}
        resolveSource={(photo, options) => controller.resolvePhoto(photo, options)}
        releaseSource={(source: ResolvedPhotoSource) => controller.releasePhoto(source)}
      />
    {/if}
  </div>
  {#snippet failed(error, reset)}
    <div class="app-content">
      <ErrorPanel
        code="interface-error"
        message={interfaceErrorMessage(error)}
        onReset={() => {
          controller.reset();
          reset();
        }}
      />
    </div>
  {/snippet}
</svelte:boundary>

<footer class:comparison={state.phase === "ready"}>
  Source photos are read-only. Browser mode never writes analysis; desktop writes only
  <code>camcomp.json</code> and lossless derived display files in application cache.
  <a href="./third-party-notices.txt" target="_blank" rel="license">Third-party notices</a>
</footer>

<style>
  .app-header {
    display: flex;
    height: 4rem;
    align-items: center;
    gap: 1rem;
    padding: 0 1.25rem;
    border-bottom: 1px solid var(--border);
    background: color-mix(in srgb, var(--surface) 94%, transparent);
  }
  .app-header a {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    color: var(--text);
    text-decoration: none;
  }
  .mark {
    width: 1.45rem;
    height: 1.45rem;
    border-right: 0.35rem solid var(--text);
    border-left: 0.55rem solid var(--teal);
    box-shadow: 0.55rem 0 0 var(--orange);
  }
  h1 {
    margin: 0;
    font-size: 1.15rem;
  }
  .app-header p {
    margin: 0;
    color: var(--muted);
  }
  .privacy {
    margin-left: auto;
    color: var(--teal);
    font-size: 0.75rem;
    font-weight: 800;
    text-transform: uppercase;
  }
  .app-content {
    min-height: calc(100vh - 7rem);
    padding: 1rem;
  }
  .app-content.comparison {
    position: fixed;
    inset: 0;
    min-height: 0;
    padding: 0;
    overflow: hidden;
  }
  .working {
    max-width: 38rem;
    margin: 5rem auto;
    padding: 2rem;
    border: 1px solid var(--border);
    border-radius: 0.8rem;
    background: var(--surface);
  }
  footer.comparison {
    display: none;
  }
  footer {
    padding: 1rem 1.25rem;
    border-top: 1px solid var(--border);
    color: var(--muted);
    font-size: 0.75rem;
  }
</style>
