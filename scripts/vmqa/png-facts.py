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
import binascii
import json
import struct
import sys
import tempfile
import unittest
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


def _png_chunk(kind: bytes, body: bytes) -> bytes:
    return (
        struct.pack(">I", len(body))
        + kind
        + body
        + struct.pack(">I", binascii.crc32(kind + body) & 0xFFFFFFFF)
    )


def _encode_png(
    width: int,
    height: int,
    color_type: int,
    pixels: list[list[tuple[int, ...]]],
    filters: list[int] | None = None,
) -> bytes:
    channels = CHANNELS[color_type]
    filters = filters or [0] * height
    rows = bytearray()
    previous = bytes(width * channels)
    for y, row_pixels in enumerate(pixels):
        raw = bytes(channel for pixel in row_pixels for channel in pixel)
        encoded = bytearray(len(raw))
        filter_type = filters[y]
        for index, value in enumerate(raw):
            left = raw[index - channels] if index >= channels else 0
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
                raise ValueError(f"unsupported test filter {filter_type}")
            encoded[index] = (value - predictor) & 0xFF
        rows.append(filter_type)
        rows.extend(encoded)
        previous = raw
    return (
        PNG_SIGNATURE
        + _png_chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, color_type, 0, 0, 0))
        + _png_chunk(b"IDAT", zlib.compress(bytes(rows), 9))
        + _png_chunk(b"IEND", b"")
    )


class ParsePngTests(unittest.TestCase):
    def parse_bytes(self, png: bytes) -> dict[str, int]:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "sample.png"
            path.write_bytes(png)
            return parse_png(path)

    def test_rgba_counts_only_every_fourth_pixel(self) -> None:
        pixels: list[list[tuple[int, ...]]] = []
        for y in range(5):
            row: list[tuple[int, ...]] = []
            for x in range(5):
                row.append((x, y, 0, 255))
            pixels.append(row)

        self.assertEqual(
            self.parse_bytes(_encode_png(5, 5, 6, pixels)),
            {
                "width": 5,
                "height": 5,
                "bitDepth": 8,
                "colorType": 6,
                "sampleStride": 4,
                "distinctColors": 4,
            },
        )

    def test_rgb_pixels_are_compared_with_opaque_alpha(self) -> None:
        pixels = [
            [(7, 8, 9), (1, 1, 1), (2, 2, 2), (3, 3, 3), (7, 8, 9)],
            [(4, 4, 4), (5, 5, 5), (6, 6, 6), (7, 7, 7), (8, 8, 8)],
            [(9, 9, 9), (10, 10, 10), (11, 11, 11), (12, 12, 12), (13, 13, 13)],
            [(14, 14, 14), (15, 15, 15), (16, 16, 16), (17, 17, 17), (18, 18, 18)],
            [(7, 8, 9), (19, 19, 19), (20, 20, 20), (21, 21, 21), (22, 22, 22)],
        ]

        facts = self.parse_bytes(_encode_png(5, 5, 2, pixels))

        self.assertEqual(facts["colorType"], 2)
        self.assertEqual(facts["distinctColors"], 2)

    def test_all_png_filters_decode_to_exact_pixels(self) -> None:
        pixels = [
            [(10, 20, 30, 255), (40, 50, 60, 255)],
            [(11, 21, 31, 255), (41, 51, 61, 255)],
            [(12, 22, 32, 255), (42, 52, 62, 255)],
            [(13, 23, 33, 255), (43, 53, 63, 255)],
            [(14, 24, 34, 255), (44, 54, 64, 255)],
        ]

        facts = self.parse_bytes(_encode_png(2, 5, 6, pixels, filters=[0, 1, 2, 3, 4]))

        self.assertEqual(facts["height"], 5)
        self.assertEqual(facts["distinctColors"], 2)

    def test_crc_mismatch_is_refused(self) -> None:
        pixels = [[(0, 0, 0, 255)]]
        png = bytearray(_encode_png(1, 1, 6, pixels))
        png[-8] ^= 1

        with self.assertRaisesRegex(PngError, "CRC mismatch"):
            self.parse_bytes(bytes(png))

    def test_unsupported_png_encoding_is_refused(self) -> None:
        png = (
            PNG_SIGNATURE
            + _png_chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 3, 0, 0, 0))
            + _png_chunk(b"IDAT", zlib.compress(b"\x00\x00"))
            + _png_chunk(b"IEND", b"")
        )

        with self.assertRaisesRegex(PngError, "unsupported PNG encoding"):
            self.parse_bytes(png)


def run_self_tests() -> int:
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(ParsePngTests)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    return 0 if result.wasSuccessful() else 1


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("png", nargs="?", type=Path)
    args = parser.parse_args()
    if args.self_test:
        return run_self_tests()
    if args.png is None:
        parser.error("the following arguments are required: png")
    try:
        print(json.dumps(parse_png(args.png), sort_keys=True, separators=(",", ":")))
    except (OSError, PngError, struct.error, zlib.error) as error:
        print(f"png-facts: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
