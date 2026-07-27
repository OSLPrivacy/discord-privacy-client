#!/usr/bin/env python3
"""Read the exact PNG facts VMQA grades, using only the Python standard library.

The VM agent samples every fourth pixel of its captured bitmap.  The host must
derive the same count from the retained PNG bytes instead of trusting the
agent-authored verdict.  Only the two non-indexed 8-bit formats emitted by
System.Drawing are accepted; an unfamiliar PNG is evidence the apparatus
changed, not something to guess through.
"""

from __future__ import annotations

import argparse
import json
import struct
import sys
import zlib
from pathlib import Path


PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
CHANNELS = {2: 3, 6: 4}


class PngError(ValueError):
    pass


def paeth(left: int, up: int, upper_left: int) -> int:
    prediction = left + up - upper_left
    left_distance = abs(prediction - left)
    up_distance = abs(prediction - up)
    upper_left_distance = abs(prediction - upper_left)
    if left_distance <= up_distance and left_distance <= upper_left_distance:
        return left
    if up_distance <= upper_left_distance:
        return up
    return upper_left


def parse_png(path: Path) -> dict[str, int]:
    data = path.read_bytes()
    if not data.startswith(PNG_SIGNATURE):
        raise PngError("PNG signature is missing")

    offset = len(PNG_SIGNATURE)
    ihdr: tuple[int, int, int, int, int, int, int] | None = None
    compressed = bytearray()
    saw_iend = False
    chunk_index = 0
    while offset < len(data):
        if offset + 12 > len(data):
            raise PngError("truncated PNG chunk header")
        length = struct.unpack(">I", data[offset : offset + 4])[0]
        chunk_type = data[offset + 4 : offset + 8]
        chunk_end = offset + 12 + length
        if chunk_end > len(data):
            raise PngError("truncated PNG chunk body")
        chunk_data = data[offset + 8 : offset + 8 + length]
        claimed_crc = struct.unpack(">I", data[offset + 8 + length : chunk_end])[0]
        actual_crc = zlib.crc32(chunk_type)
        actual_crc = zlib.crc32(chunk_data, actual_crc) & 0xFFFFFFFF
        if actual_crc != claimed_crc:
            raise PngError(f"CRC mismatch in {chunk_type!r}")

        if chunk_index == 0 and chunk_type != b"IHDR":
            raise PngError("IHDR is not the first chunk")
        if chunk_type == b"IHDR":
            if ihdr is not None or length != 13:
                raise PngError("invalid or duplicate IHDR")
            ihdr = struct.unpack(">IIBBBBB", chunk_data)
        elif chunk_type == b"IDAT":
            compressed.extend(chunk_data)
        elif chunk_type == b"IEND":
            if length != 0:
                raise PngError("invalid IEND")
            saw_iend = True
            offset = chunk_end
            break
        offset = chunk_end
        chunk_index += 1

    if ihdr is None or not compressed or not saw_iend or offset != len(data):
        raise PngError("PNG is missing IHDR, IDAT, IEND, or has trailing bytes")
    width, height, bit_depth, color_type, compression, filtering, interlace = ihdr
    if width < 1 or height < 1:
        raise PngError("PNG dimensions are empty")
    if (
        bit_depth != 8
        or color_type not in CHANNELS
        or compression != 0
        or filtering != 0
        or interlace != 0
    ):
        raise PngError(
            "unsupported PNG encoding "
            f"bitDepth={bit_depth} colorType={color_type} interlace={interlace}"
        )

    channels = CHANNELS[color_type]
    row_bytes = width * channels
    inflated = zlib.decompress(bytes(compressed))
    expected = height * (row_bytes + 1)
    if len(inflated) != expected:
        raise PngError(f"inflated byte count {len(inflated)} != {expected}")

    rows: list[bytes] = []
    previous = bytes(row_bytes)
    cursor = 0
    for _ in range(height):
        filter_type = inflated[cursor]
        encoded = inflated[cursor + 1 : cursor + 1 + row_bytes]
        cursor += row_bytes + 1
        decoded = bytearray(row_bytes)
        for index, value in enumerate(encoded):
            left = decoded[index - channels] if index >= channels else 0
            up = previous[index]
            upper_left = previous[index - channels] if index >= channels else 0
            if filter_type == 0:
                predictor = 0
            elif filter_type == 1:
                predictor = left
            elif filter_type == 2:
                predictor = up
            elif filter_type == 3:
                predictor = (left + up) // 2
            elif filter_type == 4:
                predictor = paeth(left, up, upper_left)
            else:
                raise PngError(f"unsupported PNG filter {filter_type}")
            decoded[index] = (value + predictor) & 0xFF
        previous = bytes(decoded)
        rows.append(previous)

    colors: set[bytes] = set()
    for y in range(0, height, 4):
        row = rows[y]
        for x in range(0, width, 4):
            start = x * channels
            pixel = row[start : start + channels]
            if color_type == 2:
                pixel += b"\xff"
            colors.add(pixel)

    return {
        "width": width,
        "height": height,
        "bitDepth": bit_depth,
        "colorType": color_type,
        "sampleStride": 4,
        "distinctColors": len(colors),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("png", type=Path)
    args = parser.parse_args()
    try:
        print(json.dumps(parse_png(args.png), sort_keys=True, separators=(",", ":")))
    except (OSError, PngError, struct.error, zlib.error) as error:
        print(f"png-facts: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
