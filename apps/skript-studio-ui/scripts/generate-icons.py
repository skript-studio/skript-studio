"""Generate simple placeholder app icons for Skript Studio.

Tauri requires .png (multiple sizes), .ico (Windows), and .icns (macOS)
icons to be present in src-tauri/icons/. We generate a flat orange-on-dark
icon with the SkriptStudio "spark" mark. These are placeholders — the
real icon set should be designed in a proper tool.
"""

from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

ICONS_DIR = Path("/home/z/my-project/skript-studio/src-tauri/icons")
ICONS_DIR.mkdir(parents=True, exist_ok=True)

# Brand palette
BG = (13, 17, 23, 255)        # #0d1117
ACCENT = (249, 115, 22, 255)  # #f97316
WHITE = (255, 255, 255, 255)


def draw_icon(size: int) -> Image.Image:
    """Draw a single icon at the given size."""
    img = Image.new("RGBA", (size, size), BG)
    draw = ImageDraw.Draw(img)

    # Background rounded rectangle (subtle inset)
    inset = max(1, size // 16)
    draw.rounded_rectangle(
        [inset, inset, size - inset, size - inset],
        radius=size // 6,
        fill=BG,
    )

    # Orange gradient-ish square as backdrop for the mark
    mark_inset = size // 4
    draw.rounded_rectangle(
        [mark_inset, mark_inset, size - mark_inset, size - mark_inset],
        radius=size // 8,
        fill=ACCENT,
    )

    # Letter "S" for Skript — use default font
    try:
        font_size = int(size * 0.45)
        for path in [
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
        ]:
            if Path(path).exists():
                font = ImageFont.truetype(path, font_size)
                break
        else:
            font = ImageFont.load_default()
    except Exception:
        font = ImageFont.load_default()

    text = "S"
    bbox = draw.textbbox((0, 0), text, font=font)
    text_w = bbox[2] - bbox[0]
    text_h = bbox[3] - bbox[1]
    text_x = (size - text_w) // 2 - bbox[0]
    text_y = (size - text_h) // 2 - bbox[1]
    draw.text((text_x, text_y), text, fill=WHITE, font=font)

    return img


def main() -> None:
    # PNG icons at common sizes
    for size, name in [
        (32, "32x32.png"),
        (128, "128x128.png"),
        (256, "128x128@2x.png"),
        (512, "icon.png"),
    ]:
        path = ICONS_DIR / name
        draw_icon(size).save(path, "PNG")
        print(f"  wrote {path.name} ({size}x{size})")

    # Square 512 for icns source
    icon_512 = draw_icon(512)
    icon_512.save(ICONS_DIR / "icon.png", "PNG")

    # ICO (Windows) — bundle multiple sizes
    ico_sizes = [16, 32, 48, 64, 128, 256]
    icon_256 = draw_icon(256)
    icon_256.save(
        ICONS_DIR / "icon.ico",
        format="ICO",
        sizes=[(s, s) for s in ico_sizes],
    )
    print(f"  wrote icon.ico (sizes: {ico_sizes})")

    # ICNS (macOS) — Pillow supports basic icns output
    icon_512.save(ICONS_DIR / "icon.icns", format="ICNS")
    print("  wrote icon.icns")

    print("\nAll icons generated in", ICONS_DIR)


if __name__ == "__main__":
    main()
