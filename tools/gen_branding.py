#!/usr/bin/env python3
"""Generate deterministic PNG, ICO, and ICNS branding icons using Python stdlib only."""
import math
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ICONS_DIR = ROOT / "assets" / "icons"
BRAND_DIR = ROOT / "assets" / "branding"


def png_chunk(tag: bytes, data: bytes) -> bytes:
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)


def encode_png(w: int, h: int, rgba: bytes) -> bytes:
    raw = bytearray()
    stride = w * 4
    for y in range(h):
        raw.append(0)
        raw.extend(rgba[y * stride : (y + 1) * stride])
    ihdr = struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + png_chunk(b"IHDR", ihdr) + png_chunk(b"IDAT", zlib.compress(bytes(raw), 9)) + png_chunk(b"IEND", b"")


def point_in_poly(x: float, y: float, poly: list[tuple[float, float]]) -> bool:
    inside = False
    n = len(poly)
    j = n - 1
    for i in range(n):
        xi, yi = poly[i]
        xj, yj = poly[j]
        if ((yi > y) != (yj > y)) and (x < (xj - xi) * (y - yi) / (yj - yi + 1e-12) + xi):
            inside = not inside
        j = i
    return inside


def shield_sdf(nx: float, ny: float) -> float:
    """Signed distance approximation in normalized [0,1]x[0,1] space (<0 inside)."""
    ax = abs(nx - 0.5)
    if ny < 0.06:
        return 0.06 - ny
    if ny < 0.48:
        # Top edge slope from (0.5, 0.06) to (0.87, 0.19)
        top_y = 0.06 + (ax / 0.37) * 0.13
        if ny < top_y:
            return top_y - ny
        return ax - 0.37
    # Lower curved shield taper
    t = (ny - 0.48) / (0.94 - 0.48)
    if t > 1.0:
        return ny - 0.94
    half_w = 0.37 * max(0.0, 1.0 - t * t)
    return ax - half_w


def sample_logo(nx: float, ny: float) -> tuple[int, int, int, int]:
    sd = shield_sdf(nx, ny)
    if sd > 0.0:
        return (0, 0, 0, 0)
    if sd > -0.036:
        # Cyan-blue shield border
        t = ny
        r = int(56 * (1 - t) + 14 * t)
        g = int(189 * (1 - t) + 165 * t)
        b = int(248 * (1 - t) + 233 * t)
        return (r, g, b, 255)

    # Dark navy interior
    bg_r = int(30 * (1 - ny) + 15 * ny)
    bg_g = int(41 * (1 - ny) + 23 * ny)
    bg_b = int(59 * (1 - ny) + 42 * ny)

    # Aperture center
    cx, cy = 0.5, 0.485
    dx, dy = nx - cx, ny - cy
    dist = math.hypot(dx, dy)

    if dist <= 0.044:
        # Bright lens core
        k = dist / 0.044
        r = int(224 * (1 - k) + 56 * k)
        g = int(242 * (1 - k) + 189 * k)
        b = int(254 * (1 - k) + 248 * k)
        return (r, g, b, 255)
    if dist <= 0.072:
        # Inner iris ring
        if dist >= 0.062:
            return (56, 189, 248, 255)
        return (15, 23, 42, 255)
    if dist <= 0.245:
        if dist >= 0.228:
            return (56, 189, 248, 255)
        # 6 aperture blades
        angle = math.atan2(dy, dx)
        sector = int((angle + math.pi) / (math.pi / 3.0)) % 6
        local_ang = (angle + math.pi) % (math.pi / 3.0)
        # Crisp gap between blades
        if local_ang < 0.09 or local_ang > (math.pi / 3.0 - 0.09):
            return (15, 23, 42, 255)
        if sector % 2 == 0:
            return (2, 132, 199, 255)
        return (34, 211, 238, 255)

    return (bg_r, bg_g, bg_b, 255)


def render_icon(size: int) -> bytes:
    ss = 3 if size <= 64 else 2
    buf = bytearray(size * size * 4)
    for y in range(size):
        for x in range(size):
            ar = ag = ab = aa = 0
            for sy in range(ss):
                for sx in range(ss):
                    nx = (x + (sx + 0.5) / ss) / size
                    ny = (y + (sy + 0.5) / ss) / size
                    r, g, b, a = sample_logo(nx, ny)
                    ar += r * a
                    ag += g * a
                    ab += b * a
                    aa += a
            n = ss * ss
            idx = (y * size + x) * 4
            if aa == 0:
                buf[idx : idx + 4] = b"\x00\x00\x00\x00"
            else:
                buf[idx] = ar // aa
                buf[idx + 1] = ag // aa
                buf[idx + 2] = ab // aa
                buf[idx + 3] = aa // n
    return encode_png(size, size, bytes(buf))


def build_ico(png_map: dict[int, bytes]) -> bytes:
    sizes = sorted(png_map.keys())
    header = struct.pack("<HHH", 0, 1, len(sizes))
    offset = 6 + 16 * len(sizes)
    entries = bytearray()
    payload = bytearray()
    for s in sizes:
        data = png_map[s]
        dim = 0 if s >= 256 else s
        entries.extend(struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset))
        payload.extend(data)
        offset += len(data)
    return header + bytes(entries) + bytes(payload)


def build_icns(png_map: dict[int, bytes]) -> bytes:
    # ic07=128x128 PNG, ic08=256x256 PNG, ic09=512x512 PNG
    chunks = bytearray()
    for tag, s in [(b"ic07", 128), (b"ic08", 256), (b"ic09", 512)]:
        data = png_map[s]
        chunks.extend(tag + struct.pack(">I", 8 + len(data)) + data)
    return b"icns" + struct.pack(">I", 8 + len(chunks)) + bytes(chunks)


def main() -> None:
    ICONS_DIR.mkdir(parents=True, exist_ok=True)
    BRAND_DIR.mkdir(parents=True, exist_ok=True)
    sizes = [16, 32, 48, 64, 128, 256, 512]
    pngs: dict[int, bytes] = {}
    for s in sizes:
        data = render_icon(s)
        pngs[s] = data
        (ICONS_DIR / f"icon-{s}.png").write_bytes(data)
    (ICONS_DIR / "app.ico").write_bytes(build_ico({s: pngs[s] for s in (16, 32, 48, 256)}))
    (ICONS_DIR / "app.icns").write_bytes(build_icns(pngs))
    print(f"Generated {len(sizes)} PNGs, app.ico, and app.icns in {ICONS_DIR}")


if __name__ == "__main__":
    main()
