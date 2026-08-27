<script lang="ts" context="module">
  function progressText(progress: AnalysisProgressEvent | undefined): string {
    if (!progress) return "Starting analysis…";
    if (progress.kind === "warning") return `${progress.stage}: ${progress.message}`;
    if (progress.kind === "stage-finished")
      return `${progress.stage} completed in ${progress.elapsedMs} ms`;
    if (progress.kind === "stage-started") return `${progress.stage} started`;
    return `${progress.stage}: ${progress.done}${progress.total ? ` of ${progress.total}` : ""}`;
  }
</script>

<script lang="ts">
  import type { AnalysisProgressEvent } from "../platform";

  export let phase: "analyzing" | "cancelling";
  export let progress: AnalysisProgressEvent | undefined;
  export let selectionLabel: string;
  export let writeTargetLabel: string | undefined;
  export let warnings: readonly string[] = [];
  export let onCancel: () => void;

  $: value = progress?.kind === "advanced" ? progress.done : undefined;
  $: total = progress && "total" in progress ? progress.total : null;
  $: detail = progress?.kind === "advanced" ? progress.detail : null;
</script>

<section class="progress-panel" aria-labelledby="progress-heading">
  <p class="eyebrow">Native analysis</p>
  <h2 id="progress-heading">{phase === "cancelling" ? "Cancelling safely" : selectionLabel}</h2>
  {#if writeTargetLabel}<p>Result: <code>{writeTargetLabel}</code></p>{/if}
  <p role="status" aria-live="polite">
    {phase === "cancelling"
      ? "Waiting for the current analysis step to stop…"
      : progressText(progress)}
  </p>
  <progress max={total ?? 1} value={total && value !== undefined ? value : undefined}></progress>
  {#if detail}<p class="detail">{detail}</p>{/if}
  {#if warnings.length}
    <ul class="warnings" aria-label="Analysis warnings">
      {#each warnings as warning, index (`${index}-${warning}`)}<li>{warning}</li>{/each}
    </ul>
  {/if}
  <button type="button" class="secondary" onclick={onCancel} disabled={phase === "cancelling"}>
    Cancel analysis
  </button>
</section>

<style>
  .progress-panel {
    max-width: 42rem;
    margin: 5rem auto;
    padding: 2rem;
    border: 1px solid var(--border);
    border-radius: 1rem;
    background: var(--surface);
  }
  .eyebrow,
  .detail {
    color: var(--muted);
  }
  .warnings {
    color: var(--orange);
  }
  progress {
    display: block;
    width: 100%;
    height: 0.8rem;
    margin: 1.25rem 0;
    accent-color: var(--teal);
  }
</style>
