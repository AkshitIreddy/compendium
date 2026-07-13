# -*- coding: utf-8 -*-
"""Compendium app icon generator.

Design: a fanned stack of three technique cards (the knowledge packs) on an
iris-violet gradient rounded square — geometric, flat, legible at 16px.
Renders at 4x and downsamples for clean anti-aliasing. Output: assets/icon.png
(1024x1024 master for `tauri icon`).
"""
from PIL import Image, ImageDraw, ImageFilter

S = 4096  # supersample; final 1024
FINAL = 1024


def rounded_rect(draw, box, radius, fill):
    draw.rounded_rectangle(box, radius=radius, fill=fill)


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


# ---------------------------------------------------------------- background
img = Image.new("RGBA", (S, S), (0, 0, 0, 0))

# diagonal gradient: soft violet (top-left) -> deep iris (bottom-right)
TOP = (139, 125, 250)     # #8B7DFA
BOTTOM = (62, 45, 158)    # #3E2D9E
grad = Image.new("RGBA", (S, S))
gpx = grad.load()
for y in range(S):
    for x in range(0, S, 8):  # coarse columns, then smooth via resize
        t = (x + y) / (2 * S)
        c = lerp(TOP, BOTTOM, t)
        for dx in range(8):
            gpx[min(x + dx, S - 1), y] = c + (255,)
grad = grad.resize((S // 4, S // 4)).resize((S, S))  # smooth banding

mask = Image.new("L", (S, S), 0)
mdraw = ImageDraw.Draw(mask)
mdraw.rounded_rectangle([0, 0, S, S], radius=int(S * 0.225), fill=255)
img.paste(grad, (0, 0), mask)

# subtle inner glow top edge (premium sheen)
sheen = Image.new("RGBA", (S, S), (0, 0, 0, 0))
sdraw = ImageDraw.Draw(sheen)
sdraw.rounded_rectangle([0, 0, S, int(S * 0.55)], radius=int(S * 0.225),
                        fill=(255, 255, 255, 26))
sheen = sheen.filter(ImageFilter.GaussianBlur(S * 0.04))
img = Image.alpha_composite(img, Image.composite(sheen, Image.new("RGBA", (S, S), (0, 0, 0, 0)), mask))

# ---------------------------------------------------------------- card stack
# three cards, fanned by rotation, drawn back-to-front
CARD_W, CARD_H = int(S * 0.46), int(S * 0.56)
RADIUS = int(S * 0.045)
CX, CY = S // 2, int(S * 0.52)

specs = [
    # (angle deg, offset x, offset y, fill RGBA)
    (-10, -int(S * 0.055), int(S * 0.012), (255, 255, 255, 110)),
    (-5, -int(S * 0.024), int(S * 0.004), (255, 255, 255, 165)),
    (0, int(S * 0.012), 0, (255, 255, 255, 255)),
]

for angle, ox, oy, fill in specs:
    layer = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ldraw = ImageDraw.Draw(layer)
    box = [CX - CARD_W // 2 + ox, CY - CARD_H // 2 + oy,
           CX + CARD_W // 2 + ox, CY + CARD_H // 2 + oy]

    # soft drop shadow for depth
    shadow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    shdraw = ImageDraw.Draw(shadow)
    shdraw.rounded_rectangle([box[0], box[1] + S * 0.012, box[2], box[3] + S * 0.012],
                             radius=RADIUS, fill=(20, 10, 60, 70))
    shadow = shadow.filter(ImageFilter.GaussianBlur(S * 0.018))
    shadow = shadow.rotate(angle, center=(CX, CY), resample=Image.BICUBIC)
    img = Image.alpha_composite(img, Image.composite(
        shadow, Image.new("RGBA", (S, S), (0, 0, 0, 0)), mask))

    ldraw.rounded_rectangle(box, radius=RADIUS, fill=fill)

    if fill[3] == 255:  # front card: content bars in the gradient ink
        ink = (74, 56, 180, 255)
        bar_x0 = box[0] + int(CARD_W * 0.14)
        bar_h = int(CARD_H * 0.055)
        bar_r = bar_h // 2
        # title bar (wide) + three text bars, then a "recommendation" bar
        ys = [0.16, 0.34, 0.46, 0.58]
        widths = [0.50, 0.72, 0.62, 0.68]
        for frac_y, frac_w in zip(ys, widths):
            y0 = box[1] + int(CARD_H * frac_y)
            ldraw.rounded_rectangle(
                [bar_x0, y0, bar_x0 + int(CARD_W * frac_w), y0 + bar_h],
                radius=bar_r,
                fill=ink if frac_y == ys[0] else (74, 56, 180, 120),
            )
        # accent check chip bottom-left: the "best-fit" verdict
        chip_y = box[1] + int(CARD_H * 0.76)
        chip_h = int(CARD_H * 0.11)
        ldraw.rounded_rectangle(
            [bar_x0, chip_y, bar_x0 + int(CARD_W * 0.34), chip_y + chip_h],
            radius=chip_h // 2, fill=(94, 74, 214, 255),
        )
        # check mark inside chip
        cm_x = bar_x0 + int(CARD_W * 0.06)
        cm_y = chip_y + chip_h // 2
        lw = int(S * 0.008)
        ldraw.line([(cm_x, cm_y), (cm_x + int(S * 0.014), cm_y + int(S * 0.014)),
                    (cm_x + int(S * 0.038), cm_y - int(S * 0.016))],
                   fill=(255, 255, 255, 255), width=lw, joint="curve")

    layer = layer.rotate(angle, center=(CX, CY), resample=Image.BICUBIC)
    img = Image.alpha_composite(img, Image.composite(
        layer, Image.new("RGBA", (S, S), (0, 0, 0, 0)), mask))

# ---------------------------------------------------------------- export
final = img.resize((FINAL, FINAL), Image.LANCZOS)
final.save(r"C:\Users\akshi\Desktop\Code Palace\rag knowledge base\assets\icon.png")
print("icon written: assets/icon.png (1024x1024)")
