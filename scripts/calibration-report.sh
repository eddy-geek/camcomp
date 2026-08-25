#!/usr/bin/env bash
# Produce the per-pair component distribution that `SPEC.md` §5.4.1 requires
# before any threshold is moved.
#
# Runs every labelled set twice — once with the specified defaults, once with the
# metadata prefilters relaxed — and tabulates each metric component against the
# `gN-` labels. The relaxed pass is the informative one: it shows what the
# visual algorithm alone concludes, and therefore how much of the result the
# prefilters are carrying.
set -uo pipefail

cd "$(dirname "$0")/.."
BIN=${CAMCOMP_BIN:-./target/release/camcomp}
OUT=${CAMCOMP_CALIBRATION_DIR:-target/calibration}

if [ ! -x "$BIN" ]; then
    echo "build first: cargo build --release" >&2
    exit 2
fi
if ! compgen -G "test/tstgrp*" > /dev/null; then
    echo "no reference sets under ./test; nothing to calibrate" >&2
    exit 0
fi

mkdir -p "$OUT/defaults" "$OUT/relaxed"

for set_dir in test/tstgrp*; do
    name=$(basename "$set_dir")
    "$BIN" --quiet analyze "$set_dir" --dry-run --no-cache --format json \
        > "$OUT/defaults/$name.json"
    "$BIN" --quiet analyze "$set_dir" --dry-run --no-cache --format json \
        --session-gap 90 --focal-tolerance 1.0 --allow-duplicate-config \
        > "$OUT/relaxed/$name.json"
done

echo "################ pass 1: specified defaults"
python3 scripts/calibration-table.py "$OUT"/defaults/*.json

echo
echo "################ pass 2: metadata prefilters relaxed"
python3 scripts/calibration-table.py "$OUT"/relaxed/*.json
