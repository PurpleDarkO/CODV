"""Weapon-wheel icon for WEAPON_CODV_MW2 (tools/build-mw2-weapon.py): two
crossed M4-style rifles, no text, CODV's own art (tools/mw2-wheel-icon.bin,
drawn by --make-asset).

Port of SK8V tools/build-board-icon.py (Apache-2.0, NOTICE). The wheel shows a
weapon by `gotoAndStop("INT" + hash)` on the slot sprite (SLOT_WEAPONS_4 for
melee) and on MASTER_WEAPONS (in a vehicle); every stock icon is a frame
labelled INT<hash> that places one shape. This adds that frame for the MW2
item, editing the player's own hud.gfx at the tag level:

- a DefineBitsLossless2 bitmap of the art (premultiplied ARGB) and a
  DefineShape filled with it, centred like the stock icons;
- one frame per target sprite: RemoveObject2 depth 1, FrameLabel,
  PlaceObject2, ShowFrame;
- the ammo counter and weapon icon scripts treat the item as melee (no "0"):
  both switch on WeaponsLUT constants, so the value of one Online-only melee
  constant still holding its own hash (WEAPON_STONE_HATCHET, else
  WEAPON_CANDY_CANE) becomes the item's hash. SK8V's board takes the candy
  cane; a hud.gfx that already carries SK8V's board icon (its pack's copy) is a
  valid source, so both icons show. Without a free constant only the ammo text
  stays (logged).

Standard library only (setup runs it).

    python tools/build-mw2-icon.py --source <hud.gfx> --out <hud.gfx>
    python tools/build-mw2-icon.py --make-asset     (dev: redraw the art; Pillow)
"""
import argparse
import struct
import zlib
from pathlib import Path

from _common import joaat

ASSET = Path(__file__).with_name('mw2-wheel-icon.bin')
HASH = joaat('WEAPON_CODV_MW2')
WIDTH_PX = 480              # WEAPON_BAT's icon is 2480 x 287 twips
TWIPS_PER_PIXEL = 5         # stock atlas icons use 20; 4x their resolution
SPRITES = ('SLOT_WEAPONS_4', 'MASTER_WEAPONS')
LUT = '__Packages.com.rockstargames.gtav.constants.WeaponsLUT'
DONORS = ('WEAPON_STONE_HATCHET', 'WEAPON_CANDY_CANE')

DO_INIT_ACTION, EXPORT_ASSETS, DEFINE_SPRITE, END = 59, 56, 39, 0
DEFINE_SHAPE, DEFINE_BITS_LOSSLESS2 = 2, 36
SHOW_FRAME, PLACE_OBJECT2, REMOVE_OBJECT2, FRAME_LABEL = 1, 26, 28, 43


# --- SWF tags --------------------------------------------------------------

def tag(code, body, long=False):
    if len(body) < 0x3F and not long:
        return struct.pack('<H', code << 6 | len(body)) + body
    return struct.pack('<HI', code << 6 | 0x3F, len(body)) + body


def parse(data, p, end):
    """(code, header start, body start, body length) of each tag, End included."""
    out = []
    while p < end:
        cl = struct.unpack_from('<H', data, p)[0]
        code, ln, body = cl >> 6, cl & 0x3F, p + 2
        if ln == 0x3F:
            ln = struct.unpack_from('<I', data, body)[0]
            body += 4
        out.append((code, p, body, ln))
        p = body + ln
        if code == END:
            break
    return out


def header_end(data):
    nbits = data[8] >> 3
    return 8 + (5 + 4 * nbits + 7) // 8 + 4  # signature/version/length, frame RECT, rate, count


def exports(data, tags):
    names = {}
    for code, _, body, _ in tags:
        if code == EXPORT_ASSETS:
            q = body + 2
            for _ in range(struct.unpack_from('<H', data, body)[0]):
                cid = struct.unpack_from('<H', data, q)[0]
                e = data.index(b'\0', q + 2)
                names[data[q + 2:e].decode('latin1')] = cid
                q = e + 1
    return names


class Bits:
    def __init__(self):
        self.v, self.n = 0, 0

    def put(self, value, n):
        self.v = self.v << n | (value & ((1 << n) - 1))
        self.n += n
        return self

    def bytes(self):
        pad = -self.n % 8
        return (self.v << pad).to_bytes((self.n + pad) // 8, 'big')


def nbits(*values):
    """Signed bit count SWF needs for these values."""
    return max(abs(int(v)).bit_length() for v in values) + 1


def rect(w, h):
    n = nbits(w, h)
    return Bits().put(n, 5).put(0, n).put(w, n).put(0, n).put(h, n).bytes()


def matrix(scale=None, tx=0, ty=0):
    b = Bits().put(scale is not None, 1)
    if scale is not None:
        n = nbits(scale)
        b.put(n, 5).put(scale, n).put(scale, n)
    b.put(0, 1)  # no rotate
    n = nbits(tx, ty) if tx or ty else 0
    b.put(n, 5)
    if n:
        b.put(tx, n).put(ty, n)
    return b.bytes()


def icon_tags(w, h, argb_z, bitmap_id, shape_id):
    bitmap = tag(DEFINE_BITS_LOSSLESS2, struct.pack('<HBHH', bitmap_id, 5, w, h) + argb_z, long=True)
    W, H = w * TWIPS_PER_PIXEL, h * TWIPS_PER_PIXEL
    fill = struct.pack('<BBH', 1, 0x41, bitmap_id) + matrix(scale=TWIPS_PER_PIXEL << 16)  # clipped bitmap
    move, nw, nh = nbits(W, H), nbits(W), nbits(H)
    edges = Bits().put(0, 1).put(0b00101, 5).put(move, 5).put(W, move).put(H, move).put(1, 1)  # move to (W, H), fill 1
    for dx, dy in ((-W, 0), (0, -H), (W, 0), (0, H)):
        n = nw if dx else nh
        edges.put(1, 1).put(1, 1).put(n - 2, 4).put(0, 1).put(dy != 0, 1).put(dx or dy, n)
    edges.put(0, 6)  # end of shape
    shape = struct.pack('<H', shape_id) + rect(W, H) + fill + b'\0' + b'\x10' + edges.bytes()
    return bitmap + tag(DEFINE_SHAPE, shape, long=True), (W, H)


def icon_frame(shape_id, ratio, size):
    x, y = -size[0] // 2, -size[1] // 2
    place = struct.pack('<BHH', 0x16, 1, shape_id) + matrix(tx=x, ty=y) + struct.pack('<H', ratio)
    return (tag(REMOVE_OBJECT2, struct.pack('<H', 1)) + tag(FRAME_LABEL, f'INT{HASH}'.encode() + b'\0', long=True)
            + tag(PLACE_OBJECT2, place) + tag(SHOW_FRAME, b''))


# --- the edit ----------------------------------------------------------------

def lut_patch(data, tags, names):
    """The donor (DONORS) and the offset of its int value in WeaponsLUT's push."""
    sid = names.get(LUT)
    if sid is None:
        return None, None
    _, _, body, ln = next(t for t in tags if t[0] == DO_INIT_ACTION and struct.unpack_from('<H', data, t[2])[0] == sid)
    p, end, pool, hits = body + 2, body + ln, [], []
    while p < end:
        code = data[p]
        if code < 0x80:
            p += 1
            continue
        n = struct.unpack_from('<H', data, p + 1)[0]
        rec = data[p + 3:p + 3 + n]
        if code == 0x88:  # ConstantPool
            pool = rec[2:].split(b'\0')[:struct.unpack_from('<H', rec)[0]]
        elif code == 0x96 and pool:  # Push: register 1, a constant, its int value
            q, values = 0, []
            while q < n:
                t = rec[q]
                size = {0: rec.find(b'\0', q + 1) - q, 1: 4, 2: 0, 3: 0, 4: 1, 5: 1, 6: 8, 7: 4, 8: 1, 9: 2}[t]
                values.append((t, rec[q + 1:q + 1 + size], p + 4 + q))
                q += 1 + size
            for (t1, v1, _), (t2, _, at) in zip(values, values[1:]):
                if t1 in (8, 9) and t2 == 7:
                    hits.append((pool[int.from_bytes(v1, 'little')].decode(errors='replace'), at))
        p += 3 + n
    for donor in DONORS:
        at = [a for name, a in hits if name == donor]
        # Only a constant still holding its own hash (SK8V's board may have taken one).
        if len(at) == 1 and struct.unpack_from('<I', data, at[0])[0] == joaat(donor):
            return donor, at[0]
    return None, None


def uncompressed(data):
    """A CFX (zlib) movie, as some HUD mods ship hud.gfx, as plain GFX."""
    return b'GFX' + data[3:8] + zlib.decompress(data[8:]) if data[:3] == b'CFX' else data


def build(data, asset):
    if data[:3] != b'GFX':
        raise SystemExit('hud.gfx: expected a GFX file')
    w, h = struct.unpack_from('<HH', asset, 4)
    argb_z = asset[8:]
    start = header_end(data)
    tags = parse(data, start, len(data))
    names = exports(data, tags)
    if f'INT{HASH}'.encode() in data:
        raise SystemExit('hud.gfx already has the MW2 frame')
    ids = [struct.unpack_from('<H', data, body)[0] for code, _, body, _ in tags if code in (2, 22, 32, 83, 36, 20, 39, 6, 21, 35, 37, 46, 84)]
    bitmap_id = max(i for i in ids if i < 65534) + 1
    defs, size = icon_tags(w, h, argb_z, bitmap_id, bitmap_id + 1)
    targets = {names[n] for n in SPRITES}
    donor, value_at = lut_patch(data, tags, names)
    out, inserted = bytearray(data[:start]), False
    for code, hp, body, ln in tags:
        raw = data[hp:body + ln]
        if code == DEFINE_SPRITE and struct.unpack_from('<H', data, body)[0] in targets:
            if not inserted:
                out += defs
                inserted = True
            sid, frames = struct.unpack_from('<HH', data, body)
            sub = parse(data, body + 4, body + ln)
            if sub[-1][0] != END:
                raise SystemExit(f'sprite {sid}: no End tag')
            subtags = data[body + 4:sub[-1][1]] + icon_frame(bitmap_id + 1, frames, size) + data[sub[-1][1]:body + ln]
            raw = tag(DEFINE_SPRITE, struct.pack('<HH', sid, frames + 1) + subtags, long=True)
        elif value_at is not None and hp <= value_at < body + ln:
            raw = bytearray(raw)
            struct.pack_into('<I', raw, value_at - hp, HASH)
        out += raw
    if not inserted:
        raise SystemExit('target sprites not found')
    struct.pack_into('<I', out, 4, len(out))
    return bytes(out), (w, h), size, donor


def make_asset():
    """Dev step: draws the art (white on transparent, as the stock icons) and
    stores it as the premultiplied ARGB bitmap the tag wants (needs Pillow):
    an M4-style rifle silhouette (red dot, quad rail, curved magazine) drawn
    on a 600 x 200 grid, crossed with its mirror image, the front one ringed
    in black where they cross."""
    from PIL import Image, ImageChops, ImageDraw, ImageFilter

    s = 8  # supersampling
    w, h = WIDTH_PX, 120

    def rifle():
        m = Image.new('L', (600 * s, 200 * s), 0)
        d = ImageDraw.Draw(m)

        def poly(points, fill=255):
            d.polygon([(x * s, y * s) for x, y in points], fill=fill)

        def box(x0, y0, x1, y1, fill=255):
            d.rectangle([x0 * s, y0 * s, x1 * s, y1 * s], fill=fill)

        poly([(14, 74), (66, 68), (80, 66), (84, 70), (84, 112), (78, 116), (24, 126), (14, 122), (12, 98)])  # stock
        box(84, 72, 124, 92)  # buffer tube
        poly([(84, 92), (124, 92), (124, 98), (100, 104), (84, 104)])
        poly([(118, 56), (276, 56), (276, 80), (118, 80)])  # upper receiver
        box(126, 46, 274, 56)  # flat-top rail
        box(110, 58, 120, 66)  # charging handle
        poly([(120, 78), (258, 78), (258, 104), (244, 108), (210, 104), (150, 104), (138, 98), (122, 92)])  # lower
        poly([(150, 100), (176, 100), (170, 112), (160, 156), (140, 156), (144, 120)])  # pistol grip
        poly([(176, 104), (212, 104), (212, 110), (184, 118), (178, 114)])  # trigger guard
        poly([(214, 102), (246, 102), (252, 140), (262, 176), (234, 182), (222, 142)])  # magazine
        box(276, 54, 430, 88)  # handguard
        poly([(404, 54), (424, 54), (418, 30), (410, 30)])  # front sight
        box(410, 26, 418, 32)
        box(430, 64, 520, 73)  # barrel
        box(520, 59, 548, 78)  # muzzle
        poly([(184, 46), (190, 36), (178, 30), (178, 22), (240, 22), (244, 28), (244, 40), (232, 46)])  # red dot
        box(196, 40, 230, 46)
        # Detail cuts.
        for x in range(132, 272, 8):
            box(x, 46, x + 3, 50, 0)
        for x in range(290, 420, 14):
            box(x, 64, x + 8, 70, 0)
            box(x, 76, x + 8, 80, 0)
        box(124, 78, 262, 79.5, 0)
        box(276, 54, 277.5, 88, 0)
        box(84, 66, 86, 116, 0)
        box(150, 62, 196, 68, 0)
        for y in (118, 132, 146):
            k = (y - 118)
            poly([(224 + k * 0.25, y), (250 + k * 0.3, y), (250.5 + k * 0.3, y + 2.5), (224.5 + k * 0.25, y + 2.5)], 0)
        for x in (526, 534, 542):
            box(x, 59, x + 2, 64, 0)
            box(x, 73, x + 2, 78, 0)
        return m

    a = rifle()
    side = a.width
    square = Image.new('L', (side, side), 0)
    square.paste(a, (0, (side - a.height) // 2))
    front = square.rotate(18, resample=Image.BICUBIC)
    back = front.transpose(Image.FLIP_LEFT_RIGHT)
    rim = front.filter(ImageFilter.MaxFilter(5 * s + 1))
    art = ImageChops.lighter(ImageChops.subtract(back, rim), front)
    art = art.crop(art.getbbox())
    scale = min(w / art.width, h / art.height)
    art = art.resize((round(art.width * scale), round(art.height * scale)), Image.LANCZOS)
    mask = Image.new('L', (w, h), 0)
    mask.paste(art, ((w - art.width) // 2, (h - art.height) // 2))
    a = mask.tobytes()
    argb = b''.join(bytes((v, v, v, v)) for v in a)  # white, premultiplied
    ASSET.write_bytes(b'CVIC' + struct.pack('<HH', w, h) + zlib.compress(argb, 9))
    print(f'{ASSET}: {w}x{h} px')


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('--source', type=Path, help='hud.gfx to edit (the player\'s own)')
    p.add_argument('--out', type=Path)
    p.add_argument('--make-asset', action='store_true', help='dev: redraw tools/mw2-wheel-icon.bin')
    args = p.parse_args()
    if args.make_asset:
        return make_asset()
    if not args.source or not args.out:
        p.error('--source and --out are required')
    source = uncompressed(args.source.read_bytes())
    args.out.parent.mkdir(parents=True, exist_ok=True)
    if f'INT{HASH}'.encode() in source:  # already edited (CODV's own copy)
        args.out.write_bytes(source)
        return print(f'{args.out}: {args.source} already has the INT{HASH} frame; copied')
    data, px, size, donor = build(source, ASSET.read_bytes())
    args.out.write_bytes(data)
    note = f'{donor} -> MW2 (no ammo text)' if donor else 'no free melee constant: the wheel may show an ammo count'
    print(f'{args.out}: INT{HASH} frame, {px[0]}x{px[1]} px, {size[0]}x{size[1]} twips; {note}')


if __name__ == '__main__':
    main()
