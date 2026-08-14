#!/usr/bin/env python3
"""Generate the PinBridge app icon (icons/icon.ico) with zero dependencies.

Design matches the product mark used in the UI toolbar: a dark rounded tile
with a white CPU/VM chip outline and a sampling pulse through its center.
All geometry is drawn with 2x supersampling for clean edges, then packed as
a multi-size PNG-compressed ICO (16/32/48/256).
"""

import struct
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "bindings" / "rust" / "pinbridge-ui" / "icons" / "icon.ico"

BG = (13, 14, 16, 255)        # near-black tile
FG = (242, 242, 242, 255)     # off-white artwork


def clamp(v, lo, hi):
    return lo if v < lo else hi if v > hi else v


def new_canvas(size):
    return bytearray(size * size * 4)


def blend(px, idx, color):
    sr, sg, sb, sa = color
    if sa == 255:
        px[idx:idx + 4] = bytes((sr, sg, sb, 255))
        return
    dr, dg, db, da = px[idx], px[idx + 1], px[idx + 2], px[idx + 3]
    a = sa / 255.0
    px[idx] = int(sr * a + dr * (1 - a))
    px[idx + 1] = int(sg * a + dg * (1 - a))
    px[idx + 2] = int(sb * a + db * (1 - a))
    px[idx + 3] = min(255, sa + da)


def fill_round_rect(px, size, x0, y0, x1, y1, radius, color):
    for y in range(y0, y1):
        for x in range(x0, x1):
            cx = clamp(x, x0 + radius, x1 - 1 - radius)
            cy = clamp(y, y0 + radius, y1 - 1 - radius)
            if (x - cx) ** 2 + (y - cy) ** 2 <= radius * radius:
                blend(px, (y * size + x) * 4, color)


def draw_thick_segment(px, size, ax, ay, bx, by, width, color):
    half = width / 2.0
    minx = int(min(ax, bx) - half) - 1
    maxx = int(max(ax, bx) + half) + 1
    miny = int(min(ay, by) - half) - 1
    maxy = int(max(ay, by) + half) + 1
    dx, dy = bx - ax, by - ay
    length_sq = dx * dx + dy * dy
    for y in range(max(0, miny), min(size, maxy)):
        for x in range(max(0, minx), min(size, maxx)):
            if length_sq == 0:
                t = 0.0
            else:
                t = clamp(((x - ax) * dx + (y - ay) * dy) / length_sq, 0.0, 1.0)
            px_x = ax + t * dx
            px_y = ay + t * dy
            if (x - px_x) ** 2 + (y - px_y) ** 2 <= half * half:
                blend(px, (y * size + x) * 4, color)


def draw_polyline(px, size, points, width, color):
    for (ax, ay), (bx, by) in zip(points, points[1:]):
        draw_thick_segment(px, size, ax, ay, bx, by, width, color)


def render(size):
    px = new_canvas(size)
    unit = size / 256.0
    # tile
    fill_round_rect(px, size, 0, 0, size, size, int(40 * unit), BG)
    # chip outline: rect via four thick segments
    cx0, cy0, cx1, cy1 = 92 * unit, 92 * unit, 164 * unit, 164 * unit
    stroke = 9 * unit
    draw_polyline(px, size, [(cx0, cy0), (cx1, cy0), (cx1, cy1), (cx0, cy1), (cx0, cy0)], stroke, FG)
    # pins: three per side
    pin_len = 16 * unit
    for frac in (0.30, 0.5, 0.70):
        px_pos = cx0 + (cx1 - cx0) * frac
        draw_thick_segment(px, size, px_pos, cy0 - pin_len, px_pos, cy0, stroke * 0.8, FG)
        draw_thick_segment(px, size, px_pos, cy1, px_pos, cy1 + pin_len, stroke * 0.8, FG)
        py_pos = cy0 + (cy1 - cy0) * frac
        draw_thick_segment(px, size, cx0 - pin_len, py_pos, cx0, py_pos, stroke * 0.8, FG)
        draw_thick_segment(px, size, cx1, py_pos, cx1 + pin_len, py_pos, stroke * 0.8, FG)
    # sampling pulse through the center
    pulse = [
        (102 * unit, 128 * unit), (114 * unit, 128 * unit), (121 * unit, 108 * unit),
        (132 * unit, 146 * unit), (140 * unit, 120 * unit), (152 * unit, 128 * unit),
    ]
    draw_polyline(px, size, pulse, 7 * unit, FG)
    return px


def downsample2(px, size):
    half = size // 2
    out = bytearray(half * half * 4)
    for y in range(half):
        for x in range(half):
            idx = (y * half + x) * 4
            base = ((y * 2) * size + (x * 2)) * 4
            for c in range(4):
                total = (px[base + c] + px[base + 4 + c]
                         + px[base + size * 4 + c] + px[base + size * 4 + 4 + c])
                out[idx + c] = total // 4
    return out


def png_encode(px, size):
    raw = bytearray()
    for y in range(size):
        raw.append(0)
        raw.extend(px[y * size * 4:(y + 1) * size * 4])
    compressed = zlib.compress(bytes(raw), 9)

    def chunk(tag, data):
        body = tag + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", ihdr)
            + chunk(b"IDAT", compressed)
            + chunk(b"IEND", b""))


def main():
    images = []
    for target in (256, 48, 32, 16):
        size = target * 2
        px = render(size)
        px = downsample2(px, size)
        images.append((target, png_encode(px, target)))

    header = struct.pack("<HHH", 0, 1, len(images))
    entries = bytearray()
    data = bytearray()
    offset = 6 + 16 * len(images)
    for size, png in images:
        width_byte = size if size < 256 else 0
        entries += struct.pack("<BBBBHHII", width_byte, width_byte, 0, 0, 1, 32, len(png), offset)
        data += png
        offset += len(png)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_bytes(header + bytes(entries) + bytes(data))
    print(f"wrote {OUT} ({OUT.stat().st_size} bytes, sizes 16/32/48/256)")


if __name__ == "__main__":
    main()
