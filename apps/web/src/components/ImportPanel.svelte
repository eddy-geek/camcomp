<script lang="ts" context="module">
  function capabilityLabel(kind: RuntimeCapabilities["folderAccess"]): string {
    if (kind === "directory-handle") return "Retainable directory handle";
    if (kind === "recursive-file-input") return "Reselect after restart";
    if (kind === "native-dialog") return "Native folder dialog";
    return "File selection only";
  }
</script>

<script lang="ts">
  import type { RuntimeCapabilities } from "../platform";

  export let capabilities: RuntimeCapabilities;
  export let busy = false;
  export let onChooseFolder: () => void;
  export let onChooseFiles: () => void;
  export let onRestore: () => void;
  export let onClearRetained: () => void;
  export let onDrop: (dataTransfer: DataTransfer) => void;

  let over = false;

  function dropped(event: DragEvent): void {
    event.preventDefault();
    over = false;
    if (event.dataTransfer) onDrop(event.dataTransfer);
  }
</script>

<section class="import-panel" aria-labelledby="import-heading">
  <div>
    <p class="eyebrow">Local workspace</p>
    <h2 id="import-heading">Open a comparison set</h2>
    <p class="intro">
      Load an existing <code>camcomp.json</code>, or select photos for native analysis. Photos,
      metadata, and analysis are never uploaded.
    </p>
  </div>

  <div class="actions">
    <button type="button" onclick={onChooseFolder} disabled={busy}>Open folder</button>
    <button type="button" class="secondary" onclick={onChooseFiles} disabled={busy}>
      Select files
    </button>
    {#if capabilities.persistentDirectoryPermission}
      <button type="button" class="secondary" onclick={onRestore} disabled={busy}>
        Reopen recent folder
      </button>
      <button type="button" class="secondary" onclick={onClearRetained} disabled={busy}>
        Forget recent folder
      </button>
    {/if}
  </div>

  <div
    class:over
    class="drop-zone"
    role="button"
    tabindex="0"
    aria-label="Drop photos or one camcomp JSON analysis"
    ondragenter={(event) => {
      event.preventDefault();
      over = true;
    }}
    ondragover={(event) => event.preventDefault()}
    ondragleave={() => (over = false)}
    ondrop={dropped}
    onkeydown={(event) => {
      if (event.key === "Enter" || event.key === " ") onChooseFiles();
    }}
  >
    <strong>Drop photos or camcomp.json</strong>
    <span>One analysis plus its source photos is accepted together.</span>
  </div>

  <dl class="capabilities" aria-label="Available capabilities">
    <div>
      <dt>Folder access</dt>
      <dd>{capabilityLabel(capabilities.folderAccess)}</dd>
    </div>
    <div>
      <dt>Analysis</dt>
      <dd>{capabilities.analysis ? "Native, cancellable" : "Desktop app required"}</dd>
    </div>
    <div>
      <dt>HEIC display</dt>
      <dd>
        {capabilities.heicDisplay === "native-cache"
          ? "Native lossless cache"
          : "Native probe, then sidecar"}
      </dd>
    </div>
    <div>
      <dt>Worker HEIC</dt>
      <dd>{capabilities.workerHeic ? "Available" : "Not enabled"}</dd>
    </div>
    <div>
      <dt>Installation</dt>
      <dd>{capabilities.pwaInstalled ? "Installed PWA" : "Browser-dependent"}</dd>
    </div>
    <div>
      <dt>Privacy</dt>
      <dd>Local only</dd>
    </div>
  </dl>
</section>

<style>
  .import-panel {
    display: grid;
    gap: 1.5rem;
    max-width: 58rem;
    margin: 4rem auto;
    padding: clamp(1.25rem, 4vw, 3rem);
    border: 1px solid var(--border);
    border-radius: 1rem;
    background: var(--surface);
  }
  h2 {
    margin: 0.2rem 0 0.7rem;
    font-size: clamp(1.7rem, 4vw, 2.7rem);
  }
  .eyebrow {
    margin: 0;
    color: var(--teal);
    font-size: 0.78rem;
    font-weight: 800;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }
  .intro {
    max-width: 46rem;
    color: var(--muted);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.65rem;
  }
  .drop-zone {
    display: grid;
    gap: 0.25rem;
    min-height: 8rem;
    place-content: center;
    padding: 1rem;
    border: 2px dashed var(--border);
    border-radius: 0.75rem;
    text-align: center;
    color: var(--muted);
  }
  .drop-zone.over {
    border-color: var(--orange);
    background: color-mix(in srgb, var(--orange) 8%, transparent);
    color: var(--text);
  }
  .capabilities {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(10rem, 1fr));
    gap: 0.75rem;
    margin: 0;
  }
  .capabilities div {
    padding: 0.75rem;
    border-radius: 0.5rem;
    background: var(--surface-raised);
  }
  dt {
    color: var(--muted);
    font-size: 0.75rem;
    text-transform: uppercase;
  }
  dd {
    margin: 0.2rem 0 0;
  }
</style>
