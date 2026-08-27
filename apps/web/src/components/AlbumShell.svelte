<script lang="ts">
  import type { DeepReadonly, Group, IndexedAlbum, Photo } from "../contracts";
  import type { PhotoResolutionOptions, ResolvedPhotoSource, SourceSelection } from "../platform";
  import GroupViewer from "./GroupViewer.svelte";

  export let album: IndexedAlbum;
  export let selection: SourceSelection;
  export let canRefresh: boolean;
  export let onRefresh: () => void;
  export let onReset: () => void;
  export let onReauthorize: () => void;
  export let resolveSource: (
    photo: DeepReadonly<Photo>,
    options?: PhotoResolutionOptions,
  ) => Promise<ResolvedPhotoSource>;
  export let releaseSource: (source: ResolvedPhotoSource) => void;

  let groupIndex = 0;
  $: groups = album.document.groups;
  $: selectedGroup = groups[groupIndex] as DeepReadonly<Group> | undefined;
  $: stats = album.document.stats;
</script>

<div class="album-shell">
  <aside>
    <div class="album-title">
      <p class="eyebrow">{selection.label}</p>
      <h2>Analysis result</h2>
      <p>
        {album.document.photos.length} photos · {groups.length} groups ·
        {album.document.ungrouped.length} ungrouped
      </p>
      {#if stats}
        <p class="cache-stats">{stats.analyzedPairs} analyzed · {stats.reusedPairs} reused</p>
      {/if}
    </div>
    <nav aria-label="Comparison groups">
      {#each groups as group, index (group.id)}
        <button
          type="button"
          class:active={index === groupIndex}
          class="group-button"
          aria-current={index === groupIndex ? "page" : undefined}
          onclick={() => (groupIndex = index)}
        >
          <strong>Group {index + 1}</strong>
          <span>{group.photoIds.length} photos</span>
          {#if group.flags?.length}<small>{group.flags.join(" · ")}</small>{/if}
        </button>
      {/each}
    </nav>
    <details open={groups.length === 0}>
      <summary>Ungrouped ({album.document.ungrouped.length})</summary>
      <ul>
        {#each album.document.ungrouped as entry (entry.photoId)}
          <li>
            <strong>{album.photosById.get(entry.photoId)?.path ?? entry.photoId}</strong>
            <span>{entry.reason}</span>
            {#if entry.detail}<small>{entry.detail}</small>{/if}
          </li>
        {/each}
      </ul>
    </details>
    {#if selection.kind === "analysis-only"}
      <button type="button" class="secondary full" onclick={onReauthorize}
        >Choose photo source</button
      >
    {/if}
    {#if canRefresh}
      <button type="button" class="secondary full" onclick={onRefresh}>Refresh analysis</button>
    {/if}
    <button type="button" class="secondary full" onclick={onReset}>Open another set</button>
  </aside>

  <main>
    {#if selectedGroup}
      <div class="group-nav">
        <button
          type="button"
          class="secondary"
          disabled={groupIndex === 0}
          onclick={() => (groupIndex -= 1)}>Previous</button
        >
        <span>Group {groupIndex + 1} of {groups.length}</span>
        <button
          type="button"
          class="secondary"
          disabled={groupIndex >= groups.length - 1}
          onclick={() => (groupIndex += 1)}>Next</button
        >
      </div>
      {#key selectedGroup.id}
        <GroupViewer {album} group={selectedGroup} {resolveSource} {releaseSource} />
      {/key}
    {:else}
      <section class="empty">
        <h2>No comparison groups</h2>
        <p>Every photo remains listed under Ungrouped with its analysis reason.</p>
      </section>
    {/if}
  </main>
</div>

<style>
  .album-shell {
    display: grid;
    min-height: calc(100vh - 4rem);
    grid-template-columns: minmax(15rem, 19rem) minmax(0, 1fr);
    gap: 1rem;
  }
  aside {
    display: flex;
    max-height: calc(100vh - 5rem);
    flex-direction: column;
    gap: 0.75rem;
    overflow: auto;
    padding: 1rem;
    border: 1px solid var(--border);
    border-radius: 0.75rem;
    background: var(--surface);
  }
  .album-title h2,
  .album-title p {
    margin: 0.2rem 0;
  }
  .eyebrow,
  .cache-stats {
    color: var(--muted);
    font-size: 0.75rem;
  }
  nav {
    display: grid;
    gap: 0.4rem;
  }
  .group-button {
    display: grid;
    gap: 0.1rem;
    width: 100%;
    border-color: var(--border);
    background: var(--surface-raised);
    color: var(--text);
    text-align: left;
  }
  .group-button.active {
    border-color: var(--teal);
    background: color-mix(in srgb, var(--teal) 18%, var(--surface-raised));
  }
  .group-button span,
  .group-button small,
  details span,
  details small {
    display: block;
    color: var(--muted);
  }
  details ul {
    display: grid;
    gap: 0.6rem;
    padding-left: 1.2rem;
    font-size: 0.78rem;
  }
  .full {
    width: 100%;
  }
  main {
    min-width: 0;
  }
  .group-nav {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.75rem;
    margin-bottom: 0.75rem;
  }
  .empty {
    padding: 2rem;
    border: 1px solid var(--border);
    border-radius: 0.75rem;
    background: var(--surface);
  }
  @media (max-width: 900px) {
    .album-shell {
      grid-template-columns: 1fr;
    }
    aside {
      max-height: none;
    }
  }
</style>
