# Tauri build mit allen Varianten
$ErrorActionPreference = "Stop"
cd "D:\GIT\Trispr_Flow"

Write-Host "Building Tauri installers for v0.7.3..."
Write-Host "Variants: vulkan-only, cuda-lite, cuda-complete"
Write-Host ""

# Frontend ist already gebaut, wir können direkt zu Rust/Tauri gehen
foreach ($variant in @("vulkan-only", "cuda-lite", "cuda-complete")) {
    Write-Host "Building variant: $variant..." -ForegroundColor Cyan
    
    $env:TRISPR_INSTALLER_VARIANT = $variant
    
    # Tauri release build mit Bundle
    cargo tauri build --release 2>&1 | tee "build-$variant.log"
    
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAILED: $variant" -ForegroundColor Red
        exit 1
    }
    Write-Host "✓ $variant complete" -ForegroundColor Green
}

Write-Host ""
Write-Host "All installers built successfully!" -ForegroundColor Green
ls "src-tauri\target\release\bundle\nsis\*.exe" 2>/dev/null | % {
    $size = (Get-Item $_).Length / 1MB
    Write-Host "  $(Split-Path $_ -Leaf): $($size.ToString('F1')) MB"
}
