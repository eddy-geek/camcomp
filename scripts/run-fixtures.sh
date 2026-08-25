#!/usr/bin/env bash
# Run the labelled reference sets in ./test and report the label outcome.
#
# Two passes, because the interesting question is not "does it pass" but "which
# stage decided". Pass 1 uses the specified defaults. Pass 2 relaxes only the
# three metadata prefilters (session gap, focal tolerance, one-photo-per-
# configuration) so the visual algorithm gets to decide every labelled pair.
# A false positive appearing in pass 2 and not in pass 1 means the prefilters
# were carrying the result; that is exactly what calibration needs to know.
#
# Pass 3 (--no-sessions) removes only the time prefilter. Sessions are the
# cheapest and bluntest instrument in the pipeline, and while they are in force
# they hide how the rest of the algorithm behaves: a pair that never becomes a
# candidate is never judged. Running with every photo in one session exercises
# the framing predicate and the geometric gates against every pair in a set.
#
# Usage: scripts/run-fixtures.sh [--relaxed | --no-sessions] [--table]
set -uo pipefail

cd "$(dirname "$0")/.."
BIN=${CAMCOMP_BIN:-./target/release/camcomp}
SETS=(test/tstgrp*)

if [ ! -x "$BIN" ]; then
    echo "build first: cargo build --release" >&2
    exit 2
fi
if [ ! -d "${SETS[0]}" ]; then
    echo "no reference sets under ./test; nothing to check" >&2
    exit 0
fi

EXTRA=()
TABLE=()
LABEL="defaults"
for arg in "$@"; do
    case "$arg" in
        --relaxed)
            LABEL="relaxed prefilters"
            EXTRA=(--no-session-gap --fov-tolerance 1.0 --allow-duplicate-config)
            ;;
        --no-sessions)
            LABEL="no session detection"
            EXTRA=(--no-session-gap)
            ;;
        --table) TABLE=(--table) ;;
        *) echo "unknown argument: $arg" >&2; exit 2 ;;
    esac
done

echo "=== reference sets, $LABEL ${EXTRA[*]:-}"
failed=0
for set_dir in "${SETS[@]}"; do
    echo
    echo "--- $set_dir"
    "$BIN" --quiet analyze "$set_dir" --dry-run --no-cache --expect-labels \
        "${TABLE[@]}" "${EXTRA[@]+"${EXTRA[@]}"}"
    status=$?
    case $status in
        0) : ;;
        3) failed=$((failed + 1)) ;;
        *)
            echo "!!! $set_dir failed with exit $status" >&2
            exit $status
            ;;
    esac
done

echo
if [ "$failed" -eq 0 ]; then
    echo "all ${#SETS[@]} set(s) reproduced their labels"
else
    echo "$failed of ${#SETS[@]} set(s) did not reproduce every label (exit 3, no crash)"
fi
# Not reproducing a label is a calibration finding, not a build failure. A crash
# is a build failure, and is propagated above.
exit 0
