<script lang="ts" context="module">
  function errorTitle(code: string): string {
    if (code === "native-analysis-required") return "Open these photos in camcomp desktop";
    if (code === "analysis-confirmation-required") return "Confirm the local write destination";
    if (code === "reauthorization-required") return "Folder permission expired";
    if (code === "multiple-analysis-files") return "More than one analysis was selected";
    if (code.includes("schema") || code.includes("document"))
      return "Analysis file is incompatible";
    return "The source could not be opened";
  }
</script>

<script lang="ts">
  export let code: string;
  export let message: string;
  export let onReset: () => void;
  export let onPrimary: (() => void) | undefined = undefined;

  $: primaryLabel =
    code === "reauthorization-required"
      ? "Authorize folder"
      : code === "analysis-confirmation-required"
        ? "Run analysis"
        : code === "native-analysis-required"
          ? undefined
          : "Try again";
</script>

<section class="error-panel" aria-labelledby="error-heading">
  <p class="eyebrow">
    {code === "analysis-confirmation-required" ? "Ready to analyze" : "Action needed"}
  </p>
  <h2 id="error-heading">{errorTitle(code)}</h2>
  <p role="alert">{message}</p>
  <div class="actions">
    {#if primaryLabel && onPrimary}
      <button type="button" onclick={onPrimary}>{primaryLabel}</button>
    {/if}
    <button type="button" class="secondary" onclick={onReset}>Choose another source</button>
  </div>
</section>

<style>
  .error-panel {
    max-width: 42rem;
    margin: 5rem auto;
    padding: 2rem;
    border: 1px solid var(--danger);
    border-radius: 1rem;
    background: var(--surface);
  }
  .eyebrow {
    color: var(--orange);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-weight: 800;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.6rem;
    margin-top: 1.25rem;
  }
</style>
