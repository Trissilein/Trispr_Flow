#Requires -Version 5.1
<#
.SYNOPSIS
  One-shot Windows bootstrap after cloning Trispr Flow.

.DESCRIPTION
  - Installs npm dependencies (unless skipped).
  - Tries to hydrate missing Whisper runtime folders from an installed Trispr Flow app.
  - Tries to hydrate bundled FFmpeg from an installed Trispr Flow app.
  - Reports runtime readiness for transcription and model quantization.

.EXAMPLE
  .\FIRST_RUN.bat
  .\FIRST_RUN.bat -SkipNpmInstall
#>
param(
  [switch]$SkipNpmInstall,
  [switch]$SkipRuntimeHydration,
  [switch]$RequireWhisperRuntime
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..")).Path
$BinRoot = Join-Path $RepoRoot "src-tauri\bin"

function Write-Section($text) {
  Write-Host "`n== $text ==" -ForegroundColor Cyan
}

function Write-Info($text) {
  Write-Host "  $text"
}

function Write-Warn($text) {
  Write-Host "  WARNING: $text" -ForegroundColor Yellow
}

function Test-File([string]$Path) {
  return (Test-Path -LiteralPath $Path -PathType Leaf)
}

function Test-Dir([string]$Path) {
  return (Test-Path -LiteralPath $Path -PathType Container)
}

function Get-RuntimeStatus {
  $cudaCli = Join-Path $BinRoot "cuda\whisper-cli.exe"
  $cudaServer = Join-Path $BinRoot "cuda\whisper-server.exe"
  $cudaCublas = Join-Path $BinRoot "cuda\cublas64_13.dll"
  $cudaCublasLt = Join-Path $BinRoot "cuda\cublasLt64_13.dll"
  $cudaCudart = Join-Path $BinRoot "cuda\cudart64_13.dll"
  $vulkanCli = Join-Path $BinRoot "vulkan\whisper-cli.exe"
  $vulkanServer = Join-Path $BinRoot "vulkan\whisper-server.exe"
  $quantize = Join-Path $BinRoot "quantize.exe"
  $ffmpeg = Join-Path $BinRoot "ffmpeg\ffmpeg.exe"
  $ffmpegOnPath = [bool](Get-Command ffmpeg -ErrorAction SilentlyContinue)

  $envCliPath = ""
  $envCliReady = $false
  if ($env:TRISPR_WHISPER_CLI -and -not [string]::IsNullOrWhiteSpace($env:TRISPR_WHISPER_CLI)) {
    $envCliPath = $env:TRISPR_WHISPER_CLI.Trim()
    $envCliReady = Test-File $envCliPath
  }

  $status = [ordered]@{
    cuda_cli = Test-File $cudaCli
    cuda_server = Test-File $cudaServer
    cuda_cublas = Test-File $cudaCublas
    cuda_cublaslt = Test-File $cudaCublasLt
    cuda_cudart = Test-File $cudaCudart
    vulkan_cli = Test-File $vulkanCli
    vulkan_server = Test-File $vulkanServer
    quantize = Test-File $quantize
    ffmpeg_local = Test-File $ffmpeg
    ffmpeg_on_path = $ffmpegOnPath
    env_cli_ready = $envCliReady
    env_cli_path = $envCliPath
  }

  $status["transcription_ready"] = ($status.cuda_cli -or $status.vulkan_cli -or $status.env_cli_ready)
  $status["recommended_runtime_complete"] = (
    $status.cuda_cli -and
    $status.cuda_server -and
    $status.cuda_cublas -and
    $status.cuda_cublaslt -and
    $status.cuda_cudart -and
    $status.vulkan_cli -and
    $status.vulkan_server
  )
  $status["cuda_runtime_complete"] = (
    $status.cuda_cli -and
    $status.cuda_cublas -and
    $status.cuda_cublaslt -and
    $status.cuda_cudart
  )
  $status["ffmpeg_ready"] = ($status.ffmpeg_local -or $status.ffmpeg_on_path)

  return [PSCustomObject]$status
}

function Copy-DirIfExists([string]$SourceDir, [string]$TargetDir) {
  if (-not (Test-Dir $SourceDir)) {
    return $false
  }
  New-Item -ItemType Directory -Force -Path $TargetDir | Out-Null
  Copy-Item -Path (Join-Path $SourceDir "*") -Destination $TargetDir -Recurse -Force
  return $true
}

function Copy-FileIfExists([string]$SourceFile, [string]$TargetFile) {
  if (-not (Test-File $SourceFile)) {
    return $false
  }
  $targetDir = Split-Path -Parent $TargetFile
  if ($targetDir) {
    New-Item -ItemType Directory -Force -Path $targetDir | Out-Null
  }
  Copy-Item -Path $SourceFile -Destination $TargetFile -Force
  return $true
}

function Get-ResourceBinCandidates {
  $candidates = New-Object System.Collections.Generic.List[string]

  if ($env:LOCALAPPDATA) {
    $candidates.Add((Join-Path $env:LOCALAPPDATA "Programs\Trispr Flow\resources\bin"))
    $candidates.Add((Join-Path $env:LOCALAPPDATA "Programs\Trispr_Flow\resources\bin"))
    $candidates.Add((Join-Path $env:LOCALAPPDATA "Programs\trispr-flow\resources\bin"))
    $candidates.Add((Join-Path $env:LOCALAPPDATA "Programs\com.trispr.flow\resources\bin"))
    $candidates.Add((Join-Path $env:LOCALAPPDATA "Trispr Flow\resources\bin"))
    $candidates.Add((Join-Path $env:LOCALAPPDATA "com.trispr.flow\resources\bin"))

    $programsRoot = Join-Path $env:LOCALAPPDATA "Programs"
    if (Test-Dir $programsRoot) {
      Get-ChildItem -Path $programsRoot -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match "(?i)trispr" } |
        ForEach-Object {
          $candidates.Add((Join-Path $_.FullName "resources\bin"))
        }
    }
  }
  if ($env:ProgramFiles) {
    $candidates.Add((Join-Path $env:ProgramFiles "Trispr Flow\resources\bin"))
    $candidates.Add((Join-Path $env:ProgramFiles "Trispr_Flow\resources\bin"))
  }
  if (${env:ProgramFiles(x86)}) {
    $candidates.Add((Join-Path ${env:ProgramFiles(x86)} "Trispr Flow\resources\bin"))
    $candidates.Add((Join-Path ${env:ProgramFiles(x86)} "Trispr_Flow\resources\bin"))
  }

  return ($candidates | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -Unique)
}

function Hydrate-RuntimeFromInstalledApp {
  Write-Section "Runtime Hydration"
  $candidates = Get-ResourceBinCandidates
  if (-not $candidates -or $candidates.Count -eq 0) {
    Write-Warn "No install candidate paths could be constructed."
    return $false
  }

  $copiedAnything = $false
  foreach ($candidateBin in $candidates) {
    Write-Info "Checking candidate: $candidateBin"
    if (-not (Test-Dir $candidateBin)) {
      continue
    }

    Write-Info "Found installed runtime candidate: $candidateBin"
    $copiedCuda = Copy-DirIfExists (Join-Path $candidateBin "cuda") (Join-Path $BinRoot "cuda")
    $copiedVulkan = Copy-DirIfExists (Join-Path $candidateBin "vulkan") (Join-Path $BinRoot "vulkan")
    $copiedQuantize = Copy-FileIfExists (Join-Path $candidateBin "quantize.exe") (Join-Path $BinRoot "quantize.exe")
    $copiedFfmpeg = Copy-FileIfExists (Join-Path $candidateBin "ffmpeg\ffmpeg.exe") (Join-Path $BinRoot "ffmpeg\ffmpeg.exe")

    if ($copiedCuda -or $copiedVulkan -or $copiedQuantize -or $copiedFfmpeg) {
      $copiedAnything = $true
      Write-Info "Copied runtime files from installed app."
    } else {
      Write-Warn "Candidate had no expected files (cuda/vulkan/quantize/ffmpeg)."
    }
  }

  if (-not $copiedAnything) {
    Write-Warn "No runtime files were copied from local installations."
  }
  return $copiedAnything
}

Set-Location $RepoRoot
Write-Section "Trispr Flow First Run"
Write-Info "Repo root: $RepoRoot"

if (-not $SkipNpmInstall) {
  Write-Section "npm Install"
  if (-not (Get-Command npm -ErrorAction SilentlyContinue)) {
    throw "npm not found on PATH. Install Node.js first."
  }
  & npm install
  if ($LASTEXITCODE -ne 0) {
    throw "npm install failed with exit code $LASTEXITCODE."
  }
} else {
  Write-Section "npm Install"
  Write-Info "Skipped by parameter."
}

if (-not $SkipRuntimeHydration) {
  [void](Hydrate-RuntimeFromInstalledApp)
} else {
  Write-Section "Runtime Hydration"
  Write-Info "Skipped by parameter."
}

$status = Get-RuntimeStatus
Write-Section "Runtime Status"
Write-Info ("Transcription runtime ready: {0}" -f $status.transcription_ready)
Write-Info ("Recommended CUDA+Vulkan runtime complete: {0}" -f $status.recommended_runtime_complete)
Write-Info ("CUDA runtime complete (including cublasLt64_13.dll): {0}" -f $status.cuda_runtime_complete)
Write-Info ("Quantize binary ready: {0}" -f $status.quantize)
Write-Info ("FFmpeg ready (local or PATH): {0}" -f $status.ffmpeg_ready)
if ($status.env_cli_ready) {
  Write-Info ("External TRISPR_WHISPER_CLI detected: {0}" -f $status.env_cli_path)
}

if (-not $status.transcription_ready) {
  Write-Warn "No whisper-cli runtime detected."
  Write-Host ""
  Write-Host "Action required:"
  Write-Host "  1) Install Trispr Flow once and rerun FIRST_RUN.bat (runtime files are copied from the installed app), or"
  Write-Host "  2) Build whisper.cpp and set TRISPR_WHISPER_CLI / TRISPR_WHISPER_MODEL_DIR (see docs/DEVELOPMENT.md)."
  if ($RequireWhisperRuntime) {
    exit 2
  }
  Write-Warn "Continuing without local Whisper runtime (non-fatal)."
}

if (-not $status.quantize) {
  Write-Warn "quantize.exe missing. 'Optimize' in model manager will be unavailable until bundled."
}

if ($status.cuda_cli -and -not $status.cuda_runtime_complete) {
  Write-Warn "CUDA runtime is incomplete (missing cublas/cublasLt/cudart DLLs). CUDA backend may fail and should fall back to Vulkan."
}

if (-not $status.ffmpeg_ready) {
  Write-Warn "FFmpeg missing. OPUS save/merge requires ffmpeg with libopus support."
  Write-Host "  Fix: powershell -NoProfile -ExecutionPolicy Bypass -File scripts\setup-ffmpeg.ps1"
}

Write-Host ""
Write-Host "Next steps:"
Write-Host "  npm run tauri dev"
Write-Host "  npm run test:smoke"
exit 0
