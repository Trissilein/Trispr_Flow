# Build whisper-server.exe from whisper.cpp source
# Requires: CUDA Toolkit, CMake, Visual Studio Build Tools
# Usage: powershell -ExecutionPolicy Bypass -File build-whisper-server.ps1

param(
    [string]$BuildDir = "C:\temp\whisper.cpp-build",
    [string]$TrispiFlow = "D:\GIT\Trispr_Flow"
)

Write-Host "🏗️  Building whisper-server.exe from whisper.cpp source..." -ForegroundColor Cyan

# Check prerequisites
Write-Host "Checking prerequisites..."
$cmake = Get-Command cmake -ErrorAction SilentlyContinue
$git = Get-Command git -ErrorAction SilentlyContinue
$nvcc = Get-Command nvcc -ErrorAction SilentlyContinue

if (-not $cmake) {
    Write-Host "❌ CMake not found. Install: https://cmake.org/download/" -ForegroundColor Red
    exit 1
}
if (-not $git) {
    Write-Host "❌ Git not found. Install: https://git-scm.com/download/win" -ForegroundColor Red
    exit 1
}
if (-not $nvcc) {
    Write-Host "⚠️  CUDA Toolkit not found in PATH. Attempting anyway (may fail)..." -ForegroundColor Yellow
}

# Clone whisper.cpp
Write-Host "📦 Cloning whisper.cpp..." -ForegroundColor Cyan
New-Item -ItemType Directory -Force -Path $BuildDir | Out-Null
cd $BuildDir
if (Test-Path "whisper.cpp") {
    Write-Host "  (whisper.cpp already exists, skipping clone)"
} else {
    & git clone https://github.com/ggerganov/whisper.cpp.git
}
cd whisper.cpp

# Create build directory
Write-Host "🔨 Setting up CMake build..." -ForegroundColor Cyan
if (Test-Path "build-cuda") {
    Write-Host "  (build-cuda already exists, skipping setup)"
} else {
    New-Item -ItemType Directory -Path "build-cuda" | Out-Null
}
cd build-cuda

# Configure CMake with CUDA
Write-Host "⚙️  Configuring CMake with CUDA support..." -ForegroundColor Cyan
& cmake .. -DGGML_CUDA=ON -DGGML_CUDA_F16=ON -A x64

if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ CMake configuration failed!" -ForegroundColor Red
    exit 1
}

# Build
Write-Host "🚀 Building whisper-server (this takes ~5-10 min)..." -ForegroundColor Cyan
& cmake --build . --config Release --parallel

if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Build failed!" -ForegroundColor Red
    exit 1
}

# Copy binary
Write-Host "📋 Copying binary..." -ForegroundColor Cyan
$srcBinary = "bin\Release\whisper-server.exe"
$dstBinary = "$TrispiFlow\src-tauri\bin\cuda\whisper-server.exe"

if (Test-Path $srcBinary) {
    Copy-Item $srcBinary -Destination $dstBinary -Force
    Write-Host "✅ whisper-server.exe copied to: $dstBinary" -ForegroundColor Green

    # Verify
    $fileSize = (Get-Item $dstBinary).Length / 1MB
    Write-Host "   File size: $([Math]::Round($fileSize, 2)) MB" -ForegroundColor Green
} else {
    Write-Host "❌ Binary not found at $srcBinary" -ForegroundColor Red
    exit 1
}

Write-Host "`n✨ Done! You can now build Trispr Flow with whisper-server support." -ForegroundColor Green
Write-Host "   Command: cargo build --release" -ForegroundColor Cyan
