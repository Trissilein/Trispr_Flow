param(
  [string]$VenvPath = "D:\GIT\qwen3tts-venv",
  [string]$Host = "127.0.0.1",
  [int]$Port = 8000,
  [string]$Model = "Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice",
  [string]$Device = "cpu",
  [string]$Voice = "vivian",
  [string]$ApiKey = ""
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$bridgeScript = Join-Path $repoRoot "scripts\qwen3-tts-bridge.py"
$pythonExe = Join-Path $VenvPath "Scripts\python.exe"

if (-not (Test-Path $pythonExe)) {
  throw "Python not found at '$pythonExe'. Set -VenvPath to your qwen-tts virtualenv."
}
if (-not (Test-Path $bridgeScript)) {
  throw "Bridge script not found: $bridgeScript"
}

$env:TRISPR_QWEN3_TTS_MODEL = $Model
$env:TRISPR_QWEN3_TTS_DEVICE = $Device
$env:TRISPR_QWEN3_TTS_VOICE = $Voice
if ([string]::IsNullOrWhiteSpace($ApiKey)) {
  Remove-Item Env:TRISPR_QWEN3_TTS_API_KEY -ErrorAction SilentlyContinue
} else {
  $env:TRISPR_QWEN3_TTS_API_KEY = $ApiKey
}

Write-Host "[Qwen3-TTS] Starting bridge on http://${Host}:${Port}/v1/audio/speech"
Write-Host "[Qwen3-TTS] model=$Model device=$Device voice=$Voice"
& $pythonExe $bridgeScript --host $Host --port $Port
