# HelixGT icons — where files live

## Two separate icon systems

| Location | Used for |
|----------|----------|
| `static/favicon.png` | Browser tab during `npm run dev` only |
| `static/branding/*.svg` | In-app UI (menubar, file explorer, etc.) |
| `src-tauri/icons/*` | **Windows `.exe` icon**, installer, taskbar — what Tauri bundles |

Editing files under `static/` does **not** change the desktop app icon. You must update `src-tauri/icons/`.

## Source artwork

- **App / exe icon (square):** `logo-blue-logo.svg` or the cleaned `logo-app-icon-source.svg`
- **In-app file mark (tall helix):** `logo-mark.svg`

## Regenerate Tauri icons (automated)

From the project root:

```bash
npx tauri icon static/branding/logo-app-icon-source.svg
```

This overwrites everything in `src-tauri/icons/` (`icon.ico`, `32x32.png`, `128x128.png`, etc.).

Then sync the web favicon:

```bash
copy src-tauri\icons\32x32.png static\favicon.png
```

Rebuild:

```bash
npm run tauri build
```

## Manual workflow (Inkscape — recommended if CLI output looks wrong)

Tauri uses **resvg**, which renders SVG differently than Inkscape. If gradients or strokes look off, export PNGs yourself:

1. Open `static/branding/logo-blue-logo.svg` in Inkscape.
2. **File → Export PNG** at these sizes (transparent background):
   - 32×32 → save as `src-tauri/icons/32x32.png`
   - 128×128 → `src-tauri/icons/128x128.png`
   - 256×256 → `src-tauri/icons/128x128@2x.png`
   - 512×512 or 1024×1024 → `src-tauri/icons/icon.png` (source for `.ico`)
3. Build a multi-size `.ico` from the PNGs (Inkscape cannot export `.ico` directly):
   - Online: [icoconvert.com](https://icoconvert.com/) — upload `icon.png`, include 16/32/48/64/128/256
   - Or ImageMagick: `magick convert icon.png -define icon:auto-resize=256,128,64,48,32,16 src-tauri/icons/icon.ico`
4. Copy `32x32.png` to `static/favicon.png` for dev-server parity.
5. Run `npm run tauri build`.

## Executable name

`src-tauri/tauri.conf.json` sets `"mainBinaryName": "HelixGT"` so the built binary is `HelixGT.exe`.