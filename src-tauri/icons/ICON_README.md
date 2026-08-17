# Trispr Flow Application Icon

## Design

TF block monogram on a dark rounded-square chip - matches the tray-icon style
introduced for Tris Refain (`D:\GIT\tris-refain`, "TR" monogram), so the two
apps read as a family at 16-32px tray size, while keeping Trispr Flow's own
established brand colors:

- **Cyan (#17BEDB)**: the "T"
- **Gold (#FFC107)**: the "F"
- **Dark chip (#1A1D27)**: background, replaces the previous white-circle
  Yin-Yang design - the old rings/arcs logo was too detailed to read cleanly
  at tray-icon size, which was the whole reason for the TR/TF redesign.
- Letterforms are plain rectangles (no `<text>`/font glyphs) so they stay
  pixel-identical across every rasterized size - no font-rendering ambiguity
  at 16px.

## Branding Colors

- Primary Cyan: `#17BEDB` (Cyan-500)
- Primary Gold: `#FFC107` (Amber-400)
- Accent Cyan: `#0891B2` (Cyan-700)
- Accent Gold: `#D97706` (Amber-600)

## Files

- `icon.svg`: Vector source (256x256 viewBox)
- `icon.png`: 512x512 PNG, rendered from `icon.svg`
- `icon.icns`: macOS icon, rendered from `icon.svg`
- `icon.ico`: **manually packed multi-res Windows icon** (16/20/24/32/40/48/64/128/256,
  each a native rasterization of `icon.svg` at that exact size - see below).
  DO NOT regenerate this one with the default `npx tauri icon icon.svg` - that
  command's own ICO output only embeds a single 256px frame and Windows then
  downscales it for the 16px tray icon, which reads as blurry (this bit Tris
  Refain first, see `D:\GIT\tris-refain`).
- Square logos: Windows App tile icons

## Regenerating after an `icon.svg` edit

```
npx tauri icon icon.svg                                          # icon.png, icon.icns, Square tiles
npx tauri icon --png "16,20,24,32,40,48,64,128,256" icon.svg -o _sizes   # native per-size PNGs
```
Then pack `_sizes/<N>x<N>.png` into `icon.ico` with Pillow, calling `.save()`
on the LARGEST image with the rest passed via `append_images` (Pillow's ICO
encoder filters out any requested size bigger than the base image, so the
base must be the 256px one) - passing sizes smaller than the base without
`append_images` is what silently re-triggers the blur bug above.

## Recognition

At all sizes (16x16 → 256x256 pixels):
- "T" and "F" stay legible as flat color blocks, no font hinting/antialiasing
  ambiguity since the source is plain rectangles
- Cyan and Gold stay clearly distinct against the dark chip
- Works well in System Tray at 16x16-32x32 (was the point of moving off the
  old rings/arcs design)
