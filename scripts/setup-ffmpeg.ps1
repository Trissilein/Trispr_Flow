#Requires -Version 5.1
<#
.SYNOPSIS
    Downloads and verifies FFmpeg (Windows) for OPUS encoding.

.DESCRIPTION
    Fetches a pinned FFmpeg build and installs ffmpeg.exe to:
      src-tauri/bin/ffmpeg/ffmpeg.exe

    Safety checks:
      - SHA256 checksum must match the pinned value.
      - FFmpeg must expose encoder=libopus.

    Intended usage:
      - Called automatically by installer build scripts.
      - Can be run manually by developers.

.EXAMPLE
    .\scripts\setup-ffmpeg.ps1
    .\scripts\setup-ffmpeg.ps1 -Force
#>
param(
    [switch]$Force
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..")).Path
$FfmpegDir = Join-Path $RepoRoot "src-tauri\bin\ffmpeg"
$FfmpegExe = Join-Path $FfmpegDir "ffmpeg.exe"
$TempDir = Join-Path $env:TEMP "trispr-ffmpeg-setup"

$FfmpegVersion = "7.1.1"
$AssetName = "ffmpeg-7.1.1-essentials_build.zip"
$AssetUrl = "https://github.com/GyanD/codexffmpeg/releases/download/$FfmpegVersion/$AssetName"
$ExpectedExeSha256 = "b90225987bdd042cca09a1efb5e34e9848f2d1dbf5fbcd388753a44145522997"

function Write-Section([string]$Text) {
    Write-Host "`n== $Text ==" -ForegroundColor Cyan
}

function Get-Sha256Lower([string]$Path) {
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Test-LibOpusEncoder([string]$ExePath) {
    if (-not (Test-Path -LiteralPath $ExePath -PathType Leaf)) {
        return $false
    }

    # The pinned SHA256 identifies a known libopus-enabled build.
    if ((Get-Sha256Lower $ExePath) -eq $ExpectedExeSha256) {
        return $true
    }

    # cmd.exe pipe handling is more predictable than PowerShell native-command piping here.
    $cmd = "`"$ExePath`" -version | findstr /I enable-libopus"
    $probeOutput = & cmd.exe /d /c $cmd
    $exitCode = 0
    if (Get-Variable -Name LASTEXITCODE -Scope Global -ErrorAction SilentlyContinue) {
        $exitCode = [int]$global:LASTEXITCODE
    }
    if ($exitCode -ne 0) {
        return $false
    }

    $probeOutputText = ($probeOutput | Out-String)
    return ($probeOutputText -match "(?i)--enable-libopus")
}

function Validate-ExistingBinary {
    if (-not (Test-Path -LiteralPath $FfmpegExe -PathType Leaf)) {
        return $false
    }

    $hash = Get-Sha256Lower $FfmpegExe
    if ($hash -ne $ExpectedExeSha256) {
        Write-Warning "Existing ffmpeg.exe hash mismatch. expected=$ExpectedExeSha256 actual=$hash"
        return $false
    }

    if (-not (Test-LibOpusEncoder $FfmpegExe)) {
        Write-Warning "Existing ffmpeg.exe does not support encoder=libopus."
        return $false
    }

    Write-Host "  OK: Existing ffmpeg.exe matches pinned hash and supports libopus."
    return $true
}

Write-Section "FFmpeg setup (OPUS-capable)"
Write-Host "Repo root: $RepoRoot"
Write-Host "Target   : $FfmpegExe"

if (-not $Force) {
    if (Validate-ExistingBinary) {
        Write-Host ""
        Write-Host "FFmpeg setup skipped (already valid)." -ForegroundColor Green
        exit 0
    }
} else {
    Write-Host "Force mode enabled: refreshing FFmpeg binary."
}

New-Item -ItemType Directory -Force -Path $FfmpegDir | Out-Null
New-Item -ItemType Directory -Force -Path $TempDir | Out-Null

$ZipPath = Join-Path $TempDir $AssetName
$ExtractDir = Join-Path $TempDir "ffmpeg_extracted"

if (Test-Path -LiteralPath $ExtractDir) {
    Remove-Item -LiteralPath $ExtractDir -Recurse -Force
}

Write-Section "Downloading"
Write-Host "Source: $AssetUrl"
Invoke-WebRequest -Uri $AssetUrl -OutFile $ZipPath -UseBasicParsing

Write-Section "Extracting"
Expand-Archive -Path $ZipPath -DestinationPath $ExtractDir -Force

$ExtractedExe = Get-ChildItem -Path $ExtractDir -Filter "ffmpeg.exe" -Recurse -File |
    Where-Object { $_.FullName -match "[/\\]bin[/\\]ffmpeg\.exe$" } |
    Select-Object -First 1

if (-not $ExtractedExe) {
    throw "Could not locate ffmpeg.exe inside extracted archive."
}

Copy-Item -LiteralPath $ExtractedExe.FullName -Destination $FfmpegExe -Force

Write-Section "Verifying checksum"
$installedHash = Get-Sha256Lower $FfmpegExe
if ($installedHash -ne $ExpectedExeSha256) {
    throw "Checksum validation failed for ffmpeg.exe. expected=$ExpectedExeSha256 actual=$installedHash"
}
Write-Host "  OK: SHA256 matches pinned build."

Write-Section "Verifying libopus encoder"
if (-not (Test-LibOpusEncoder $FfmpegExe)) {
    throw "FFmpeg verification failed: encoder=libopus is unavailable."
}
Write-Host "  OK: encoder=libopus is available."

Write-Section "Summary"
Write-Host "FFmpeg  : $FfmpegExe"
Write-Host "SHA256  : $installedHash"
Write-Host "Version : $(& $FfmpegExe -version | Select-Object -First 1)"
Write-Host ""
Write-Host "FFmpeg setup completed successfully." -ForegroundColor Green
