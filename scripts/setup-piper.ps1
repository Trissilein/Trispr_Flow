#Requires -Version 5.1
<#
.SYNOPSIS
    Downloads Piper TTS binary and default voice models into src-tauri/bin/piper/.

.DESCRIPTION
    Fetches the piper_windows_amd64.zip release from GitHub and extracts:
      - piper.exe
      - onnxruntime.dll, onnxruntime_providers_shared.dll
      - espeak-ng-data/ directory (phoneme data for all languages)

    Also downloads the following default voice models into src-tauri/bin/piper/voices/:
      - de_DE-thorsten-medium  (German, ~53 MB)
      - en_US-amy-medium       (English, ~63 MB)

    These files are referenced in tauri.conf.json / tauri.conf.vulkan.json as bundle resources
    and end up in <install_dir>/resources/bin/piper/ after the NSIS installer runs.

.PARAMETER PiperVersion
    Piper release tag to download (default: v1.2.0).

.PARAMETER SkipVoices
    Skip voice model downloads (e.g. when only refreshing the binary).

.EXAMPLE
    .\scripts\setup-piper.ps1
    .\scripts\setup-piper.ps1 -PiperVersion v1.2.0 -SkipVoices
#>
param(
    [string]$PiperVersion = "v1.2.0",
    [switch]$SkipVoices
)

$ErrorActionPreference = "Stop"
$ScriptDir  = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot   = (Resolve-Path (Join-Path $ScriptDir "..")).Path
$PiperDir   = Join-Path $RepoRoot "src-tauri\bin\piper"
$VoicesDir  = Join-Path $PiperDir "voices"
$TmpDir     = Join-Path $env:TEMP "trispr-piper-setup"

function Write-Section($text) { Write-Host "`n== $text ==" -ForegroundColor Cyan }
function Confirm-File($path, $desc) {
    if (-not (Test-Path $path)) { throw "Expected $desc not found at: $path" }
    Write-Host "  OK: $desc"
}

# ---------------------------------------------------------------------------
Write-Section "Piper TTS setup — $PiperVersion"
Write-Host "Piper dir : $PiperDir"
Write-Host "Voices dir: $VoicesDir"

New-Item -ItemType Directory -Force -Path $PiperDir  | Out-Null
New-Item -ItemType Directory -Force -Path $VoicesDir | Out-Null
New-Item -ItemType Directory -Force -Path $TmpDir    | Out-Null

# ---------------------------------------------------------------------------
Write-Section "Downloading piper binary"

$ZipUrl  = "https://github.com/rhasspy/piper/releases/download/$PiperVersion/piper_windows_amd64.zip"
$ZipPath = Join-Path $TmpDir "piper_windows_amd64.zip"

Write-Host "  Fetching: $ZipUrl"
Invoke-WebRequest -Uri $ZipUrl -OutFile $ZipPath -UseBasicParsing

$ExtractDir = Join-Path $TmpDir "piper_extracted"
if (Test-Path $ExtractDir) { Remove-Item -Recurse -Force $ExtractDir }
Expand-Archive -Path $ZipPath -DestinationPath $ExtractDir

# The zip contains a top-level "piper\" folder
$PiperExtracted = Join-Path $ExtractDir "piper"
if (-not (Test-Path $PiperExtracted)) {
    # Some releases use the flat layout
    $PiperExtracted = $ExtractDir
}

# Copy required files
$FilesToCopy = @(
    "piper.exe",
    "onnxruntime.dll",
    "onnxruntime_providers_shared.dll"
)
foreach ($f in $FilesToCopy) {
    $src = Join-Path $PiperExtracted $f
    if (Test-Path $src) {
        Copy-Item -Path $src -Destination $PiperDir -Force
        Write-Host "  Copied: $f"
    } else {
        Write-Warning "  Not found in release: $f (skipping)"
    }
}

# Copy espeak-ng-data directory
$EspeakSrc = Join-Path $PiperExtracted "espeak-ng-data"
if (Test-Path $EspeakSrc) {
    $EspeakDst = Join-Path $PiperDir "espeak-ng-data"
    if (Test-Path $EspeakDst) { Remove-Item -Recurse -Force $EspeakDst }
    Copy-Item -Path $EspeakSrc -Destination $EspeakDst -Recurse -Force
    Write-Host "  Copied: espeak-ng-data/"
} else {
    Write-Warning "  espeak-ng-data not found in release (phoneme support may be limited)"
}

Confirm-File (Join-Path $PiperDir "piper.exe") "piper.exe"

# ---------------------------------------------------------------------------
if (-not $SkipVoices) {
    Write-Section "Downloading voice models"

    # Voice model base URL (Piper voice models hosted on Hugging Face)
    $HfBase = "https://huggingface.co/rhasspy/piper-voices/resolve/main"

    $Voices = @(
        @{ Lang = "de/de_DE/thorsten/medium"; Name = "de_DE-thorsten-medium" },
        @{ Lang = "en/en_US/amy/medium";      Name = "en_US-amy-medium"      }
    )

    foreach ($v in $Voices) {
        $OnnxUrl     = "$HfBase/$($v.Lang)/$($v.Name).onnx?download=true"
        $OnnxCfgUrl  = "$HfBase/$($v.Lang)/$($v.Name).onnx.json?download=true"
        $OnnxDst     = Join-Path $VoicesDir "$($v.Name).onnx"
        $OnnxCfgDst  = Join-Path $VoicesDir "$($v.Name).onnx.json"

        if (Test-Path $OnnxDst) {
            Write-Host "  Skipping $($v.Name).onnx (already exists)"
        } else {
            Write-Host "  Fetching: $($v.Name).onnx (~50-70 MB)..."
            Invoke-WebRequest -Uri $OnnxUrl -OutFile $OnnxDst -UseBasicParsing
            Write-Host "  OK: $($v.Name).onnx"
        }

        if (Test-Path $OnnxCfgDst) {
            Write-Host "  Skipping $($v.Name).onnx.json (already exists)"
        } else {
            Write-Host "  Fetching: $($v.Name).onnx.json..."
            Invoke-WebRequest -Uri $OnnxCfgUrl -OutFile $OnnxCfgDst -UseBasicParsing
            Write-Host "  OK: $($v.Name).onnx.json"
        }
    }
}

# ---------------------------------------------------------------------------
Write-Section "Summary"
Write-Host "Binary : $(Join-Path $PiperDir 'piper.exe')"
Write-Host "Voices : $VoicesDir"
Get-ChildItem -Path $VoicesDir -Filter "*.onnx" -ErrorAction SilentlyContinue |
    ForEach-Object { Write-Host "  - $($_.Name)" }

Write-Host ""
Write-Host 'Next: npm run tauri build (or build-both-installers.bat)' -ForegroundColor Green
