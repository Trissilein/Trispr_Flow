param(
  [int]$Warmup = 3,
  [int]$Runs = 30,
  [switch]$Live,
  [switch]$NoRefinement,
  [switch]$FailOnSloMiss,
  [string]$RefinementModel,
  [ValidateSet("vulkan", "vulkan-only", "cuda-lite", "cuda-complete", "ci")]
  [string]$TauriVariant = "vulkan",
  [string[]]$Fixtures
)

$ErrorActionPreference = 'Stop'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path (Join-Path $ScriptDir '..')
Set-Location $RepoRoot

$resultsDir = Join-Path $RepoRoot 'bench/results'
$reportPath = Join-Path $resultsDir 'latest.json'
$errorPath = Join-Path $resultsDir 'latest-error.txt'
$benchConfigPath = Join-Path $resultsDir 'tauri.benchmark.dev.json'
$baseConfigPath = Join-Path $RepoRoot 'src-tauri/tauri.conf.json'
$baseConfigBackupPath = Join-Path $resultsDir 'tauri.conf.base.bak.json'
New-Item -ItemType Directory -Path $resultsDir -Force | Out-Null
if (Test-Path $reportPath) {
  Remove-Item $reportPath -Force
}
if (Test-Path $errorPath) {
  Remove-Item $errorPath -Force
}
if (Test-Path $benchConfigPath) {
  Remove-Item $benchConfigPath -Force
}
if (Test-Path $baseConfigBackupPath) {
  Remove-Item $baseConfigBackupPath -Force
}

function Resolve-NpmCmd {
  $npmCandidates = @(
    (Get-Command npm.cmd -ErrorAction SilentlyContinue),
    (Get-Command npm -ErrorAction SilentlyContinue)
  ) | Where-Object { $_ -and $_.Source }
  if ($npmCandidates.Count -gt 0) {
    return $npmCandidates[0].Source
  }

  $nodeCmd = Get-Command node -ErrorAction SilentlyContinue
  if ($nodeCmd -and $nodeCmd.Source) {
    $nodeDir = Split-Path $nodeCmd.Source -Parent
    $npmCmd = Join-Path $nodeDir "npm.cmd"
    if (Test-Path $npmCmd) {
      return $npmCmd
    }
  }

  throw "npm.cmd not found. Install Node.js or add npm.cmd to PATH."
}

function Get-FreeTcpPort {
  $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
  $listener.Start()
  $port = ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
  $listener.Stop()
  return $port
}

if ($Live -and (-not $Fixtures -or $Fixtures.Count -eq 0)) {
  Write-Warning "Live mode selected without explicit fixtures. Falling back to bench/fixtures/short/*.wav"
}

$env:TRISPR_RUN_LATENCY_BENCHMARK = '1'
$env:TRISPR_RUN_LATENCY_BENCHMARK_EXIT = '1'
$env:TRISPR_BENCHMARK_WARMUP_RUNS = [string]$Warmup
$env:TRISPR_BENCHMARK_MEASURE_RUNS = [string]$Runs
$env:TRISPR_BENCHMARK_INCLUDE_REFINEMENT = if ($NoRefinement) { '0' } else { '1' }
$hadLocalBackendEnv = Test-Path Env:TRISPR_LOCAL_BACKEND
$previousLocalBackendEnv = $env:TRISPR_LOCAL_BACKEND
if ($TauriVariant -eq 'vulkan' -or $TauriVariant -eq 'vulkan-only') {
  $env:TRISPR_LOCAL_BACKEND = 'vulkan'
} elseif ($TauriVariant -eq 'cuda-lite' -or $TauriVariant -eq 'cuda-complete') {
  $env:TRISPR_LOCAL_BACKEND = 'cuda'
} else {
  Remove-Item Env:TRISPR_LOCAL_BACKEND -ErrorAction SilentlyContinue
}
if ([string]::IsNullOrWhiteSpace($RefinementModel)) {
  Remove-Item Env:TRISPR_BENCHMARK_REFINE_MODEL -ErrorAction SilentlyContinue
} else {
  $env:TRISPR_BENCHMARK_REFINE_MODEL = $RefinementModel.Trim()
}

if ($Fixtures -and $Fixtures.Count -gt 0) {
  $resolvedFixtures = @()
  foreach ($fixture in $Fixtures) {
    $resolvedFixtures += (Resolve-Path $fixture).Path
  }
  $env:TRISPR_BENCHMARK_FIXTURES = ($resolvedFixtures -join ';')
} else {
  Remove-Item Env:TRISPR_BENCHMARK_FIXTURES -ErrorAction SilentlyContinue
}

Write-Host "[Latency Benchmark] Warmup=$Warmup Runs=$Runs Refinement=$([string](-not $NoRefinement)) FailOnSloMiss=$([string]$FailOnSloMiss) Variant=$TauriVariant Model=$($env:TRISPR_BENCHMARK_REFINE_MODEL)"
Write-Host "[Latency Benchmark] LocalBackend=$($env:TRISPR_LOCAL_BACKEND)"
$devPort = Get-FreeTcpPort
$npmCmd = Resolve-NpmCmd

& node scripts/generate-tauri-variant-config.mjs --variant $TauriVariant --out $benchConfigPath
if ($LASTEXITCODE -ne 0) {
  throw "failed to generate Tauri benchmark config for variant '$TauriVariant'"
}

$overrideConfig = Get-Content $benchConfigPath -Raw | ConvertFrom-Json
$overrideConfig.build = @{
  beforeDevCommand = "npm run dev:web -- --port $devPort --strictPort"
  devUrl = "http://localhost:$devPort"
}
$utf8NoBom = [System.Text.UTF8Encoding]::new($false)
$benchmarkConfigJson = ($overrideConfig | ConvertTo-Json -Depth 20) + "`n"
[System.IO.File]::WriteAllText($benchConfigPath, $benchmarkConfigJson, $utf8NoBom)
Copy-Item -LiteralPath $baseConfigPath -Destination $baseConfigBackupPath -Force
Copy-Item -LiteralPath $benchConfigPath -Destination $baseConfigPath -Force
Write-Host "[Latency Benchmark] Using isolated dev server port: $devPort"

# Ensure no stale app process keeps target/debug/trispr-flow.exe locked.
Get-Process -Name "trispr-flow" -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue }
Start-Sleep -Milliseconds 250

try {
  & $npmCmd run tauri -- dev --no-watch --config $benchConfigPath
  if ($LASTEXITCODE -ne 0) {
    throw "tauri dev benchmark run failed with exit code $LASTEXITCODE"
  }
} finally {
  if (Test-Path $baseConfigBackupPath) {
    Copy-Item -LiteralPath $baseConfigBackupPath -Destination $baseConfigPath -Force
    Remove-Item $baseConfigBackupPath -Force -ErrorAction SilentlyContinue
  }
  Remove-Item Env:TRISPR_RUN_LATENCY_BENCHMARK -ErrorAction SilentlyContinue
  Remove-Item Env:TRISPR_RUN_LATENCY_BENCHMARK_EXIT -ErrorAction SilentlyContinue
  Remove-Item Env:TRISPR_BENCHMARK_WARMUP_RUNS -ErrorAction SilentlyContinue
  Remove-Item Env:TRISPR_BENCHMARK_MEASURE_RUNS -ErrorAction SilentlyContinue
  Remove-Item Env:TRISPR_BENCHMARK_INCLUDE_REFINEMENT -ErrorAction SilentlyContinue
  Remove-Item Env:TRISPR_BENCHMARK_REFINE_MODEL -ErrorAction SilentlyContinue
  Remove-Item Env:TRISPR_BENCHMARK_FIXTURES -ErrorAction SilentlyContinue
  if ($hadLocalBackendEnv) {
    $env:TRISPR_LOCAL_BACKEND = $previousLocalBackendEnv
  } else {
    Remove-Item Env:TRISPR_LOCAL_BACKEND -ErrorAction SilentlyContinue
  }
  Remove-Item $benchConfigPath -ErrorAction SilentlyContinue
}

if (-not (Test-Path $reportPath)) {
  if (Test-Path $errorPath) {
    $benchmarkError = Get-Content $errorPath -Raw
    Write-Error "Latency benchmark failed before writing report: $benchmarkError"
    exit 1
  }
  Write-Error "Latency benchmark report not found: $reportPath"
  exit 1
}

$report = Get-Content $reportPath -Raw | ConvertFrom-Json
$p50 = [int]$report.p50_ms
$p95 = [int]$report.p95_ms
$sloP50 = [int]$report.slo_p50_ms
$sloP95 = [int]$report.slo_p95_ms
$samples = @($report.samples).Count

Write-Host "[Latency Benchmark] Samples=$samples p50=${p50}ms p95=${p95}ms"
Write-Host "[Latency Benchmark] Targets: p50<=${sloP50}ms p95<=${sloP95}ms"

if (-not [bool]$report.slo_pass) {
  if ($FailOnSloMiss) {
    Write-Error "SLO MISS (strict): p50=${p50}ms, p95=${p95}ms"
    exit 1
  }
  Write-Warning "SLO MISS (warn-only): p50=${p50}ms, p95=${p95}ms"
  exit 0
}

Write-Host "[Latency Benchmark] SLO PASS"
exit 0
