#Requires -Version 5.1
param(
  [string]$ArchiveUrl = "",
  [string]$ArchivePath = "",
  [string]$ExpectedArchiveSha256 = "",
  [string]$ManifestPath = "src-tauri\runtime-manifests\vulkan-v0.8.4-hotfix.json"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..\..")).Path
$ManifestFullPath = if ([System.IO.Path]::IsPathRooted($ManifestPath)) { $ManifestPath } else { Join-Path $RepoRoot $ManifestPath }
$TargetDir = Join-Path $RepoRoot "src-tauri\bin\vulkan"
$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("trispr-vulkan-runtime-{0}" -f [guid]::NewGuid().ToString("N"))

function Write-Section([string]$Text) {
  Write-Host "`n== $Text ==" -ForegroundColor Cyan
}

function Get-Sha256Lower([string]$Path) {
  return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

if ([string]::IsNullOrWhiteSpace($ArchiveUrl) -and [string]::IsNullOrWhiteSpace($ArchivePath)) {
  throw "A trusted Vulkan runtime archive is required. Provide -ArchiveUrl or -ArchivePath."
}

New-Item -ItemType Directory -Force -Path $TempDir | Out-Null
try {
  if (-not [string]::IsNullOrWhiteSpace($ArchivePath)) {
    $ZipPath = if ([System.IO.Path]::IsPathRooted($ArchivePath)) { $ArchivePath } else { Join-Path $RepoRoot $ArchivePath }
    if (-not (Test-Path -LiteralPath $ZipPath -PathType Leaf)) {
      throw "ArchivePath not found: $ZipPath"
    }
    Write-Section "Using local trusted Vulkan archive"
    Write-Host "Archive: $ZipPath"
  } else {
    $ZipPath = Join-Path $TempDir "vulkan-runtime.zip"
    Write-Section "Downloading trusted Vulkan archive"
    Write-Host "Source: $ArchiveUrl"
    Invoke-WebRequest -Uri $ArchiveUrl -OutFile $ZipPath -UseBasicParsing
  }

  if (-not [string]::IsNullOrWhiteSpace($ExpectedArchiveSha256)) {
    Write-Section "Verifying archive hash"
    $actualZipHash = Get-Sha256Lower $ZipPath
    $expectedZipHash = $ExpectedArchiveSha256.ToLowerInvariant()
    if ($actualZipHash -ne $expectedZipHash) {
      throw "Archive SHA256 mismatch. expected=$expectedZipHash actual=$actualZipHash"
    }
    Write-Host "  OK: archive SHA256 matches."
  } else {
    throw "ExpectedArchiveSha256 is required for trusted archive hydration."
  }

  Write-Section "Extracting archive"
  $ExtractDir = Join-Path $TempDir "extract"
  Expand-Archive -LiteralPath $ZipPath -DestinationPath $ExtractDir -Force

  $manifest = Get-Content -LiteralPath $ManifestFullPath -Raw | ConvertFrom-Json
  if (-not $manifest.files -or $manifest.files.Count -eq 0) {
    throw "Manifest contains no file entries: $ManifestFullPath"
  }

  if (Test-Path -LiteralPath $TargetDir) {
    Remove-Item -LiteralPath $TargetDir -Recurse -Force
  }
  New-Item -ItemType Directory -Force -Path $TargetDir | Out-Null

  foreach ($entry in $manifest.files) {
    $matches = @(Get-ChildItem -LiteralPath $ExtractDir -Recurse -File | Where-Object { $_.Name -eq $entry.name })
    if ($matches.Count -ne 1) {
      throw "Expected exactly one '$($entry.name)' in archive, found $($matches.Count)."
    }
    Copy-Item -LiteralPath $matches[0].FullName -Destination (Join-Path $TargetDir $entry.name) -Force
  }

  Write-Section "Validating hydrated Vulkan payload"
  & node (Join-Path $RepoRoot "scripts\validate-runtime-manifest.mjs") --manifest $ManifestFullPath --root $TargetDir --label "hydrated-vulkan"
  if ($LASTEXITCODE -ne 0) {
    throw "Hydrated Vulkan payload failed manifest validation."
  }

  Write-Host "`nVulkan runtime hydration completed successfully." -ForegroundColor Green
} finally {
  if (Test-Path -LiteralPath $TempDir) {
    Remove-Item -LiteralPath $TempDir -Recurse -Force -ErrorAction SilentlyContinue
  }
}