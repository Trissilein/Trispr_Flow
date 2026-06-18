<#
.SYNOPSIS
  Build and package the `opus` on-demand module (trispr-opus sidecar + bundled
  FFmpeg) into a release-ready zip, compute its SHA256, and emit a
  modules-index.json entry. Does NOT publish anything to GitHub.

.DESCRIPTION
  Produces, under module-sidecars/opus/dist/:
    opus-<version>.zip     the module package (trispr-module.json at root,
                           bin/trispr-opus.exe, bin/ffmpeg/ffmpeg.exe)
    modules-index.json     the index entry pointing at the future asset URL
    opus-<version>.sha256  the package checksum (also embedded in the index)

  Publishing is a separate, manual step (see docs/MODULE-PUBLISHING.md). This
  script intentionally stops at "artifacts on disk" so a human decides when the
  public release assets are created.

.NOTES
  Windows-only for now (bundles ffmpeg.exe). FFmpeg is taken from
  src-tauri/bin/ffmpeg/ffmpeg.exe — the same binary the installer bundles.
#>
[CmdletBinding()]
param(
    [string]$RepoRoot = (Resolve-Path "$PSScriptRoot\..\.."),
    [string]$AssetTag = "modules-index"
)

$ErrorActionPreference = "Stop"

$opusCrate   = Join-Path $RepoRoot "module-sidecars\opus"
$manifestSrc = Join-Path $opusCrate "trispr-module.json"
$ffmpegSrc   = Join-Path $RepoRoot "src-tauri\bin\ffmpeg\ffmpeg.exe"
$distDir     = Join-Path $opusCrate "dist"
$stagingDir  = Join-Path $distDir "staging"

# --- read version from the package manifest ---
if (-not (Test-Path $manifestSrc)) { throw "Manifest not found: $manifestSrc" }
$manifest = Get-Content $manifestSrc -Raw | ConvertFrom-Json
$version  = $manifest.version
if ([string]::IsNullOrWhiteSpace($version)) { throw "Manifest has no version" }
$moduleId = $manifest.id
Write-Host "Packaging module '$moduleId' v$version" -ForegroundColor Cyan

# --- preflight ---
if (-not (Test-Path $ffmpegSrc)) {
    throw "FFmpeg not found at $ffmpegSrc. Place ffmpeg.exe there (same binary the installer bundles)."
}

# --- build the sidecar (release) ---
Write-Host "Building trispr-opus (release)..." -ForegroundColor Cyan
cargo build --release --manifest-path (Join-Path $opusCrate "Cargo.toml")
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
$sidecarExe = Join-Path $opusCrate "target\release\trispr-opus.exe"
if (-not (Test-Path $sidecarExe)) { throw "Built sidecar missing: $sidecarExe" }

# --- assemble staging tree ---
if (Test-Path $stagingDir) { Remove-Item -Recurse -Force $stagingDir }
$binDir    = Join-Path $stagingDir "bin"
$ffmpegDir = Join-Path $binDir "ffmpeg"
New-Item -ItemType Directory -Force -Path $ffmpegDir | Out-Null

Copy-Item $manifestSrc (Join-Path $stagingDir "trispr-module.json")
Copy-Item $sidecarExe  (Join-Path $binDir "trispr-opus.exe")
Copy-Item $ffmpegSrc   (Join-Path $ffmpegDir "ffmpeg.exe")

# --- zip (contents at archive root so trispr-module.json is top-level) ---
$zipName = "$moduleId-$version.zip"
$zipPath = Join-Path $distDir $zipName
if (Test-Path $zipPath) { Remove-Item -Force $zipPath }
Compress-Archive -Path (Join-Path $stagingDir "*") -DestinationPath $zipPath -CompressionLevel Optimal
Remove-Item -Recurse -Force $stagingDir

# --- checksum + size ---
$sha   = (Get-FileHash -Algorithm SHA256 $zipPath).Hash.ToLower()
$size  = (Get-Item $zipPath).Length
Set-Content -Path (Join-Path $distDir "$moduleId-$version.sha256") -Value "$sha  $zipName"

$assetUrl = "https://github.com/Trissilein/Trispr_Flow/releases/download/$AssetTag/$zipName"

# --- modules-index.json ---
$index = [ordered]@{
    schema_version = 1
    modules = @(
        [ordered]@{
            id              = $moduleId
            kind            = $manifest.kind
            name            = $manifest.name
            version         = $version
            asset_url       = $assetUrl
            sha256          = $sha
            size            = $size
            min_app_version = "0.8.5"
        }
    )
}
$indexPath = Join-Path $distDir "modules-index.json"
$index | ConvertTo-Json -Depth 6 | Set-Content -Path $indexPath -Encoding utf8

Write-Host ""
Write-Host "Done." -ForegroundColor Green
Write-Host "  package : $zipPath"
Write-Host "  size    : $size bytes"
Write-Host "  sha256  : $sha"
Write-Host "  index   : $indexPath"
Write-Host "  asset   : $assetUrl"
Write-Host ""
Write-Host "Publishing is manual — see docs/MODULE-PUBLISHING.md" -ForegroundColor Yellow
