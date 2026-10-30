# Publishing on-demand modules

On-demand modules (e.g. `opus`) are delivered as **GitHub release assets** in
`Trissilein/Trispr_Flow`. The app discovers them through a stable
`modules-index.json` published under its own release tag (`modules-index`), so
the index URL never changes as app releases come and go.

The delivery layer (`src-tauri/src/modules/delivery.rs`) fetches the index,
downloads the package zip, verifies its SHA256, unpacks it, and installs it via
the existing staged/atomic engine. See `docs/MODULE-DELIVERY-FEASIBILITY.md` for
the architecture.

## Build the package (no publish)

```powershell
pwsh scripts\windows\build-opus-module.ps1
```

This produces, under `module-sidecars/opus/dist/`:

| File | Purpose |
|------|---------|
| `opus-<version>.zip` | the module package (`trispr-module.json` at root, `bin/trispr-opus.exe`, `bin/ffmpeg/ffmpeg.exe`) |
| `modules-index.json` | the index entry pointing at the future asset URL |
| `opus-<version>.sha256` | the package checksum (also embedded in the index) |

The script stops here on purpose — nothing is uploaded. A human decides when the
public release assets are created.

## Publish (manual)

Both files live as assets on **one** release tagged `modules-index`. The opus zip
is referenced by `modules-index.json`'s `asset_url`, which already points at this
tag.

First time only — create the release:

```powershell
gh release create modules-index `
  --repo Trissilein/Trispr_Flow `
  --title "Module index" `
  --notes "Stable index + assets for on-demand modules. Do not delete." `
  module-sidecars/opus/dist/opus-<version>.zip `
  module-sidecars/opus/dist/modules-index.json
```

Updating an existing module (new version or new module) — re-run the build, then
upload with clobber:

```powershell
gh release upload modules-index --repo Trissilein/Trispr_Flow --clobber `
  module-sidecars/opus/dist/opus-<version>.zip `
  module-sidecars/opus/dist/modules-index.json
```

> When adding a **second** module, hand-merge its entry into the same
> `modules-index.json` (the build script currently emits an index for opus only).
> The index is the single source of truth for what the app can discover.

## Verify after publish

```powershell
# index is reachable and lists opus
curl.exe -sL https://github.com/Trissilein/Trispr_Flow/releases/download/modules-index/modules-index.json

# in the app: Modules Hub → Available to add → opus → Download
# or call the command directly via devtools: invoke('list_available_modules')
```

The downloaded zip's SHA256 must match the index entry or the install is
rejected.

## FFmpeg / licensing note

The opus package bundles `ffmpeg.exe` (the same binary the installer ships, a
gyan.dev full build → GPL). Redistributing it as a release asset carries the same
GPL obligations as bundling it in the installer. If that becomes a concern, the
alternative is a `runtime`-kind FFmpeg fetched from an official source at first
run; the sidecar already looks for `./ffmpeg/ffmpeg.exe` next to itself, so that
swap is localized.
