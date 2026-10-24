#!/usr/bin/env python3
"""Draws the DMG window background: a light field with a dark grey arrow.

The field is light on purpose: Finder draws icon labels in dark text and
cannot be told otherwise, so a dark background would hide them.

The output is plain PNG, written with the standard library only, so the
packaging step needs no image tools. Run it only when the artwork changes
and commit the results; `build-dmg.sh` just consumes them.

Writes dmg-background.png (660x400) and dmg-background@2x.png (1320x800).
"""

import math
import struct
import sys
import zlib
from pathlib import Path

WIDTH, HEIGHT = 660, 400
BACKGROUND = (0xF5, 0xF6, 0xF2)
ACCENT = (0x3C, 0x40, 0x48)

# The arrow sits between the app icon (x=165) and the Applications link
# (x=495), on the icon row (y=200). Units are 1x pixels.
SHAFT = ((270.0, 200.0), (390.0, 200.0))
HEAD = (((360.0, 172.0), (390.0, 200.0)), ((360.0, 228.0), (390.0, 200.0)))
STROKE = 6.0
SAMPLES = 4  # supersampling per axis, for smooth edges


def distance_to_segment(px, py, segment):
	(ax, ay), (bx, by) = segment
	dx, dy = bx - ax, by - ay
	t = max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)))

	return math.hypot(px - (ax + t * dx), py - (ay + t * dy))


def coverage(x, y, scale):
	"""Fraction of pixel (x, y) covered by the arrow strokes."""
	segments = (SHAFT, *HEAD)
	hits = 0

	for sy in range(SAMPLES):
		for sx in range(SAMPLES):
			px = (x + (sx + 0.5) / SAMPLES) / scale
			py = (y + (sy + 0.5) / SAMPLES) / scale

			if any(distance_to_segment(px, py, s) <= STROKE / 2 for s in segments):
				hits += 1

	return hits / (SAMPLES * SAMPLES)


def render(scale):
	rows = []

	for y in range(HEIGHT * scale):
		row = bytearray([0])  # PNG filter type: none

		for x in range(WIDTH * scale):
			amount = coverage(x, y, scale)
			row.extend(round(b + (a - b) * amount) for a, b in zip(ACCENT, BACKGROUND))

		rows.append(bytes(row))

	return b"".join(rows)


def chunk(kind, data):
	body = kind + data

	return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))


def write_png(path, scale):
	header = struct.pack(">IIBBBBB", WIDTH * scale, HEIGHT * scale, 8, 2, 0, 0, 0)
	data = zlib.compress(render(scale), 9)

	path.write_bytes(
		b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", data) + chunk(b"IEND", b"")
	)


def main():
	here = Path(__file__).parent

	write_png(here / "dmg-background.png", 1)
	write_png(here / "dmg-background@2x.png", 2)


if __name__ == "__main__":
	sys.exit(main())
