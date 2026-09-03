# OPPO Find X9 Pro metadata specification

## Scope and evidence

This specification is the authoritative description of the OPPO Find X9 Pro
metadata handling in camcomp. It supersedes the historical OPPO notes in
`IMPLEM-DROP1.md` and the short description in `docs/GUIDE.md`.

The current calibration corpus is `data/metadata-calibration/`. Its image files
were renamed with the mode, focal length, orientation, format, and (where
available) the camera UI's flash setting. The names are labels for validation,
not metadata sources: the flash setting is explicitly known to be inconsistent.
The photographed blue selection is useful corroborating evidence, but production
classification must use encoded metadata.

The corpus contains 32 image files from one OPPO Find X9 Pro session:

- 14 `.jpg` files, all physically JPEGs with Ultra HDR gain-map XMP metadata;
- 18 `.heic` files, of which some are physically JPEGs despite their extension;
- true HEIC samples at 8192 × 6144 (50 MP), 4096 × 3072, and other dimensions.

`exiftool` is useful for investigation, but it warns about the camera's malformed
EXIF `UserComment` character-code prefix. camcomp therefore recovers the
printable payload directly.

## Metadata sources and precedence

For an OPPO file, camcomp reads:

1. `UserComment` — the malformed but recoverable `oplus_<integer>` mode code;
2. `Flash` — the standard EXIF bit field, used only to record whether the flash
   fired;
3. EXIF dimensions and decoded/probed dimensions;
4. file magic and container metadata, independently of the filename extension;
5. JPEG XMP/MPF gain-map metadata in the frontend HDR inspector.

A missing or unrecognized value is retained as missing or unknown. A filename is
never allowed to manufacture a mode or an HDR claim.

## `UserComment` mode code

The mode code is an unsigned decimal integer. Camcomp masks the verified bits,
identifies the verified pipeline markers, and retains any remaining profile bits
in `modeTagUnknownBits`. An unknown profile is appended to the configuration
label, for example `auto-x200140000`, instead of being silently folded into
`auto`. The photo still proceeds through inspection and grouping.

### Verified mode and capture markers

| value | meaning | configuration identity |
| --- | --- | --- |
| `0x0100` | Master mode | yes: `master` |
| `0x2000` | 50 MP high-resolution mode | yes: `hi-res` |
| `0x10010` | Portrait-mode composite marker observed in the calibration set | yes: `portrait` |
| `0x0800` | Night mode | yes: `night` |
| `0x1000` | Text mode | yes: `text` |
| `0x0020` | Portrait-orientation capture | no: `modeFlags: ["portrait"]` |
| `0x04000000` | Burst frame | no: `modeFlags: ["burst"]` |

`0x10010` is intentionally documented as a composite marker. The corpus
establishes that the complete pair of vendor bits identifies Portrait mode; it
does not establish independent meanings for each constituent bit.

`Flash` is not a reliable mode marker. The files named `Flash_Auto` and
`Flash_Always` can share the same `UserComment`, and EXIF records the result
(`On, Fired` or `Auto, Did not fire`) rather than the camera UI setting. One
clear filename inconsistency is
`1x 23mm Mode_Master Orient_Portrait Format_JPG Flash_Always.jpg`: its EXIF says
`Off, Did not fire`. camcomp therefore exposes only the non-identity
`flash-fired` flag. It does not claim to recover `Auto`, `On`, or `Always` from
EXIF.

### Observed mode-code families

The following are the mode families established by the renamed calibration
files. The profile portion is a writer/container signature, not automatically a
new shooting mode.

| calibration label | representative code(s) | normalized mode | observation |
| --- | --- | --- | --- |
| Auto JPEG / Pro XDR | `0x200300000`, `0x200300020` | `auto` | JPEG gain-map files; `0x20` adds portrait orientation |
| Auto Night | `0x200300820` | `night` | `0x0800` is the repeatable Night marker; portrait orientation is also present |
| Auto Portrait | `0x200310030` | `portrait` | `0x10010` is the composite Portrait-mode marker; orientation is also present |
| Auto HEIC | `0x220100000`, `0x220100020` | `auto` | Same code/profile also appears on the Underwater sample; see limitations |
| Flash Auto/Always | `0x210300000` | `auto` | Flash setting differs in filenames while the mode code remains the same |
| Master | `0x310300100`, `0x310300500`, `0x320100100` | `master` | Several writer/export profiles preserve the Master bit |
| Master Portrait | `0x300300120` | `master` | Master plus portrait orientation; remaining profile is unverified |
| Text | `0x200141000`, `0x210101000` | `text` | `0x1000` is the repeatable Text marker; writer profiles differ |
| 50 MP Hi-Res | `0x220102000` or no `UserComment` | `hi-res` | 8192 × 6144; no JPEG gain-map marker in true HEIC samples |

The `0x200300000` and `0x220100000` profiles recur across multiple focal
lengths and modes in this corpus and are recognized as writer profiles. Other
one-off profiles remain visible as `-x...` until independent evidence confirms
their writer identity.

## 50 MP and Pro XDR/HDR

The product rule supplied with this calibration set is:

- true 50 MP Hi-Res (8192 × 6144) must not carry Pro XDR/HDR;
- other tested capture modes are expected to carry Pro XDR/HDR where the encoded
  source format supports it.

The objective metadata supports the first rule for the true HEIC samples: they
are 8192 × 6144, 8-bit HEVC/HEIC, and do not carry the JPEG `hdrgm`/GContainer
marker. The frontend must therefore not infer HDR from the `hi-res` mode bit
alone.

For JPEG, Pro XDR is evidenced by structure, not by `UserComment`:

- `XMP-hdrgm:Version=1.0`;
- a Google GContainer directory containing `Primary` and `GainMap` JPEG items;
- an MPF secondary JPEG whose length matches the gain-map declaration.

A `.heic` extension does not prove HEIC. Several calibration files with that
extension are physically JPEGs and carry gain-map metadata. Conversely, a true
HEIC file with no recognized HDR signaling remains unverified by camcomp's
current JPEG-only HDR inspector. Wide-gamut Display P3 metadata is not, by
itself, HDR evidence.

The current frontend inspector also reports some OPPO gain-map JPEGs as
`unverified / invalid-trailing-jpeg` because the gain-map JPEG is located by
MPF with trailing padding after it. That is a separate container-parser
limitation; it must not be converted into a Rust mode decision or treated as
proof that the source lacks Pro XDR.

## HDR format across all OPPO data

### Format rule

OPPO embeds HDR exclusively in JPEG format as Ultra HDR v1 gain maps
(XMP/GContainer + MPF + trailing gain-map JPEG). True HEIC output from the OPPO
Find X9 Pro never carries gain-map metadata. No OPPO HEIC file in any corpus
directory contains a `colr`/`nclx` box either — they use ICC `prof` colour
profiles (Display P3) instead.

This means OPPO HEIC is wide-gamut SDR, not HDR. The frontend HEIC CICP probe
(`probeHeifCicp`) correctly returns `undefined` for all OPPO HEICs, and
`cicpToDisplayDynamicRange` classifies them as `{ kind: "sdr" }`.

### Corpus inventory

| directory | OPPO JPEGs | gain-map HDR | OPPO HEICs | JPEG-in-.heic | notes |
| --- | --- | --- | --- | --- | --- |
| `data/metadata-calibration/` | 14 | all 14 | 10 true HEIC | 8 (7 with GM) | `Mode_Text.heic` is JPEG without GM |
| `data/cmp-garage-hdr/` | 11 (`IMG*`) | all 11 | — | — | |
| `data/jpg-uhdr-night/` | 3 (`IMG*`) | all 3 | — | — | |
| `data/juan/` | — | — | ~50 true HEIC | 10 (all with GM) | pairs in the same burst |
| `data/ferr-oppo/` | — | — | ~170 true HEIC | 0 | all true HEIC, no HDR |
| `data/sept2/` | — | — | ~20 true HEIC | 0 | burst and single frames |
| `data/peakfinder/` | 25 | 0 (all SDR) | — | — | Peakfinder app screenshots |
| `test/tstgrp2/` | 2 (`g3-*`) | 0 (SDR) | 1 (`g2-oppo-*`) | 0 | OPPO JPEGs are Peakfinder |
| `test/tstgrp9/` | 3 (`IMG*`) | all 3 | — | — | night-mode comparison |

### JPEG-in-.heic detail

Several OPPO `.heic` files are physically JPEGs (magic bytes `FFD8FF`). These
arise from specific capture modes (Flash Auto/Always, Master, some Auto) where
the camera writes JPEG content under a `.heic` extension. In the calibration
set, 8 of 18 `.heic` files are actually JPEGs; 7 of those 8 carry gain-map
metadata. The exception is `1x 23mm Mode_Text.heic`, which is JPEG but has no
gain-map XMP.

In Tauri, magic-byte detection classifies these correctly as
`displayFormat: "jpeg"`, so the gain-map inspection runs. In the browser, the
HEIC resolver throws `UnsupportedHeicError` for non-HEIC-magic files; display
requires a JPEG sidecar or worker decode is not attempted.

## Limitations and confirmed inconsistencies

- **Underwater is not encoded distinctly in this corpus.**
  `Mode_Underwater.heic` and `Mode_Auto Orient_Landscape Format_HEIC.heic`
  share `oplus_9127854080`, dimensions, camera metadata, and the same true-HEIC
  container shape. camcomp must classify both as Auto unless a future sample
  exposes a distinct tag.
- **Flash UI settings are not recoverable reliably.** The renamed setting is a
  validation hint only; EXIF provides fired/not-fired, not the selected UI
  setting.
- **A missing UserComment is recoverable for true 50 MP OPPO output.** The
  8192 × 6144 dimensions identify Hi-Res through the `PixelDimensions` fallback.
- **Unknown profiles remain first-class diagnostics.** They are retained in the
  configuration label and `modeTagUnknownBits`, while the rest of analysis and
  grouping continues. They must not be whitelisted merely to make a fixture
  pass.
- **Filename and extension are not authoritative.** The calibration set includes
  JPEG bytes with `.heic` names; magic bytes, dimensions, EXIF, and XMP/MPF must
  be considered independently.
- **True OPPO HEIC is wide-gamut SDR, not HDR.** All true HEIC files use ICC
  `prof` (Display P3), not CICP `nclx`. The HEIC CICP probe returns `undefined`
  and the frontend classifies them as SDR. No PQ or HLG transfer function has
  been observed in any OPPO HEIC sample.

## Implementation references

- Rust mode extraction: `crates/camcomp-core/src/metadata.rs`
- Dimension fallback and normalized metadata: `crates/camcomp-core/src/pipeline.rs`
- Calibration regression test: `crates/camcomp-core/tests/reference_sets.rs`
- JPEG gain-map inspection: `apps/web/src/sources/jpeg-hdr.ts`
- HEIC CICP colour probing: `apps/web/src/sources/worker-heic.ts` (`probeHeifCicp`)
- HEIC source resolution and HDR classification: `apps/web/src/sources/heic.ts`
- HDR presentation policy: `apps/web/src/viewer/hdr.ts`
- Real-file HDR regression coverage: `apps/web/tests/hdr.test.ts`
