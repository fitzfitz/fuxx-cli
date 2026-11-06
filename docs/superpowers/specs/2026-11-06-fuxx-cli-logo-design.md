# Design Spec: fuxx-cli Logo Suite

* **Date:** 2026-11-06
* **Topic:** Brand Identity / Mascot Logo Suite for `fuxx-cli`
* **Status:** Approved by User

---

## 1. Context & Objective

`fuxx-cli` is a native, GPU-accelerated terminal for running AI coding agents. The project requires a distinctive, punchy logo suite based on the mascot `╭∩╮`.

The logo suite must be provided in three distinct visual styles:
1. **Modern Terminal Squircle** (sleek, high-contrast modern developer tool aesthetic)
2. **3D Cyber Enamel Badge** (dimensional lighting, frosted glass / obsidian depth, ambient neon bloom)
3. **Retro CRT Terminal** (nostalgic phosphor CRT monitor window with scanlines and blocky pixel art)

Each style will be exported as transparent PNGs in three standard sizes:
- **Small**: `64x64` px (Favicons, status bars, terminal badges)
- **Medium**: `256x256` px (User avatars, documentation headers, app icons)
- **Large**: `1024x1024` px (Marketing banners, high-DPI displays, App Store assets)
- Master source files (`.svg`) for vector reproducibility.

All generated assets will be stored in:
`/Users/fitzgeral/Kerja/proj/fuxx-cli/assets/logo/`

---

## 2. Visual Specifications

### 2.1 Color Palette
- **Terminal Neon Green (Core)**: `#00FF66`
- **Glow & Phosphor Bloom**: `#00FF88` (with low-opacity gradient falloffs `#00FF6644`, `#00FF6611`)
- **Dark Slate / Charcoal Base**: `#0F172A`
- **Obsidian Deep Shadow**: `#080C14`
- **Border Rim**: `#00FF66` (30%–60% opacity)
- **Canvas Background**: `rgba(0, 0, 0, 0)` (Full alpha transparency outside the badge shape)

### 2.2 Style A: Modern Terminal Squircle
- **Canvas**: 1024x1024 square with transparent outer margins.
- **Badge**: Centered rounded squircle (approx. 860x860 px, radius 210 px).
- **Surface**: Solid `#0F172A` with subtle linear gradient to `#080C14`.
- **Border**: 12 px perimeter stroke `#00FF66` at 35% opacity.
- **Mascot Glyph**: Precisely kerned and geometrically centered `╭∩╮` in solid `#00FF66`.

### 2.3 Style B: 3D Cyber / Enamel Badge
- **Canvas**: 1024x1024 square with transparent outer margins.
- **Badge**: Metallic-trimmed squircle with top-left specular reflection highlight, beveled edge, and subtle dark drop shadow.
- **Surface**: Deep radial gradient `#1E293B` to `#0A0F1D`.
- **Mascot Glyph**: Layered `╭∩╮` featuring a bright green/white core with a multi-layered gaussian blur bloom creating a neon-glow lighting effect.

### 2.4 Style C: Retro CRT / Pixel-Art Terminal
- **Canvas**: 1024x1024 square with transparent outer margins.
- **Frame**: Retro CRT monitor chassis with rounded display border and inner bezel.
- **Screen Surface**: Deep phosphor green-black background (`#051508`) with horizontal scanlines.
- **Mascot Glyph**: Pixelated / stepped 8-bit bitmap rendition of `╭∩╮` in vivid phosphor lime (`#39FF14`) with green screen glow.

---

## 3. Directory Layout

```
/Users/fitzgeral/Kerja/proj/fuxx-cli/assets/logo/
├── modern/
│   ├── fuxx-logo-small.png       (64x64)
│   ├── fuxx-logo-medium.png      (256x256)
│   ├── fuxx-logo-large.png       (1024x1024)
│   └── fuxx-logo-master.svg
├── 3d-cyber/
│   ├── fuxx-logo-small.png       (64x64)
│   ├── fuxx-logo-medium.png      (256x256)
│   ├── fuxx-logo-large.png       (1024x1024)
│   └── fuxx-logo-master.svg
└── retro-crt/
    ├── fuxx-logo-small.png       (64x64)
    ├── fuxx-logo-medium.png      (256x256)
    ├── fuxx-logo-large.png       (1024x1024)
    └── fuxx-logo-master.svg
```

---

## 4. Pipeline & Verification

1. **Master Generation**:
   * Generate vector SVG templates for each style containing exact paths, gradients, filters, and glyph geometries.
2. **Headless High-Resolution PNG Rendering**:
   * Render SVG master to 1024x1024 PNG with transparent background using Chrome headless (`--headless --screenshot --window-size=1024,1024 --default-background-color=00000000`).
3. **Downsampling to Medium & Small**:
   * Use Python Pillow with `Resampling.LANCZOS` to generate `256x256` and `64x64` PNGs to maintain razor-sharp glyph edges.
4. **Verification**:
   * Inspect output file sizes, pixel dimensions (`(64,64)`, `(256,256)`, `(1024,1024)`), and ensure alpha channel transparency (`mode == 'RGBA'`).
