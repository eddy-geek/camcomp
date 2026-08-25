// Minimal frontend for the Drop 1 Tauri spike.
//
// Deliberately plain: no framework, no bundler. What it has to show is the
// contract with the Rust core — commands, typed progress events, cancellation,
// structured errors — not a finished UI. Drop 2 replaces it.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { open } = window.__TAURI__.dialog;

const PROGRESS_EVENT = "camcomp://progress";

const el = (id) => document.getElementById(id);
const ui = {
  capabilities: el("capabilities"),
  pickFolder: el("pick-folder"),
  pickFiles: el("pick-files"),
  cancel: el("cancel"),
  selection: el("selection"),
  dropZone: el("drop-zone"),
  stage: el("stage"),
  bar: el("bar"),
  warnings: el("warnings"),
  summary: el("summary"),
  groups: el("groups"),
  ungroupedDetails: el("ungrouped-details"),
  ungrouped: el("ungrouped"),
};

let running = false;

function setRunning(value) {
  running = value;
  ui.cancel.disabled = !value;
  ui.pickFolder.disabled = value;
  ui.pickFiles.disabled = value;
}

function resetProgress() {
  ui.warnings.replaceChildren();
  ui.bar.value = 0;
  ui.stage.textContent = "starting…";
}

listen(PROGRESS_EVENT, (event) => {
  const payload = event.payload;
  switch (payload.kind) {
    case "stage-started":
      ui.stage.textContent = `${payload.stage}…`;
      ui.bar.removeAttribute("value");
      break;
    case "advanced": {
      const detail = payload.detail ? ` — ${payload.detail}` : "";
      const of = payload.total ? `/${payload.total}` : "";
      ui.stage.textContent = `${payload.stage} ${payload.done}${of}${detail}`;
      if (payload.total) {
        ui.bar.max = payload.total;
        ui.bar.value = payload.done;
      }
      break;
    }
    case "stage-finished":
      ui.stage.textContent = `${payload.stage} done in ${payload.elapsedMs} ms`;
      break;
    case "warning": {
      const item = document.createElement("li");
      item.textContent = `[${payload.stage}] ${payload.message}`;
      ui.warnings.append(item);
      break;
    }
  }
});

async function loadCapabilities() {
  try {
    const caps = await invoke("capabilities");
    ui.capabilities.textContent =
      `schema ${caps.schemaVersion} · ${caps.algorithm} ${caps.algorithmVersion} · ` +
      `${caps.formats.join(", ")} · ` +
      (caps.inProcessEngine ? "in-process engine" : "external engine");
  } catch (error) {
    ui.capabilities.textContent = describeError(error);
  }
}

function describeError(error) {
  if (error && typeof error === "object" && "code" in error) {
    return `${error.message} (${error.code})`;
  }
  return String(error);
}

async function analyze(inputs) {
  if (running || inputs.length === 0) return;
  setRunning(true);
  resetProgress();
  try {
    const response = await invoke("analyze", { request: { inputs } });
    render(response);
  } catch (error) {
    ui.summary.textContent = `Analysis failed: ${describeError(error)}`;
    ui.groups.replaceChildren();
  } finally {
    setRunning(false);
  }
}

function render({ document: doc, written }) {
  const stats = doc.stats ?? {};
  ui.summary.textContent =
    `${stats.photos ?? doc.photos.length} photo(s), ${stats.sessions ?? 0} session(s), ` +
    `${stats.candidatePairs ?? 0} candidate pair(s) — ${stats.analyzedPairs ?? 0} analyzed, ` +
    `${stats.reusedPairs ?? 0} reused. ` +
    `${doc.groups.length} group(s), ${doc.ungrouped.length} ungrouped in ${stats.durationMs ?? 0} ms` +
    (written ? ` · saved to ${written}` : "");

  const pathOf = new Map(doc.photos.map((p) => [p.id, p.path]));
  ui.groups.replaceChildren(
    ...doc.groups.map((group) => {
      const box = document.createElement("div");
      box.className = "group";
      const title = document.createElement("h3");
      const min = group.evidence?.minAnchorScore;
      title.textContent = `${group.photoIds.length} photos` +
        (min != null ? ` · min score ${min.toFixed(3)}` : "");
      box.append(title);
      if (group.flags?.length) {
        const flags = document.createElement("p");
        flags.className = "flags";
        flags.textContent = group.flags.join(", ");
        box.append(flags);
      }
      const list = document.createElement("ul");
      for (const id of group.photoIds) {
        const item = document.createElement("li");
        const isAnchor = id === group.anchorPhotoId;
        item.textContent = pathOf.get(id) ?? id;
        if (isAnchor) {
          item.classList.add("anchor");
          item.textContent += " (anchor)";
        } else {
          const alignment = group.alignments?.find((a) => a.photoId === id);
          if (alignment) {
            item.textContent +=
              ` — confidence ${alignment.confidence.toFixed(2)} (${alignment.provenance})`;
          } else {
            item.textContent += " — no alignment evidence";
          }
        }
        list.append(item);
      }
      box.append(list);
      return box;
    }),
  );

  ui.ungroupedDetails.hidden = doc.ungrouped.length === 0;
  ui.ungrouped.replaceChildren(
    ...doc.ungrouped.map((item) => {
      const li = document.createElement("li");
      li.textContent = `${pathOf.get(item.photoId) ?? item.photoId} — ${item.reason}`;
      return li;
    }),
  );
}

async function loadJson(path) {
  try {
    const doc = await invoke("load_analysis", { path });
    render({ document: doc, written: null });
    ui.stage.textContent = "loaded from cache";
  } catch (error) {
    ui.summary.textContent = `Could not load: ${describeError(error)}`;
  }
}

ui.pickFolder.addEventListener("click", async () => {
  const folder = await open({ directory: true, multiple: false });
  if (!folder) return;
  ui.selection.textContent = folder;
  await analyze([folder]);
});

ui.pickFiles.addEventListener("click", async () => {
  const files = await open({
    multiple: true,
    filters: [
      { name: "Photos", extensions: ["jpg", "jpeg", "png", "heic", "heif"] },
      { name: "Analysis", extensions: ["json"] },
    ],
  });
  if (!files) return;
  const list = Array.isArray(files) ? files : [files];
  ui.selection.textContent = `${list.length} file(s)`;
  const json = list.find((p) => p.toLowerCase().endsWith(".json"));
  if (json) {
    await loadJson(json);
    return;
  }
  await analyze(list);
});

ui.cancel.addEventListener("click", async () => {
  const stopped = await invoke("cancel_analysis");
  ui.stage.textContent = stopped ? "cancelling…" : "nothing to cancel";
});

// Tauri delivers dropped paths through its own event, because the browser's
// DataTransfer does not expose a real filesystem path.
listen("tauri://drag-drop", async (event) => {
  const paths = event.payload?.paths ?? [];
  ui.dropZone.classList.remove("over");
  if (paths.length === 0) return;
  ui.selection.textContent = `${paths.length} dropped item(s)`;
  const json = paths.find((p) => p.toLowerCase().endsWith(".json"));
  if (json) {
    await loadJson(json);
    return;
  }
  await analyze(paths);
});
listen("tauri://drag-enter", () => ui.dropZone.classList.add("over"));
listen("tauri://drag-leave", () => ui.dropZone.classList.remove("over"));

// The drop zone is also a keyboard-reachable control, so it is not the only way
// to get photos in.
ui.dropZone.addEventListener("keydown", (event) => {
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    ui.pickFolder.click();
  }
});
ui.dropZone.addEventListener("click", () => ui.pickFolder.click());

loadCapabilities();
