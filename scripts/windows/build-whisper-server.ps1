# Build whisper-server.exe from whisper.cpp source
# Usage: powershell -ExecutionPolicy Bypass -File scripts\windows\build-whisper-server.ps1

param(
    [string]$BuildDir = "C:\temp\whisper.cpp-build",
    [string]$TrispiFlow = $PSScriptRoot
)

Write-Host "Building whisper-server.exe..." -ForegroundColor Cyan

# Check prerequisites
$cmake = Get-Command cmake -ErrorAction SilentlyContinue
if (-not $cmake) {
    Write-Host "ERROR: CMake not found in PATH" -ForegroundColor Red
    Write-Host "Install from: https://cmake.org/download/" -ForegroundColor Yellow
    exit 1
}

$git = Get-Command git -ErrorAction SilentlyContinue
if (-not $git) {
    Write-Host "ERROR: Git not found in PATH" -ForegroundColor Red
    exit 1
}

Write-Host "OK: CMake and Git found"

# Create build directory
New-Item -ItemType Directory -Force -Path $BuildDir | Out-Null
cd $BuildDir

# Clone whisper.cpp
if (-not (Test-Path "whisper.cpp")) {
    Write-Host "Cloning whisper.cpp..." -ForegroundColor Cyan
    & git clone https://github.com/ggerganov/whisper.cpp.git
}

cd whisper.cpp

# Create build directory
if (-not (Test-Path "build-cuda")) {
    New-Item -ItemType Directory -Path "build-cuda" | Out-Null
}
cd build-cuda

# Configure CMake
# Note: Skip GPU architecture detection by forcing specific architectures before
# CMakeLists.txt tries to auto-detect. This avoids the "native" architecture issue.
# Also set CUDA flags to allow unsupported compiler (VS 2026 with CUDA 13.0)
Write-Host "Configuring CMake with CUDA..." -ForegroundColor Cyan
$env:CUDAFLAGS = "-allow-unsupported-compiler"
# Arch list covers the GPUs we actually use: 75 = Turing (T500 notebook),
# 89 = Ada Lovelace (RTX 40xx), 120 = Blackwell (RTX 50xx). Other archs left
# out to keep build time / binary size small. If a user shows up with an
# unsupported card (e.g. RTX 30xx = sm_86), the runtime detects the
# "no kernel image" crash and surfaces a clear error suggesting Vulkan.
& cmake .. -DGGML_CUDA=ON -DGGML_CUDA_F16=ON `
    -DCMAKE_CUDA_ARCHITECTURES="75;89;120" `
    -DCMAKE_CUDA_FLAGS="-allow-unsupported-compiler" `
    -A x64

if ($LASTEXITCODE -ne 0) {
    Write-Host "CMake configuration failed!" -ForegroundColor Red
    exit 1
}

# Build
Write-Host "Building whisper-server (this takes 5-15 minutes)..." -ForegroundColor Cyan
& cmake --build . --config Release --parallel

if ($LASTEXITCODE -ne 0) {
    Write-Host "Build failed!" -ForegroundColor Red
    exit 1
}

# Copy the full CUDA runtime set. ggml-cuda.dll must match the freshly built
# EXEs, otherwise older installs can fail with "no kernel image" on sm_75 GPUs.
$cudaTargetDir = Join-Path $TrispiFlow "src-tauri\bin\cuda"
New-Item -ItemType Directory -Force -Path $cudaTargetDir | Out-Null
$runtimeFiles = @(
    "whisper-server.exe",
    "whisper-cli.exe",
    "whisper.dll",
    "ggml.dll",
    "ggml-base.dll",
    "ggml-cpu.dll",
    "ggml-cuda.dll"
)
$missingFiles = @()
foreach ($file in $runtimeFiles) {
    $srcFile = "bin\Release\$file"
    $dstFile = Join-Path $cudaTargetDir $file
    if (Test-Path $srcFile) {
        Write-Host "Copying $file..." -ForegroundColor Cyan
        Copy-Item $srcFile -Destination $dstFile -Force
        $fileSize = (Get-Item $dstFile).Length / 1MB
        Write-Host "  -> $dstFile ($([Math]::Round($fileSize, 2)) MB)" -ForegroundColor Green
    } else {
        Write-Host "ERROR: $file not found at $srcFile" -ForegroundColor Red
        $missingFiles += $file
    }
}
if ($missingFiles.Count -gt 0) {
    Write-Host "Build produced no output for: $($missingFiles -join ', ')" -ForegroundColor Red
    exit 1
}

Write-Host "Next: cd src-tauri; cargo build --release" -ForegroundColor Green
