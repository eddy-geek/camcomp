#!/usr/bin/env python3
"""Ask whether a focal tolerance exists that agrees with the `gN-` labels.

Two questions, both answerable from metadata alone:

1. Does correcting the 35 mm equivalent focal length for digital zoom help? The
   Oppo folds `DigitalZoomRatio` into `FocalLengthIn35mmFormat`, the Pixel does
   not (see `artifacts/IMPLEM-DROP1.md` §6.7).
2. Given a session gap, is there any `focalTolerance` that accepts every pair the
   labels put together and rejects every same-session pair they put apart?

Only same-session pairs are considered: the focal predicate never sees the others,
so making it discriminate them would be measuring the wrong thing.

Usage: scripts/focal-hypothesis.py [--gap SECONDS] inspect-output.json [...]
       where each file is `camcomp inspect PATH --format json`
"""

import json
import sys
from datetime import datetime
from itertools import combinations

# Devices whose `FocalLengthIn35mmFormat` already includes digital zoom.
# Derived, not assumed: the implied crop factor `focal35 / focalLength` varies
# with zoom for these and stays constant for the others.
ZOOM_ALREADY_INCLUDED = {"OPPO Find X9 Pro"}


def label_of(path):
    name = path.rsplit("/", 1)[-1]
    prefix = name.split("-", 1)[0]
    return prefix if prefix.startswith("g") and len(prefix) > 1 else None


def instant(photo):
    text = photo["metadata"].get("correctedAt")
    return datetime.fromisoformat(text).timestamp() if text else None


def effective_focal(photo):
    meta = photo["metadata"]
    focal35 = meta.get("focalLength35mm")
    if not focal35:
        return None
    model = meta.get("cameraModel") or ""
    zoom = meta.get("digitalZoomRatio") or 0.0
    if model in ZOOM_ALREADY_INCLUDED or zoom <= 1.0:
        return focal35
    return focal35 * zoom


def relative_difference(a, b):
    return abs(a - b) / min(a, b)


def sessions(photos, gap):
    """Sliding-gap sessions, the same rule the engine uses."""
    timed = sorted(
        ((instant(p), p) for p in photos if instant(p) is not None),
        key=lambda t: t[0],
    )
    out, current, last = [], [], None
    for when, photo in timed:
        if last is not None and when - last > gap:
            out.append(current)
            current = []
        current.append(photo)
        last = when
    if current:
        out.append(current)
    return out


def collect(paths, gap):
    accept, reject = [], []
    for path in paths:
        with open(path) as handle:
            photos = json.load(handle)
        set_name = (
            path.rsplit("/", 1)[-1].removeprefix("insp-").removesuffix(".json")
        )
        for session in sessions(photos, gap):
            for a, b in combinations(session, 2):
                la, lb = label_of(a["path"]), label_of(b["path"])
                if la is None or lb is None or la == "gx" or lb == "gx":
                    continue
                reported = None
                ra, rb = (
                    a["metadata"].get("focalLength35mm"),
                    b["metadata"].get("focalLength35mm"),
                )
                if ra and rb:
                    reported = relative_difference(ra, rb)
                fa, fb = effective_focal(a), effective_focal(b)
                corrected = relative_difference(fa, fb) if fa and fb else None
                row = (set_name, la if la == lb else f"{la}/{lb}",
                       a["path"], b["path"], reported, corrected)
                (accept if la == lb else reject).append(row)
    return accept, reject


def show(title, rows):
    print(f"\n{title} ({len(rows)} same-session pair(s))")
    print(f"  {'set':<5} {'label':<8} {'reported':>9} {'corrected':>10}  pair")
    for set_name, label, pa, pb, rep, cor in sorted(
        rows, key=lambda r: -(r[5] if r[5] is not None else -1)
    ):
        r = f"{rep:.3f}" if rep is not None else "unknown"
        c = f"{cor:.3f}" if cor is not None else "unknown"
        print(
            f"  {set_name:<5} {label:<8} {r:>9} {c:>10}  "
            f"{pa.rsplit('/', 1)[-1][:33]} + {pb.rsplit('/', 1)[-1][:33]}"
        )


def window(accept, reject, index, name):
    need = [(r[index], r) for r in accept if r[index] is not None]
    forbid = [(r[index], r) for r in reject if r[index] is not None]
    lower, worst = max(need, default=(0.0, None))
    upper, tightest = min(forbid, default=(float("inf"), None))
    print(f"\n{name}")
    print(f"  widest pair that must be ACCEPTED  {lower:.4f}"
          + (f"  ({worst[0]}/{worst[1]})" if worst else ""))
    print(f"  closest pair that must be REJECTED {upper:.4f}"
          + (f"  ({tightest[0]}/{tightest[1]})" if tightest else ""))
    if lower < upper:
        print(f"  => VALID: {lower:.4f} <= focalTolerance < {upper:.4f}")
        if lower <= 0.15 < upper:
            print("  => the specified default 0.15 is inside this window")
        else:
            print("  => the specified default 0.15 is NOT inside this window")
    else:
        print("  => NO valid tolerance: the populations interleave")
        rest = [v for v, r in need if r is not worst]
        second = max(rest, default=0.0)
        if second < upper:
            print(f"     setting aside {worst[0]}/{worst[1]} alone would give a "
                  f"valid window: {second:.4f} <= focalTolerance < {upper:.4f}")
            print(f"     ({worst[2].rsplit('/', 1)[-1]}")
            print(f"      + {worst[3].rsplit('/', 1)[-1]})")


def main(argv):
    gap = 30
    paths = []
    i = 0
    while i < len(argv):
        if argv[i] == "--gap":
            gap = int(argv[i + 1])
            i += 2
        else:
            paths.append(argv[i])
            i += 1
    if not paths:
        print(__doc__)
        return 2

    print(f"session gap: {gap} s")
    accept, reject = collect(paths, gap)
    show("labelled SAME — a tolerance must accept these", accept)
    show("labelled APART — a tolerance must reject these", reject)
    window(accept, reject, 4, "Using the reported 35 mm equivalent")
    window(accept, reject, 5, "Using the zoom-corrected 35 mm equivalent")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
