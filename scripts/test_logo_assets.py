import os
import sys
from PIL import Image

EXPECTED_STYLES = ["modern", "3d-cyber", "retro-crt"]
EXPECTED_SIZES = {
    "fuxx-logo-small.png": (64, 64),
    "fuxx-logo-medium.png": (256, 256),
    "fuxx-logo-large.png": (1024, 1024),
}

def verify_logos(base_dir: str):
    failures = []
    for style in EXPECTED_STYLES:
        style_dir = os.path.join(base_dir, style)
        if not os.path.isdir(style_dir):
            failures.append(f"Missing directory: {style_dir}")
            continue

        svg_path = os.path.join(style_dir, "fuxx-logo-master.svg")
        if not os.path.isfile(svg_path) or os.path.getsize(svg_path) == 0:
            failures.append(f"Missing or empty SVG: {svg_path}")

        for filename, (expected_w, expected_h) in EXPECTED_SIZES.items():
            png_path = os.path.join(style_dir, filename)
            if not os.path.isfile(png_path):
                failures.append(f"Missing PNG file: {png_path}")
                continue

            try:
                with Image.open(png_path) as img:
                    if img.size != (expected_w, expected_h):
                        failures.append(f"{png_path}: Expected size {(expected_w, expected_h)}, got {img.size}")
                    if img.mode != "RGBA":
                        failures.append(f"{png_path}: Expected mode 'RGBA', got '{img.mode}'")
                    # Check that top-left pixel is transparent
                    top_left = img.getpixel((0, 0))
                    if len(top_left) == 4 and top_left[3] != 0:
                        failures.append(f"{png_path}: Top-left pixel alpha is {top_left[3]}, expected 0 (transparent)")
            except Exception as e:
                failures.append(f"{png_path}: Failed to open/verify - {e}")

    if failures:
        print("VERIFICATION FAILED:")
        for f in failures:
            print(f"  - {f}")
        sys.exit(1)
    else:
        print("ALL LOGO ASSETS VERIFIED SUCCESSFULLY.")

if __name__ == "__main__":
    target = sys.argv[1] if len(sys.argv) > 1 else "/Users/fitzgeral/Kerja/proj/fuxx-cli/assets/logo"
    verify_logos(target)
