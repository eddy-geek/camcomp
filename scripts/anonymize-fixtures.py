#!/usr/bin/env python3
"""Replace the real GPS position in the committed reference photos with synthetic
coordinates.

The substitution is done by overwriting the GPS rationals where they already sit
in the EXIF block. Nothing is added, removed, or moved, so the file length and
every byte offset in it stay exactly as they were. That matters: these fixtures
carry JPEG MPF gain maps and OPPO HEIF trailers that record absolute offsets, and
a metadata rewriter that repacks the file (exiftool does) silently breaks
gain-map detection.

Everything else is left alone. Camera make and model, lens, exposure, timestamps,
HDR gain maps, maker notes and thumbnails are all load-bearing for the fixtures.
The engine never reads GPS, so the substituted values only need to be well formed.

One synthetic position per reference set, on a 0.1 degree grid:

    test/tstgrpN  ->  lat 45.N0000, lon 7.N0000, altitude 1000 m

0.1 degree is exactly 6 arcminutes, so each value is representable without
rounding in the degree/minute/second rationals EXIF uses.

Idempotent: running it again writes the same values.
"""

from __future__ import annotations

import pathlib
import struct
import sys

GPS_IFD_TAG = 0x8825
TAG_LATITUDE = 0x0002
TAG_LONGITUDE = 0x0004
TAG_ALTITUDE = 0x0006
TYPE_RATIONAL = 5
ALTITUDE_M = 1000
SUFFIXES = {".jpg", ".jpeg", ".png", ".heic"}


class Unsupported(Exception):
    """The file carries no patchable GPS block."""


def find_tiff_header(data: bytes) -> int:
    """Offset of the TIFF header of the Exif block.

    Works for both JPEG APP1 and the HEIF `Exif` item, which share the
    `Exif\\0\\0` prefix followed by a TIFF header. In HEIF the literal `Exif`
    also appears as an item type in the item info box, so every candidate is
    checked for a real TIFF header rather than taking the first hit.
    """
    at = data.find(b"Exif\x00\x00")
    while at >= 0:
        tiff = at + 6
        if data[tiff : tiff + 4] in (b"II\x2a\x00", b"MM\x00\x2a"):
            return tiff
        at = data.find(b"Exif\x00\x00", at + 1)
    raise Unsupported("no Exif TIFF header")


def entries(data: bytes, tiff: int, ifd: int, endian: str):
    """Yield (tag, type, count, value_offset) for one IFD."""
    count = struct.unpack_from(endian + "H", data, tiff + ifd)[0]
    for i in range(count):
        at = tiff + ifd + 2 + i * 12
        tag, typ, n = struct.unpack_from(endian + "HHI", data, at)
        yield tag, typ, n, at + 8


def gps_ifd_offset(data: bytes, tiff: int, endian: str) -> int:
    ifd0 = struct.unpack_from(endian + "I", data, tiff + 4)[0]
    for tag, _typ, _n, value_at in entries(data, tiff, ifd0, endian):
        if tag == GPS_IFD_TAG:
            return struct.unpack_from(endian + "I", data, value_at)[0]
    raise Unsupported("no GPS IFD")


def degrees_to_dms(value: float) -> list[tuple[int, int]]:
    """Degrees as the three rationals EXIF stores, without losing precision."""
    deg = int(value)
    minutes_total = round((value - deg) * 60 * 10000)
    return [(deg, 1), (minutes_total, 10000), (0, 1)]


def patch(path: pathlib.Path, lat: float, lon: float) -> str:
    data = bytearray(path.read_bytes())
    tiff = find_tiff_header(data)
    endian = "<" if data[tiff : tiff + 2] == b"II" else ">"
    gps = gps_ifd_offset(data, tiff, endian)

    written = []
    for tag, typ, count, value_at in entries(data, tiff, gps, endian):
        if tag == TAG_LATITUDE:
            rationals, label = degrees_to_dms(lat), f"lat {lat}"
        elif tag == TAG_LONGITUDE:
            rationals, label = degrees_to_dms(lon), f"lon {lon}"
        elif tag == TAG_ALTITUDE:
            rationals, label = [(ALTITUDE_M, 1)], f"alt {ALTITUDE_M}m"
        else:
            continue
        if typ != TYPE_RATIONAL or count != len(rationals):
            raise Unsupported(f"tag {tag:#06x} is type {typ} count {count}")
        # A rational never fits the inline 4 bytes, so the entry always points
        # out into the block. Overwriting there keeps the length identical.
        at = tiff + struct.unpack_from(endian + "I", data, value_at)[0]
        for i, (num, den) in enumerate(rationals):
            struct.pack_into(endian + "II", data, at + i * 8, num, den)
        written.append(label)

    if not written:
        raise Unsupported("GPS IFD carries no position")

    path.write_bytes(data)
    return ", ".join(written)


def main() -> int:
    root = pathlib.Path(__file__).resolve().parent.parent
    failures = 0

    for n in range(1, 10):
        directory = root / "test" / f"tstgrp{n}"
        if not directory.is_dir():
            continue
        lat, lon = 45 + n / 10, 7 + n / 10
        for path in sorted(directory.iterdir()):
            if path.suffix.lower() not in SUFFIXES:
                continue
            relative = path.relative_to(root)
            before = len(path.read_bytes())
            try:
                written = patch(path, lat, lon)
            except Unsupported as error:
                print(f"skip {relative}: {error}")
                continue
            after = len(path.read_bytes())
            if before != after:
                print(f"FAIL {relative}: length changed {before} -> {after}")
                failures += 1
                continue
            print(f"{relative}: {written}")

    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
