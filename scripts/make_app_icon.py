#!/usr/bin/env python3
"""Make fuxx.app's icon and the browser bar's mark from the 3D Cyber logo (M11c Task 6).

- packaging/macos/fuxx.icns: the logo's visible area scaled to Apple's 824 px icon body, centred on a transparent
  1024 px canvas, exported at every size macOS asks for (16-512 px, each also @2x) through `iconutil`.
- assets/logo/3d-cyber/fuxx-mark.png: the same logo trimmed and centred with no margin, 64 px, for the popped-out
  browser window's toolbar (compiled into fuxx).
- assets/logo/3d-cyber/fuxx-app-icon-512.png: the icon as one 512 px picture, for the Dock when fuxx isn't running
  from the app package (`cargo run`; compiled into fuxx).

Both outputs are committed, so building the app needs neither Python nor Pillow. Run again after the logo changes:
    python3 -I scripts/make_app_icon.py
"""

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "assets/logo/3d-cyber/fuxx-logo-large.png"
ICNS = ROOT / "packaging/macos/fuxx.icns"
MARK = ROOT / "assets/logo/3d-cyber/fuxx-mark.png"
# The icon as one picture, for a build not running from the app package to show in the Dock (compiled into fuxx).
DOCK = ROOT / "assets/logo/3d-cyber/fuxx-app-icon-512.png"
DOCK_SIZE = 512

# Apple's macOS icon grid: a 1024 px canvas whose artwork sits in an 824 px body (a 100 px margin all round).
CANVAS = 1024
BODY = 824
MARK_SIZE = 64
# The point sizes an .iconset holds; each also comes @2x.
ICON_SIZES = (16, 32, 128, 256, 512)


def fit(bbox, canvas, body):
    """Where a picture's visible area `bbox` (left, top, right, bottom) goes on a `canvas`-sized square: scaled
    (keeping its shape) until its longer side is `body`, and centred. Returns (width, height, x, y)."""
    left, top, right, bottom = bbox
    w, h = right - left, bottom - top
    scale = body / max(w, h)
    fw, fh = round(w * scale), round(h * scale)
    return fw, fh, (canvas - fw) // 2, (canvas - fh) // 2


def place(source, canvas, body):
    """`source`'s visible area, fitted onto a transparent `canvas` square (see `fit`)."""
    bbox = source.getchannel("A").getbbox()
    w, h, x, y = fit(bbox, canvas, body)
    picture = source.crop(bbox).resize((w, h), Image.LANCZOS)
    out = Image.new("RGBA", (canvas, canvas), (0, 0, 0, 0))
    out.paste(picture, (x, y), picture)
    return out


def main():
    if shutil.which("iconutil") is None:
        sys.exit("iconutil not found: this runs on macOS")
    source = Image.open(SOURCE).convert("RGBA")
    master = place(source, CANVAS, BODY)
    with tempfile.TemporaryDirectory() as tmp:
        iconset = Path(tmp) / "fuxx.iconset"
        iconset.mkdir()
        for size in ICON_SIZES:
            for scale, suffix in ((1, ""), (2, "@2x")):
                px = size * scale
                master.resize((px, px), Image.LANCZOS).save(iconset / f"icon_{size}x{size}{suffix}.png")
        subprocess.run(["iconutil", "-c", "icns", "-o", str(ICNS), str(iconset)], check=True)
    place(source, MARK_SIZE, MARK_SIZE).save(MARK)
    master.resize((DOCK_SIZE, DOCK_SIZE), Image.LANCZOS).save(DOCK)
    print(f"wrote {ICNS.relative_to(ROOT)}, {MARK.relative_to(ROOT)} and {DOCK.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
