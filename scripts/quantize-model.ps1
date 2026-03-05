param(
  [ValidateSet("turbo", "large")]
  [string]$Model = "turbo",
  [ValidateSet("q5_0")]
  [string]$Quant = "q5_0",
  [string]$WhisperRoot = ""
)

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..")).Path

if ([string]::IsNullOrWhiteSpace($WhisperRoot)) {
  $envCandidates = @($env:TRISPR_WHISPER_ROOT, $env:WHISPER_ROOT) |
    Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
  foreach ($candidate in $envCandidates) {
    if (Test-Path $candidate) {
      $WhisperRoot = (Resolve-Path $candidate).Path
      break
    }
  }

  if ([string]::IsNullOrWhiteSpace($WhisperRoot)) {
    $siblingRoot = Join-Path $RepoRoot "..\whisper.cpp"
    if (Test-Path $siblingRoot) {
      $WhisperRoot = (Resolve-Path $siblingRoot).Path
    }
  }
}

if ([string]::IsNullOrWhiteSpace($WhisperRoot) -or -not (Test-Path $WhisperRoot)) {
  Write-Error "whisper.cpp not found. Use -WhisperRoot <path> or set TRISPR_WHISPER_ROOT."
  exit 1
}

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
