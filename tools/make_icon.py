"""One-off: generate rust/o4fix-app/icons/icon.ico (output is committed)."""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

out = Path(__file__).resolve().parents[1] / "rust/o4fix-app/icons/icon.ico"
out.parent.mkdir(parents=True, exist_ok=True)
img = Image.new("RGBA", (256, 256), (0, 0, 0, 0))
d = ImageDraw.Draw(img)
d.rounded_rectangle([8, 8, 248, 248], radius=48, fill=(18, 22, 30, 255),
                    outline=(90, 200, 250, 255), width=8)
try:
    big = ImageFont.truetype("arialbd.ttf", 110)
    small = ImageFont.truetype("arial.ttf", 54)
except OSError:
    big = small = ImageFont.load_default()
d.text((128, 112), "O4", font=big, anchor="mm", fill=(90, 200, 250, 255))
d.text((128, 196), "fix", font=small, anchor="mm", fill=(230, 235, 240, 255))
img.save(out, sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
print("wrote", out)
