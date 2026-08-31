# Using camcomp

You have a folder of photos of the same things, taken with two phones, or with one
phone in two modes. camcomp works out which photos go together so you can compare
them side by side.

This guide covers grouping, browser and desktop import, cached-analysis reload, and
the comparison viewer.

Nothing leaves your machine. No photo is ever modified, moved, or renamed. The only
file camcomp writes into your photo folder is `camcomp.json`.

---

## 1. Your first run

```sh
camcomp analyze ~/Pictures/comparison
```

You get a summary and a `camcomp.json` next to the photos:

```text
6 photo(s), 2 session(s), 5 candidate pair(s) — 3 analyzed, 0 reused
pairs: 2 compatible, 3 incompatible, 0 uncertain
2 group(s), 2 ungrouped — 4210 ms, 4 features extracted

group-4a3a0704219b — 2 photo(s), min score 0.892
  * IMG20260822131856.heic                              anchor
    PXL_20260822_111856756.jpg                          confidence 0.89 (direct-pair)

ungrouped:
  IMG20260822131909.heic                                no-focal-compatible-peer
  PXL_20260822_111909642.jpg                            no-focal-compatible-peer
```

Run it again and it takes milliseconds — results are cached in `camcomp.json` and
only changed photos are re-examined.

## 2. Reading the output

### Groups

A group is a set of photos camcomp believes show the same scene. One member is the
**anchor**, marked `*`. Every other member gets a transform that maps its
coordinates onto the anchor's, which is what will let the viewer zoom and pan all
the photos together.

`confidence 0.89 (direct-pair)` means the transform came from directly comparing
that photo with the anchor, and camcomp is fairly sure of it. `(composed)` means it
was derived through a third photo and is less reliable.

`NO ALIGNMENT — pair not analyzed` means the photo is in the group but camcomp has
no transform for it. See §5.

### Ungrouped photos

Every photo you gave camcomp appears somewhere. If it is not in a group, it is
listed as ungrouped with a reason:

| reason | what happened | what to do |
|---|---|---|
| `no-session-peer` | nothing else was taken within 60 seconds of it | raise `--session-gap`, or group it by hand |
| `no-focal-compatible-peer` | peers exist but their field of view is too different | raise `--fov-tolerance` (or `--fov-tolerance-native-lens`), or group it by hand |
| `no-distinct-configuration-peer` | its match is from the same camera *and* mode | pass `--allow-duplicate-config` |
| `all-pairs-incompatible` | camcomp looked and concluded these are different scenes | group it by hand if you disagree |
| `all-pairs-uncertain` | camcomp looked and could not tell | usually a low-texture or heavily changed scene |
| `inconsistent-with-group-members` | it matched one photo but contradicted another already in the group | look at the group; something is off |
| `unusable-timestamp` | no capture time in the file | group it by hand |
| `unreadable-file` | the file could not be decoded | check the file |

`no-session-peer` and `no-focal-compatible-peer` are the two you will actually see.

### Configurations

A *configuration* is a camera plus a shooting mode — that is the thing you are
comparing. camcomp detects them for you:

```sh
camcomp inspect ~/Pictures/comparison
```

```text
configuration oppo-find-x9-pro--hi-res — OPPO Find X9 Pro (hi-res) (2 photo(s))
configuration oppo-find-x9-pro--master — OPPO Find X9 Pro (master) (1 photo(s))
configuration pixel-9-pro-xl--hdr-plus — Pixel 9 Pro XL (hdr-plus) (3 photo(s))
```

For OPPO metadata modes, bit meanings, Pro XDR/HDR evidence, and known
limitations, see the [OPPO metadata specification](../artifacts/SPEC-OPPO.md).
For Google phones camcomp recognises the HDR+ pipeline as `hdr-plus`. For
anything else the mode is simply left blank and the configuration is just the
camera.

By default a group holds **at most one photo per configuration**, because comparing
a configuration against itself is not the point. If you want two shots from the
same phone and mode in one group, pass `--allow-duplicate-config`.

## 3. When the automatic grouping is wrong

It will be sometimes. Two knobs cover almost every case.

### Photos taken slightly too far apart

camcomp starts a new *session* whenever more than 60 seconds pass between shots —
long enough to absorb two device clocks disagreeing by a few seconds, short
enough to keep the pairing work bounded. If you were slower than that, matching
photos land in different sessions and are never compared.

```sh
camcomp analyze photos/ --session-gap 120
```

If it is really a clock disagreement, correct that instead — it is more precise:

```sh
camcomp analyze photos/ --time-offset pixel-9-pro-xl=-4
```

The device key is the `deviceKey` shown by `camcomp inspect --format json`; the
value is seconds to add.

### Photos at different zoom levels

camcomp compares *angle of view*, not focal length directly — a fixed focal
ratio is a small difference at the wide end and a huge one at the long end, so a
single ratio cannot separate "same lens, different phone" from "a real zoom
step" (`artifacts/IMPLEM-DROP1.md` §6.11). Two phones' ultrawide cameras get a
looser tolerance than a pair where either side used digital zoom, because that
is a hardware difference rather than a choice the photographer made.

Two complications:

- Two different phones' "ultrawide" cameras can differ in field of view by 25 %
  or more even with no zoom.
- Some phones fold digital zoom into their reported focal length and some do
  not, so the numbers can disagree even when the framing matches. camcomp
  reports both the focal length and the zoom ratio and does not silently
  reconcile them the same way for every device.

If matching photos are being rejected with `no-focal-compatible-peer`, widen the
relevant tolerance:

```sh
camcomp analyze photos/ --fov-tolerance 0.3                 # either photo zoomed or cropped
camcomp analyze photos/ --fov-tolerance-native-lens 0.3      # both at native field of view
```

Check the result. Widening either is the fastest way to create wrong groups.

## 4. Fixing a group by hand

Overrides are recorded separately from what camcomp worked out on its own, so the
original analysis stays inspectable and your corrections survive re-running the
analysis.

```sh
# what are the IDs?
camcomp explain photos/

# put a photo into a group
camcomp override photos/ --add group-61a15592f217=photo-cbec07d0231e

# take one out
camcomp override photos/ --remove group-61a15592f217=photo-cbec07d0231e

# make a different photo the anchor
camcomp override photos/ --anchor group-61a15592f217=photo-cbec07d0231e

# break a group in two
camcomp override photos/ --split group-61a15592f217=photo-aaa,photo-bbb

# join two groups
camcomp override photos/ --merge group-aaa,group-bbb

# permit two photos from the same camera and mode
camcomp override photos/ --allow-duplicate group-61a15592f217
```

Add `--dry-run` to see the effect without writing.

Your decision always wins. If you add a photo camcomp thinks does not belong, it
stays, and the group is flagged so the disagreement remains visible:

```text
group-61a15592f217 — 3 photo(s), min score 0.925 [manual-membership, unverified-pairs]
```

## 5. Getting a transform for a manually added photo

A photo you added by hand may have no alignment, because camcomp never compared it
with the anchor:

```text
    PXL_20260822_095351447.jpg                    NO ALIGNMENT — pair not analyzed
```

Ask for it by naming the photo:

```sh
camcomp analyze photos/ --force-pair photo-cbec07d0231e
```

```text
    PXL_20260822_095351447.jpg                    confidence 0.95 (direct-pair)
```

`--force-pair` overrides the focal/framing prefilter. It accepts a photo ID, in
which case every pair involving that photo is compared, or a single pair ID if
you want to be narrower.

To see what camcomp knows about one photo, including its pair IDs:

```sh
camcomp explain photos/ photo-cbec07d0231e
# or just part of the filename
camcomp explain photos/ PXL_2026
```

```text
photo-cbec07d0231e — g1-pxl-PXL_20260822_095351447.jpg
  configuration pixel-9-pro-xl--hdr-plus  focal35 110 mm  mode hdr-plus
  ungrouped: no-focal-compatible-peer
  pair-photo-61a15592f217-photo-cbec07d0231e
    with g1-oppo-master-IMG20260822115411.heic — incompatible | reason focal-mismatch
```

## 6. Understanding a decision

```sh
camcomp explain photos/ pair-photo-9279ee42f3ef-photo-ea9585ed4ba2
```

```text
pair-photo-9279ee42f3ef-photo-ea9585ed4ba2
  compatible | score 0.969 | overlap 0.97 coverage 0.98 confidence 0.97 appearance 0.96 inliers 2407
```

Or the whole table at once, which is what you want when tuning:

```sh
camcomp analyze photos/ --dry-run --table
```

The five numbers mean:

- **overlap** — how much of the scene the two photos share
- **coverage** — how spread out the matched features are, rather than clustered on
  one object
- **confidence** — how well the geometric fit holds up
- **appearance** — how similar the edges look once the photos are aligned; this one
  is built to survive different exposure and colour processing
- **inliers** — how many features agreed on the geometry

A rejection names its cause. `focal-mismatch` means the zoom levels were too far
apart and no pixels were examined. `gate-failed` means the geometry was not
credible — the reported gate says which check failed. `no-consensus` means no
consistent geometry could be found at all, which is the normal answer for two
genuinely different scenes.

### `uncertain` is not a failure

camcomp reports `uncertain` when the evidence is too weak to decide — a flat wall
or a heavily changed scene, most often. It will not guess. Putting two unrelated
photos in one group silently corrupts a comparison, whereas an `uncertain` costs
you one click, so camcomp is deliberately biased towards admitting it does not
know.

## 7. How well does it work?

On the eight labelled reference sets in `test/`, at default settings:

- **no false positives** — it never put together photos that belong apart
- **every pair it examined, it judged correctly**
- it reproduced 12 of 13 expected groups automatically

The 1 it missed was blocked *before* looking at any pixels, by the
one-photo-per-configuration rule (two shots from the same phone and mode). It is
one command or one override away.

Be aware of two real limitations:

- **The overall score is not a reliable measure of "same scene."** On the reference
  sets, correct pairs score 0.89–0.98 and incorrect ones score 0.88–0.98. What
  actually keeps false positives at zero is the metadata filtering and the
  geometric checks, not the score. Do not tune by score alone.
- **The overlap number cannot see a zoom-in.** If one photo is a zoomed crop of
  another, overlap reads 1.00 — a perfect score for two photos that do not frame
  the same area. The framing tolerance is what catches this, which is why
  widening it is risky.

Full evidence is in [`artifacts/IMPLEM-DROP1.md`](../artifacts/IMPLEM-DROP1.md) §6.

## 8. Speed and size

- A folder re-analyzed with nothing changed takes milliseconds and reads no images.
- Cold, plan on roughly one second per pair compared, plus 0.1–0.4 s to decode
  each photo depending on resolution.
- Change a threshold and the affected results are recomputed; add or remove a photo
  and only the pairs involving it are.
- Every session is analyzed in full: nothing is capped or skipped for cost, so a
  very long burst takes proportionally longer rather than reporting a partial
  result.

Supported formats: **JPEG, PNG, HEIC**. HEIC needs no extra software. RAW is not
supported.

## 9. If something goes wrong

```sh
camcomp validate photos/          # is the analysis file structurally sound?
camcomp analyze photos/ --no-cache   # ignore the cache and start over
camcomp analyze photos/ --dry-run    # change nothing on disk
```

An interrupted write cannot damage an existing `camcomp.json`: the new file is
written alongside and swapped into place only once complete.

If `camcomp.json` was written by an incompatible future version, camcomp says so
plainly instead of misreading it. Delete it and re-analyze.

Exit codes: `0` fine, `1` something failed, `2` bad command line, `3` the analysis
ran but did not meet an `--expect-*` check.

## 10. Command reference

```text
camcomp inspect  PATH...   metadata only, no image analysis
camcomp analyze  PATH...   the full analysis
camcomp validate PATH      structural check of an existing camcomp.json
camcomp explain  PATH [ID] why a pair, group or photo ended up as it did
camcomp override PATH ...  adjust groups by hand
camcomp schema             the JSON Schema for camcomp.json
```

Common options:

```text
--dry-run                  do not write camcomp.json
--no-cache                 ignore any previous analysis
--table                    per-pair metric table
--format json              machine-readable output
--json-progress            newline-delimited progress on stderr
--quiet                    no progress output

--session-gap SECONDS              default 60
--fov-tolerance RATIO              default 0.15 (either photo zoomed or cropped)
--fov-tolerance-native-lens RATIO  default 0.20 (both at native field of view)
--threshold SCORE                  default 0.80
--allow-duplicate-config   permit two photos from one camera+mode per group
--time-offset KEY=SECONDS  correct one device's clock
--analysis-edge PIXELS     analysis resolution, default 1200
--force-pair ID            compare regardless of the filters; takes a photo ID
                           (all its pairs) or a single pair ID
```

Every option you pass is recorded in `camcomp.json`, so a result always says which
settings produced it.

## 11. Browser and desktop application

```sh
mise run web:dev
mise run tauri:dev
```

Both commands use the same static Svelte frontend. The browser command opens the PWA
at `http://127.0.0.1:5173`; the Tauri command adds native analysis and persistence.

### Opening an existing analysis

Use **Open folder** when the folder contains `camcomp.json`, or use **Select files**
and choose one `camcomp.json`. A valid cache is validated and shown immediately. This
load does not hash, analyze, or rewrite any source photo. A JSON-only browser selection
keeps the analysis open and offers **Choose photo source** so you can grant the folder
or files referenced by its relative paths.

Chromium uses a directory handle when available and can offer the recent folder again;
permission must still be granted when the browser asks. Safari and Firefox use recursive
folder input and require reselection after restart. Dragging one folder, one JSON file,
photos, or one JSON file with its photos follows the same rules. More than one JSON file
is rejected as ambiguous.

### Creating or refreshing an analysis

New analysis is available only in Tauri. Before it starts, camcomp shows the selected
source and safe `camcomp.json` destination. It rejects a filesystem or volume root as a
write target. Progress stages, warnings, cancellation, completion, and analyzed/reused
pair counts remain visible. **Refresh analysis** uses Drop 1's incremental cache reuse.
Cancelling or failing never overwrites a previous valid analysis, and malformed or
incompatible JSON is never replaced automatically.

A standalone browser keeps a photos-only selection intact but explains that the desktop
app is required; it never claims to have analyzed or saved those photos.

## 12. Comparison viewer

Choose a group in the left navigation. Every member remains present even when its source
or alignment is unavailable. Two or three photos use columns in a landscape viewport and
rows in a portrait viewport; four use a 2 × 2 grid; larger groups use a deterministic
scrollable grid.

Each photo displays:

- effective 35 mm equivalent focal length, including a zoom-corrected label when Drop 1
  applied that policy, then reported equivalent or physical focal length as fallbacks;
- oriented resolution;
- camera make and model;
- source substitution and alignment status.

Use the mouse, trackpad, or the **Zoom in**, **Zoom out**, and **Fit all** controls. In
**Normalized** mode, every viewer shares the same normalized center and visible span. In
**Experimental global aligned** mode, centers map from the active photo to the group
anchor and then through each target's persisted inverse homography. It remains marked
experimental until the predefined real-landmark gate is recorded. A member with missing, invalid, or
out-of-bounds global mapping falls back visibly to normalized coordinates. This mode does
not perform local correspondence interpolation or per-pixel warping.

Groups containing exactly two photos offer **Comparison slider**. The divider is a
semantic range input with arrow-key support. Switching between grid and slider keeps the
photo order, navigation mode, and canonical center/span.

## 13. Formats, offline use, and privacy

JPEG and PNG display directly. HEIC handling is capability-based:

1. use the exact HEIC when the runtime successfully decodes it natively;
2. otherwise try a same-directory, same-stem `.jpg`, `.jpeg`, then `.png` whose decoded
   dimensions match the analysis;
3. when a CSP-compatible worker is available, show the measured high-memory warning and
   decode only after **Decode HEIC anyway** is selected;
4. otherwise explain that Tauri is required.

Sidecar use is always labelled because compression, color, crop, or content can differ.
The browser worker uses lossless PNG output, runs one decode at a time, cancels when the
album closes, and rejects images above 60 MP. A representative 50 MP comparison measured
about 2 GiB extra Chromium RAM; other applications may become less responsive. Tauri
remains the lower-memory fallback and decodes HEIC off the UI thread into a digest-keyed
lossless PNG in application cache. It resolves at most two distinct uncached HEICs at
once to improve group startup without allowing every group member to allocate a
full-resolution decode simultaneously; duplicate requests for one source are serialized
and reuse the completed cache entry. Neither path creates a display copy in the source
folder. The browser worker's LGPL source and replacement information is available from
**Third-party notices** in the application footer.

The installed PWA caches fingerprinted application-shell assets only. Selected photos,
EXIF, `camcomp.json`, object URLs, and decoded images are not placed in Cache Storage and
are never sent over the network. Offline startup does not imply that a previously selected
folder is still authorized.

RAW files, browser-side grouping analysis, local correspondence-refined navigation,
preference scoring, packaging, signing, and auto-update remain outside this delivery.
