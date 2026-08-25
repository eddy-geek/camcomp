#!/usr/bin/env python3
"""Tabulate per-pair metric components against the `gN-` filename labels.

Reads `camcomp.json` documents (as produced by `camcomp analyze --format json`)
and prints, per component, the achieved range on true pairs and on near-miss
negatives. `SPEC.md` §5.4.1 requires this distribution before any threshold is
moved: a component whose dynamic range is compressed is silently deciding
nothing, and the near-miss column is the interesting one.

Usage: scripts/calibration-table.py doc.json [doc.json ...]
"""

import json
import sys


def label_of(path):
    name = path.rsplit("/", 1)[-1]
    prefix = name.split("-", 1)[0]
    return prefix if prefix.startswith("g") and len(prefix) > 1 else None


COMPONENTS = [
    ("overlap", "geometricOverlap"),
    ("coverage", "featureCoverage"),
    ("confid", "inlierConfidence"),
    ("appear", "alignedAppearanceSimilarity"),
    ("scale", "affineScale"),
    ("inliers", "inliers"),
    ("reproj", "medianReprojectionErrorPx"),
]


def main(paths):
    rows = []
    for path in paths:
        with open(path) as handle:
            doc = json.load(handle)
        photo_path = {p["id"]: p["path"] for p in doc["photos"]}
        focal = {p["id"]: p["metadata"].get("focalLength35mm") for p in doc["photos"]}
        source = doc.get("source", {}).get("root", path)
        set_name = source.rstrip("/").rsplit("/", 1)[-1]
        for pair in doc["pairs"]:
            la = label_of(photo_path[pair["a"]])
            lb = label_of(photo_path[pair["b"]])
            if la is None or lb is None:
                expected = "?"
            elif la == "gx" or lb == "gx":
                expected = "undecided"
            elif la == lb:
                expected = "same"
            else:
                expected = "different"
            rows.append(
                {
                    "set": set_name,
                    "a": photo_path[pair["a"]].rsplit("/", 1)[-1],
                    "b": photo_path[pair["b"]].rsplit("/", 1)[-1],
                    "expected": expected,
                    "state": pair["state"],
                    "score": pair.get("score"),
                    "reason": pair.get("reason") or "",
                    "focal_ratio": (
                        round(min(focal[pair["a"]], focal[pair["b"]])
                              / max(focal[pair["a"]], focal[pair["b"]]), 4)
                        if focal[pair["a"]] and focal[pair["b"]]
                        else None
                    ),
                    "metrics": pair.get("metrics") or {},
                }
            )

    header = f"{'set':<9} {'expected':<10} {'state':<12} {'score':<7} {'f-ratio':<8}"
    header += " ".join(f"{name:>8}" for name, _ in COMPONENTS)
    header += "  pair"
    print(header)
    print("-" * len(header))
    for row in sorted(rows, key=lambda r: (r["expected"], r["set"])):
        line = (
            f"{row['set']:<9} {row['expected']:<10} {row['state']:<12} "
            f"{(f'{row['score']:.3f}' if row['score'] is not None else '-'):<7} "
            f"{(f'{row['focal_ratio']:.3f}' if row['focal_ratio'] else '-'):<8}"
        )
        for _, key in COMPONENTS:
            value = row["metrics"].get(key)
            if value is None:
                line += f"{'-':>9}"
            elif isinstance(value, float):
                line += f"{value:>9.3f}"
            else:
                line += f"{value:>9}"
        line += f"  {row['a'][:30]} + {row['b'][:30]}"
        print(line)

    print()
    print("component ranges by expected outcome")
    for expected in ("same", "different", "undecided"):
        group = [r for r in rows if r["expected"] == expected and r["metrics"]]
        if not group:
            continue
        print(f"\n  {expected} ({len(group)} analyzed pair(s))")
        for name, key in COMPONENTS:
            values = [r["metrics"][key] for r in group if key in r["metrics"]]
            if not values:
                continue
            print(
                f"    {name:<9} min {min(values):>9.3f}  max {max(values):>9.3f}"
                f"  mean {sum(values) / len(values):>9.3f}"
            )
        scores = [r["score"] for r in group if r["score"] is not None]
        if scores:
            print(
                f"    {'score':<9} min {min(scores):>9.3f}  max {max(scores):>9.3f}"
                f"  mean {sum(scores) / len(scores):>9.3f}"
            )

    print()
    same = [r for r in rows if r["expected"] == "same"]
    diff = [r for r in rows if r["expected"] == "different"]
    fp = [r for r in diff if r["state"] == "compatible"]
    fn = [r for r in same if r["state"] != "compatible"]
    print(f"true pairs: {len(same)}, of which {len(same) - len(fn)} compatible")
    print(f"negative pairs: {len(diff)}, of which {len(fp)} compatible (FALSE POSITIVES)")
    for row in fp:
        print(f"  ! {row['set']}: {row['a']} + {row['b']}  score {row['score']:.3f}")
    for row in fn:
        print(f"  - {row['set']}: {row['a']} + {row['b']}  {row['state']}/{row['reason']}")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(2)
    main(sys.argv[1:])
