param(
  [int]$Warmup = 3,
  [int]$Runs = 30,
  [switch]$Live,
  [switch]$NoRefinement,
  [switch]$FailOnSloMiss,
  [string[]]$Fixtures
)

$ErrorActionPreference = 'Stop'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path (Join-Path $ScriptDir '..')
Set-Location $RepoRoot

$resultsDir = Join-Path $RepoRoot 'bench/results'
$reportPath = Join-Path $resultsDir 'latest.json'
$benchConfigPath = Join-Path $resultsDir 'tauri.benchmark.dev.json'
New-Item -ItemType Directory -Path $resultsDir -Force | Out-Null
if (Test-Path $reportPath) {
  Remove-Item $reportPath -Force
}
if (Test-Path $benchConfigPath) {
  Remove-Item $benchConfigPath -Force
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

if ($Fixtures -and $Fixtures.Count -gt 0) {
  $resolvedFixtures = @()
  foreach ($fixture in $Fixtures) {
    $resolvedFixtures += (Resolve-Path $fixture).Path
  }
  $env:TRISPR_BENCHMARK_FIXTURES = ($resolvedFixtures -join ';')
} else {
  Remove-Item Env:TRISPR_BENCHMARK_FIXTURES -ErrorAction SilentlyContinue
}

Write-Host "[Latency Benchmark] Warmup=$Warmup Runs=$Runs Refinement=$([string](-not $NoRefinement)) FailOnSloMiss=$([string]$FailOnSloMiss)"
$devPort = Get-FreeTcpPort
$overrideConfig = @{
  build = @{
    beforeDevCommand = "npm run dev -- --port $devPort --strictPort"
    devUrl = "http://localhost:$devPort"
  }
}
$overrideConfig | ConvertTo-Json -Depth 5 | Set-Content -Path $benchConfigPath -Encoding UTF8
Write-Host "[Latency Benchmark] Using isolated dev server port: $devPort"

# Ensure no stale app process keeps target/debug/trispr-flow.exe locked.
Get-Process -Name "trispr-flow" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 250

try {
  npm run tauri -- dev --no-watch --config $benchConfigPath
} finally {
  Remove-Item Env:TRISPR_RUN_LATENCY_BENCHMARK -ErrorAction SilentlyContinue
  Remove-Item Env:TRISPR_RUN_LATENCY_BENCHMARK_EXIT -ErrorAction SilentlyContinue
  Remove-Item Env:TRISPR_BENCHMARK_WARMUP_RUNS -ErrorAction SilentlyContinue
  Remove-Item Env:TRISPR_BENCHMARK_MEASURE_RUNS -ErrorAction SilentlyContinue
  Remove-Item Env:TRISPR_BENCHMARK_INCLUDE_REFINEMENT -ErrorAction SilentlyContinue
  Remove-Item Env:TRISPR_BENCHMARK_FIXTURES -ErrorAction SilentlyContinue
  Remove-Item $benchConfigPath -ErrorAction SilentlyContinue
}

if (-not (Test-Path $reportPath)) {
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
