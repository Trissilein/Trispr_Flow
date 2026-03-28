param(
  [Parameter(Mandatory = $true)]
  [string]$Tag,
  [string]$Repo = "Trissilein/Trispr_Flow",
  [string]$AssetGlob = "installers\*.exe",
  [bool]$LatestPerVariant = $true,
  [switch]$CreateReleaseIfMissing,
  [switch]$Clobber
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..\..")).Path
Set-Location $RepoRoot

$GhExe = $null
$ghCommand = Get-Command gh -ErrorAction SilentlyContinue
if ($ghCommand) {
  $GhExe = $ghCommand.Source
}
if (-not $GhExe) {
  $fallbackGh = Join-Path ${env:ProgramFiles} "GitHub CLI\gh.exe"
  if (Test-Path $fallbackGh) {
    $GhExe = $fallbackGh
  }
}
if (-not $GhExe) {
  throw "GitHub CLI (gh) is not installed or not on PATH."
}

function Invoke-Gh {
  param([string[]]$GhArgs)
  & $GhExe @GhArgs
  if ($LASTEXITCODE -ne 0) {
    throw "gh command failed: $GhExe $($GhArgs -join ' ')"
  }
}

function Parse-InstallerMetadata {
  param([Parameter(Mandatory = $true)][string]$Name)

  $match = [regex]::Match(
    $Name,
    '^TrsprFlw\.v(?<version>\d+\.\d+\.\d+)\.(?<variant>vulkan-only|cuda-lite|cuda-complete)-(?<stamp>\d{2}\.\d{2}\.-\d{2}\.\d{2}(?:-\d{2})?)\.exe$'
  )
  if (-not $match.Success) {
    return $null
  }

  [PSCustomObject]@{
    Version = $match.Groups['version'].Value
    Variant = $match.Groups['variant'].Value
    Stamp   = $match.Groups['stamp'].Value
  }
}

$candidates = Get-ChildItem -Path $AssetGlob -File -ErrorAction SilentlyContinue
if (-not $candidates -or $candidates.Count -eq 0) {
  throw "No installer assets found for pattern '$AssetGlob'."
}

if ($LatestPerVariant) {
  $tagVersion = $null
  if ($Tag -match '^v(?<v>\d+\.\d+\.\d+)$') {
    $tagVersion = $Matches['v']
  }

  $parsed = @(
    foreach ($file in $candidates) {
      $meta = Parse-InstallerMetadata -Name $file.Name
      if ($null -eq $meta) {
        continue
      }
      if ($tagVersion -and $meta.Version -ne $tagVersion) {
        continue
      }

      [PSCustomObject]@{
        File    = $file
        Version = $meta.Version
        Variant = $meta.Variant
        Stamp   = $meta.Stamp
      }
    }
  )

  if ($parsed.Count -eq 0) {
    throw 'No parseable installer artifacts matched the given tag. Disable latest selection with -LatestPerVariant:$false if needed.'
  }

  $assets = @(
    $parsed |
      Group-Object Variant |
      ForEach-Object {
        $_.Group |
          Sort-Object { $_.File.LastWriteTimeUtc } -Descending |
          Select-Object -First 1
      } |
      Sort-Object Variant |
      ForEach-Object { $_.File }
  )
} else {
  $assets = @($candidates | Sort-Object Name)
}

if ($assets.Count -eq 0) {
  throw "No installer assets selected for upload."
}

Write-Host "Uploading assets to $Repo tag $Tag" -ForegroundColor Cyan
Write-Host "Selection mode: $(if ($LatestPerVariant) { 'latest-per-variant' } else { 'all-matching-files' })"
foreach ($asset in $assets) {
  Write-Host "  - $($asset.Name)"
}

$releaseExists = $true
try {
  Invoke-Gh -GhArgs @("release", "view", $Tag, "--repo", $Repo)
} catch {
  $releaseExists = $false
}

if (-not $releaseExists) {
  if (-not $CreateReleaseIfMissing) {
    throw "Release '$Tag' does not exist. Re-run with -CreateReleaseIfMissing or create release manually."
  }
  Write-Host "Release '$Tag' not found. Creating draft release..." -ForegroundColor Yellow
  Invoke-Gh -GhArgs @("release", "create", $Tag, "--repo", $Repo, "--title", $Tag, "--notes", "Installer assets upload.")
}

$uploadArgs = @("release", "upload", $Tag, "--repo", $Repo)
if ($Clobber) {
  $uploadArgs += "--clobber"
}
$uploadArgs += $assets.FullName
Invoke-Gh -GhArgs $uploadArgs

Write-Host "Upload complete." -ForegroundColor Green
