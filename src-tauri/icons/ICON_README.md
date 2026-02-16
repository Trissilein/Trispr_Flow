# Trispr Flow Application Icon

## Design

The icon follows the Trispr Flow brand identity with a Yin-Yang inspired design:

- **Cyan (#17BEDB)**: Upper-left arc, representing the listening/input phase (microphone)
- **Gold (#FFC107)**: Lower-right arc, representing the transcription/output phase
- **Cyan Dot**: Upper position, indicates recording/active state
- **Gold Dot**: Lower position, indicates processing/output state
- **White Center**: Clean background for clarity at small sizes

## Branding Colors

- Primary Cyan: `#17BEDB` (Cyan-500)
- Primary Gold: `#FFC107` (Amber-400)
- Accent Cyan: `#0891B2` (Cyan-700)
- Accent Gold: `#D97706` (Amber-600)

## Files

- `icon.png`: 512x512 PNG source (generated from SVG)
- `icon.svg`: Vector source
- `icon.ico`: Windows executable icon (auto-generated from PNG during build)
- `icon.icns`: macOS icon (auto-generated from PNG during build)
- Square logos: Windows App tile icons

## Usage

The icon is automatically converted during the Tauri build process:
1. `icon.png` → `icon.ico` (Windows EXE)
2. `icon.png` → `icon.icns` (macOS)
3. Square logos → Windows App tiles (Store, Shortcuts, etc.)

## Recognition

At all sizes (16x16 → 256x256 pixels):
- The Yin-Yang shape remains recognizable
- Cyan and Gold colors are clearly distinct
- The dots provide a focal point for brand identity
- Works well in System Tray at 16x16-32x32
