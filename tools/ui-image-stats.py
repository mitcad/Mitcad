#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Pixel statistics of a region of an XWD screen dump (xwd -root), for UI
tests that check what the view draws without comparing to stored images.

Usage: ui-image-stats.py dump.xwd x y width height [other.xwd]
       ui-image-stats.py --shot screenshot.png

Prints JSON with pixel counts in the region:
  pixels   all of them
  object   differ from the row's background (the most common colour of the
           region's first pixels in the row: the view's gradient runs down)
  dark     luminance below 80 (a dark background)
  light    luminance above 170 (light lines on a dark theme's background)
  grey    objects of little colour darker than the light background (edges
           and lines; also the darkest shading)
  warm     red clearly above green and blue (a red body)
  yellow   strong red and green, little blue (Section Analysis' caps)
  changed  differ from the same pixel of other.xwd, when given
Only the standard library is used.

--shot analyses a PNG written by the app's --screenshot (the macOS UI
test, tools/ui-macos-test.sh; the Windows test does the same in
tools/ui-windows-test.ps1) and prints JSON:
  width, height   of the image
  background      median brightness of the light background gradient
  face_percent    share of "face" pixels: inside a uniformly coloured area
                  and clearly darker than the row's background (the
                  orientation cube in the upper right corner is left out)
  shades          the face pixels grouped by brightness, brightest first:
                  every group of at least 1.5 % of the image, with its
                  luma, mean r, g, b, pixels and percent
  bluish          every shade is the bluish grey of the body (b >= r and
                  b - r <= 40)
  side_ratio      the darker shades' pixels / the brightest one's (side
                  faces relative to the top face), null with one shade
  colors          distinct colours (5 bits per channel): 1 for a blank image
A Retina screenshot (twice the window's 1280 pixels) is analysed with
every second pixel and twice the distances.
"""

import json
import struct
import sys
import zlib
from collections import Counter
from itertools import accumulate


def region_rows(path, rx, ry, rw, rh):
    data = open(path, "rb").read()
    fields = struct.unpack(">25I", data[:100])
    header_size, version, pixmap_format = fields[0], fields[1], fields[2]
    width, height = fields[4], fields[5]
    byte_order, bits_per_pixel, bytes_per_line = fields[7], fields[11], fields[12]
    masks = fields[14], fields[15], fields[16]
    ncolors = fields[19]
    if version != 7 or pixmap_format != 2 or bits_per_pixel not in (24, 32):
        raise SystemExit("unsupported XWD: need a 24/32-bit ZPixmap dump")
    pixels = data[header_size + ncolors * 12:]
    step = bits_per_pixel // 8
    order = "little" if byte_order == 0 else "big"
    shifts = [(m & -m).bit_length() - 1 for m in masks]
    rx, ry = max(0, rx), max(0, ry)
    rw, rh = min(rw, width - rx), min(rh, height - ry)
    rows = []
    for y in range(ry, ry + rh):
        start = y * bytes_per_line + rx * step
        line = pixels[start:start + rw * step]
        row = []
        for x in range(0, len(line), step):
            value = int.from_bytes(line[x:x + step], order)
            row.append(tuple((value & m) >> s for m, s in zip(masks, shifts)))
        rows.append(row)
    return rows


def unfilter_up(row, prev):
    """Adds two rows byte by byte modulo 256, 8 bytes at a time."""
    n = len(row)
    a = int.from_bytes(row, "little")
    b = int.from_bytes(prev, "little")
    low = int.from_bytes(b"\x7f" * n, "little")
    high = int.from_bytes(b"\x80" * n, "little")
    return ((((a & low) + (b & low)) ^ ((a ^ b) & high))).to_bytes(n, "little")


def read_png(path):
    """Decodes an 8-bit, non-interlaced RGB or RGBA PNG (what Qt writes);
    returns width, height, bytes per pixel and the rows' bytes."""
    data = open(path, "rb").read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise SystemExit("not a PNG file: " + path)
    pos, idat, header = 8, [], None
    while pos + 8 <= len(data):
        size, kind = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + size]
        pos += 12 + size
        if kind == b"IHDR":
            header = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            idat.append(body)
        elif kind == b"IEND":
            break
    if header is None or not idat:
        raise SystemExit("truncated PNG file: " + path)
    width, height, depth, color, _, _, interlace = header
    if depth != 8 or color not in (2, 6) or interlace != 0:
        raise SystemExit("unsupported PNG: need 8-bit RGB or RGBA, not interlaced")
    bpp = 3 if color == 2 else 4
    stride = width * bpp
    raw = zlib.decompress(b"".join(idat))
    if len(raw) < height * (stride + 1):
        raise SystemExit("truncated PNG data: " + path)
    rows, prev = [], bytes(stride)
    for y in range(height):
        start = y * (stride + 1)
        kind, line = raw[start], raw[start + 1:start + 1 + stride]
        if kind == 1:  # Sub: a running sum per channel
            out = bytearray(stride)
            for c in range(bpp):
                out[c::bpp] = bytes([v & 255 for v in accumulate(line[c::bpp])])
            line = bytes(out)
        elif kind == 2:  # Up
            line = unfilter_up(line, prev)
        elif kind in (3, 4):  # Average, Paeth: each byte depends on its left one
            out = bytearray(line)
            for i in range(stride):
                a = out[i - bpp] if i >= bpp else 0
                b = prev[i]
                if kind == 3:
                    out[i] = (out[i] + ((a + b) >> 1)) & 255
                else:
                    c = prev[i - bpp] if i >= bpp else 0
                    p = a + b - c
                    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                    pred = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                    out[i] = (out[i] + pred) & 255
            line = bytes(out)
        elif kind != 0:
            raise SystemExit("unsupported PNG filter %d" % kind)
        rows.append(line)
        prev = line
    return width, height, bpp, rows


def shot_stats(path):
    """The statistics of --shot: the same as tools/ui-windows-test.ps1's."""
    w, h, bpp, rows = read_png(path)
    scale = max(1, round(w / 1280))
    step, off = scale, 2 * scale
    background = []
    for y in range(h):
        row = rows[y]
        edge = sorted((row[i] * 299 + row[i + 1] * 587 + row[i + 2] * 114) // 1000
                      for i in range(max(0, w - 9 * scale) * bpp, w * bpp, bpp))
        background.append(edge[len(edge) // 2])
    median = sorted(background)[h // 2]
    cube = min(w, h) // 3
    samples = ((w + step - 1) // step) * ((h + step - 1) // step)
    sums = [[0, 0, 0, 0] for _ in range(256 // 8 + 1)]
    colors, faces, d = set(), 0, off * bpp
    for y in range(0, h, step):
        row = rows[y]
        inside = off <= y < h - off
        up, down = (rows[y - off], rows[y + off]) if inside else (row, row)
        limit = background[y] - 12
        for x in range(0, w, step):
            i = x * bpp
            r, g, b = row[i], row[i + 1], row[i + 2]
            colors.add((r >> 3, g >> 3, b >> 3))
            luma = (r * 299 + g * 587 + b * 114) // 1000
            if luma > limit or not inside or x < off or x >= w - off:
                continue
            if x > w - cube and y < cube:
                continue
            uniform = True
            for other, j in ((row, i - d), (row, i + d), (up, i), (down, i)):
                if (abs(r - other[j]) > 4 or abs(g - other[j + 1]) > 4
                        or abs(b - other[j + 2]) > 4):
                    uniform = False
                    break
            if not uniform:
                continue
            bucket = sums[luma // 8]
            bucket[0] += 1
            bucket[1] += r
            bucket[2] += g
            bucket[3] += b
            faces += 1
    shades = []
    for index in range(len(sums) - 1, -1, -1):
        n, r, g, b = sums[index]
        if n * 1000 < 15 * samples:
            continue
        shades.append({"luma": index * 8 + 4, "pixels": n, "percent": 100.0 * n / samples,
                       "r": r // n, "g": g // n, "b": b // n})
    side_ratio = None
    if len(shades) >= 2:
        side_ratio = sum(s["pixels"] for s in shades[1:]) / shades[0]["pixels"]
    return {"width": w, "height": h, "background": median,
            "face_percent": 100.0 * faces / samples, "shades": shades,
            "bluish": all(s["b"] >= s["r"] and s["b"] - s["r"] <= 40 for s in shades),
            "side_ratio": side_ratio, "colors": len(colors)}


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "--shot":
        print(json.dumps(shot_stats(sys.argv[2])))
        return
    if len(sys.argv) not in (6, 7):
        raise SystemExit(__doc__)
    region = [int(v) for v in sys.argv[2:6]]
    rows = region_rows(sys.argv[1], *region)
    other = region_rows(sys.argv[6], *region) if len(sys.argv) == 7 else None
    stats = {"pixels": 0, "object": 0, "dark": 0, "light": 0, "grey": 0, "warm": 0, "yellow": 0}
    if other is not None:
        stats["changed"] = 0
    for y, row in enumerate(rows):
        background = Counter(row[:8]).most_common(1)[0][0]
        for x, (r, g, b) in enumerate(row):
            stats["pixels"] += 1
            luminance = 0.299 * r + 0.587 * g + 0.114 * b
            if max(abs(r - background[0]), abs(g - background[1]), abs(b - background[2])) > 24:
                stats["object"] += 1
                if max(r, g, b) - min(r, g, b) <= 30 and luminance < 170:
                    stats["grey"] += 1
            if luminance < 80:
                stats["dark"] += 1
            if luminance > 170:
                stats["light"] += 1
            if r > g + 40 and r > b + 40:
                stats["warm"] += 1
            if r > 150 and g > 110 and b < g - 60:
                stats["yellow"] += 1
            if other is not None:
                o = other[y][x]
                if max(abs(r - o[0]), abs(g - o[1]), abs(b - o[2])) > 40:
                    stats["changed"] += 1
    print(json.dumps(stats))


if __name__ == "__main__":
    main()
