# Drop 2 Full-Viewport Comparison Plan

- **Status:** implementation planned
- **Date:** 2026-08-31
- **Inputs:** `REQUIREMENTS.md`, `IMPLEM-DROP2.md`, the current Svelte/OpenSeadragon implementation, and the reported group-view screenshot
- **Scope:** stabilize synchronized navigation, maximize image area, auto-hide comparison chrome, and add exact 100%/200% pixel inspection controls

## 1. User outcomes

1. A group remains visually stable after loading, resizing, fitting, panning, or zooming. Programmatic updates in one image must never be interpreted as fresh user input from that image.
2. While an album is open, the comparison workspace consumes the complete webview viewport. Images touch one another with no card radius, border, page padding, or grid gap.
3. App, group, photo, metadata, and navigation chrome is hidden while the pointer is over image content. Controls remain discoverable by moving to the top edge, and remain keyboard-accessible through `:focus-within` behavior.
4. The group list collapses to a narrow left-edge activation rail. Moving the pointer to that edge, or tabbing focus into the sidebar, expands it as an overlay without reducing the image viewport.
5. **Fit all** restores a normalized whole-image comparison. **100%** displays one oriented source-image pixel per best-estimate physical screen pixel. **200%** displays one image pixel across two physical screen pixels. Both pixel controls preserve the current shared center coordinate.

## 2. Root cause and invariants

### 2.1 Infinite shrinking

`OpenSeadragonViewerAdapter` currently emits controller interactions from every `animation` and `animation-finish` event. Those events include viewport changes initiated by `applyViewport` and by container reflow. Its `#suppress` flag is cleared after one animation frame, but OpenSeadragon animation and resize events can arrive later. A follower then becomes the navigation leader and publishes its reconciled viewport.

The shared state stores both normalized span axes. When images or containers differ in aspect ratio, OpenSeadragon cannot reproduce both axes exactly while preserving image geometry; `fitBounds` expands one axis. Repeated follower-to-leader round trips therefore compound small aspect/rounding differences and make every image appear to shrink indefinitely.

### 2.2 Navigation invariants

- Only direct pointer/touch navigation or an explicit controller zoom command may publish an interaction.
- Applying state to a follower must be one-way and silent, regardless of animation duration or resize timing.
- Normalized mode shares center and normalized span for Fit all and ordinary proportional navigation.
- Pixel mode shares center and physical display scale, while every adapter derives its own normalized span from its image dimensions and viewport size.
- Changing to 100%/200% preserves `NavigationController.state.center`.
- Fit all intentionally returns to center `[0.5, 0.5]` and normalized span `[1, 1]`, matching current behavior.

## 3. Navigation implementation

### 3.1 Viewer contract

Extend the viewer interaction with a measured `screenPixelsPerImagePixel` value. Extend `ViewerAdapter` with `applyPixelRatio(center, ratio)`.

The OpenSeadragon adapter will:

- observe user-originated canvas drag, scroll, pinch, and double-click events instead of generic animation events;
- publish the resulting viewport after OpenSeadragon has processed the gesture, coalesced through `requestAnimationFrame`;
- explicitly publish after `zoomBy`, because toolbar zoom is an intentional interaction;
- never publish from `applyViewport`, `applyPixelRatio`, image opening, animation completion, or resize-only changes;
- calculate physical display scale from the visible image bounds, container CSS dimensions, and `window.devicePixelRatio`;
- calculate a pixel-mode image rectangle around the supplied normalized center using oriented image dimensions and the container's CSS size multiplied by device pixel ratio, divided by the requested display ratio.

`devicePixelRatio` is the best cross-browser/Tauri estimate available for physical backing pixels per CSS pixel. Browser zoom and OS scaling can affect it; the UI should use the conventional 100%/200% labels without claiming monitor-calibrated physical dimensions.

### 3.2 Controller state

Add an explicit navigation scale mode:

- `{ kind: "normalized", span }`; or
- `{ kind: "pixel", screenPixelsPerImagePixel }`.

Keep the existing public `span` in interaction data for normalized navigation and testing, but apply state according to the active scale mode. Add `zoomToPixelRatio(1)` and `zoomToPixelRatio(2)` controller methods. In pixel mode, a user wheel/pinch update refreshes the shared pixel ratio and applies adapter-specific bounds to peers. Remounted viewers receive the current normalized span or pixel ratio as appropriate.

Clamp invalid/extreme values at the controller boundary and ignore non-finite measured ratios. Continue mapping centers through the selected normalized/aligned point mapper.

## 4. Full-viewport workspace

### 4.1 App shell

When `state.phase === "ready"`, mark the root header/content/footer as comparison mode:

- fix the album content to the viewport (`inset: 0`) with no page padding;
- move the app header into a top overlay included in the chrome reveal area;
- hide the footer in comparison mode;
- avoid changing import, progress, error, and empty-state layouts.

### 4.2 Album shell and sidebar

- Make the album shell fill width and height with a single image-content column.
- Position the sidebar absolutely over the left edge at a high stacking level.
- Collapse it to a narrow transparent/gradient activation rail by default using transform/opacity/pointer-event-safe styling.
- Expand it on `:hover` and `:focus-within`; preserve scrolling and all existing buttons.
- Put previous/group/next navigation in the top chrome overlay rather than reserving document flow height.
- Ensure the empty-album presentation remains usable rather than becoming an empty black viewport.

### 4.3 Group and photo layout

- Make `GroupViewer`, its viewer container, and photo grid fill the available viewport height.
- Remove grid gaps.
- Remove photo card border/radius and minimum heights that exceed the viewport.
- Give each grid cell and OpenSeadragon container a strict `min-height: 0` so grid sizing does not recursively grow from metadata content.
- Overlay the group toolbar, aligned-mode note, photo title/badges, HDR/source warnings, and EXIF metadata above the image rather than allocating rows.
- Hide those overlays by default and reveal them when the shared workspace chrome is hovered/focused. Alerts and loading/error states remain visible so failures are not concealed.
- Keep the two-photo slider full-height and adapt its controls to the same overlay behavior.

Chrome transitions must affect opacity/visibility/transform only, not image-container geometry. This prevents hover from triggering viewer resize and preserves the current center/zoom.

## 5. Controls and accessibility

- Add adjacent **Fit all**, **100%**, and **200%** buttons with unambiguous accessible names.
- Keep existing `+`/`−`, dynamic-range, navigation-mode, and slider controls.
- Top chrome appears on hover and on `:focus-within`, so keyboard users never tab into invisible controls.
- Sidebar expansion also supports `:focus-within`.
- Respect `prefers-reduced-motion` by disabling nonessential chrome/sidebar transitions.
- Do not use `display: none` for focusable hidden controls.

## 6. Test plan

### 6.1 Unit tests

- Navigation controller does not write follower state back to the leader.
- 100% and 200% preserve the canonical center and apply the requested ratio to every adapter.
- Pixel mode survives adapter remounts.
- Pixel-mode interactions synchronize the measured ratio while normalized interactions continue synchronizing span.
- Non-finite/invalid pixel ratios are ignored or clamped safely.

### 6.2 Component tests

- Group toolbar exposes Fit all, 100%, and 200% controls and routes them to the controller.
- Ready album shell exposes full-viewport comparison classes/structure without breaking empty/ungrouped content.
- Hover-hidden chrome remains represented in the accessibility tree and focus-reveal selectors are present through behavioral/style assertions where practical.

### 6.3 Browser tests

Using the synthetic two-photo fixture in Chromium, WebKit, and Firefox:

- assert images have no gap and the comparison region reaches viewport edges;
- assert sidebar is collapsed initially and expands at the left activation edge;
- assert top/photo chrome is hidden away from controls and appears when moving to the top edge;
- click 100% and 200%, verify each OpenSeadragon image’s rendered-to-natural scale within a device-pixel-aware tolerance, and verify the shared center does not jump;
- resize and wait to prove rendered image dimensions stabilize instead of decreasing over successive samples;
- retain existing slider, alignment fallback, HDR, and real-photo smoke coverage.

### 6.4 Required verification

Run, at minimum:

1. targeted navigation/component tests during implementation;
2. frontend formatting check, ESLint, Svelte typecheck, and all Vitest tests;
3. viewer Playwright tests across configured browsers;
4. production frontend build.

If browser rendering reveals platform-specific pixel rounding, use a tolerance of at most one physical backing pixel per viewport edge and document the limitation rather than feeding measured follower bounds back into shared state.

## 7. Risks and mitigations

- **Browser/OS scale ambiguity:** `devicePixelRatio` is the only broadly available browser/Tauri signal; treat it as a best estimate and test calculations independently from a specific monitor.
- **Different image resolutions:** pixel mode must use per-adapter spans, not one normalized span.
- **Different viewport aspect ratios:** center is canonical; each adapter derives bounds locally without publishing programmatic reconciliation.
- **OpenSeadragon event ordering:** coalesce explicit user events after the library handler and cover actual drag/wheel behavior in Playwright.
- **Hidden control discoverability:** retain top/left edge activation areas, focus reveal, accessible names, and visible focus rings.
- **Alerts hidden with chrome:** loading, source errors, and high-memory confirmations remain visible independently.
- **Layout resize feedback:** overlays never participate in grid track sizing, and reveal transitions never alter dimensions.
