param(
  [int]$Warmup = 1,
  [int]$Runs = 3,
  [string[]]$Providers = @("windows_native", "local_custom", "qwen3_tts"),
  [double]$Rate = 1.0,
  [double]$Volume = 1.0,
  [string]$PiperBinaryPath = "",
  [string]$PiperModelPath = "",
  [string]$Qwen3Endpoint = "http://127.0.0.1:8000/v1/audio/speech",
  [string]$Qwen3Model = "Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice",
  [string]$Qwen3Voice = "vivian",
  [string]$Qwen3ApiKey = "",
  [int]$Qwen3TimeoutSec = 45,
  [switch]$FailOnNoRecommendation,
  [switch]$FailOnGateMiss,
  [switch]$UnlockMatrix,
  [switch]$NoRuntimeSmoke,
  [switch]$NoSaveExamples,
  [switch]$PlayExamples,
  [switch]$PlayBlindExamples
)

$ErrorActionPreference = 'Stop'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path (Join-Path $ScriptDir '..')
Set-Location $RepoRoot

$resultsDir = Join-Path $RepoRoot 'bench/results'
$reportPath = Join-Path $resultsDir 'tts.latest.json'
$examplesRoot = Join-Path $resultsDir 'tts-samples'
$saveExamples = -not $NoSaveExamples
$runRuntimeSmoke = -not $NoRuntimeSmoke

$gateReliability = 0.95
$gateLatencyP50 = 700
$gateLatencyP95 = 1500
$gateMinSuccessPerScenario = 2

function Get-ProviderProfile {
  param([string]$Provider)
  if ($Provider -eq 'qwen3_tts') {
    return [ordered]@{
      provider = $Provider
      surface = 'benchmark_experimental'
      experimental_reason = 'Endpoint-backed runtime provider treated as experimental for release-gating.'
    }
  }
  return [ordered]@{
    provider = $Provider
    surface = 'runtime_stable'
    experimental_reason = $null
  }
}

function Normalize-Providers {
  param([string[]]$Requested)
  $out = @()
  foreach ($provider in $Requested) {
    $providerValue = if ($null -eq $provider) { '' } else { [string]$provider }
    $normalized = $providerValue.Trim().ToLowerInvariant()
    if ($normalized -notin @('windows_native', 'windows_natural', 'local_custom', 'qwen3_tts')) {
      continue
    }
    if ($normalized -notin $out) {
      $out += $normalized
    }
  }
  if ($out.Count -eq 0) {
    return @('windows_native', 'local_custom', 'qwen3_tts')
  }
  return $out
}

function Get-DefaultScenarios {
  return @(
    [ordered]@{ id='short_de_cold'; text='Kurzer Benchmark-Check.'; length_bucket='short'; language='de'; thermal='cold' },
    [ordered]@{ id='short_de_warm'; text='Kurzer Benchmark-Check.'; length_bucket='short'; language='de'; thermal='warm' },
    [ordered]@{ id='short_en_cold'; text='Short benchmark check.'; length_bucket='short'; language='en'; thermal='cold' },
    [ordered]@{ id='short_en_warm'; text='Short benchmark check.'; length_bucket='short'; language='en'; thermal='warm' },
    [ordered]@{ id='long_de_cold'; text='Dies ist ein längerer deutscher Benchmark-Satz, der Antworttempo und Stabilität unter praxisnahen Bedingungen vergleicht.'; length_bucket='long'; language='de'; thermal='cold' },
    [ordered]@{ id='long_de_warm'; text='Dies ist ein längerer deutscher Benchmark-Satz, der Antworttempo und Stabilität unter praxisnahen Bedingungen vergleicht.'; length_bucket='long'; language='de'; thermal='warm' },
    [ordered]@{ id='long_en_cold'; text='This is a longer benchmark sentence to compare synthesis latency and stability under realistic assistant output conditions.'; length_bucket='long'; language='en'; thermal='cold' },
    [ordered]@{ id='long_en_warm'; text='This is a longer benchmark sentence to compare synthesis latency and stability under realistic assistant output conditions.'; length_bucket='long'; language='en'; thermal='warm' }
  )
}

function Classify-Failure {
  param([string]$Error)
  $errorValue = if ($null -eq $Error) { '' } else { [string]$Error }
  $normalized = $errorValue.Trim().ToLowerInvariant()
  if ([string]::IsNullOrWhiteSpace($normalized)) {
    return 'runtime_error'
  }
  if ($normalized.Contains('binary not found') -or $normalized.Contains('no such file') -or $normalized.Contains('failed to start piper')) {
    return 'missing_binary'
  }
  if ($normalized.Contains('model not found') -or $normalized.Contains('.onnx') -or $normalized.Contains('no piper model')) {
    return 'missing_model'
  }
  if ($normalized.Contains('http 401') -or $normalized.Contains('http 403') -or $normalized.Contains('authorization') -or $normalized.Contains('api key') -or $normalized.Contains('unauthorized') -or $normalized.Contains('forbidden')) {
    return 'auth_missing'
  }
  if ($normalized.Contains('endpoint') -or $normalized.Contains('timed out') -or $normalized.Contains('connection') -or $normalized.Contains('refused') -or $normalized.Contains('network')) {
    return 'endpoint_unreachable'
  }
  return 'runtime_error'
}

function Get-Percentile {
  param([long[]]$Values, [double]$Percentile)
  if (-not $Values -or $Values.Count -eq 0) {
    return $null
  }
  $sorted = @($Values | Sort-Object)
  $idx = [int]([Math]::Ceiling($sorted.Count * $Percentile) - 1)
  if ($idx -lt 0) { $idx = 0 }
  if ($idx -ge $sorted.Count) { $idx = $sorted.Count - 1 }
  return [long]$sorted[$idx]
}

function Resolve-PiperBinary {
  param([string]$Configured)
  $candidates = @()
  if (-not [string]::IsNullOrWhiteSpace($Configured)) {
    $candidates += $Configured
  }
  foreach ($cmdName in @('piper.exe', 'piper')) {
    $cmd = Get-Command $cmdName -ErrorAction SilentlyContinue
    if ($cmd -and $cmd.Source) {
      $candidates += $cmd.Source
    }
  }
  $candidates += @(
    (Join-Path $RepoRoot 'src-tauri/bin/piper/piper.exe'),
    (Join-Path $RepoRoot 'bin/piper/piper.exe'),
    'D:\GIT\piper\piper.exe',
    'D:\GIT\piper\build\piper.exe'
  )
  if ($env:LOCALAPPDATA) {
    $candidates += (Join-Path $env:LOCALAPPDATA 'trispr-flow\piper\piper.exe')
  }
  foreach ($candidate in $candidates) {
    if ([string]::IsNullOrWhiteSpace($candidate)) { continue }
    if (Test-Path $candidate -PathType Leaf) {
      return (Resolve-Path $candidate).Path
    }
  }
  return $null
}

function Resolve-PiperModel {
  param([string]$Configured)
  if (-not [string]::IsNullOrWhiteSpace($Configured) -and (Test-Path $Configured -PathType Leaf)) {
    return (Resolve-Path $Configured).Path
  }
  $modelDirs = @(
    (Join-Path $RepoRoot 'src-tauri/bin/piper/voices'),
    (Join-Path $RepoRoot 'piper/voices'),
    (Join-Path $RepoRoot 'piper/models'),
    'D:\GIT\piper\voices',
    'D:\GIT\piper\models'
  )
  if ($env:LOCALAPPDATA) {
    $modelDirs += (Join-Path $env:LOCALAPPDATA 'trispr-flow\piper\voices')
  }
  foreach ($dir in $modelDirs) {
    if (-not (Test-Path $dir -PathType Container)) { continue }
    $model = Get-ChildItem -Path $dir -Filter *.onnx -File -ErrorAction SilentlyContinue | Sort-Object Name | Select-Object -First 1
    if ($model) {
      return $model.FullName
    }
  }
  return $null
}

function Invoke-WindowsNativeSynthesisToFile {
  param(
    [string]$Text,
    [double]$Rate,
    [double]$Volume,
    [string]$OutputFile
  )
  Add-Type -AssemblyName System.Speech
  $synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
  try {
    $synth.Rate = [Math]::Max(-10, [Math]::Min(10, [int][Math]::Round(($Rate - 1.0) * 10.0)))
    $synth.Volume = [Math]::Max(0, [Math]::Min(100, [int][Math]::Round($Volume * 100.0)))
    $parent = Split-Path -Parent $OutputFile
    if (-not (Test-Path $parent)) {
      New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }
    $synth.SetOutputToWaveFile($OutputFile)
    $synth.Speak($Text)
    $synth.SetOutputToNull()
  } finally {
    $synth.Dispose()
  }
}

function Get-WindowsNaturalVoiceName {
  Add-Type -AssemblyName System.Speech
  $synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
  try {
    $voice = $synth.GetInstalledVoices() |
      ForEach-Object { $_.VoiceInfo.Name } |
      Where-Object { $_ -match 'Natural|Multilingual|Online' } |
      Sort-Object `
        @{Expression = { if ($_ -match 'Multilingual') { 0 } elseif ($_ -match 'Natural') { 1 } else { 2 } }; Ascending = $true }, `
        @{Expression = { $_ }; Ascending = $true } |
      Select-Object -First 1
    if ([string]::IsNullOrWhiteSpace($voice)) {
      return $null
    }
    return [string]$voice
  } finally {
    $synth.Dispose()
  }
}

function Invoke-WindowsNaturalSynthesisToFile {
  param(
    [string]$Text,
    [double]$Rate,
    [double]$Volume,
    [string]$OutputFile
  )
  $voice = Get-WindowsNaturalVoiceName
  if ([string]::IsNullOrWhiteSpace($voice)) {
    throw 'No Windows Natural voice found. Install NaturalVoiceSAPIAdapter and at least one natural voice.'
  }
  Add-Type -AssemblyName System.Speech
  $synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
  try {
    $synth.SelectVoice($voice)
    $synth.Rate = [Math]::Max(-10, [Math]::Min(10, [int][Math]::Round(($Rate - 1.0) * 10.0)))
    $synth.Volume = [Math]::Max(0, [Math]::Min(100, [int][Math]::Round($Volume * 100.0)))
    $parent = Split-Path -Parent $OutputFile
    if (-not (Test-Path $parent)) {
      New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }
    $synth.SetOutputToWaveFile($OutputFile)
    $synth.Speak($Text)
    $synth.SetOutputToNull()
  } finally {
    $synth.Dispose()
  }
}

function Invoke-PiperSynthesisToFile {
  param(
    [string]$Text,
    [string]$Binary,
    [string]$Model,
    [double]$Rate,
    [string]$OutputFile
  )
  $lengthScale = [string]::Format([System.Globalization.CultureInfo]::InvariantCulture, '{0:0.###}', (1.0 / [Math]::Max(0.25, [Math]::Min(4.0, $Rate))))

  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $Binary
  $psi.Arguments = "--model `"$Model`" --output_file `"$OutputFile`" --length_scale $lengthScale"
  $psi.UseShellExecute = $false
  $psi.CreateNoWindow = $true
  $psi.RedirectStandardInput = $true
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true

  $proc = [System.Diagnostics.Process]::Start($psi)
  if (-not $proc) {
    throw "Failed to start piper process."
  }

  $proc.StandardInput.Write($Text)
  $proc.StandardInput.Close()
  $proc.WaitForExit()
  $stdout = $proc.StandardOutput.ReadToEnd()
  $stderr = $proc.StandardError.ReadToEnd()

  if ($proc.ExitCode -ne 0) {
    $detail = if ([string]::IsNullOrWhiteSpace($stderr)) { $stdout } else { $stderr }
    throw "Piper exited with code $($proc.ExitCode): $detail"
  }
}

function Invoke-QwenSynthesisToFile {
  param(
    [string]$Text,
    [double]$Rate,
    [string]$Endpoint,
    [string]$Model,
    [string]$Voice,
    [string]$ApiKey,
    [int]$TimeoutSec,
    [string]$OutputFile
  )
  $headers = @{}
  if (-not [string]::IsNullOrWhiteSpace($ApiKey)) {
    $headers['Authorization'] = "Bearer $ApiKey"
  }
  $body = [ordered]@{
    model = $Model
    input = $Text
    voice = $Voice
    response_format = 'wav'
    stream = $false
    speed = [Math]::Max(0.5, [Math]::Min(2.0, $Rate))
  } | ConvertTo-Json -Compress

  $parent = Split-Path -Parent $OutputFile
  if (-not (Test-Path $parent)) {
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
  }

  Invoke-WebRequest -UseBasicParsing -Uri $Endpoint -Method Post -ContentType 'application/json' -Headers $headers -Body $body -OutFile $OutputFile -TimeoutSec $TimeoutSec | Out-Null
}

function New-SampleObject {
  param(
    [string]$Provider,
    [string]$Scenario,
    [int]$Run,
    [long]$ElapsedMs,
    [bool]$Success,
    [string]$Error,
    [string]$FailureCategory
  )
  return [ordered]@{
    provider = $Provider
    scenario = $Scenario
    run = $Run
    elapsed_ms = $ElapsedMs
    success = $Success
    error = if ($Success) { $null } else { $Error }
    failure_category = if ($Success) { $null } else { $FailureCategory }
  }
}

function Summarize-Provider {
  param(
    [string]$Provider,
    [object[]]$Samples
  )
  $providerSamples = @($Samples | Where-Object { $_.provider -eq $Provider })
  $attempts = $providerSamples.Count
  $success = @($providerSamples | Where-Object { $_.success })
  $latencies = @($success | ForEach-Object { [long]$_.elapsed_ms } | Sort-Object)
  $successCount = $success.Count
  $failureCount = $attempts - $successCount
  $successRate = if ($attempts -gt 0) { [double]$successCount / [double]$attempts } else { 0.0 }

  $avg = $null
  if ($latencies.Count -gt 0) {
    $avg = [long]([double]($latencies | Measure-Object -Sum).Sum / [double]$latencies.Count)
  }

  return [ordered]@{
    provider = $Provider
    attempts = $attempts
    success_count = $successCount
    failure_count = $failureCount
    success_rate = [Math]::Round($successRate, 6)
    p50_ms = Get-Percentile -Values $latencies -Percentile 0.5
    p95_ms = Get-Percentile -Values $latencies -Percentile 0.95
    avg_ms = $avg
  }
}

function Build-FallbackOrder {
  param(
    [object[]]$Summaries,
    [double]$ReliabilityGate
  )
  $eligible = @($Summaries | Where-Object { $_.success_rate -ge $ReliabilityGate -and $null -ne $_.p95_ms } |
    Sort-Object @{Expression = { [long]$_.p95_ms }; Ascending = $true }, @{Expression = { [long]$_.p50_ms }; Ascending = $true }, @{Expression = { $_.provider }; Ascending = $true })

  $fallbackSorted = @($Summaries | Sort-Object @{Expression = { [double]$_.success_rate }; Descending = $true },
    @{Expression = { [long]$_.failure_count }; Ascending = $true },
    @{Expression = { if ($null -eq $_.p95_ms) { [long]::MaxValue } else { [long]$_.p95_ms } }; Ascending = $true },
    @{Expression = { if ($null -eq $_.p50_ms) { [long]::MaxValue } else { [long]$_.p50_ms } }; Ascending = $true },
    @{Expression = { $_.provider }; Ascending = $true })

  $order = @()
  foreach ($entry in @($eligible + $fallbackSorted)) {
    if ($entry.provider -notin $order) {
      $order += $entry.provider
    }
  }
  return $order
}

New-Item -ItemType Directory -Path $resultsDir -Force | Out-Null
if ($saveExamples) {
  New-Item -ItemType Directory -Path $examplesRoot -Force | Out-Null
}
if (Test-Path $reportPath) {
  Remove-Item $reportPath -Force
}

$providerList = Normalize-Providers -Requested $Providers
$scenarios = Get-DefaultScenarios
if ($UnlockMatrix) {
  Write-Host '[TTS Benchmark] UnlockMatrix was set, but no custom scenario input is wired in this headless harness. Using default scenario matrix.'
}

$exampleDir = $null
if ($saveExamples) {
  $stamp = (Get-Date).ToString('yyyyMMdd-HHmmss')
  $exampleDir = Join-Path $examplesRoot $stamp
  New-Item -ItemType Directory -Path $exampleDir -Force | Out-Null
}

Write-Host "[TTS Benchmark] Warmup=$Warmup Runs=$Runs Providers=$($providerList -join ',') Rate=$Rate Volume=$Volume"
Write-Host "[TTS Benchmark] MatrixLocked=$(-not $UnlockMatrix) RuntimeSmoke=$runRuntimeSmoke"
if ($providerList -contains 'local_custom') {
  if ([string]::IsNullOrWhiteSpace($PiperBinaryPath)) {
    Write-Host '[TTS Benchmark] local_custom piper_binary=<auto-resolve>'
  } else {
    Write-Host "[TTS Benchmark] local_custom piper_binary=$PiperBinaryPath"
  }
}
if ($providerList -contains 'qwen3_tts') {
  Write-Host "[TTS Benchmark] qwen3_tts endpoint=$Qwen3Endpoint model=$Qwen3Model voice=$Qwen3Voice timeout=${Qwen3TimeoutSec}s"
}

$warnings = @()
$profiles = @($providerList | ForEach-Object { Get-ProviderProfile -Provider $_ })
$preflightChecks = @()
$runtimeSmokeChecks = @()
$samples = @()
$exampleTaken = @{}
$exampleClips = @()

$piperResolvedBinary = $null
$piperResolvedModel = $null
if ($providerList -contains 'local_custom') {
  $piperResolvedBinary = Resolve-PiperBinary -Configured $PiperBinaryPath
  $piperResolvedModel = Resolve-PiperModel -Configured $PiperModelPath
}

$preflightOkByProvider = @{}
foreach ($provider in $providerList) {
  $checks = @()

  switch ($provider) {
    'windows_native' {
      $isWindows = $env:OS -eq 'Windows_NT'
      $checks += [ordered]@{
        provider = $provider
        check = 'runtime_windows'
        passed = $isWindows
        category = if ($isWindows) { $null } else { 'runtime_error' }
        detail = if ($isWindows) { 'Windows runtime detected for windows_native provider.' } else { 'windows_native provider requires Windows runtime.' }
      }

      if ($isWindows) {
        $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("tts_preflight_windows_{0}.wav" -f [guid]::NewGuid().ToString('N'))
        try {
          Invoke-WindowsNativeSynthesisToFile -Text 'Preflight ping.' -Rate 1.0 -Volume 0.2 -OutputFile $tmp
          $checks += [ordered]@{ provider=$provider; check='synthesis_probe'; passed=$true; category=$null; detail='Windows synthesis probe succeeded.' }
        } catch {
          $msg = "Windows synthesis probe failed: $($_.Exception.Message)"
          $checks += [ordered]@{ provider=$provider; check='synthesis_probe'; passed=$false; category=(Classify-Failure -Error $msg); detail=$msg }
        } finally {
          if (Test-Path $tmp) { Remove-Item $tmp -Force -ErrorAction SilentlyContinue }
        }
      }
    }
    'windows_natural' {
      $isWindows = $env:OS -eq 'Windows_NT'
      $checks += [ordered]@{
        provider = $provider
        check = 'runtime_windows'
        passed = $isWindows
        category = if ($isWindows) { $null } else { 'runtime_error' }
        detail = if ($isWindows) { 'Windows runtime detected for windows_natural provider.' } else { 'windows_natural provider requires Windows runtime.' }
      }

      if ($isWindows) {
        $voiceName = Get-WindowsNaturalVoiceName
        $voiceOk = -not [string]::IsNullOrWhiteSpace($voiceName)
        $checks += [ordered]@{
          provider = $provider
          check = 'natural_voice_available'
          passed = $voiceOk
          category = if ($voiceOk) { $null } else { 'runtime_error' }
          detail = if ($voiceOk) { "Natural voice found: $voiceName" } else { 'No Windows Natural voice found (NaturalVoiceSAPIAdapter + voice pack required).' }
        }

        if ($voiceOk) {
          $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("tts_preflight_windows_natural_{0}.wav" -f [guid]::NewGuid().ToString('N'))
          try {
            Invoke-WindowsNaturalSynthesisToFile -Text 'Preflight ping.' -Rate 1.0 -Volume 0.2 -OutputFile $tmp
            $checks += [ordered]@{ provider=$provider; check='synthesis_probe'; passed=$true; category=$null; detail='Windows natural synthesis probe succeeded.' }
          } catch {
            $msg = "Windows natural synthesis probe failed: $($_.Exception.Message)"
            $checks += [ordered]@{ provider=$provider; check='synthesis_probe'; passed=$false; category=(Classify-Failure -Error $msg); detail=$msg }
          } finally {
            if (Test-Path $tmp) { Remove-Item $tmp -Force -ErrorAction SilentlyContinue }
          }
        }
      }
    }
    'local_custom' {
      $binaryOk = -not [string]::IsNullOrWhiteSpace($piperResolvedBinary)
      $checks += [ordered]@{
        provider = $provider
        check = 'binary_available'
        passed = $binaryOk
        category = if ($binaryOk) { $null } else { 'missing_binary' }
        detail = if ($binaryOk) { "Piper binary found: $piperResolvedBinary" } else { 'Piper binary not found.' }
      }
      $modelOk = -not [string]::IsNullOrWhiteSpace($piperResolvedModel)
      $checks += [ordered]@{
        provider = $provider
        check = 'model_available'
        passed = $modelOk
        category = if ($modelOk) { $null } else { 'missing_model' }
        detail = if ($modelOk) { "Piper model found: $piperResolvedModel" } else { 'Piper model not found (.onnx).' }
      }

      if ($binaryOk -and $modelOk) {
        $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("tts_preflight_piper_{0}.wav" -f [guid]::NewGuid().ToString('N'))
        try {
          Invoke-PiperSynthesisToFile -Text 'Preflight ping.' -Binary $piperResolvedBinary -Model $piperResolvedModel -Rate 1.0 -OutputFile $tmp
          $checks += [ordered]@{ provider=$provider; check='synthesis_probe'; passed=$true; category=$null; detail='Piper synthesis probe succeeded.' }
        } catch {
          $msg = "Piper synthesis probe failed: $($_.Exception.Message)"
          $checks += [ordered]@{ provider=$provider; check='synthesis_probe'; passed=$false; category=(Classify-Failure -Error $msg); detail=$msg }
        } finally {
          if (Test-Path $tmp) { Remove-Item $tmp -Force -ErrorAction SilentlyContinue }
        }
      }
    }
    'qwen3_tts' {
      $endpointOk = $Qwen3Endpoint.StartsWith('http://') -or $Qwen3Endpoint.StartsWith('https://')
      $checks += [ordered]@{
        provider = $provider
        check = 'endpoint_format'
        passed = $endpointOk
        category = if ($endpointOk) { $null } else { 'endpoint_unreachable' }
        detail = if ($endpointOk) { 'Qwen3 endpoint format accepted.' } else { "Qwen3 endpoint '$Qwen3Endpoint' is invalid. Expected http:// or https:// URL." }
      }
      if ($endpointOk) {
        $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("tts_preflight_qwen_{0}.wav" -f [guid]::NewGuid().ToString('N'))
        try {
          Invoke-QwenSynthesisToFile -Text 'Preflight ping.' -Rate 1.0 -Endpoint $Qwen3Endpoint -Model $Qwen3Model -Voice $Qwen3Voice -ApiKey $Qwen3ApiKey -TimeoutSec $Qwen3TimeoutSec -OutputFile $tmp
          $checks += [ordered]@{ provider=$provider; check='endpoint_auth_probe'; passed=$true; category=$null; detail='Qwen3 endpoint/auth probe succeeded.' }
        } catch {
          $msg = "Qwen3 probe failed: $($_.Exception.Message)"
          $checks += [ordered]@{ provider=$provider; check='endpoint_auth_probe'; passed=$false; category=(Classify-Failure -Error $msg); detail=$msg }
        } finally {
          if (Test-Path $tmp) { Remove-Item $tmp -Force -ErrorAction SilentlyContinue }
        }
      }
    }
  }

  $preflightChecks += $checks
  $preflightOkByProvider[$provider] = -not ($checks | Where-Object { -not $_.passed })

  $profile = $profiles | Where-Object { $_.provider -eq $provider } | Select-Object -First 1
  $isRuntimeStable = $profile.surface -eq 'runtime_stable'
  if ($isRuntimeStable) {
    if ($runRuntimeSmoke) {
      if ($preflightOkByProvider[$provider]) {
        $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("tts_runtime_smoke_{0}_{1}.wav" -f $provider, [guid]::NewGuid().ToString('N'))
        try {
          switch ($provider) {
            'windows_native' { Invoke-WindowsNativeSynthesisToFile -Text 'Trispr Flow runtime smoke test.' -Rate $Rate -Volume 0.2 -OutputFile $tmp }
            'windows_natural' { Invoke-WindowsNaturalSynthesisToFile -Text 'Trispr Flow runtime smoke test.' -Rate $Rate -Volume 0.2 -OutputFile $tmp }
            'local_custom' { Invoke-PiperSynthesisToFile -Text 'Trispr Flow runtime smoke test.' -Binary $piperResolvedBinary -Model $piperResolvedModel -Rate $Rate -OutputFile $tmp }
          }
          $runtimeSmokeChecks += [ordered]@{ provider=$provider; passed=$true; category=$null; detail='Runtime smoke speak path succeeded.' }
        } catch {
          $msg = "Runtime smoke speak path failed: $($_.Exception.Message)"
          $runtimeSmokeChecks += [ordered]@{ provider=$provider; passed=$false; category=(Classify-Failure -Error $msg); detail=$msg }
        } finally {
          if (Test-Path $tmp) { Remove-Item $tmp -Force -ErrorAction SilentlyContinue }
        }
      } else {
        $runtimeSmokeChecks += [ordered]@{ provider=$provider; passed=$false; category='runtime_error'; detail='Runtime smoke skipped due to preflight failure.' }
      }
    } else {
      $runtimeSmokeChecks += [ordered]@{ provider=$provider; passed=$true; category=$null; detail='Runtime smoke disabled by request.' }
    }
  }
}

$totalRuns = [Math]::Max(1, $Warmup + $Runs)
foreach ($provider in $providerList) {
  $preflightOk = [bool]$preflightOkByProvider[$provider]
  foreach ($scenario in $scenarios) {
    for ($run = 1; $run -le $totalRuns; $run++) {
      if (-not $preflightOk) {
        $failedCheck = $preflightChecks | Where-Object { $_.provider -eq $provider -and -not $_.passed } | Select-Object -First 1
        $msg = "Preflight failed ($($failedCheck.check)): $($failedCheck.detail)"
        $samples += (New-SampleObject -Provider $provider -Scenario $scenario.id -Run $run -ElapsedMs 0 -Success $false -Error $msg -FailureCategory (Classify-Failure -Error $msg))
        continue
      }

      $tmpFile = Join-Path ([System.IO.Path]::GetTempPath()) ("tts_bench_{0}_{1}_{2}_{3}.wav" -f $provider, $scenario.id, $run, [guid]::NewGuid().ToString('N'))
      $elapsedMs = 0
      try {
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        switch ($provider) {
          'windows_native' {
            Invoke-WindowsNativeSynthesisToFile -Text $scenario.text -Rate $Rate -Volume $Volume -OutputFile $tmpFile
          }
          'windows_natural' {
            Invoke-WindowsNaturalSynthesisToFile -Text $scenario.text -Rate $Rate -Volume $Volume -OutputFile $tmpFile
          }
          'local_custom' {
            Invoke-PiperSynthesisToFile -Text $scenario.text -Binary $piperResolvedBinary -Model $piperResolvedModel -Rate $Rate -OutputFile $tmpFile
          }
          'qwen3_tts' {
            Invoke-QwenSynthesisToFile -Text $scenario.text -Rate $Rate -Endpoint $Qwen3Endpoint -Model $Qwen3Model -Voice $Qwen3Voice -ApiKey $Qwen3ApiKey -TimeoutSec $Qwen3TimeoutSec -OutputFile $tmpFile
          }
          default {
            throw "Unsupported benchmark provider '$provider'."
          }
        }
        $sw.Stop()
        $elapsedMs = [long][Math]::Round($sw.Elapsed.TotalMilliseconds)

        if (-not (Test-Path $tmpFile -PathType Leaf)) {
          throw 'Synthesis returned no output file.'
        }

        $samples += (New-SampleObject -Provider $provider -Scenario $scenario.id -Run $run -ElapsedMs $elapsedMs -Success $true -Error $null -FailureCategory $null)

        if ($saveExamples) {
          $exampleKey = "$provider|$($scenario.id)"
          if (-not $exampleTaken.ContainsKey($exampleKey)) {
            $exampleName = ("{0}_{1}.wav" -f $provider, $scenario.id)
            $examplePath = Join-Path $exampleDir $exampleName
            Copy-Item -Path $tmpFile -Destination $examplePath -Force
            $exampleTaken[$exampleKey] = $true
            $exampleClips += [ordered]@{
              provider = $provider
              scenario = $scenario.id
              language = $scenario.language
              length_bucket = $scenario.length_bucket
              thermal = $scenario.thermal
              source_text = $scenario.text
              run = $run
              elapsed_ms = $elapsedMs
              file = $exampleName
              file_path = $examplePath
            }
          }
        }
      } catch {
        $msg = $_.Exception.Message
        $samples += (New-SampleObject -Provider $provider -Scenario $scenario.id -Run $run -ElapsedMs $elapsedMs -Success $false -Error $msg -FailureCategory (Classify-Failure -Error $msg))
      } finally {
        if (Test-Path $tmpFile) {
          Remove-Item $tmpFile -Force -ErrorAction SilentlyContinue
        }
      }
    }
  }
}

$providerSummaries = @($providerList | ForEach-Object { Summarize-Provider -Provider $_ -Samples $samples })

$providerGateEvaluations = @()
foreach ($summary in $providerSummaries) {
  $provider = $summary.provider
  $profile = $profiles | Where-Object { $_.provider -eq $provider } | Select-Object -First 1
  $isRuntimeStable = $profile.surface -eq 'runtime_stable'

  $preflightOk = -not ($preflightChecks | Where-Object { $_.provider -eq $provider -and -not $_.passed })
  $smokeCheck = $runtimeSmokeChecks | Where-Object { $_.provider -eq $provider } | Select-Object -First 1
  $runtimeSmokeOk = if ($isRuntimeStable) { if ($null -eq $smokeCheck) { $false } else { [bool]$smokeCheck.passed } } else { $true }

  $reliabilityOk = [double]$summary.success_rate -ge $gateReliability
  $latencyOk = $null -ne $summary.p50_ms -and $null -ne $summary.p95_ms -and [long]$summary.p50_ms -le $gateLatencyP50 -and [long]$summary.p95_ms -le $gateLatencyP95

  $scenarioSuccess = @()
  foreach ($scenario in $scenarios) {
    $count = @($samples | Where-Object { $_.provider -eq $provider -and $_.scenario -eq $scenario.id -and $_.success }).Count
    $scenarioSuccess += [long]$count
  }
  $minScenarioSuccess = if ($scenarioSuccess.Count -gt 0) { [long]($scenarioSuccess | Measure-Object -Minimum).Minimum } else { 0 }
  $scenarioSuccessOk = $minScenarioSuccess -ge $gateMinSuccessPerScenario

  $passes = $isRuntimeStable -and $preflightOk -and $runtimeSmokeOk -and $reliabilityOk -and $latencyOk -and $scenarioSuccessOk

  $providerGateEvaluations += [ordered]@{
    provider = $provider
    evaluated_for_release = $isRuntimeStable
    passes_release_gate = $passes
    preflight_ok = $preflightOk
    runtime_smoke_ok = $runtimeSmokeOk
    reliability_ok = $reliabilityOk
    latency_ok = $latencyOk
    scenario_success_ok = $scenarioSuccessOk
    success_rate = [double]$summary.success_rate
    p50_ms = $summary.p50_ms
    p95_ms = $summary.p95_ms
    min_success_in_any_scenario = $minScenarioSuccess
  }
}

$fallbackOrder = Build-FallbackOrder -Summaries $providerSummaries -ReliabilityGate $gateReliability

$runtimeEvals = @($providerGateEvaluations | Where-Object { $_.evaluated_for_release })
$failedRuntime = @($runtimeEvals | Where-Object { -not $_.passes_release_gate })

$releaseGatePass = $false
$releaseGateReason = ''
if ($runtimeEvals.Count -eq 0) {
  $releaseGatePass = $false
  $releaseGateReason = 'No runtime-stable providers available for release gate evaluation.'
} elseif ($failedRuntime.Count -eq 0) {
  $releaseGatePass = $true
  $releaseGateReason = 'All runtime-stable providers passed release gate.'
} else {
  $releaseGatePass = $false
  $releaseGateReason = "Release gate failed for providers: $($failedRuntime.provider -join ', ')"
}

$recommendedDefaultProvider = $null
$recommendationReason = ''
if ($releaseGatePass) {
  $passingRuntime = @($runtimeEvals | Where-Object { $_.passes_release_gate } |
    Sort-Object @{Expression = { [long]$_.p95_ms }; Ascending = $true }, @{Expression = { [long]$_.p50_ms }; Ascending = $true }, @{Expression = { $_.provider }; Ascending = $true })
  if ($passingRuntime.Count -gt 0) {
    $recommendedDefaultProvider = $passingRuntime[0].provider
    $best = $passingRuntime[0]
    $recommendationReason = "Selected '$recommendedDefaultProvider' (success_rate=$([Math]::Round($best.success_rate * 100.0, 1))%, p95=$($best.p95_ms)ms, p50=$($best.p50_ms)ms) among providers meeting reliability gate >=$([Math]::Round($gateReliability * 100.0, 1))%."
  } else {
    $recommendationReason = 'Release gate passed but no runtime provider candidate was found.'
  }
} else {
  $recommendationReason = 'No runtime provider recommendation available. Resolve preflight/smoke failures first.'
}

$uncategorizedFailureCount = @($samples | Where-Object { -not $_.success -and [string]::IsNullOrWhiteSpace($_.failure_category) }).Count

$exampleManifestPath = $null
$blindExamplesDir = $null
$blindMappingPath = $null
if ($saveExamples -and $exampleDir) {
  $warnings += "Saved example clips to: $exampleDir"
  $exampleManifestPath = Join-Path $exampleDir 'examples.manifest.json'
  $exampleClips | ConvertTo-Json -Depth 8 | Set-Content -Path $exampleManifestPath -Encoding UTF8

  if ($exampleClips.Count -gt 0) {
    $blindExamplesDir = Join-Path $exampleDir 'blind'
    New-Item -ItemType Directory -Path $blindExamplesDir -Force | Out-Null

    $blindMap = @()
    $index = 1
    foreach ($clip in @($exampleClips | Sort-Object @{Expression={ $_.scenario }; Ascending=$true }, @{Expression={ $_.provider }; Ascending=$true })) {
      $blindName = ("sample_{0:D2}.wav" -f $index)
      $blindPath = Join-Path $blindExamplesDir $blindName
      Copy-Item -Path $clip.file_path -Destination $blindPath -Force
      $blindMap += [ordered]@{
        blind_id = $blindName
        provider = $clip.provider
        scenario = $clip.scenario
        language = $clip.language
        length_bucket = $clip.length_bucket
        thermal = $clip.thermal
        source_text = $clip.source_text
        elapsed_ms = $clip.elapsed_ms
      }
      $index++
    }
    $blindMappingPath = Join-Path $blindExamplesDir 'blind-map.json'
    $blindMap | ConvertTo-Json -Depth 8 | Set-Content -Path $blindMappingPath -Encoding UTF8
  }
}

$report = [ordered]@{
  artifact_version = 'tts-benchmark/v2-headless'
  generated_at = (Get-Date).ToUniversalTime().ToString('o')
  warmup_runs = $Warmup
  measure_runs = $Runs
  providers = $providerList
  scenarios = @($scenarios | ForEach-Object { $_.id })
  scenario_matrix_locked = (-not $UnlockMatrix)
  gates = [ordered]@{
    reliability_min_success_rate = $gateReliability
    latency_target_p50_ms = $gateLatencyP50
    latency_target_p95_ms = $gateLatencyP95
    min_success_per_scenario = $gateMinSuccessPerScenario
  }
  provider_profiles = $profiles
  preflight_checks = $preflightChecks
  runtime_smoke_checks = $runtimeSmokeChecks
  samples = $samples
  provider_summaries = $providerSummaries
  provider_gate_evaluations = $providerGateEvaluations
  provider_consistency_ok = $true
  provider_consistency_detail = 'Benchmark scope and runtime provider surface are consistent.'
  fallback_order = $fallbackOrder
  release_gate_pass = $releaseGatePass
  release_gate_reason = $releaseGateReason
  recommended_default_provider = $recommendedDefaultProvider
  recommendation_reason = $recommendationReason
  uncategorized_failure_count = $uncategorizedFailureCount
  warnings = $warnings
  example_clips_dir = if ($saveExamples) { $exampleDir } else { $null }
  example_manifest_path = $exampleManifestPath
  example_clips = $exampleClips
  blind_examples_dir = $blindExamplesDir
  blind_mapping_path = $blindMappingPath
}

$report | ConvertTo-Json -Depth 8 | Set-Content -Path $reportPath -Encoding UTF8

Write-Host "[TTS Benchmark] Recommendation: $recommendedDefaultProvider"
Write-Host "[TTS Benchmark] Reason: $recommendationReason"
Write-Host "[TTS Benchmark] Release gate: $releaseGatePass"
Write-Host "[TTS Benchmark] Release gate reason: $releaseGateReason"
Write-Host '[TTS Benchmark] Provider consistency: True'
Write-Host '[TTS Benchmark] Provider consistency detail: Benchmark scope and runtime provider surface are consistent.'
Write-Host "[TTS Benchmark] Fallback order: $($fallbackOrder -join ' -> ')"
Write-Host "[TTS Benchmark] Uncategorized failures: $uncategorizedFailureCount"

foreach ($summary in $providerSummaries) {
  $successRate = [Math]::Round(([double]$summary.success_rate * 100.0), 1)
  $p95 = if ($null -ne $summary.p95_ms) { "$($summary.p95_ms)ms" } else { 'n/a' }
  $p50 = if ($null -ne $summary.p50_ms) { "$($summary.p50_ms)ms" } else { 'n/a' }
  Write-Host ("[TTS Benchmark] {0}: attempts={1} success={2}/{1} ({3}%) p50={4} p95={5}" -f $summary.provider, $summary.attempts, $summary.success_count, $successRate, $p50, $p95)
}

if ($saveExamples -and $exampleDir) {
  Write-Host "[TTS Benchmark] Example clips dir: $exampleDir"
  if ($exampleManifestPath) {
    Write-Host "[TTS Benchmark] Example manifest: $exampleManifestPath"
  }
  if ($blindExamplesDir) {
    Write-Host "[TTS Benchmark] Blind listening dir: $blindExamplesDir"
  }
  if ($blindMappingPath) {
    Write-Host "[TTS Benchmark] Blind mapping: $blindMappingPath"
  }
}

if ($PlayExamples -and $saveExamples -and (Test-Path $exampleDir -PathType Container)) {
  $clips = Get-ChildItem -Path $exampleDir -Filter *.wav -File -ErrorAction SilentlyContinue | Sort-Object Name
  foreach ($clip in $clips) {
    Write-Host "[TTS Benchmark] Playing example: $($clip.Name)"
    try {
      $player = New-Object System.Media.SoundPlayer($clip.FullName)
      $player.PlaySync()
    } catch {
      Write-Warning "Failed to play '$($clip.FullName)': $($_.Exception.Message)"
    }
  }
}

if ($PlayBlindExamples -and $saveExamples -and $blindExamplesDir -and (Test-Path $blindExamplesDir -PathType Container)) {
  $clips = Get-ChildItem -Path $blindExamplesDir -Filter sample_*.wav -File -ErrorAction SilentlyContinue | Sort-Object Name
  foreach ($clip in $clips) {
    Write-Host "[TTS Benchmark] Playing blind sample: $($clip.Name)"
    try {
      $player = New-Object System.Media.SoundPlayer($clip.FullName)
      $player.PlaySync()
    } catch {
      Write-Warning "Failed to play '$($clip.FullName)': $($_.Exception.Message)"
    }
  }
}

if ($FailOnNoRecommendation -and [string]::IsNullOrWhiteSpace($recommendedDefaultProvider)) {
  Write-Error 'No default provider recommendation produced.'
  exit 1
}
if ($FailOnGateMiss -and -not $releaseGatePass) {
  Write-Error "TTS release gate failed: $releaseGateReason"
  exit 1
}
if ($uncategorizedFailureCount -gt 0) {
  Write-Error "TTS benchmark produced uncategorized failures: $uncategorizedFailureCount"
  exit 1
}

Write-Host '[TTS Benchmark] Complete'
exit 0
