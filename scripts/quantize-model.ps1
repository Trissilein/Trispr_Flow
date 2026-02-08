param(
  [ValidateSet("turbo", "large")]
  [string]$Model = "turbo",
  [ValidateSet("q5_0")]
  [string]$Quant = "q5_0",
  [string]$WhisperRoot = "D:\GIT\whisper.cpp"
)

$modelsDir = Join-Path $WhisperRoot "models"

if ($Model -eq "large") {
  $inputFile = Join-Path $modelsDir "ggml-large-v3.bin"
  $outputFile = Join-Path $modelsDir "ggml-large-v3-$Quant.bin"
} else {
  $inputFile = Join-Path $modelsDir "ggml-large-v3-turbo.bin"
  $outputFile = Join-Path $modelsDir "ggml-large-v3-turbo-$Quant.bin"
}

$quantizeCandidates = @(
  (Join-Path $WhisperRoot "build\bin\Release\quantize.exe"),
  (Join-Path $WhisperRoot "build\bin\quantize.exe")
)

$quantizeExe = $quantizeCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1

if (-not (Test-Path $inputFile)) {
  Write-Error "Input model not found: $inputFile"
  exit 1
}

if (-not $quantizeExe) {
  Write-Error "quantize.exe not found. Build it first:"
  Write-Host "  cmake -S $WhisperRoot -B $WhisperRoot\build"
  Write-Host "  cmake --build $WhisperRoot\build --target quantize --config Release"
  exit 1
}

Write-Host "Quantizing:"
Write-Host "  Input : $inputFile"
Write-Host "  Output: $outputFile"
Write-Host "  Quant : $Quant"

& $quantizeExe $inputFile $outputFile $Quant

if ($LASTEXITCODE -ne 0) {
  Write-Error "Quantize failed with exit code $LASTEXITCODE"
  exit $LASTEXITCODE
}

Write-Host "Done. Output saved to: $outputFile"
