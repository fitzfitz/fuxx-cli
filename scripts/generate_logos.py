#!/usr/bin/env python3
"""
Generate multi-resolution transparent PNG logos from master vector SVGs.

Renders 1024x1024 master SVGs using headless Chrome, then downsamples
to 256x256 (medium) and 64x64 (small) using Pillow with high-quality
Lanczos resampling.
"""

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path
from PIL import Image

STYLES = ["modern", "3d-cyber", "retro-crt"]

RESOLUTIONS = {
    "large": ("fuxx-logo-large.png", (1024, 1024)),
    "medium": ("fuxx-logo-medium.png", (256, 256)),
    "small": ("fuxx-logo-small.png", (64, 64)),
}


def find_chrome() -> str:
    """Locate Google Chrome or Chromium executable."""
    env_chrome = os.environ.get("CHROME_BIN")
    if env_chrome and os.path.isfile(env_chrome) and os.access(env_chrome, os.X_OK):
        return env_chrome

    candidates = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
    ]

    for candidate in candidates:
        if os.path.isfile(candidate) and os.access(candidate, os.X_OK):
            return candidate
        which = shutil.which(candidate)
        if which:
            return which

    raise RuntimeError(
        "Could not find Google Chrome or Chromium. Set CHROME_BIN environment variable."
    )


def render_svg_to_large_png(chrome_bin: str, svg_path: Path, output_png_path: Path):
    """Render an SVG file to a 1024x1024 transparent PNG using headless Chrome."""
    file_url = svg_path.resolve().as_uri()
    cmd = [
        chrome_bin,
        "--headless",
        f"--screenshot={output_png_path.resolve()}",
        "--window-size=1024,1024",
        "--default-background-color=00000000",
        "--hide-scrollbars",
        file_url,
    ]

    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError(
            f"Chrome screenshot failed with returncode {result.returncode}:\n"
            f"stdout: {result.stdout}\nstderr: {result.stderr}"
        )

    if not output_png_path.is_file() or output_png_path.stat().st_size == 0:
        raise RuntimeError(f"Output PNG was not created or is empty: {output_png_path}")


def process_style(chrome_bin: str, style_dir: Path):
    """Render master SVG and produce large, medium, and small PNGs."""
    style_name = style_dir.name
    master_svg = style_dir / "fuxx-logo-master.svg"
    if not master_svg.is_file():
        raise FileNotFoundError(f"Master SVG missing: {master_svg}")

    large_name, large_dim = RESOLUTIONS["large"]
    large_png = style_dir / large_name

    print(f"[{style_name}] Rendering master SVG to {large_name} ({large_dim[0]}x{large_dim[1]})...")
    render_svg_to_large_png(chrome_bin, master_svg, large_png)

    # Open large image and verify dimensions & mode
    with Image.open(large_png) as img:
        img_rgba = img.convert("RGBA") if img.mode != "RGBA" else img.copy()

        if img_rgba.size != large_dim:
            img_rgba = img_rgba.resize(large_dim, Image.Resampling.LANCZOS)
            img_rgba.save(large_png, format="PNG")

        # Downsample to medium and small
        for key in ["medium", "small"]:
            res_name, (w, h) = RESOLUTIONS[key]
            res_path = style_dir / res_name
            print(f"[{style_name}] Downsampling to {res_name} ({w}x{h})...")
            resized = img_rgba.resize((w, h), Image.Resampling.LANCZOS)
            resized.save(res_path, format="PNG")

    print(f"[{style_name}] All PNG resolutions generated successfully.")


def main():
    parser = argparse.ArgumentParser(description="Generate multi-resolution PNG logos from master SVGs.")
    parser.add_argument(
        "--logo-dir",
        type=Path,
        default=Path(__file__).resolve().parent.parent / "assets" / "logo",
        help="Base directory containing logo style subfolders",
    )
    parser.add_argument(
        "--chrome-bin",
        type=str,
        default=None,
        help="Path to Chrome/Chromium binary (optional)",
    )
    args = parser.parse_args()

    chrome_bin = args.chrome_bin or find_chrome()
    print(f"Using Chrome binary: {chrome_bin}")
    print(f"Logo directory: {args.logo_dir}")

    for style in STYLES:
        style_dir = args.logo_dir / style
        if not style_dir.is_dir():
            print(f"Error: Style directory does not exist: {style_dir}", file=sys.stderr)
            sys.exit(1)
        process_style(chrome_bin, style_dir)

    print("\nSuccessfully generated all PNG logos.")


if __name__ == "__main__":
    main()
