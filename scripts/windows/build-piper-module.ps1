<#
.SYNOPSIS
  Package the `piper_tts` runtime module (piper.exe + DLLs + espeak data + voices)
  into a release-ready zip, compute its SHA256, and emit a modules-index.json entry.
  Does NOT publish anything to GitHub.

.DESCRIPTION
  Produces, under module-sidecars/piper_tts/dist/:
    piper_tts-<version>.zip     the module package (trispr-module.json at root,
                                bin/piper/ with exe, DLLs, ort model, espeak-ng-data/,
                                and bundled voices/)
    modules-index.json          the index entry pointing at the future asset URL
    piper_tts-<version>.sha256  the package checksum (also embedded in the index)

  Source binaries come from src-tauri/bin/piper/ — the same directory the installer
  bundles. No Rust build required: piper.exe is a pre-built binary.

  Publishing is a separate, manual step (see docs/MODULE-PUBLISHING.md).

.NOTES
  Windows-only. To add the new module to an existing modules-index.json, merge the
  entry manually; this script always emits a single-module index for review.
#>
[CmdletBinding()]
param(
    [string]$RepoRoot = (Resolve-Path "$PSScriptRoot\..\.."),
    [string]$AssetTag = "modules-index"
)

$ErrorActionPreference = "Stop"

$moduleDir   = Join-Path $RepoRoot "module-sidecars\piper_tts"
$manifestSrc = Join-Path $moduleDir "trispr-module.json"
$piperSrc    = Join-Path $RepoRoot "src-tauri\bin\piper"
$distDir     = Join-Path $moduleDir "dist"
$stagingDir  = Join-Path $distDir "staging"

# --- read version from the package manifest ---
if (-not (Test-Path $manifestSrc)) { throw "Manifest not found: $manifestSrc" }
$manifest = Get-Content $manifestSrc -Raw | ConvertFrom-Json
$version  = $manifest.version
if ([string]::IsNullOrWhiteSpace($version)) { throw "Manifest has no version" }
$moduleId = $manifest.id
Write-Host "Packaging module '$moduleId' v$version" -ForegroundColor Cyan

# --- preflight ---
if (-not (Test-Path (Join-Path $piperSrc "piper.exe"))) {
    throw "piper.exe not found at $piperSrc\piper.exe. Ensure src-tauri/bin/piper/ is populated."
}

# --- assemble staging tree ---
if (Test-Path $stagingDir) { Remove-Item -Recurse -Force $stagingDir }
$binPiperDir = Join-Path $stagingDir "bin\piper"
New-Item -ItemType Directory -Force -Path $binPiperDir | Out-Null

Copy-Item $manifestSrc (Join-Path $stagingDir "trispr-module.json")

# Copy piper.exe and all DLLs / data files
$filesToCopy = @(
    "piper.exe",
    "onnxruntime.dll",
    "onnxruntime_providers_shared.dll",
    "piper_phonemize.dll",
    "espeak-ng.dll",
    "libtashkeel_model.ort"
)
foreach ($f in $filesToCopy) {
    $src = Join-Path $piperSrc $f
    if (-not (Test-Path $src)) { throw "Required file missing: $src" }
    Copy-Item $src (Join-Path $binPiperDir $f)
}

# Copy espeak-ng-data directory
$espeakSrc = Join-Path $piperSrc "espeak-ng-data"
if (-not (Test-Path $espeakSrc)) { throw "espeak-ng-data missing at $espeakSrc" }
Copy-Item -Recurse $espeakSrc (Join-Path $binPiperDir "espeak-ng-data")

# Copy voices directory if it has any content
$voicesSrc = Join-Path $piperSrc "voices"
if ((Test-Path $voicesSrc) -and ((Get-ChildItem $voicesSrc -File).Count -gt 0)) {
    Write-Host "Bundling voices from $voicesSrc..." -ForegroundColor Cyan
    Copy-Item -Recurse $voicesSrc (Join-Path $binPiperDir "voices")
} else {
    Write-Host "No voices found in $voicesSrc - voices must be downloaded via the app." -ForegroundColor Yellow
    New-Item -ItemType Directory -Force -Path (Join-Path $binPiperDir "voices") | Out-Null
}

# --- zip (contents at archive root so trispr-module.json is top-level) ---
New-Item -ItemType Directory -Force -Path $distDir | Out-Null
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

# --- modules-index.json (single-module, merge manually for multi-module index) ---
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
Write-Host "Publishing is manual - see docs/MODULE-PUBLISHING.md" -ForegroundColor Yellow
