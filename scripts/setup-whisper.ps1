param(
  [switch]$CpuFallback,
  [string]$CudaRoot = "",
  [string]$CudaToolset = "",
  [string]$WhisperRoot = "D:\!GIT\whisper.cpp",
  [string]$CudaArch = ""
)

$ErrorActionPreference = "Stop"

function Write-Section($text) {
  Write-Host "== $text =="
}

function Get-VSInstances {
  $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
  if (-not (Test-Path $vswhere)) { return @() }
  $raw = & $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -format json 2>$null
  if (-not $raw) { return @() }
  return ($raw | ConvertFrom-Json)
}

function Import-VsDevCmd {
  param([string]$VsPath)
  if (-not $VsPath) { return }
  $vsDevCmd = Join-Path $VsPath "Common7\Tools\VsDevCmd.bat"
  if (-not (Test-Path $vsDevCmd)) { return }
  $envDump = cmd /c "`"$vsDevCmd`" -arch=amd64 -no_logo && set"
  foreach ($line in $envDump) {
    $parts = $line -split "=", 2
    if ($parts.Length -eq 2) {
      $envName = $parts[0]
      $envValue = $parts[1]
      if ($envName -and $envValue) {
        Set-Item -Path ("Env:" + $envName) -Value $envValue
      }
    }
  }
}

function Detect-CudaArch {
  $smi = Get-Command nvidia-smi -ErrorAction SilentlyContinue
  if (-not $smi) { return "" }
  $cc = & nvidia-smi --query-gpu=compute_cap --format=csv,noheader 2>$null
  if (-not $cc) { return "" }
  $cc = $cc.Trim()
  if ($cc -match "^(\d+)\.(\d+)") {
    return "$($matches[1])$($matches[2])"
  }
  return ""
}

if (!(Test-Path $WhisperRoot)) {
  throw "whisper.cpp not found at $WhisperRoot"
}

Write-Section "Trispr Flow :: whisper.cpp build"
Write-Host "Root: $WhisperRoot"

$instances = Get-VSInstances
$vs2022 = $instances | Where-Object { $_.installationVersion -like "17.*" } | Select-Object -First 1
$vs18 = $instances | Where-Object { $_.installationVersion -like "18.*" } | Select-Object -First 1

$selectedVs = $null
$generatorName = ""

if (-not $CpuFallback) {
  if ($vs2022) {
    $selectedVs = $vs2022
    $generatorName = "Visual Studio 17 2022"
  } elseif ($vs18) {
    $selectedVs = $vs18
    $generatorName = "Visual Studio 18 2026"
  } else {
    throw "No Visual Studio with C++ tools found. Install VS Build Tools."
  }
} else {
  if ($vs2022) {
    $selectedVs = $vs2022
    $generatorName = "Visual Studio 17 2022"
  } elseif ($vs18) {
    $selectedVs = $vs18
    $generatorName = "Visual Studio 18 2026"
  }
}

if ($selectedVs) {
  Import-VsDevCmd -VsPath $selectedVs.installationPath
}

$useCuda = -not $CpuFallback
if ($useCuda) {
  $vcTag = if ($generatorName -eq "Visual Studio 17 2022") { "v170" } else { "v180" }
  $cudaProps = Join-Path $selectedVs.installationPath "MSBuild\Microsoft\VC\$vcTag\BuildCustomizations\CUDA 13.0.props"
  if (-not (Test-Path $cudaProps)) {
    throw "CUDA BuildCustomizations not found at $cudaProps"
  }

  $nvcc = Get-Command nvcc -ErrorAction SilentlyContinue
  if (-not $nvcc) {
    $cudaRoot = if ([string]::IsNullOrWhiteSpace($CudaRoot)) { "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA" } else { $CudaRoot }
    if (Test-Path $cudaRoot) {
      $candidates = Get-ChildItem -Path $cudaRoot -Directory -ErrorAction SilentlyContinue | Sort-Object Name -Descending
      foreach ($dir in $candidates) {
        $candidate = Join-Path $dir.FullName "bin\nvcc.exe"
        if (Test-Path $candidate) {
          $env:Path = (Join-Path $dir.FullName "bin") + ";" + $env:Path
          $env:CUDAToolkit_ROOT = $dir.FullName
          $env:CUDACXX = $candidate
          $nvcc = Get-Command nvcc -ErrorAction SilentlyContinue
          break
        }
      }
    }
  }
  if (-not $nvcc) {
    throw "CUDA toolkit (nvcc) not found. Install CUDA Toolkit."
  }

  if ([string]::IsNullOrWhiteSpace($CudaArch)) {
    $CudaArch = Detect-CudaArch
  }
}

$cl = Get-Command cl -ErrorAction SilentlyContinue
if (-not $cl) {
  throw "MSVC build tools (cl.exe) not found. Install VS Build Tools with 'Desktop development with C++'."
}

Push-Location $WhisperRoot
try {
  $buildDirName = if ($CpuFallback) { "build-cpu" } else { "build-cuda" }
  $buildDirPath = Join-Path $WhisperRoot $buildDirName
  if (Test-Path $buildDirPath) {
    Remove-Item -Recurse -Force $buildDirPath
  }

  if ($useCuda) {
    if ([string]::IsNullOrWhiteSpace($CudaToolset)) { $CudaToolset = "13.0" }
    $generatorArgs = @("-G", $generatorName, "-A", "x64")
    $toolsetArgs = @("-T", "cuda=$CudaToolset")
    $instanceArg = @("-DCMAKE_GENERATOR_INSTANCE=$($selectedVs.installationPath)")
    $archArg = @()
    if (-not [string]::IsNullOrWhiteSpace($CudaArch)) {
      $archArg = @("-DCMAKE_CUDA_ARCHITECTURES=$CudaArch")
    }
    $cudaFlags = @("-DCMAKE_CUDA_FLAGS=--allow-unsupported-compiler")
    if ($env:CUDACXX) {
      cmake -B $buildDirName -S . -DGGML_CUDA=ON -DWHISPER_BUILD_TESTS=OFF -DCMAKE_CUDA_COMPILER=$env:CUDACXX @generatorArgs @toolsetArgs @instanceArg @archArg @cudaFlags
    } else {
      cmake -B $buildDirName -S . -DGGML_CUDA=ON -DWHISPER_BUILD_TESTS=OFF @generatorArgs @toolsetArgs @instanceArg @archArg @cudaFlags
    }
  } else {
    cmake -B $buildDirName -S . -DGGML_CUDA=OFF -DWHISPER_CUDA=OFF -DWHISPER_BUILD_TESTS=OFF
  }

  cmake --build $buildDirName --config Release --target whisper-cli
} finally {
  Pop-Location
}

$buildDir = Join-Path $WhisperRoot $buildDirName
$cliPath = Join-Path $buildDir "bin\Release\whisper-cli.exe"
if (!(Test-Path $cliPath)) {
  throw "whisper-cli.exe not found at $cliPath"
}

$envPath = "D:\GIT\Trispr_Flow\.env.local"
$envLines = @(
  "TRISPR_WHISPER_CLI=$cliPath",
  "TRISPR_WHISPER_MODEL_DIR=$WhisperRoot\models"
)

$envLines | Set-Content -Path $envPath -Encoding UTF8

Write-Host "OK: whisper-cli built"
Write-Host "OK: wrote $envPath"
Write-Host "Next: cd D:\GIT\Trispr_Flow; npm run tauri dev"
