=== === === === === === === === === === === === 
DROP 1
=== === === === === === === === === === === === 

---
---
---
read requirement, spec and plan in ./artifacts ; implement the plan. your goal is to get it to build, pass unit-test, and complete the ./test/tstgrp* scenarii without crash, at least on 1 platform (desktop or web)
---
write your notes so far to IMPLEM-DROP1.md and continue
---
update implementation notes incl a detailed status of your new tests + of the tstgrp* calibration tests. (there may be mistakes in the user-made tagging of groups, 100% not needed) and write a new user-facing doc/ then continue your wrap-up. I've tried to fix things so you can run commands but if obvious commands fail stop early.
---
start from requirements (for broad context) and #[[artifacts/IMPLEM-DROP1.md]] for the current status. 1) relax a bit the grouping to to match a few more of the calibration sets (goal is NOT to cover 100% at all costs relax the focal so that both wide angle (0.6x of oppo and 0.5x of pixel) 2) when running the calibration suite, attempt a run without the 30s time span detection, in order to more thoroughly test the other part fof the algo 3) harden the oppo mode detection here is a sample over a much larger set of pics: ``` exiftool -UserComment **/*.heic | rg '^User' | sort | uniq -c 2 User Comment : oplus_13156483328 2 User Comment : oplus_13156483360 112 User Comment : oplus_537919488 54 User Comment : oplus_537919520 23 User Comment : oplus_537919744 20 User Comment : oplus_537919776 61 User Comment : oplus_537927680 17 User Comment : oplus_537927712 11 User Comment : oplus_605028352 1 User Comment : oplus_672137216 1 User Comment : oplus_672137248 2 User Comment : oplus_8861515808 2 User Comment : oplus_8861523968 2 User Comment : oplus_8861524000 ``` I have mostly/only used "auto", "master", and "hi-res", at various focals. I think main can shoot in high-res spontaneously though. and master might not be distinguished easily except for shorter exposure (darker) #1 is oplus_537919488 which is certain "auto" I can do a dedicated shoot if unclear
---
continue, make sure to apply your initial insight to use the FOV ratio instead of the 35mm focal length ratio
---
suddenly the tests seem to take a loooong time. is it expected ? I killed 1 test which seemed stuck for a very long time taking 100% of all CPUs. maybe we should have timeouts to avoid that ? riught it's ./camcomp/target/debug/deps/reference_sets-61a3f29c88e61908 which is very long. I'm noty clear iof thats' expected, but that's really too long if it's not a bug, maybe downscale the images more ?? make sure to investigate before any decision. start with an update to current IMPLEM notes so we don't get lost.
---
it's been running for at least 5 minutes, with RAM usage varying but right now at 1.8G virtual / 700M resident, so I suggest not waiting and investigating the bug
---
I'm worried you're overcomplexifying, by wanting to keep the 5-test architecture. be more aggressive and creative in reworking how the coveraage is distributed accross tests, and as well youy can have build a much smaller sample for some of them I guess. last resirt, even bundling all current coverage in single test would be acceptable
---
turn this into a PLAN-TEST-REFACTOR.md and stop there.
---
implement #[artifacts/PLAN-DROP1-TEST-REFACTOR.md]]
try separately the rust `heic` crate to see if it speeds up decode in both debug or release. if so, switch to it, AGPL is not an issue, just create an AGPL LICENSE.md anyway.
---
estimate time needed to run the algo on all pictures in ./data/* eg simulate dumbed-down metadata pre-filter to estimate how many picture comparison will be needed, times the time of 1 comparison

---
the 3 pictures in this group are (I think) g1-oppo-ai-IMG20260822115355.heic focal35: 140 mm - FOV 14.7 deg g1-oppo-master-IMG20260822115411.heic - focal35: 140 mm - FOV 14.7 deg g1-pxl-PXL_20260822_095351447.jpg - focal35: 110 mm - FOV 18.6 deg check how many more pictures & picture pairs would be analyzed in the whole ./data/* folders, if we were to relax the angular difference to include that one (to 0.15 or 0.21, whichever works) if it seems acceptable, do it also I see many ` "budgetSkippedPairs"` in my output, this is bad UX to silently do half the work... either remove the budget feature or ask when budget is exhausted

---
for budget, KISS, just remove the feature and mark progress a bit more clearly (eg Step 3/3 ; X% completed)
for the new FOV threshold, prrfect, continue, I have fixed grp5 accoridingly, highlight if any other needs fixing


=== === === === === === === === === === === === 
DROP 2
=== === === === === === === === === === === === 

---
Q: Which delivery target should PLAN-DROP2 treat as required?
Even if drop 1 may end up desktop-only bcause of image analysis constraints, I would like drop 2 to be web-first, static PWA as much as possible. it is fine if it is chrome-only (so that eg File System Access API / OPFS can be used to load full folder easily). if not, fall back to tauri as needed.
Q: What should replace the plain-JavaScript DROP1 spike frontend?
check availability of high-level components that reduce complex code to choose. ideally I want to be able to switch between a 'comparison slider' mode like https://github.com/NUKnightLab/juxtapose/tree/master (only available for groups of only 2 images) ; and the specified "synchronized pan & zoom" I'm wondering if OpenSeaDragon is useful at our 50 MP scale -- maybe not but its UX is nice anyway, and it does synchonized pan and zoom like this
```js
var duomo = {
  Image: {
    xmlns: "http://schemas.microsoft.com/deepzoom/2008",
    Url: "//openseadragon.github.io/example-images/duomo/duomo_files/",
    Format: "jpg",
    Overlap: "2",
    TileSize: "256",
    Size: {
      Width:  "13920",
      Height: "10200"
    }
  }
};

var viewer1 = OpenSeadragon({
  id: "viewer1",
  prefixUrl: "//openseadragon.github.io/openseadragon/images/",
  tileSources: duomo
});

var viewer2 = OpenSeadragon({
  id: "viewer2",
  prefixUrl: "//openseadragon.github.io/openseadragon/images/",
  tileSources: duomo
});

var viewer1Leading = false;
var viewer2Leading = false;

var viewer1Handler = function() {
  if (viewer2Leading) {
    return;
  }

  viewer1Leading = true;
  viewer2.viewport.zoomTo(viewer1.viewport.getZoom());
  viewer2.viewport.panTo(viewer1.viewport.getCenter());
  viewer1Leading = false;
};

var viewer2Handler = function() {
  if (viewer1Leading) {
    return;
  }

  viewer2Leading = true;
  viewer1.viewport.zoomTo(viewer2.viewport.getZoom());
  viewer1.viewport.panTo(viewer2.viewport.getCenter());
  viewer2Leading = false;
};

viewer1.addHandler('zoom', viewer1Handler);
viewer2.addHandler('zoom', viewer2Handler);
viewer1.addHandler('pan', viewer1Handler);
viewer2.addHandler('pan', viewer2Handler);

```

I feel vanilla JS or lightweight (Svelte) may be enough but do think about it
Q: Should DROP2 include a graphical UI for the completed core’s add/remove/split/merge/re-anchor overrides?
Defer editing
Q: When explicitly selected or dropped photos span multiple directories, where should camcomp.json be saved?
First common root




Q: What must DROP2 deliver for analysis in the static PWA?
Tauri analysis only
Q: For arbitrary files selected/dropped in the browser, how should the JSON output be saved?
skip
Q: When an opened folder contains a valid camcomp.json, what should happen automatically?
Load without scan
Q: Should the plan commit to Svelte 5 + TypeScript + Vite, while deferring viewer dependencies to a DROP3 gate?
PLan for both DROP2 and DROP3 (viewer). give me further detailed analysis in ADR-DROP3.md with pros/cons and your recommendation for this use-case



Q: How should the implementation plans be split now that DROP3 is included?
one combined plan. also check if https://github.com/Loganavter/Improve-ImgSLI which seems to have a similar goal to DROP3 could be use, maybe forked, in order to reduce implementation effort
Q: Which DROP3 requirements should the plan cover?
Drop 3.1 only
Q: Should this DROP3 plan include feature-aligned pointer navigation using DROP1 homographies/correspondences?
I would say (B) but it depends how hard it is. for sure, do say in the ADR what are the impacts on the archi choices, and then let me decide before the plan
Q: Must the standalone Chrome PWA display HEIC photos in DROP3?
can we supoprt only Safari ? or does it make the folder load more difficult ?


Q: Which ADR D1 browser contract should the combined plan require?

I would say try to cover A and D : degrade gracefully from native Safari towards js decoding with something like

```
import { heicTo, isHeic } from "heic-to"

const file = fileInput.files[0]

// Check if the file is HEIC/HEIF
if (await isHeic(file)) {
  // Convert to JPEG
  const jpeg = await heicTo({
    blob: file,
    type: "image/jpeg",
    quality: 0.8
  })
  
  // Convert to PNG
  const png = await heicTo({
    blob: file,
    type: "image/png"
  })
}
```

in the non-safari case, also attempt to load FOO.jpg before FOO.heic, so that the user can optimize easily by converting offline
and same for file, support best UX available on each of Chromium-based/Safari/firefox browsers.

Q: Which ADR D2 navigation scope should DROP3.1 include?
Normalized plus global
Q: Do you accept the ADR recommendation of Svelte + TypeScript + Vite for the shared PWA/Tauri frontend?
Accept Svelte
Q: Do you accept the gated OpenSeadragon/tile-pyramid strategy for approximately 50 MP photos?
do "7.3 OpenSeadragon with simple image sources" i.e. us OpenSeaDragon so we can easily switch to a pyramid, but we'll only do that if we run into issues with the simple one.

<quote>

The heic-to/csp spike used a disposable outer worker around its decoder worker,
ImageBitmap, lossless OffscreenCanvas PNG output, a 256 MiB compressed-input guard,
a 60 MP decoded-pixel guard, and one-at-a-time scheduling. Results on the local macOS
arm64 host with Playwright Chromium 149:

- all 18 Drop 1 HEIC fixtures decoded at the Rust-reported oriented dimensions;
- coverage included 4096×3072, 5888×4416, 8192×6144, 3072×4096, and 6144×8192;
- end-to-end decode/display times were 2.7–7.6 seconds per image;
- the production CSP stayed free of unsafe-eval;
- terminating the album during an in-flight 50 MP decode cancelled cleanly;
- one 8192×6144 HEIC plus one 4080×3072 JPEG viewer raised total Chromium RSS from
  292 MiB to a 2,120 MiB peak: **+1,828 MiB** for one decoded HEIC comparison.

Correctness, orientation dimensions, CSP, and cancellation passed, but the 50 MP memory
result is unacceptable for a product intended to show two to four photos, and color
profile fidelity was not independently reviewed. Gate A therefore rejected heic-to;
the spike dependency and runtime code were removed, so no LGPL component or obligation
ships. The accepted production path remains sidecar then Tauri, as required when no
</quote>
I guess PNG will also raise RAM a bit... I thing 2 GB is still (barely) acceptable, until we optimize later.
do include it, with a user warning of high ram


=== === === === === === === === === === === === 
DROP 3 HDR
=== === === === === === === === === === === === 

---
start at @README.md  . we are at @IMPLEM-DROP2.md .
images are usually HDR, either built in heic or with JPG Ultra HDR gain map which is supportedby major browsers:  Google Chrome, Microsoft Edge, Brave, Opera, and Apple Safari fully support JPEG Ultra HDR (gain map) images (while Mozilla Firefox falls back to standard SDR)

audit whether all the rendering path (eg conversion + openseamap rendering) will support it properly ie forward the gain map and use compatible browser rendering primitives. when it does not (eg PNG I guess ??) investigate alternatives with their pros/cons. highlight where it's not possible at all (we should warn the user when falling back to SDR eg on Firefox). Do not code but make a report + ADR in ADR-DROP3-HDR.md

---
agree with all decisions. . the HEIC I have currently do not seem HDR-y, if confirmed, there is no more need for  the heic HDR rendering.
de-risk what you can then create PLAN-DROP3-HDR.md ready for implementation

---
I put a lot more pictures in the data/metadata-calibration folder. each picture shows the mode that was used in the image content (highlighted in blue in the word doc being photographed) to make it easier. it covers heic, hi-res, flash, ...
note that 50 MP hi-res is supposed to not have HDR (aka Pro XDR) by design, the rest is supposed to have it
 use that to validate and extend the existing metadata parsing setup. Add a dedicated SPEC-OPPO.md file documenting the metadata, and replace any outdated markdown content on the topic by a ref to it. commit.
 I have manually renamed the image's file name with the characteristics (although it can be a bit inconsistent especially Flash setting)

---
context @REQUIREMENTS.md  and @IMPLEM-DROP2.md
I have issues with the group comparison display
* bug : the app behaves like if I where zooming out permanenetly, infinite loop where images get smaller and smaller. maybe rounding behaviour + each image reacting to other image's adjustement  ?
* UI : I want 100% of the chrome to be dedicated to the image content, no margins, all the chrome must disappear unless I move mouse ; left sidebar collapses almost completely and only expands if I move mouse to the left ; no margin between the picture
* feat : I want 2 button to trigger '100%' and '200%' besides 'Fit all' for pixel peep. they keep but the same center coordinate and increase zoom so that 1 pixel == 1  screen pixel (determined as well as possible within web/tauri APIs...)

---
bug: part of the chrome is not hidden, see screenshot - middle picture in of 3 in this case

---
remove the top overlay showing mostly just 'camcomp' title. it prevent clicking the controls under it

---
start from @README.md .
heic decoding in the tauri version is very slow, it takes 100% CPU during 40 seconds to decode 1 image then moves to next one!
can you investigate
- parallel handling when multiple images in a group
- multithreaded decode
- GPU acceleration 
feel free to reconsider the heic library used.
you may as well profile to figure out the bottlenecks.

---
start from README. in the UI, comparison page, left overlay with all groups, groups should be shown in sorted time order, with a clear separator when there are different days, and time clearly indicated below the group.
also put all the per-picture metadata at the bottom instead of the current split split title-on-top/exif-at-the-bottom
at the end commit just your work


---
* add hotkeys eg arrows to pan, pgup/dn  for prev/next group, and space for next group, +/- for zoom in/out
* look at screenshot, the chrome is too opaque and takes too much space esp. at the bottom, make it more compact, HDR/EXIF warning can also be mouse over a simple warning icon
* update needed md files and commit

---
start from README. in the UI, comparison page, left overlay with all groups,  make it more dense and useful : add focal length of the anchor picture ; and fit things in 2 lines, remove separator and give it a flatter light look. commit

---
start from README.md ; in the UI, comparison page, I have issues 
 [1] HDR metadata detection 
 [2] 'Missing source: object object'
look at ./data/camcomp.json group 132 for an example
also, popups are truncated and I woul like popups on all truncated text field
most issues are visible in attached screenshot of group 132

---
- I now have a clearer error message: 'Missing source: The HEIC source could not be decoded.' -> fix it
- and still a 'Decoded copy HDR?' (unclear message!) "HDR metadata unverified", maybe because some cache needs to be cleaned ? see screenshot.

---
start from README, and @PLAN-DROP3-HDR.md 
"""
- Tauri HEIC display uses heif-oxide, resolves up to two distinct uncached photos
  concurrently, and writes digest-keyed SDR PNGs only to application cache storage. PNG
  encoding is lossless, but HDR HEIF output is not preserved. Files whose .heic suffix
  contains JPEG data display directly by detected content. Duplicate requests share the
  same cache result; the source folder is never modified.
"""

I want HDR support in tauri + google-chrome as well, eg with using not png but jpeg hdr as intermediate format. search for a better solution ; append to @PLAN-DROP3-HDR.md your plan, then implement.

---
start from README.md and artifacts/IMPLEM-DROP3-HDR.md 
in the comparison view, bottom metadata, HDR section:
- in the popup display the rough number of HDR stops in the image if able to compute it
- in the popup display 'GM' if a gain map was detected
- Check why 'HDR metadata unverified' is still displayed for both heic and jpg.

---
start from README.md ; artifacts/IMPLEM-DROP3-HDR.md ; and last commit desc

in the comparison view, bottom metadata, HDR section, Check why 'HDR metadata unverified' is still displayed for jpg ; and tell what to expect in terms of the current heic pictures. can you find any pictures from oppo in the calibration or in .data/ with HDR content ? in which format, gainmap or else ? (I know the pixel files named PXL_* have it)

at the end fix all the 'unused' rust build warnings.

---
start from README.md ; artifacts/IMPLEM-DROP3-HDR.md ; and last commit desc in the comparison view, bottom metadata, HDR section, Check why 'HDR metadata unverified' is still displayed for jpg ; and tell what to expect in terms of the current heic pictures. can you find any pictures from oppo in the calibration or in .data/ with HDR content ? in which format, gainmap or else ? (I know the pixel files named PXL_* have it) at the end fix all the 'unused' rust build warnings.

---
- add the relevant information to the corresponding files eg IMPLEM-DROP3-HDR.md, SPEC-OPPO.md. - "camcomp's current HDR inspector only handles JPEG Ultra HDR gain maps. True HEIC/HEVC containers may carry HDR via different mechanisms , but none of those are inspected yet." --> fix that. commit

---
start from README. in tauri app, once I have opened a folder, there is no way to refresh / open another folder. can you add an Menu > Open ? and a refresh on F5 ?


---
in the comparison view in the bottom metadata panel I  do not see the Oppo mode (master mode etc) 