#!/usr/bin/env python3
"""Compares 8-bit RGB/RGBA PNGs, ignoring alpha, with no dependencies.

compare_png.py [--night] [--max-mean M] reference.png other.png...
Exits 1 if any mean difference (0-255) exceeds M.
"""
import struct
import sys
import zlib


def load(path):
    data = open(path, "rb").read()
    pos, idat = 8, b""
    while pos < len(data):
        size, kind = struct.unpack(">I4s", data[pos : pos + 8])
        body = data[pos + 8 : pos + 8 + size]
        pos += 12 + size
        if kind == b"IHDR":
            width, height, depth, color = struct.unpack(">IIBB", body[:10])
            assert depth == 8 and color in (2, 6), f"{path}: unsupported PNG"
        elif kind == b"IDAT":
            idat += body
    raw, bpp = zlib.decompress(idat), {2: 3, 6: 4}[color]
    stride, prev, out = width * bpp, bytearray(width * bpp), []
    for y in range(height):
        at = y * (stride + 1)
        f, row = raw[at], bytearray(raw[at + 1 : at + 1 + stride])
        for x in range(stride):
            a = row[x - bpp] if x >= bpp else 0
            b = prev[x]
            c = prev[x - bpp] if x >= bpp else 0
            if f == 1:
                row[x] = (row[x] + a) & 255
            elif f == 2:
                row[x] = (row[x] + b) & 255
            elif f == 3:
                row[x] = (row[x] + ((a + b) >> 1)) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                best = a if pa <= pb and pa <= pc else b if pb <= pc else c
                row[x] = (row[x] + best) & 255
        out.append(bytes(v for k, v in enumerate(row) if bpp == 3 or k % 4 != 3))
        prev = row
    return width, height, b"".join(out)


def check_night(path, img):
    width, height, data = img
    # Ignore the NVIDIA badge in the bottom-right corner.
    scene = data[:width * (height * 4 // 5) * 3]
    pixels = zip(scene[::3], scene[1::3], scene[2::3])
    gray, red, green = 0, 0, 0
    for r, g, b in pixels:
        gray += r == g == b and 24 <= r <= 27
        red += r > g + 32 and r > b + 32
        green += g > r + 32 and g > b + 32
    passed = gray > len(scene) // 12 and red > 100 and green > 100
    print(f"{path}: Night Vision palette {'passed' if passed else 'failed'} "
          f"(dark gray {gray}, red {red}, green {green})")
    return passed


def main(args):
    night = args[:1] == ["--night"]
    if night:
        args = args[1:]
    limit = None
    if args[:1] == ["--max-mean"]:
        limit, args = float(args[1]), args[2:]
    if len(args) < 2:
        sys.exit(__doc__)
    ref = load(args[0])
    failed = night and not check_night(args[0], ref)
    for path in args[1:]:
        img = load(path)
        if img[:2] != ref[:2]:
            sys.exit(f"{path}: size {img[:2]} differs from {ref[:2]}")
        diff = [abs(a - b) for a, b in zip(ref[2], img[2])]
        mean = sum(diff) / len(diff)
        print(f"{path}: max {max(diff)} mean {mean:.4f} over-8 {sum(d > 8 for d in diff)}")
        failed |= limit is not None and mean > limit
        if night:
            failed |= not check_night(path, img)
    sys.exit(failed)


main(sys.argv[1:])
