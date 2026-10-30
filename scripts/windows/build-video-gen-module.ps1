<#
.SYNOPSIS
  Package the `output_video_generation` runtime module (hyperframes Node package)
  into a release-ready zip, compute its SHA256, and emit a modules-index.json entry.
  Does NOT publish anything to GitHub.

.DESCRIPTION
  Produces, under module-sidecars/video_gen/dist/:
    output_video_generation-<version>.zip   the module package (trispr-module.json at root,
                                            bin/hyperframes/ with package.json + node_modules/)
    modules-index.json                       the index entry pointing at the future asset URL
    output_video_generation-<version>.sha256 the package checksum

  Source comes from src-tauri/bin/hyperframes/ (the directory run_hyperframes_render uses).
  Node itself is NOT bundled — Node 22+ must be installed on the user's machine or provided
  via settings.video_generation.node_binary_path.

  Publishing is a separate, manual step (see docs/MODULE-PUBLISHING.md).
#>
[CmdletBinding()]
param(
    [string]$RepoRoot = (Resolve-Path "$PSScriptRoot\..\.."),
    [string]$AssetTag = "modules-index"
)

$ErrorActionPreference = "Stop"

$moduleDir      = Join-Path $RepoRoot "module-sidecars\video_gen"
$manifestSrc    = Join-Path $moduleDir "trispr-module.json"
$hyperframesSrc = Join-Path $RepoRoot "src-tauri\bin\hyperframes"
$distDir        = Join-Path $moduleDir "dist"
$stagingDir     = Join-Path $distDir "staging"

# --- read version from the package manifest ---
if (-not (Test-Path $manifestSrc)) { throw "Manifest not found: $manifestSrc" }
$manifest = Get-Content $manifestSrc -Raw | ConvertFrom-Json
$version  = $manifest.version
if ([string]::IsNullOrWhiteSpace($version)) { throw "Manifest has no version" }
$moduleId = $manifest.id
Write-Host "Packaging module '$moduleId' v$version" -ForegroundColor Cyan

# --- preflight ---
if (-not (Test-Path (Join-Path $hyperframesSrc "package.json"))) {
    throw "hyperframes package.json not found at $hyperframesSrc\package.json. Run: npm ci in src-tauri/bin/hyperframes/"
}
if (-not (Test-Path (Join-Path $hyperframesSrc "node_modules"))) {
    throw "hyperframes node_modules missing at $hyperframesSrc\node_modules. Run: npm ci in src-tauri/bin/hyperframes/"
}

# --- assemble staging tree ---
if (Test-Path $stagingDir) { Remove-Item -Recurse -Force $stagingDir }
$binHfDir = Join-Path $stagingDir "bin\hyperframes"
New-Item -ItemType Directory -Force -Path $binHfDir | Out-Null

Copy-Item $manifestSrc (Join-Path $stagingDir "trispr-module.json")

# Copy hyperframes: package.json, package-lock.json, node_modules/
Copy-Item (Join-Path $hyperframesSrc "package.json")      (Join-Path $binHfDir "package.json")
Copy-Item (Join-Path $hyperframesSrc "package-lock.json") (Join-Path $binHfDir "package-lock.json")
Write-Host "Copying node_modules (may take a moment)..." -ForegroundColor Cyan
Copy-Item -Recurse (Join-Path $hyperframesSrc "node_modules") (Join-Path $binHfDir "node_modules")

# --- zip ---
New-Item -ItemType Directory -Force -Path $distDir | Out-Null
$zipName = "$moduleId-$version.zip"
$zipPath = Join-Path $distDir $zipName
if (Test-Path $zipPath) { Remove-Item -Force $zipPath }
Compress-Archive -Path (Join-Path $stagingDir "*") -DestinationPath $zipPath -CompressionLevel Optimal
Remove-Item -Recurse -Force $stagingDir

# --- checksum + size ---
$sha  = (Get-FileHash -Algorithm SHA256 $zipPath).Hash.ToLower()
$size = (Get-Item $zipPath).Length
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
Write-Host "Note: Node.js is NOT bundled. Users need Node 22+ installed or must set" -ForegroundColor Yellow
Write-Host "      settings.video_generation.node_binary_path." -ForegroundColor Yellow
Write-Host "Publishing is manual - see docs/MODULE-PUBLISHING.md" -ForegroundColor Yellow
