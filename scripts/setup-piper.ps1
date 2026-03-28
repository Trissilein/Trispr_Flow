#Requires -Version 5.1
<#
.SYNOPSIS
    Downloads Piper TTS binary and default voice models into src-tauri/bin/piper/.

.DESCRIPTION
    Fetches the piper_windows_amd64.zip release from GitHub and extracts:
      - piper.exe
      - onnxruntime.dll, onnxruntime_providers_shared.dll
      - espeak-ng-data/ directory (phoneme data for all languages)

    Also downloads Piper voice models into src-tauri/bin/piper/voices/.
    Default behavior (used by installer builds) bundles only:
      - de_DE-thorsten-medium  (German, ~53 MB)

    Optional curated catalog (download with -IncludeCuratedVoices):
      - de_DE-thorsten-medium
      - de_DE-mls-medium
      - en_GB-alan-medium
      - en_GB-alba-medium
      - en_GB-cori-high

    These files are referenced in tauri.conf.json / tauri.conf.vulkan.json as bundle resources
    and end up in <install_dir>/resources/bin/piper/ after the NSIS installer runs.

.PARAMETER PiperVersion
    Piper release tag to download (default: 2023.11.14-2).

.PARAMETER SkipVoices
    Skip voice model downloads (e.g. when only refreshing the binary).

.PARAMETER IncludeCuratedVoices
    Download full curated voice catalog instead of only bundled base voice.

.EXAMPLE
    .\scripts\setup-piper.ps1
    .\scripts\setup-piper.ps1 -PiperVersion 2023.11.14-2 -SkipVoices
#>
param(
    [string]$PiperVersion = "2023.11.14-2",
    [switch]$SkipVoices,
    [switch]$IncludeCuratedVoices
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
function Test-NonEmptyFile($path) {
    if (-not (Test-Path $path)) { return $false }
    return (Get-Item $path).Length -gt 0
}

# ---------------------------------------------------------------------------
Write-Section "Piper TTS setup - $PiperVersion"
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
    "onnxruntime_providers_shared.dll",
    "espeak-ng.dll",
    "piper_phonemize.dll",
    "libtashkeel_model.ort"
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

    # Curated installer catalog (>= medium, no US voices)
    $CuratedVoices = @(
        @{ Key = "de_DE-thorsten-medium"; HfPath = "de/de_DE/thorsten/medium"; ApproxMb = 53 },
        @{ Key = "de_DE-mls-medium";      HfPath = "de/de_DE/mls/medium";      ApproxMb = 54 },
        @{ Key = "en_GB-alan-medium";     HfPath = "en/en_GB/alan/medium";     ApproxMb = 56 },
        @{ Key = "en_GB-alba-medium";     HfPath = "en/en_GB/alba/medium";     ApproxMb = 57 },
        @{ Key = "en_GB-cori-high";       HfPath = "en/en_GB/cori/high";       ApproxMb = 81 }
    )

    # cuda-complete offline payload keeps only base voice bundled
    $BundledVoiceKeys = @("de_DE-thorsten-medium")
    if ($IncludeCuratedVoices) {
        $VoicesToDownload = $CuratedVoices
    } else {
        $VoicesToDownload = $CuratedVoices | Where-Object { $BundledVoiceKeys -contains $_.Key }
    }

    foreach ($v in $VoicesToDownload) {
        $OnnxUrl     = "$HfBase/$($v.HfPath)/$($v.Key).onnx?download=true"
        $OnnxCfgUrl  = "$HfBase/$($v.HfPath)/$($v.Key).onnx.json?download=true"
        $OnnxDst     = Join-Path $VoicesDir "$($v.Key).onnx"
        $OnnxCfgDst  = Join-Path $VoicesDir "$($v.Key).onnx.json"

        if (Test-NonEmptyFile $OnnxDst) {
            Write-Host "  Skipping $($v.Key).onnx (already exists)"
        } else {
            Write-Host "  Fetching: $($v.Key).onnx (~$($v.ApproxMb) MB)..."
            Invoke-WebRequest -Uri $OnnxUrl -OutFile $OnnxDst -UseBasicParsing
            Write-Host "  OK: $($v.Key).onnx"
        }

        if (Test-NonEmptyFile $OnnxCfgDst) {
            Write-Host "  Skipping $($v.Key).onnx.json (already exists)"
        } else {
            Write-Host "  Fetching: $($v.Key).onnx.json..."
            Invoke-WebRequest -Uri $OnnxCfgUrl -OutFile $OnnxCfgDst -UseBasicParsing
            Write-Host "  OK: $($v.Key).onnx.json"
        }
    }
}

# ---------------------------------------------------------------------------
Write-Section "Summary"
Write-Host "Binary : $(Join-Path $PiperDir 'piper.exe')"
Write-Host "Voices : $VoicesDir"
Get-ChildItem -Path $VoicesDir -Filter "*.onnx" -ErrorAction SilentlyContinue |
    ForEach-Object { Write-Host ("  - {0}" -f $_.Name) }

Write-Host ""
Write-Host 'Next: npm run tauri build (or build-both-installers.bat)' -ForegroundColor Green
