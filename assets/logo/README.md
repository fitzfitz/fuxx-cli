# `fuxx` Brand Assets & Logo Catalog

Official visual identity assets, master vector sources, and exported raster files for the **`fuxx`** terminal.

---

## 1. Mascot & Identity: `╭∩╮`

The **`╭∩╮`** mascot embodies the unapologetic, hacker-centric philosophy of `fuxx`:
- **Defiance & Autonomy:** An antidote to babysitting slow, needy AI coding agents. `fuxx` keeps agents running in parallel workspaces and rings you only when human attention is strictly required.
- **Terminal Heritage:** Constructed from pure box-drawing and standard unicode glyphs (`╭`, `∩`, `╮`), the mark pays homage to ASCII terminal culture while scaling into high-fidelity modern iconography.
- **Instant Recognition:** Bold, recognizable geometry that stays legible whether shrunk to a 16px favicon or rendered at 1024px on a splash screen.

---

## 2. Visual Styles

The logo suite includes three distinct stylistic treatments tailored for different environments:

| Style | Directory | Description | Ideal Context |
| :--- | :--- | :--- | :--- |
| **Modern** | `assets/logo/modern/` | Sleek charcoal squircle badge with subtle neon green stroke and minimalist vector curves. Clean, crisp, high contrast. | Terminal tab icons, sidebar badges, minimal UI, production UI chrome. |
| **3D Cyber** | `assets/logo/3d-cyber/` | Metallic beveled rim, deep radial illumination, glass specular sheen highlight, subtle cyber grid overlay, and multi-layered neon glow with white-hot core. | macOS Dock / App Bundle icon (`.icns`), hero banner, splash screen, GitHub README header. |
| **Retro CRT** | `assets/logo/retro-crt/` | Vintage CRT monitor chassis, curved recessed display, cathode scanlines, phosphor glow, and stepped 8-bit pixel mascot glyph. | Retro themes, release artwork, community wallpapers, easter eggs. |

---

## 3. Asset Specifications & File Matrix

All master vector files are built on a `1024x1024` viewBox with transparent outer bounds. Raster PNGs are exported as 32-bit `RGBA` with full transparency outside the badge envelope.

```
assets/logo/
├── README.md
├── modern/
│   ├── fuxx-logo-master.svg       # 1024x1024 vector master source
│   ├── fuxx-logo-large.png        # 1024x1024 @ 32-bit RGBA
│   ├── fuxx-logo-medium.png       # 256x256  @ 32-bit RGBA (Lanczos)
│   └── fuxx-logo-small.png        # 64x64    @ 32-bit RGBA (Lanczos)
├── 3d-cyber/
│   ├── fuxx-logo-master.svg       # 1024x1024 vector master source (SVG filters)
│   ├── fuxx-logo-large.png        # 1024x1024 @ 32-bit RGBA
│   ├── fuxx-logo-medium.png       # 256x256  @ 32-bit RGBA (Lanczos)
│   └── fuxx-logo-small.png        # 64x64    @ 32-bit RGBA (Lanczos)
└── retro-crt/
    ├── fuxx-logo-master.svg       # 1024x1024 vector master source (Scanlines + Pixel grid)
    ├── fuxx-logo-large.png        # 1024x1024 @ 32-bit RGBA
    ├── fuxx-logo-medium.png       # 256x256  @ 32-bit RGBA (Lanczos)
    └── fuxx-logo-small.png        # 64x64    @ 32-bit RGBA (Lanczos)
```

### Available Resolutions

| Resolution Tier | Dimensions | Format | Primary Use Case |
| :--- | :--- | :--- | :--- |
| **Master Vector** | `1024x1024` viewBox | `.svg` | Resolution-independent scaling, print, SVG embeds |
| **Large** | `1024x1024` px | `.png` (RGBA) | High-DPI app bundle (`icon_512x512@2x.png`), DMG installer artwork, promo cards |
| **Medium** | `256x256` px | `.png` (RGBA) | Desktop notifications, dialog modals, documentation illustrations |
| **Small** | `64x64` px | `.png` (RGBA) | Terminal tab icons, status bar icons, favicons, status badges |

---

## 4. Color Palette

The logo suite uses a cohesive cyberpunk palette balanced between dark chassis tones and radiant phosphor greens:

| Color Name | Hex Code | RGB | Role / Usage |
| :--- | :--- | :--- | :--- |
| **Terminal Green (Primary)** | `#00FF66` | `rgb(0, 255, 102)` | Primary mascot stroke, squircle accent, neon ambient glow |
| **Phosphor Lime** | `#39FF14` | `rgb(57, 255, 20)` | Retro CRT 8-bit pixel glyph and chassis power LED |
| **Neon Hotspot Core** | `#EAFFFA` | `rgb(234, 255, 250)` | 3D Cyber inner glyph core for physical neon tube intensity |
| **Cyber Cyan Rim** | `#00FFCC` / `#00E5FF` | `rgb(0, 255, 204)` | 3D Cyber metallic bevel gradient specular highlight |
| **Deep Charcoal Slate** | `#0F172A` | `rgb(15, 23, 42)` | Modern & 3D Cyber badge background fill |
| **Dark Void** | `#080C14` / `#060913` | `rgb(8, 12, 20)` | Bottom gradient falloff for depth and contrast |
| **Phosphor Screen Green**| `#0C2D13` / `#051508` | `rgb(12, 45, 19)` | Retro CRT curved display radial phosphor illumination |
| **CRT Chassis Slate** | `#282E36` / `#14171A` | `rgb(40, 46, 54)` | Retro CRT outer monitor bezel body |

---

## 5. Recommended Usage Guidelines

### Terminal Tab & Sidebar Icons
- **Recommended File:** `assets/logo/modern/fuxx-logo-small.png`
- **Reason:** Crisp lines and 35% opacity outline prevent visual clutter in compact UI chrome (16–24 px).

### Desktop Notifications & Status Badges
- **Recommended File:** `assets/logo/modern/fuxx-logo-medium.png` or `assets/logo/3d-cyber/fuxx-logo-medium.png`
- **Reason:** Provides immediate brand recognition in macOS UserNotifications banners and notification center cards.

### README & Documentation Headers
- **Recommended File:** `assets/logo/3d-cyber/fuxx-logo-large.png` or `assets/logo/retro-crt/fuxx-logo-large.png`
- **Reason:** High-impact visual presence with rich shadows, glow effects, and specular bevels.

### Application Packaging & macOS Bundle (`.app` / `.dmg`)
- **Recommended File:** `assets/logo/3d-cyber/` or `assets/logo/modern/`
- **Conversion to `.icns`:**
  To generate an Apple iconset for `fuxx.app/Contents/Resources/fuxx.icns`:
  ```bash
  mkdir fuxx.iconset
  sips -z 16 16     assets/logo/modern/fuxx-logo-small.png  --out fuxx.iconset/icon_16x16.png
  sips -z 32 32     assets/logo/modern/fuxx-logo-small.png  --out fuxx.iconset/icon_16x16@2x.png
  sips -z 32 32     assets/logo/modern/fuxx-logo-small.png  --out fuxx.iconset/icon_32x32.png
  sips -z 64 64     assets/logo/modern/fuxx-logo-small.png  --out fuxx.iconset/icon_32x32@2x.png
  sips -z 128 128   assets/logo/modern/fuxx-logo-medium.png --out fuxx.iconset/icon_128x128.png
  sips -z 256 256   assets/logo/modern/fuxx-logo-medium.png --out fuxx.iconset/icon_128x128@2x.png
  sips -z 256 256   assets/logo/modern/fuxx-logo-medium.png --out fuxx.iconset/icon_256x256.png
  sips -z 512 512   assets/logo/modern/fuxx-logo-large.png  --out fuxx.iconset/icon_256x256@2x.png
  sips -z 512 512   assets/logo/modern/fuxx-logo-large.png  --out fuxx.iconset/icon_512x512.png
  sips -z 1024 1024 assets/logo/modern/fuxx-logo-large.png  --out fuxx.iconset/icon_512x512@2x.png
  iconutil -c icns fuxx.iconset -o assets/fuxx.icns
  rm -rf fuxx.iconset
  ```

---

## 6. Build & Maintenance Tooling

The logo assets are maintained via automated scripts located in `scripts/`.

### Regenerating PNG Assets

When master SVGs are modified, regenerate all raster outputs using `scripts/generate_logos.py`:

```bash
# Uses system Google Chrome or Chromium to render pixel-perfect transparent PNGs
python3 scripts/generate_logos.py
```

Optional arguments:
- `--chrome-bin <path>`: Specify a custom Chrome/Chromium binary location (default searches standard macOS and Linux locations or respects `CHROME_BIN`).
- `--logo-dir <path>`: Target directory containing style subdirectories (defaults to `assets/logo`).

### Validating Asset Integrity

Run the automated test suite to verify file existence, correct dimensions, RGBA 32-bit color mode, and transparent border margins:

```bash
python3 scripts/test_logo_assets.py
```

Output on success:
```text
ALL LOGO ASSETS VERIFIED SUCCESSFULLY.
```
