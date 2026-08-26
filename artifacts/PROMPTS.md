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