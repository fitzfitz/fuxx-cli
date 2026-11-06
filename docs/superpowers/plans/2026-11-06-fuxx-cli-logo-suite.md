# fuxx-cli Logo Suite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Generate three distinct visual styles of the `fuxx-cli` mascot logo (`╭∩╮`) in transparent PNG format across Small (64x64), Medium (256x256), and Large (1024x1024) resolutions, plus master vector SVGs.

**Architecture:** Create precision vector SVG source templates for Modern, 3D Cyber, and Retro CRT styles. Use headless Chrome with zero-background rendering to rasterize master 1024x1024 transparent PNGs, then use Pillow with Lanczos resampling to produce crisp 256x256 and 64x64 PNGs. Validate all outputs with an automated test script checking dimensions, RGBA color mode, and transparent border margins.

**Tech Stack:** SVG, Python 3, Pillow (`PIL`), Headless Chromium (`/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`).

## Global Constraints
- Target directory: `/Users/fitzgeral/Kerja/proj/fuxx-cli/assets/logo/`
- Subdirectories: `modern/`, `3d-cyber/`, `retro-crt/`
- File naming: `fuxx-logo-small.png` (64x64), `fuxx-logo-medium.png` (256x256), `fuxx-logo-large.png` (1024x1024), `fuxx-logo-master.svg`
- All PNGs must be 32-bit RGBA with transparent backgrounds outside the badge
- Primary colors: Neon Green (`#00FF66` / `#39FF14`), Charcoal Slate (`#0F172A`)

---

### Task 1: Create Generation & Verification Test Suite

**Files:**
- Create: `scripts/test_logo_assets.py`

**Interfaces:**
- Produces: Command-line verification utility `scripts/test_logo_assets.py` that verifies PNG dimensions, color modes, and non-empty pixel data.

- [ ] **Step 1: Write test verification script**

Write `scripts/test_logo_assets.py`:
```python
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
```

- [ ] **Step 2: Run test to verify it fails initially (assets do not exist yet)**

Run: `python3 scripts/test_logo_assets.py`
Expected: FAIL with "Missing directory: .../modern"

- [ ] **Step 3: Commit verification test script**

```bash
git add scripts/test_logo_assets.py
git commit -m "test: add logo assets validation script"
```

---

### Task 2: Build Master Vector SVGs for Modern, 3D Cyber, and Retro CRT

**Files:**
- Create: `assets/logo/modern/fuxx-logo-master.svg`
- Create: `assets/logo/3d-cyber/fuxx-logo-master.svg`
- Create: `assets/logo/retro-crt/fuxx-logo-master.svg`

**Interfaces:**
- Produces: 3 standalone scalable SVG master files centered on a 1024x1024 viewBox with transparent backgrounds.

- [ ] **Step 1: Create Modern Style Master SVG**
Write `assets/logo/modern/fuxx-logo-master.svg`:
A sleek squircle with `#0F172A` body, `#00FF66` stroke (opacity 0.35), and centered bold `╭∩╮` glyph in `#00FF66`.

- [ ] **Step 2: Create 3D Cyber Style Master SVG**
Write `assets/logo/3d-cyber/fuxx-logo-master.svg`:
Dimensional badge with bevel highlights, metallic borders, radial lighting, glow filter, and glowing `╭∩╮`.

- [ ] **Step 3: Create Retro CRT Style Master SVG**
Write `assets/logo/retro-crt/fuxx-logo-master.svg`:
CRT monitor bezel with curved screen, horizontal scanline pattern, phosphor green background glow, and pixelated 8-bit `╭∩╮` glyph.

- [ ] **Step 4: Verify SVGs exist and are valid XML**

Run: `python3 -c "import xml.etree.ElementTree as ET; [ET.parse(f'assets/logo/{s}/fuxx-logo-master.svg') for s in ['modern', '3d-cyber', 'retro-crt']]; print('SVGs valid')"`
Expected: "SVGs valid"

- [ ] **Step 5: Commit SVG master files**

```bash
git add assets/logo/*/fuxx-logo-master.svg
git commit -m "feat(assets): add master SVGs for modern, 3d-cyber, and retro-crt logos"
```

---

### Task 3: Build Generation Script and Export Multi-Resolution PNGs

**Files:**
- Create: `scripts/generate_logos.py`
- Output:
  - `assets/logo/modern/fuxx-logo-{large,medium,small}.png`
  - `assets/logo/3d-cyber/fuxx-logo-{large,medium,small}.png`
  - `assets/logo/retro-crt/fuxx-logo-{large,medium,small}.png`

**Interfaces:**
- Consumes: `assets/logo/*/fuxx-logo-master.svg`
- Produces: 9 PNG files across Small (`64x64`), Medium (`256x256`), and Large (`1024x1024`).

- [ ] **Step 1: Write rasterization script `scripts/generate_logos.py`**
Implement headless Chrome renderer to capture 1024x1024 transparent screenshots of each SVG master, then resize using PIL `Resampling.LANCZOS` to 256x256 and 64x64.

- [ ] **Step 2: Execute generator script**
Run: `python3 scripts/generate_logos.py`
Expected: Generation of all 9 PNG files completed with exit code 0.

- [ ] **Step 3: Run validation test**
Run: `python3 scripts/test_logo_assets.py`
Expected: "ALL LOGO ASSETS VERIFIED SUCCESSFULLY."

- [ ] **Step 4: Commit generated PNG assets**
```bash
git add assets/logo/ scripts/generate_logos.py
git commit -m "feat(assets): generate small, medium, and large PNG logos across all styles"
```

---

### Task 4: Documentation & Quick Reference

**Files:**
- Create: `assets/logo/README.md`

- [ ] **Step 1: Create Logo Catalog Documentation**
Write `assets/logo/README.md` documenting:
- Available styles (Modern, 3D Cyber, Retro CRT)
- Available sizes (Small 64x64, Medium 256x256, Large 1024x1024)
- Color codes and recommended usage guidelines (readme badges, terminal tabs, application bundle icon)

- [ ] **Step 2: Verify git status and clean state**
Run: `git status`

- [ ] **Step 3: Commit documentation**
```bash
git add assets/logo/README.md
git commit -m "docs(assets): add logo catalog and usage guidelines"
```
