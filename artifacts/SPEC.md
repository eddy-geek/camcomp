# camcomp Specification

## 1. Scope

This document refines the functional requirements in `REQUIREMENTS.md`. It specifies the Drop 1 grouping analysis, its cacheable output, and the alignment data needed by later display drops.

Drop 1 shall:

- inspect photo metadata;
- find candidate photos from the same shooting session and a compatible field of view;
- determine whether candidates show substantially the same scene despite moderate camera movement and different image processing;
- organize compatible photos into editable groups;
- persist the result and analysis evidence in `camcomp.json`;
- retain enough registration data for feature-aligned navigation in a later Drop 3 evolution.

Drop 1 does not perform semantic object recognition, modify source photos, or guarantee pixel-perfect registration of scenes with strong parallax or moving subjects.

## 2. Definitions

- **Configuration**: a device and shooting-mode combination.
- **Oriented image**: image pixels after applying EXIF orientation.
- **Analysis image**: a reduced-resolution oriented image used for feature analysis.
- **Session**: a chronological sequence in which the gap between consecutive corrected capture times is at most 30 seconds.
- **Candidate pair**: two photos in one session that pass metadata prefilters.
- **Compatible pair**: a candidate pair that passes geometric hard gates and has a composite compatibility score of at least `0.80`.
- **Incompatible pair**: a candidate pair with sufficient evidence to conclude the photos do not show the same scene.
- **Uncertain pair**: a candidate pair whose evidence is too weak to decide, for example because one image is low-texture or RANSAC found no consensus. Uncertain is a first-class outcome; it is never coerced to compatible or incompatible.
- **Anchor**: the group photo to whose coordinate system the other group photos are registered.
- **Normalized coordinate**: an oriented-image coordinate represented in `[0, 1] × [0, 1]`, independent of image resolution.
- **Analysis pixel**: a pixel of the analysis image. Distances expressed in analysis pixels are not comparable between photos of different analysis dimensions and are never persisted without their unit.

The compatibility score is an algorithmic confidence score, not a percentage of identical pixels and not a probability. The `0.80` threshold is an initial default to be calibrated on representative datasets.

## 3. Input and normalization

### 3.1 Source identity

Each source photo shall have a stable analysis ID and a fingerprint. The initial fingerprint shall record:

- relative path when a common source folder exists;
- file size;
- modification time;
- SHA-256 content digest.

Only the SHA-256 digest is part of the cache *identity*. Size and modification time are a cheap change-detection fast path: if both are unchanged the digest may be assumed unchanged and not recomputed. A changed size or modification time forces a re-hash but, if the digest matches, all cached analysis for that photo remains valid. Touching a file, changing its timezone metadata, or copying it to another folder must therefore not invalidate analysis.

The digest permits safe cache reuse after renaming and prevents stale analysis when size and timestamps are unchanged. Two byte-identical files with different names are one content identity but two distinct photos; the analysis ID is per file path, the feature cache key is per digest.

### 3.2 Metadata

Where available, Drop 1 shall extract:

- capture timestamp and timestamp source;
- EXIF orientation;
- camera make and model;
- lens model;
- physical focal length;
- 35 mm equivalent focal length;
- image dimensions;
- shooting-mode tags selected by configuration.

Device clocks may disagree. The analysis parameters shall support an optional per-device time offset applied before session detection. A missing or ambiguous timestamp shall be reported and shall require manual grouping unless another explicit fallback is later specified.

### 3.3 Orientation and scale

Feature extraction and all persisted coordinates shall use the oriented image. The original dimensions, oriented dimensions, EXIF orientation, and analysis-image dimensions shall be retained so transformations can be audited or converted to pixel coordinates.

The default analysis image shall preserve aspect ratio and have a longest edge of approximately 1,200 pixels. This value shall be configurable.

## 4. Candidate generation

### 4.1 Sessions

Photos shall be ordered by corrected capture time. A new session begins whenever the gap from the previous photo exceeds 30 seconds. This is a sliding-gap rule; a session may therefore last longer than 30 seconds in total.

The sliding-gap rule has no upper bound: continuous shooting keeps extending one session indefinitely, and a session of `n` photos yields `n(n-1)/2` candidate pairs. Sessions are consequently a *coarse* partition. Section 4.3 discusses cheap prefilters that keep the visual cost of a large session proportional to its useful work rather than capping it.

Many device clocks have one-second resolution and photos from different devices frequently share a timestamp. Session assignment and all downstream ordering shall use a documented deterministic tie-break (corrected timestamp, then configuration ID, then analysis ID) so results do not depend on filesystem iteration order.

Photos need not be consecutive to belong to one comparison group. All plausible pairs within a session may be evaluated after metadata filtering.

### 4.2 Focal-length compatibility

Field-of-view filtering shall prefer 35 mm equivalent focal length. For a pair, the comparison is symmetric and uses the smaller of the two values as the denominator, so the predicate does not depend on argument order:

```text
relativeDifference(a, b) = abs(a - b) / min(a, b)
```

Two photos are focal-compatible when the difference is at most `0.15`. Group membership additionally requires focal compatibility with the group anchor, using the same formula.

Anchor selection happens during group construction (Section 6), after pairwise analysis, so the *prefilter* cannot be anchor-relative without a circular dependency. The prefilter is therefore pairwise and symmetric; the anchor-relative check is a group-construction constraint. This is deliberate: focal compatibility is not transitive, and a group can be narrower than the pairwise relation suggests.

Physical focal lengths from different camera models shall not be directly compared as though they represented the same field of view. If equivalent focal length is missing, a known sensor/crop conversion may be used. Otherwise focal compatibility shall be marked uncertain rather than inferred from incompatible metadata. An uncertain focal comparison shall not exclude a pair from visual analysis; it shall only be recorded, because the visual evidence is the stronger signal.

### 4.3 Cheap prefilters

Pairwise SIFT matching is the dominant cost of Drop 1. Brute-force KNN between two photos with a few thousand descriptors each is on the order of `10^7` 128-dimensional distance evaluations, so cost grows quadratically with session size.

Before full pairwise analysis, candidate pairs are reduced by cheap, order-independent filters:

1. session and focal prefilters (Sections 4.1 and 4.2), which decide most pairs in a large multi-device session from metadata alone, at zero pixel cost;
2. optionally, a global-appearance prescreen on the analysis image — for example a small-image color/gradient histogram or a downscaled perceptual descriptor — used only to *reject* obviously unrelated pairs with a deliberately permissive threshold, never to accept a pair.

Every candidate pair the prefilters do not decide is visually analyzed; there is no cap on the number of pairs analyzed per session. An earlier revision capped evaluated pairs per session and reported the remainder as `uncertain` with reason `budget`. That cap was removed: it could leave a run silently partially analyzed, which is worse UX than a longer run that finishes completely. If SIFT cost on very large sessions becomes a measured problem, the prescreen in item 2 is the intended lever, not a work cap.

A global-appearance prescreen must be validated against the calibration set for false rejections before it is enabled by default; a prescreen that discards true matches is worse than a slow analysis.

## 5. Pairwise visual analysis

### 5.1 Feature extraction and matching

The reference implementation shall use local, scale- and rotation-tolerant features. SIFT is the preferred initial detector and descriptor; AKAZE is an acceptable lower-cost alternative if profiling demonstrates a need.

For each candidate pair:

1. extract keypoints and descriptors from analysis images;
2. find descriptor nearest neighbours;
3. apply a ratio test to remove ambiguous matches;
4. optionally require mutual agreement between forward and reverse matches;
5. fit a homography with RANSAC;
6. retain inlier correspondences and registration diagnostics.

Feature extraction results shall be cached per photo so that one photo is not repeatedly decoded and analyzed for every candidate pair.

### 5.2 Geometric hard gates

A pair cannot be compatible unless its registration is geometrically credible. Initial configurable gates are:

- at least 25 RANSAC inliers;
- acceptable median reprojection error at analysis resolution;
- inliers distributed across at least 5 cells of a `4 × 4` grid in each image;
- estimated common coverage of at least `0.60` of the smaller field of view;
- no degenerate, non-invertible, reflected, or implausibly distorted transform.

Exact reprojection and distortion limits shall be established with the working dataset. A fitted homography based only on a small common object must fail the spatial-distribution gate even if its local reprojection error is low.

Low-texture images may not provide enough features. Such a result shall be `uncertain`, not silently treated as a strong visual mismatch.

### 5.3 Metrics

For pairs that pass the hard gates, calculate normalized metrics in `[0, 1]`:

- **geometric overlap**: computed in the anchor's normalized oriented frame, the area of the intersection between the anchor rectangle and the other photo's rectangle mapped through the homography, divided by the smaller of the two areas in that same frame, clamped to `[0, 1]`. Computing it in one common frame is required: a homography with a large scale factor would otherwise produce a ratio greater than one, and the "smaller area" would be ambiguous;
- **feature coverage**: spatial extent and grid distribution of inlier features in both images;
- **inlier confidence**: a robust combination of inlier count, inlier ratio, and reprojection quality;
- **aligned appearance similarity**: similarity over valid overlapping pixels after registration, using normalized gradients or edges to reduce sensitivity to exposure, white balance, HDR, sharpening, and denoising.

Raw, unregistered SSIM shall not determine compatibility. SSIM may be retained as a diagnostic after registration, but the primary evidence is geometrically consistent local-feature matching.

### 5.4 Compatibility score

The initial score is:

```text
score =
    0.35 × geometricOverlap +
    0.25 × featureCoverage +
    0.20 × inlierConfidence +
    0.20 × alignedAppearanceSimilarity
```

A pair is compatible when:

```text
all geometric hard gates pass AND score >= 0.80
```

The score components, weights, gates, threshold, and algorithm version shall be persisted. The requirement that photos show substantially the same area is represented by geometric overlap and feature coverage.

Scores near the threshold shall remain visible to the user so manual correction does not require rerunning analysis.

#### 5.4.1 Interaction between the threshold and the gates

`0.80` is a much stricter requirement than it appears, and this must be understood before calibrating. The components do not reach `1.0` in practice: real matched pairs in the illustrative structure of Section 8.2 sit at `0.86` overlap, `0.82` coverage, and `0.88` appearance similarity. A pair sitting exactly on the `0.60` overlap hard gate needs

```text
0.35 × 0.60 = 0.21
```

from overlap, so the remaining three components must average about `0.91` to reach `0.80`. In effect the score demands an overlap of roughly `0.72` or more even though the gate admits `0.60`. Either the gate or the threshold is therefore doing nothing useful in that region.

Two consequences:

- calibration shall report the achieved distribution of each component on the labelled set, not only the final score, so that a component with a compressed dynamic range is not silently deciding every pair;
- the threshold and the weights shall be recalibrated together once that distribution is known. `0.80` is a placeholder chosen before any measurement, and Section 9 shall be treated as the authority over it.

Rescaling components against their observed usable range (rather than their theoretical `[0, 1]`) is a permitted outcome of calibration, provided the mapping is persisted with the algorithm parameters.

## 6. Group construction

Pairwise compatibility forms a graph, but connected components shall not directly define groups because compatibility is not necessarily transitive.

The initial grouping procedure shall:

1. choose an anchor that has strong, spatially distributed matches to the largest useful candidate set;
2. add photos that are focal-compatible with and visually compatible with the anchor;
3. check compatibility among non-anchor members where candidate data exists;
4. split or flag a group when members have substantial pairwise disagreement;
5. prefer no more than one photo from a configuration in a group, while allowing an explicit manual override.

The anchor shall remain stable when a cached analysis is reloaded. Changing the anchor requires recomputing or composing alignment transforms and recording the resulting provenance.

A photo that joins no group shall be recorded explicitly as ungrouped, together with the reason (no session peer, no focal-compatible peer, all pairs incompatible, all pairs uncertain, unusable timestamp, unreadable file). Ungrouped photos are a normal outcome and a primary input to manual correction; the document is not a valid analysis if some inputs simply do not appear in it.

A group of one photo is not produced automatically. A group becomes meaningful at two members.

Automatic group membership shall be editable. Manual operations shall include adding a photo, removing a photo, splitting a group, merging groups, and selecting another anchor. Manual overrides shall be stored separately from automatic evidence so the original analysis remains inspectable.

Manual operations may create states the automatic rules would reject, for example a group whose members were never pairwise analyzed or were judged incompatible. Such members shall be accepted, flagged, and — where alignment is needed by Drop 3 — analyzed on demand. The user's decision wins over the automatic evidence, but the disagreement stays visible.

## 7. Persisted alignment model

### 7.1 Global mapping

Each non-anchor group photo shall store an invertible `3 × 3` homography mapping its normalized oriented coordinates into the anchor's normalized oriented coordinates:

```text
anchorPoint ~ photoToAnchorHomography × photoPoint
```

The inverse may be calculated at load time, but it may also be persisted with a validation checksum or residual to simplify clients. Matrix ordering and coordinate direction shall be explicit in the JSON schema.

Normalized oriented coordinates allow Drop 3 to use the transform at full image resolution even though Drop 1 estimated it from a thumbnail.

### 7.2 Sparse local mapping evidence

A global homography cannot fully represent parallax, depth changes, lens distortion, or moving subjects. Drop 1 shall therefore also retain a bounded, spatially distributed set of RANSAC inlier correspondences. Each correspondence contains:

- normalized point in the photo;
- corresponding normalized point in the anchor;
- descriptor-match distance or normalized match quality;
- reprojection error, expressed in normalized anchor units;
- optional local scale and orientation.

Every persisted distance shall be unambiguous about its unit. Hard gates and pair diagnostics are evaluated in analysis pixels, because a pixel threshold is what a RANSAC estimator consumes and what is comparable to detector localization accuracy. Persisted correspondences are in normalized anchor units, because they must survive a change of analysis resolution. Fields that carry a pixel value shall be named accordingly (for example `medianReprojectionErrorPx`) and the analysis dimensions they refer to shall be retrievable from the photo record.

Approximately 100–300 high-quality correspondences per aligned photo are sufficient for the initial format. Sampling shall preserve spatial coverage rather than merely choosing the lowest descriptor distances.

Descriptors and complete detector output need not be embedded in `camcomp.json`; they may be held in a separate versioned cache if later re-analysis requires them.

### 7.3 Drop 3 pointer mapping

Given a pointer over any source photo, a future Drop 3 implementation can:

1. map the source point to anchor coordinates using its global homography;
2. map the anchor point into each target photo using the inverse target homography;
3. center each target viewport on the mapped point;
4. where enough nearby correspondences exist, refine the mapping with a local piecewise-affine mesh or another bounded local interpolation;
5. fall back to the global homography or normalized image coordinates outside reliable regions.

The UI shall expose alignment confidence and avoid abrupt viewport jumps when a local mapping is unavailable. Registration describes static scene geometry; a moving subject may legitimately map poorly.

## 8. `camcomp.json`

### 8.1 Versioning and cache behavior

The file shall contain:

- schema version;
- application and algorithm versions;
- analysis parameters and metric weights;
- source root and photo fingerprints;
- extracted metadata and configurations;
- session assignments;
- pairwise results and diagnostics for every candidate pair;
- automatic groups;
- ungrouped photos with their reason;
- manual overrides;
- group-relative alignment data.

Unknown fields shall be ignored by compatible readers. A schema-major-version mismatch shall produce a clear error rather than silently interpreting incompatible data.

On reload, unchanged photo fingerprints may reuse cached metadata, features, pair scores, and alignments. Changed, added, or removed photos shall invalidate only affected pair and group results where possible.

### 8.2 Illustrative structure

```json
{
  "schemaVersion": "1.0",
  "algorithm": {
    "name": "sift-homography-gradient-v1",
    "version": "1.0.0",
    "parameters": {
      "sessionGapSeconds": 30,
      "fieldOfViewTolerance": 0.15,
      "compatibilityThreshold": 0.8,
      "analysisLongestEdge": 1200
    },
    "weights": {
      "geometricOverlap": 0.35,
      "featureCoverage": 0.25,
      "inlierConfidence": 0.2,
      "alignedAppearanceSimilarity": 0.2
    }
  },
  "photos": [
    {
      "id": "photo-001",
      "path": "camera-a/image-001.jpg",
      "fingerprint": {
        "size": 1234567,
        "modifiedAt": "2026-08-23T10:20:30Z",
        "sha256": "..."
      },
      "dimensions": {
        "original": [4032, 3024],
        "oriented": [4032, 3024],
        "analysis": [1200, 900]
      },
      "metadata": {
        "capturedAt": "2026-08-23T10:00:00Z",
        "cameraModel": "...",
        "focalLength35mm": 24
      },
      "configurationId": "configuration-001"
    }
  ],
  "pairs": [
    {
      "a": "photo-001",
      "b": "photo-002",
      "state": "compatible",
      "score": 0.87,
      "hardGatesPassed": true,
      "metrics": {
        "inliers": 84,
        "inlierRatio": 0.63,
        "medianReprojectionErrorPx": 1.4,
        "geometricOverlap": 0.86,
        "featureCoverage": 0.82,
        "inlierConfidence": 0.91,
        "alignedAppearanceSimilarity": 0.88
      }
    }
  ],
  "groups": [
    {
      "id": "group-001",
      "anchorPhotoId": "photo-001",
      "photoIds": ["photo-001", "photo-002"],
      "alignments": [
        {
          "photoId": "photo-002",
          "coordinateSystem": "normalized-oriented",
          "direction": "photo-to-anchor",
          "homography": [1, 0, 0, 0, 1, 0, 0, 0, 1],
          "correspondences": [
            {
              "photo": [0.25, 0.4],
              "anchor": [0.27, 0.39],
              "matchQuality": 0.94,
              "reprojectionErrorNormalized": 0.0012
            }
          ],
          "confidence": 0.9
        }
      ],
      "manualOverrides": []
    }
  ],
  "ungrouped": [
    {
      "photoId": "photo-003",
      "reason": "no-compatible-peer"
    }
  ]
}
```

The example is informative. A formal JSON Schema shall be introduced with the implementation and tested using round-trip fixtures.

## 9. Calibration and acceptance

Calibration uses several real photo sets, eventually including:

- true matches with translation, rotation, perspective, and small focal changes;
- different exposure, white balance, HDR, and sharpening;
- repeated architectural patterns;
- adjacent but different scenes from one session;
- low-texture scenes;
- foreground parallax and moving subjects;
- portrait/landscape orientation changes;
- incorrect or offset device clocks.

For each labeled pair, record the score, every component, the hard-gate outcome, and the expected result. Threshold changes shall be evaluated against both false positives and false negatives. False positives are particularly harmful because they can silently place unrelated photos in one comparison group; uncertain cases should be presented for human review.

Calibration needs a stated objective, otherwise "minimize both" is unactionable. The initial objective is: **no false positives on the labelled set**, then maximize recall, with any residual ambiguity emitted as `uncertain` rather than as a decision. A false negative or an uncertain pair costs the user one manual grouping action; a false positive silently corrupts a comparison. These are not symmetric errors and the tuning must not treat them as such.

Thresholds derived from a small number of sets are provisional and shall be marked as such in the persisted parameters. They become defensible when the labelled sets span several devices, scenes, and lighting conditions, not when a particular pair count is reached.

This should be complemented by synthetics tests where appropriate.

## 10. Open decisions

- Exact EXIF or application-specific source for shooting modes.
- Reprojection and transform-plausibility limits after dataset calibration.
- Whether per-device clock offsets are entered manually or inferred from high-confidence visual matches.
- Whether large feature caches are embedded, stored beside `camcomp.json`, or always regenerated.
- Whether a global-appearance prescreen (Section 4.3, item 2) is worth enabling once per-pair cost on very large sessions is measured.
- Whether the composite score survives calibration at all, or is replaced by a small decision tree over the gates and components, which would be easier to explain to the user and easier to defend than opaque weights.
- Handling of timestamps without timezone information when devices are in different timezones, which is a plausible failure mode for a phone plus a camera.
