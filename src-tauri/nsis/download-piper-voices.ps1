#Requires -Version 5.1
param(
    [string]$SelectedFile,
    [string]$ExtraFile,
    [string]$VoicesDir,
    [string]$InvalidOut,
    [string]$FailedOut
)

$ErrorActionPreference = 'Stop'

$hfBase = 'https://huggingface.co/rhasspy/piper-voices/resolve/main'
$keyRegex = '^[a-z]{2}_[A-Z]{2}-[a-z0-9_]+-(x_low|low|medium|high)$'

function Test-NonEmptyFile {
    param([string]$Path)
    if (-not (Test-Path $Path)) { return $false }
    return (Get-Item $Path).Length -gt 0
}

function Add-KeyLines {
    param(
        [string]$Path,
        [System.Collections.Generic.HashSet[string]]$Seen,
        [System.Collections.Generic.List[string]]$Out
    )

    if (-not $Path -or -not (Test-Path $Path)) {
        return
    }

    foreach ($line in (Get-Content -Path $Path -ErrorAction SilentlyContinue)) {
        $key = ($line ?? '').Trim()
        if ([string]::IsNullOrWhiteSpace($key)) {
            continue
        }
        if ($Seen.Add($key)) {
            [void]$Out.Add($key)
        }
    }
}

if (-not (Test-Path $VoicesDir)) {
    New-Item -Path $VoicesDir -ItemType Directory -Force | Out-Null
}

$seenKeys = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
$mergedKeys = New-Object 'System.Collections.Generic.List[string]'

Add-KeyLines -Path $SelectedFile -Seen $seenKeys -Out $mergedKeys
Add-KeyLines -Path $ExtraFile -Seen $seenKeys -Out $mergedKeys

$invalidKeys = New-Object 'System.Collections.Generic.List[string]'
$failedKeys = New-Object 'System.Collections.Generic.List[string]'

foreach ($key in $mergedKeys) {
    if ($key -notmatch $keyRegex) {
        [void]$invalidKeys.Add($key)
        continue
    }

    $parts = $key.Split('-')
    if ($parts.Count -ne 3) {
        [void]$invalidKeys.Add($key)
        continue
    }

    $locale = $parts[0]
    $voice = $parts[1]
    $quality = $parts[2]
    $lang = ($locale.Split('_')[0]).ToLowerInvariant()
    $hfPath = "$lang/$locale/$voice/$quality/$key"

    $onnxPath = Join-Path $VoicesDir "$key.onnx"
    $jsonPath = Join-Path $VoicesDir "$key.onnx.json"

    if ((Test-NonEmptyFile -Path $onnxPath) -and (Test-NonEmptyFile -Path $jsonPath)) {
        continue
    }

    $onnxUrl = "$hfBase/$hfPath.onnx?download=true"
    $jsonUrl = "$hfBase/$hfPath.onnx.json?download=true"

    try {
        if (-not (Test-NonEmptyFile -Path $onnxPath)) {
            Invoke-WebRequest -Uri $onnxUrl -OutFile $onnxPath -UseBasicParsing
        }
        if (-not (Test-NonEmptyFile -Path $jsonPath)) {
            Invoke-WebRequest -Uri $jsonUrl -OutFile $jsonPath -UseBasicParsing
        }
    }
    catch {
        [void]$failedKeys.Add($key)
        if (-not (Test-NonEmptyFile -Path $onnxPath)) {
            Remove-Item -Path $onnxPath -Force -ErrorAction SilentlyContinue
        }
        if (-not (Test-NonEmptyFile -Path $jsonPath)) {
            Remove-Item -Path $jsonPath -Force -ErrorAction SilentlyContinue
        }
    }
}

if ($InvalidOut) {
    if ($invalidKeys.Count -gt 0) {
        Set-Content -Path $InvalidOut -Value ($invalidKeys -join [Environment]::NewLine) -Encoding UTF8
    }
    else {
        Remove-Item -Path $InvalidOut -Force -ErrorAction SilentlyContinue
    }
}

if ($FailedOut) {
    if ($failedKeys.Count -gt 0) {
        Set-Content -Path $FailedOut -Value ($failedKeys -join [Environment]::NewLine) -Encoding UTF8
    }
    else {
        Remove-Item -Path $FailedOut -Force -ErrorAction SilentlyContinue
    }
}

exit 0
