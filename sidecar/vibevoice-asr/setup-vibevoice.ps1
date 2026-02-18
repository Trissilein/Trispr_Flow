param(
  [switch]$PrefetchModel
)

$ErrorActionPreference = "Stop"
$EXIT_SUCCESS = 0
$EXIT_PYTHON_NOT_FOUND = 10
$EXIT_PYTHON_VERSION_DETECT = 11
$EXIT_PYTHON_UNSUPPORTED = 12
$EXIT_REQUIREMENTS_MISSING = 13
$EXIT_VENV_CREATE = 20
$EXIT_VENV_PYTHON_MISSING = 21
$EXIT_PIP_UPGRADE = 30
$EXIT_PIP_INSTALL = 31
$EXIT_PREFETCH = 40
$EXIT_UNKNOWN = 99

$script:SetupExitCode = $EXIT_UNKNOWN

function Write-Step([string]$text) {
  Write-Host ""
  Write-Host "== $text =="
}

function Throw-SetupError([int]$code, [string]$message) {
  $script:SetupExitCode = $code
  throw $message
}

function Resolve-PythonExe {
  $candidates = @(
    @("py", "-3.13"),
    @("py", "-3.12"),
    @("py", "-3.11"),
    @("python"),
    @("python3")
  )

  foreach ($candidate in $candidates) {
    $cmd = $candidate[0]
    $args = @()
    if ($candidate.Length -gt 1) {
      $args += $candidate[1]
    }
    $args += "-c"
    $args += "import sys; print(sys.executable)"

    try {
      $output = & $cmd @args 2>$null
      if ($LASTEXITCODE -eq 0 -and $output) {
        $exe = ($output | Select-Object -First 1).Trim()
        if (-not [string]::IsNullOrWhiteSpace($exe) -and (Test-Path $exe)) {
          return $exe
        }
      }
    } catch {
      continue
    }
  }

  Throw-SetupError $EXIT_PYTHON_NOT_FOUND "Python 3.11+ not found. Please install Python from https://www.python.org/downloads/"
}

function Get-PythonVersion([string]$pythonExe) {
  $ver = & $pythonExe -c "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}')" 2>$null
  if ($LASTEXITCODE -ne 0 -or -not $ver) {
    Throw-SetupError $EXIT_PYTHON_VERSION_DETECT "Failed to detect Python version for: $pythonExe"
  }
  return ($ver | Select-Object -First 1).Trim()
}

function Assert-SupportedPython([string]$version) {
  $parts = $version.Split(".")
  if ($parts.Length -lt 2) {
    throw "Unexpected Python version format: $version"
  }
  $major = [int]$parts[0]
  $minor = [int]$parts[1]
  if ($major -ne 3 -or $minor -lt 11) {
    Throw-SetupError $EXIT_PYTHON_UNSUPPORTED "Python $version is not supported. Use Python 3.11, 3.12, or 3.13."
  }
}

function Install-Dependencies([string]$pythonExe, [string]$venvDir, [string]$requirementsPath) {
  Write-Step "Creating virtual environment"
  if (-not (Test-Path $venvDir)) {
    & $pythonExe -m venv $venvDir
    if ($LASTEXITCODE -ne 0) {
      Throw-SetupError $EXIT_VENV_CREATE "Failed to create virtual environment at $venvDir"
    }
  }

  $venvPython = Join-Path $venvDir "Scripts\python.exe"
  if (-not (Test-Path $venvPython)) {
    Throw-SetupError $EXIT_VENV_PYTHON_MISSING "Virtual environment Python not found at $venvPython"
  }

  Write-Step "Upgrading pip"
  & $venvPython -m pip install --upgrade pip | Out-Host
  if ($LASTEXITCODE -ne 0) {
    Throw-SetupError $EXIT_PIP_UPGRADE "Failed to upgrade pip"
  }

  Write-Step "Installing VibeVoice dependencies"
  & $venvPython -m pip install -r $requirementsPath | Out-Host
  if ($LASTEXITCODE -ne 0) {
    Throw-SetupError $EXIT_PIP_INSTALL "Dependency installation failed"
  }

  return [string]$venvPython
}

function Prefetch-Model([string]$venvPython) {
  Write-Step "Prefetching VibeVoice model (this can take a while)"
  $prefetchScript = Join-Path ([System.IO.Path]::GetTempPath()) ("trispr-vibevoice-prefetch-" + [guid]::NewGuid().ToString("N") + ".py")
  $code = @'
import os
from huggingface_hub import snapshot_download
from transformers import AutoTokenizer

model_id = 'microsoft/VibeVoice-ASR'
lm_model = os.getenv('VIBEVOICE_LM_MODEL', 'Qwen/Qwen2.5-1.5B')

# Prefetch VibeVoice ASR checkpoint files
snapshot_download(repo_id=model_id, resume_download=True)

# Prefetch tokenizer needed by VibeVoiceASRProcessor
AutoTokenizer.from_pretrained(lm_model, trust_remote_code=True)

print('Model and tokenizer cached successfully')
'@
  Set-Content -Path $prefetchScript -Value $code -Encoding UTF8

  try {
    & $venvPython $prefetchScript | Out-Host
    if ($LASTEXITCODE -ne 0) {
      Throw-SetupError $EXIT_PREFETCH "Model prefetch failed"
    }
  } finally {
    Remove-Item $prefetchScript -ErrorAction SilentlyContinue
  }
}
try {
  Write-Step "Locating Python"
  $pythonExe = Resolve-PythonExe
  $pythonVersion = Get-PythonVersion $pythonExe
  Assert-SupportedPython $pythonVersion
  Write-Host "Python: $pythonExe ($pythonVersion)"

  $scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
  $requirementsPath = Join-Path $scriptDir "requirements.txt"
  if (-not (Test-Path $requirementsPath)) {
    Throw-SetupError $EXIT_REQUIREMENTS_MISSING "requirements.txt not found next to setup script: $requirementsPath"
  }

  $venvDir = Join-Path $env:LOCALAPPDATA "com.trispr.flow\vibevoice-venv"
  $venvPython = Install-Dependencies -pythonExe $pythonExe -venvDir $venvDir -requirementsPath $requirementsPath
  if ($venvPython -is [array]) {
    $venvPython = [string]$venvPython[-1]
  }
  $venvPython = [string]$venvPython
  $venvPython = $venvPython.Trim()
  if (-not (Test-Path $venvPython)) {
    Throw-SetupError $EXIT_VENV_PYTHON_MISSING "Virtual environment Python not found after setup: $venvPython"
  }

  if ($PrefetchModel) {
    Prefetch-Model -venvPython $venvPython
  }

  Write-Step "Setup complete"
  Write-Host "Virtual environment: $venvDir"
  Write-Host "VibeVoice sidecar dependencies are installed."
  Write-Host "No Git installation is required."
  exit $EXIT_SUCCESS
} catch {
  $message = $_.Exception.Message
  if (-not $script:SetupExitCode) {
    $script:SetupExitCode = $EXIT_UNKNOWN
  }
  Write-Host ""
  Write-Host "== Setup failed =="
  [Console]::Error.WriteLine(("[E{0}] {1}" -f $script:SetupExitCode, $message))
  exit $script:SetupExitCode
}
