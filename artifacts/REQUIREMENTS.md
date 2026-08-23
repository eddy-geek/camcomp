# camcomp - Functional Requirements

## 1. Goal

Build an application that can compare photos taken by several cameras or phones, or several modes of the same phone.

## 2. High-level milestones

- **Drop 1** – Grouping algorithm : identify compatible photo groups and export a cacheable JSON file.
- **Drop 2** – Import interface: open a folder, select files, drag-and-drop, and (re)load a previous JSON analysis.
- **Drop 3.1** – Side-by-side photo display for a group.
- **Drop 3.2** – Configurable EXIF tag display, random order, and optional blind-test mode.
- **Drop 4** – Blind test scoring between two configurations.

## 3. Drop 1 – Photo grouping

- Input: a set of photos from multiple devices or modes.
- The algorithm must identify groups of "compatible" photos.
- Compatibility rules:
  - photos part of single 'shooting session' are normally taken less than 30 seconds from each other (use a sliding window of 30 seconds between each photo);
  - they do not have to be consecutive in chronological order : a group is a subset of photos from a shooting session;
  - same focal length, within 15% of the group's reference photo (the *anchor*, see `SPEC.md`; not necessarily the chronologically first photo);
  - same scene/landscape/surface. What I mean by this is that the photos should cover the same area of the scene, with some tolerance for lateral/vertical/angular movement of the camera or subject.
- The algorithm should be tuned to minimize both false positives and false negatives (the exact metrics and acceptable thresholds are to be determined empirically).
- There should be an easy way for the human to adjust the grouping.
- Photos that end up in no group (ungrouped singletons) must still be listed in the output, with the reason they were not grouped, so the user can group them manually. Grouping must not silently discard input.
- Output: a JSON file that identifies groups and the devices that took each photo, and that can be cached and reloaded later.
- When importing from a folder, the application should automatically look for an existing camcomp.json analysis file.
- The application must never modify, move, or rename the source photos. The only file it writes into the source folder is `camcomp.json`.

## 4. Drop 2 – Import

- The user can either:
  - open a local folder, or
  - select / drag-and-drop a set of files.
- The user can also drag-and-drop a JSON file from a previous run.
- The application should detect any existing camcomp.json analysis file in the opened folder.
- The JSON export produced by Drop 1 should be saved as part of this drop.

## 5. Drop 3 – Side-by-side display

- For a selected group, show all of its photos on one page.
- Layout must adapt to the number of devices and to the screen shape:
  - 2 or 3 devices: columns or rows depending on viewport orientation;
  - 4 devices: shown as a square (2x2) grid.
- The user can zoom into the photos.
- When panning/zooming, the application should try to keep the center of each photo aligned across all the displayed images.
- In the first version of this phase, the application does **not** attempt to align/overlay photos that are large and slightly shifted, it only does synchronized zoom & panning based on image coordinates.
- in a further evolution, when moving the pointer across a zoomed-in image,  even if the images were not perfectly visually aligned, the viewports shows properly aligned features for visual comparison.
- A fixed list of EXIF tags should be shown by default : focal length, resolution, camera model

## 6. Tags and blind mode

- Allow to configure list of EXIF tags for each photo.
- One dedicated configurable tag may identify special shooting modes (e.g. "Master mode" on the Oppo Find X9 Pro) (see [Q10]).
- The order of the photos on the page is randomized.
- Two display modes:
  - normal: show the tags immediately;
  - blind test: hide all EXIF tags until the user clicks then reveal them for all the pictures in the group.
- Click interaction in blind mode: the first click reveals the tags for the whole group, the second click advances to the next group. 
- A click *on a photo* selects/prefers that photo (and, in blind mode, reveals); a click on the page background or a dedicated control advances to the next group.

## 7. Drop 4 – Blind test scoring

- The user first selects two or more configurations from the complete list of configurations detected by the grouping step. A "configuration" is a device+mode combination.
- Only groups that contain a photo from *every* selected configuration are scored; groups with partial coverage are skipped  so the totals stay comparable.
- The same side-by-side display from Drop 3 is used.
- The application records which of the displayed photos the user clicks as their preferred one. With more than two configurations this is a "pick the best of N" vote, not a pairwise duel; the score is therefore a plurality count, and the number of scored groups must be reported alongside the totals.
- Each preferred photo scores one point. Ties/abstentions ("no preference") must be expressible, otherwise the user is forced to invent a preference and the result is noise.
- At the end of the album, the application shows the total points obtained by each device and each configuration, plus the number of groups scored and the number skipped.
- Scoring results must be persisted (in `camcomp.json` or a sibling file) so a session can be interrupted and resumed.

## 8. Non-functional expectations

- Target dataset size for Drop 1: hundreds of photos per folder, a handful of photos per shooting session. Beyond that, analysis time must degrade gracefully (see the pair-budget rule in `SPEC.md` §4.3) rather than becoming quadratically unusable.
- Analysis must be interruptible, must report progress, and must be resumable from the cached `camcomp.json`.
- Photos are private data. In the default (local/desktop) delivery, no photo or metadata leaves the machine.

## 9. Open questions / non-obvious blanks

Question identifiers are stable and not necessarily contiguous; they are never reused.

[Q10] What is the exact tag name / value to use to identify special modes such as the Oppo Master mode?  to be checked in the EXIF data if one exists, otherwise use a title or comment tag ?
