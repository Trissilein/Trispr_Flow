#Requires -Version 5.1
<#
.SYNOPSIS
  Hydrates bundled Whisper runtime folders from a published Trispr Flow installer.

.DESCRIPTION
  Release CI cannot build installers from a clean checkout because `src-tauri/bin/cuda`
  and `src-tauri/bin/vulkan` are intentionally ignored and kept out of git.
  This script downloads a published installer asset, performs a silent install into a
  temporary directory, and copies the bundled runtime payloads back into the repo.

  By default it scans recent published releases and picks the first release that
  contains a matching installer asset for the requested variant. The current tag can
  be skipped so tag builds rehydrate from the previous stable release.

.EXAMPLE
  .\scripts\hydrate-whisper-runtime-from-release.ps1 -SkipTag v0.7.4

.EXAMPLE
  .\scripts\hydrate-whisper-runtime-from-release.ps1 -LocalInstallerPath D:\tmp\TrsprFlw.v0.7.4.cuda-complete.exe
#>
param(
  [string]$Repo = "Trissilein/Trispr_Flow",
  [string]$InstallerVariant = "cuda-complete",
  [string]$SkipTag = "",
  [string]$InstallRoot = "",
  [string]$LocalInstallerPath = "",
  [string]$SeedTag = "",
  [string]$SeedAssetUrl = "",
  [switch]$CopyOptionalPayloads
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..")).Path
$BinRoot = Join-Path $RepoRoot "src-tauri\bin"

function Write-Section([string]$Text) {
  Write-Host "`n== $Text ==" -ForegroundColor Cyan
}

function Get-GitHubHeaders {
  $headers = @{
    "User-Agent" = "Trispr-Flow-Release-Hydration"
    "Accept" = "application/vnd.github+json"
    "X-GitHub-Api-Version" = "2022-11-28"
  }

  $token = $env:GITHUB_TOKEN
  if ([string]::IsNullOrWhiteSpace($token)) {
    $token = $env:GH_TOKEN
  }
  if (-not [string]::IsNullOrWhiteSpace($token)) {
    $headers["Authorization"] = "Bearer $token"
  }

  return $headers
}

function Get-InstallerAssetRegex([string]$Variant) {
  $escapedVariant = [regex]::Escape($Variant)
  return '^TrsprFlw\.v\d+\.\d+\.\d+\.' + $escapedVariant + '-\d{2}\.\d{2}\.-\d{2}\.\d{2}(?:-\d{2})?\.exe$'
}

function Get-ReleaseAssetCandidate {
  param(
    [string]$RepoName,
    [string]$Variant,
    [string]$ExcludedTag
  )

  $headers = Get-GitHubHeaders
  $uri = "https://api.github.com/repos/$RepoName/releases?per_page=20"
  $releases = Invoke-RestMethod -Headers $headers -Uri $uri -Method Get
  $assetRegex = Get-InstallerAssetRegex -Variant $Variant

  foreach ($release in $releases) {
    if ($release.draft -or $release.prerelease) {
      continue
    }
    if (-not [string]::IsNullOrWhiteSpace($ExcludedTag) -and $release.tag_name -eq $ExcludedTag) {
      continue
    }

    foreach ($asset in $release.assets) {
      if ($asset.name -match $assetRegex) {
        return [PSCustomObject]@{
          TagName = $release.tag_name
          AssetName = $asset.name
          DownloadUrl = $asset.browser_download_url
        }
      }
    }
  }

  throw "No published release asset matching variant '$Variant' was found in repo '$RepoName'."
}

function Copy-DirIfExists([string]$SourceDir, [string]$TargetDir) {
  if (-not (Test-Path -LiteralPath $SourceDir -PathType Container)) {
    return $false
  }
  New-Item -ItemType Directory -Force -Path $TargetDir | Out-Null
  Copy-Item -Path (Join-Path $SourceDir "*") -Destination $TargetDir -Recurse -Force
  return $true
}

function Copy-FileIfExists([string]$SourceFile, [string]$TargetFile) {
  if (-not (Test-Path -LiteralPath $SourceFile -PathType Leaf)) {
    return $false
  }
  $targetDir = Split-Path -Parent $TargetFile
  if ($targetDir) {
    New-Item -ItemType Directory -Force -Path $targetDir | Out-Null
  }
  Copy-Item -LiteralPath $SourceFile -Destination $TargetFile -Force
  return $true
}

function Get-InstalledBinRoot([string]$BaseDir) {
  $candidates = @(
    (Join-Path $BaseDir "bin"),
    (Join-Path $BaseDir "resources\bin")
  )

  foreach ($candidate in $candidates) {
    if (Test-Path -LiteralPath $candidate -PathType Container) {
      return $candidate
    }
  }

  return $null
}

function Test-InstalledRuntimeReady([string]$BaseDir) {
  $binRoot = Get-InstalledBinRoot -BaseDir $BaseDir
  if ([string]::IsNullOrWhiteSpace($binRoot)) {
    return $null
  }

  $required = @(
    (Join-Path $binRoot "cuda\whisper-cli.exe"),
    (Join-Path $binRoot "cuda\cublasLt64_13.dll"),
    (Join-Path $binRoot "vulkan\whisper-cli.exe"),
    (Join-Path $binRoot "quantize.exe")
  )

  foreach ($path in $required) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
      return $null
    }
  }

  return $binRoot
}

function Get-SeedAssetUrl {
  param(
    [string]$RepoName,
    [string]$Tag
  )

  $headers = Get-GitHubHeaders
  $uri = "https://api.github.com/repos/$RepoName/releases/tags/$Tag"
  $release = Invoke-RestMethod -Headers $headers -Uri $uri -Method Get

  # Prefer an explicit runtime-seed*.zip asset; fall back to the first .zip.
  $seedAsset = $release.assets | Where-Object { $_.name -match '(?i)^runtime-seed.*\.zip$' } | Select-Object -First 1
  if (-not $seedAsset) {
    $seedAsset = $release.assets | Where-Object { $_.name -match '(?i)\.zip$' } | Select-Object -First 1
  }
  if (-not $seedAsset) {
    throw "Runtime-seed release '$Tag' has no .zip asset to hydrate from."
  }

  return [PSCustomObject]@{
    AssetName = $seedAsset.name
    DownloadUrl = $seedAsset.browser_download_url
  }
}

# Downloads a runtime-seed ZIP (containing a bin/ tree with cuda/, vulkan/ and
# quantize.exe) and returns the resolved bin root, or $null if it lacks the
# expected runtime files. Used to bootstrap the FIRST release of a new runtime
# version, when no published installer carries the matching DLLs yet.
function Resolve-RuntimeFromSeed {
  param(
    [string]$RepoName,
    [string]$Tag,
    [string]$AssetUrl,
    [string]$WorkRoot
  )

  $assetName = "runtime-seed.zip"
  $downloadUrl = $AssetUrl
  if ([string]::IsNullOrWhiteSpace($downloadUrl)) {
    if ([string]::IsNullOrWhiteSpace($Tag)) {
      return $null
    }
    $seed = Get-SeedAssetUrl -RepoName $RepoName -Tag $Tag
    $assetName = $seed.AssetName
    $downloadUrl = $seed.DownloadUrl
    Write-Host "Seed release tag : $Tag"
  }

  Write-Host "Seed asset       : $assetName"
  Write-Host "Seed url         : $downloadUrl"

  $seedZip = Join-Path $WorkRoot $assetName
  $extractRoot = Join-Path $WorkRoot "seed-extract"

  New-Item -ItemType Directory -Force -Path $WorkRoot | Out-Null
  if (Test-Path -LiteralPath $extractRoot) {
    Remove-Item -LiteralPath $extractRoot -Recurse -Force
  }

  Invoke-WebRequest -Headers (Get-GitHubHeaders) -Uri $downloadUrl -OutFile $seedZip -UseBasicParsing
  Expand-Archive -LiteralPath $seedZip -DestinationPath $extractRoot -Force

  # The ZIP may wrap the payload in a bin/ folder or place cuda/ + vulkan/ at the
  # root. Probe both the extract root and any single top-level directory.
  $candidates = @($extractRoot)
  $candidates += (Get-ChildItem -LiteralPath $extractRoot -Directory -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName })

  foreach ($candidate in $candidates) {
    $binRoot = Test-InstalledRuntimeReady -BaseDir $candidate
    if (-not [string]::IsNullOrWhiteSpace($binRoot)) {
      return $binRoot
    }
    # Allow a ZIP whose root already IS the bin folder (cuda/ + vulkan/ direct).
    if ((Test-Path -LiteralPath (Join-Path $candidate "cuda\cublasLt64_13.dll") -PathType Leaf) -and
        (Test-Path -LiteralPath (Join-Path $candidate "vulkan\whisper-cli.exe") -PathType Leaf)) {
      return $candidate
    }
  }

  return $null
}

Write-Section "Whisper runtime hydration from published installer"
Write-Host "Repo root : $RepoRoot"
Write-Host "Repo      : $Repo"
Write-Host "Variant   : $InstallerVariant"

$tempRoot = if (-not [string]::IsNullOrWhiteSpace($env:RUNNER_TEMP)) {
  Join-Path $env:RUNNER_TEMP "trispr-runtime-hydration"
} else {
  Join-Path $env:TEMP "trispr-runtime-hydration"
}

if ([string]::IsNullOrWhiteSpace($InstallRoot)) {
  $InstallRoot = Join-Path $tempRoot "install"
}

# Seed source (explicit override OR env fallback). A runtime seed bootstraps the
# FIRST release of a new runtime version, when no published installer carries the
# matching DLLs yet (e.g. the CUDA 12 -> CUDA 13 migration).
$seedTagEffective = $SeedTag
if ([string]::IsNullOrWhiteSpace($seedTagEffective)) {
  $seedTagEffective = $env:RUNTIME_SEED_TAG
}
$explicitSeed = (-not [string]::IsNullOrWhiteSpace($SeedAssetUrl)) -or (-not [string]::IsNullOrWhiteSpace($SeedTag))

$installedBinRoot = $null

if ($explicitSeed) {
  Write-Section "Hydrating from runtime seed (explicit)"
  $installedBinRoot = Resolve-RuntimeFromSeed -RepoName $Repo -Tag $SeedTag -AssetUrl $SeedAssetUrl -WorkRoot $tempRoot
  if ([string]::IsNullOrWhiteSpace($installedBinRoot)) {
    throw "Runtime seed (tag='$SeedTag' url='$SeedAssetUrl') did not contain the expected CUDA 13 + Vulkan runtime files."
  }
} else {
  $installerPath = $LocalInstallerPath
  if ([string]::IsNullOrWhiteSpace($installerPath)) {
    $candidate = Get-ReleaseAssetCandidate -RepoName $Repo -Variant $InstallerVariant -ExcludedTag $SkipTag
    $installerPath = Join-Path $tempRoot $candidate.AssetName

    Write-Section "Downloading installer asset"
    Write-Host "Release tag : $($candidate.TagName)"
    Write-Host "Asset       : $($candidate.AssetName)"
    Write-Host "Target file : $installerPath"

    New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null
    Invoke-WebRequest -Headers (Get-GitHubHeaders) -Uri $candidate.DownloadUrl -OutFile $installerPath -UseBasicParsing
  } else {
    Write-Section "Using local installer asset"
    Write-Host "Installer: $installerPath"
  }

  if (-not (Test-Path -LiteralPath $installerPath -PathType Leaf)) {
    throw "Installer asset not found at '$installerPath'."
  }

  if (Test-Path -LiteralPath $InstallRoot) {
    Remove-Item -LiteralPath $InstallRoot -Recurse -Force
  }

  Write-Section "Running silent install"
  Write-Host "Install dir: $InstallRoot"
  $process = Start-Process -FilePath $installerPath -ArgumentList "/S", ("/D=" + $InstallRoot) -PassThru

  $pollSeconds = 2
  $timeoutSeconds = 600
  $installerExited = $false
  for ($elapsed = 0; $elapsed -lt $timeoutSeconds; $elapsed += $pollSeconds) {
    Start-Sleep -Seconds $pollSeconds
    $process.Refresh()
    $installedBinRoot = Test-InstalledRuntimeReady -BaseDir $InstallRoot
    if (-not [string]::IsNullOrWhiteSpace($installedBinRoot)) {
      break
    }
    if ($process.HasExited) {
      $installerExited = $true
      break
    }
  }

  if (-not [string]::IsNullOrWhiteSpace($installedBinRoot)) {
    if (-not $process.HasExited) {
      Wait-Process -Id $process.Id -Timeout 10 -ErrorAction SilentlyContinue
      $process.Refresh()
      if (-not $process.HasExited) {
        Write-Warning "Installer process is still running after payload hydration; terminating it because runtime files are already present."
        Stop-Process -Id $process.Id -Force
      }
    } elseif ($process.ExitCode -ne 0) {
      Write-Warning "Silent installer exited with code $($process.ExitCode), but the runtime payload was already materialized."
    }
  } else {
    if (-not $process.HasExited) {
      Stop-Process -Id $process.Id -Force
    }

    # Distinguish a genuine timeout from the (far more common) case where the
    # installer finished cleanly but bundled an older runtime than this build
    # requires -- the previous "within 600 seconds" message was misleading.
    $diag = if ($installerExited) {
      "Installer exited after ${elapsed}s but did not contain the expected runtime files (e.g. cuda\cublasLt64_13.dll). The latest published installer likely bundles an OLDER runtime than this build requires (e.g. CUDA 12 -> CUDA 13 migration)."
    } else {
      "Installer did not materialize the expected runtime files under '$InstallRoot' within ${timeoutSeconds}s."
    }

    if (-not [string]::IsNullOrWhiteSpace($seedTagEffective)) {
      Write-Warning $diag
      Write-Section "Falling back to runtime seed '$seedTagEffective'"
      $installedBinRoot = Resolve-RuntimeFromSeed -RepoName $Repo -Tag $seedTagEffective -AssetUrl "" -WorkRoot $tempRoot
      if ([string]::IsNullOrWhiteSpace($installedBinRoot)) {
        throw "$diag`nRuntime-seed fallback '$seedTagEffective' also lacked the expected runtime files."
      }
    } else {
      throw "$diag`nNo runtime seed configured (set -SeedTag or `$env:RUNTIME_SEED_TAG) to bootstrap a new runtime version."
    }
  }
}

Write-Section "Copying runtime payloads into repo"
$copiedCuda = Copy-DirIfExists (Join-Path $installedBinRoot "cuda") (Join-Path $BinRoot "cuda")
$copiedVulkan = Copy-DirIfExists (Join-Path $installedBinRoot "vulkan") (Join-Path $BinRoot "vulkan")
$copiedQuantize = Copy-FileIfExists (Join-Path $installedBinRoot "quantize.exe") (Join-Path $BinRoot "quantize.exe")

if ($CopyOptionalPayloads) {
  [void](Copy-DirIfExists (Join-Path $installedBinRoot "ffmpeg") (Join-Path $BinRoot "ffmpeg"))
  [void](Copy-DirIfExists (Join-Path $installedBinRoot "piper") (Join-Path $BinRoot "piper"))
}

if (-not $copiedCuda -or -not $copiedVulkan) {
  throw "Failed to hydrate both cuda and vulkan runtime folders from '$installedBinRoot'."
}

Write-Section "Summary"
Write-Host "Hydrated cuda   : $copiedCuda"
Write-Host "Hydrated vulkan : $copiedVulkan"
Write-Host "Hydrated quantize.exe: $copiedQuantize"
Write-Host "Target bin root : $BinRoot"
Write-Host ""
Write-Host "Whisper runtime hydration completed successfully." -ForegroundColor Green
