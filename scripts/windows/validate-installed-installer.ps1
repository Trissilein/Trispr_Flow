#Requires -Version 5.1
param(
  [Alias("ExePath")]
  [Parameter(Mandatory = $true)]
  [string]$InstallerPath,
  [string]$ManifestPath = "src-tauri\runtime-manifests\vulkan-v0.8.4-hotfix.json",
  [string]$InstallRoot = "",
  [string]$ModelPath = "",
  [string]$FixturePath = "bench\fixtures\short\short_de_like.wav",
  [int]$Port = 8178,
  [switch]$RequireSmoke
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..\..")).Path
$InstallerFullPath = if ([System.IO.Path]::IsPathRooted($InstallerPath)) { $InstallerPath } else { Join-Path $RepoRoot $InstallerPath }
$ManifestFullPath = if ([System.IO.Path]::IsPathRooted($ManifestPath)) { $ManifestPath } else { Join-Path $RepoRoot $ManifestPath }
$FixtureFullPath = if ([System.IO.Path]::IsPathRooted($FixturePath)) { $FixturePath } else { Join-Path $RepoRoot $FixturePath }
if ([string]::IsNullOrWhiteSpace($InstallRoot)) {
  $InstallRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("trispr-flow-installer-validate-{0}" -f [guid]::NewGuid().ToString("N"))
}
$InstallRoot = [System.IO.Path]::GetFullPath($InstallRoot)
if ([string]::IsNullOrWhiteSpace($ModelPath)) {
  $ModelPath = Join-Path $env:LOCALAPPDATA "Trispr Flow\models\ggml-large-v3-turbo.bin"
}

$ServerProcess = $null
$BackupDir = Join-Path ([System.IO.Path]::GetTempPath()) ("trispr-flow-installer-state-{0}" -f [guid]::NewGuid().ToString("N"))

function Write-Section([string]$Text) {
  Write-Host "`n== $Text ==" -ForegroundColor Cyan
}

function Get-TrisprUninstallEntries {
  $roots = @(
    "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall",
    "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall",
    "HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"
  )
  foreach ($root in $roots) {
    if (-not (Test-Path $root)) {
      continue
    }
    Get-ChildItem $root -ErrorAction SilentlyContinue | ForEach-Object {
      $props = Get-ItemProperty $_.PsPath -ErrorAction SilentlyContinue
      $displayName = ""
      if ($props -and ($props.PSObject.Properties.Name -contains "DisplayName")) {
        $displayName = [string]$props.DisplayName
      }
      if ($displayName -eq "Trispr Flow") {
        $table = @{}
        foreach ($property in $props.PSObject.Properties) {
          if ($property.Name -notlike "PS*") {
            $table[$property.Name] = $property.Value
          }
        }
        [PSCustomObject]@{ Path = $_.PsPath; Properties = $table }
      }
    }
  }
}

function Restore-UninstallEntries($Entries) {
  foreach ($entry in $Entries) {
    New-Item -Path $entry.Path -Force | Out-Null
    foreach ($name in $entry.Properties.Keys) {
      Set-ItemProperty -Path $entry.Path -Name $name -Value $entry.Properties[$name] -Force
    }
  }
}

function Test-EntryPointsAtInstallRoot($Entry) {
  $installLocation = ""
  $uninstallString = ""
  if ($Entry.Properties.ContainsKey("InstallLocation")) {
    $installLocation = [string]$Entry.Properties["InstallLocation"]
  }
  if ($Entry.Properties.ContainsKey("UninstallString")) {
    $uninstallString = [string]$Entry.Properties["UninstallString"]
  }
  return $installLocation.Contains($InstallRoot) -or $uninstallString.Contains($InstallRoot)
}

function Backup-Shortcuts {
  New-Item -ItemType Directory -Force -Path $BackupDir | Out-Null
  $paths = @(
    (Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\Trispr Flow.lnk"),
    (Join-Path $env:ProgramData "Microsoft\Windows\Start Menu\Programs\Trispr Flow.lnk")
  )
  foreach ($path in $paths) {
    if (Test-Path -LiteralPath $path -PathType Leaf) {
      $encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($path))
      Copy-Item -LiteralPath $path -Destination (Join-Path $BackupDir $encoded) -Force
    }
  }
}

function Restore-Shortcuts {
  $paths = @(
    (Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\Trispr Flow.lnk"),
    (Join-Path $env:ProgramData "Microsoft\Windows\Start Menu\Programs\Trispr Flow.lnk")
  )
  $shell = New-Object -ComObject WScript.Shell
  foreach ($path in $paths) {
    $encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($path))
    $backup = Join-Path $BackupDir $encoded
    if (Test-Path -LiteralPath $backup -PathType Leaf) {
      New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path) | Out-Null
      Copy-Item -LiteralPath $backup -Destination $path -Force
      continue
    }
    if (Test-Path -LiteralPath $path -PathType Leaf) {
      $shortcut = $shell.CreateShortcut($path)
      if ($shortcut.TargetPath -and $shortcut.TargetPath.Contains($InstallRoot)) {
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
      }
    }
  }
}

function Invoke-ManifestValidation([string]$Root, [string]$Label) {
  & node (Join-Path $RepoRoot "scripts\validate-runtime-manifest.mjs") --manifest $ManifestFullPath --root $Root --label $Label
  if ($LASTEXITCODE -ne 0) {
    throw "$Label failed manifest validation."
  }
}

function ConvertTo-ProcessArgument([string]$Value) {
  return '"' + $Value.Replace('"', '\"') + '"'
}

function Invoke-CliSmoke([string]$VulkanRoot) {
  if (-not (Test-Path -LiteralPath $ModelPath -PathType Leaf) -or -not (Test-Path -LiteralPath $FixtureFullPath -PathType Leaf)) {
    $message = "Skipping CLI/server smoke because model or fixture is unavailable. model=$ModelPath fixture=$FixtureFullPath"
    if ($RequireSmoke) {
      throw $message
    }
    Write-Warning $message
    return
  }

  Write-Section "Running installed whisper-cli smoke"
  $cliPath = Join-Path $VulkanRoot "whisper-cli.exe"
  $outputBase = Join-Path ([System.IO.Path]::GetTempPath()) ("trispr-installed-cli-smoke-{0}" -f [guid]::NewGuid().ToString("N"))
  & $cliPath -m $ModelPath -f $FixtureFullPath -t 8 -l auto -nt -otxt -of $outputBase -np -dev 0
  if ($LASTEXITCODE -ne 0) {
    throw "Installed whisper-cli smoke failed with exit code $LASTEXITCODE."
  }
  Remove-Item -LiteralPath ($outputBase + ".txt") -Force -ErrorAction SilentlyContinue

  Write-Section "Running installed whisper-server smoke"
  $serverPath = Join-Path $VulkanRoot "whisper-server.exe"
  $stdoutPath = Join-Path ([System.IO.Path]::GetTempPath()) ("trispr-server-smoke-{0}.out.log" -f [guid]::NewGuid().ToString("N"))
  $stderrPath = Join-Path ([System.IO.Path]::GetTempPath()) ("trispr-server-smoke-{0}.err.log" -f [guid]::NewGuid().ToString("N"))
  $serverArgs = "-m {0} --host 127.0.0.1 --port {1} -t 8" -f (ConvertTo-ProcessArgument $ModelPath), $Port
  $script:ServerProcess = Start-Process -FilePath $serverPath -ArgumentList $serverArgs -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath -PassThru
  $deadline = [DateTime]::UtcNow.AddSeconds(45)
  while ([DateTime]::UtcNow -lt $deadline) {
    if ($script:ServerProcess.HasExited) {
      $stdout = if (Test-Path $stdoutPath) { Get-Content -LiteralPath $stdoutPath -Raw } else { "" }
      $stderr = if (Test-Path $stderrPath) { Get-Content -LiteralPath $stderrPath -Raw } else { "" }
      throw "whisper-server exited before readiness. exit=$($script:ServerProcess.ExitCode) stdout=$stdout stderr=$stderr"
    }
    try {
      $response = Invoke-WebRequest -Uri ("http://127.0.0.1:{0}/" -f $Port) -UseBasicParsing -TimeoutSec 2
      if ($response.StatusCode -ge 200) {
        Write-Host ("  OK: whisper-server responded with HTTP {0}." -f $response.StatusCode)
        return
      }
    } catch {
      [System.Threading.Thread]::Sleep(500)
    }
  }
  throw "whisper-server did not respond on port $Port within 45 seconds."
}

if (-not (Test-Path -LiteralPath $InstallerFullPath -PathType Leaf)) {
  throw "Installer not found: $InstallerFullPath"
}
if (-not (Test-Path -LiteralPath $ManifestFullPath -PathType Leaf)) {
  throw "Manifest not found: $ManifestFullPath"
}

$beforeRegistry = @(Get-TrisprUninstallEntries)
Backup-Shortcuts

try {
  if (Test-Path -LiteralPath $InstallRoot) {
    Remove-Item -LiteralPath $InstallRoot -Recurse -Force
  }

  Write-Section "Installing into validation root"
  Write-Host "Installer: $InstallerFullPath"
  Write-Host "Install : $InstallRoot"
  $installerProcess = Start-Process -FilePath $InstallerFullPath -ArgumentList @("/S", ("/D=" + $InstallRoot)) -Wait -PassThru
  if ($installerProcess.ExitCode -ne 0) {
    throw "Installer exited with code $($installerProcess.ExitCode)."
  }

  $vulkanRoot = Join-Path $InstallRoot "bin\vulkan"
  Write-Section "Validating installed Vulkan payload"
  Invoke-ManifestValidation -Root $vulkanRoot -Label "installed-vulkan"
  Invoke-CliSmoke -VulkanRoot $vulkanRoot

  Write-Host "`nInstalled installer validation completed successfully." -ForegroundColor Green
} finally {
  if ($script:ServerProcess -and -not $script:ServerProcess.HasExited) {
    Stop-Process -Id $script:ServerProcess.Id -Force -ErrorAction SilentlyContinue
  }
  $currentRegistry = @(Get-TrisprUninstallEntries)
  foreach ($entry in $currentRegistry) {
    if (Test-EntryPointsAtInstallRoot $entry) {
      Remove-Item -Path $entry.Path -Recurse -Force -ErrorAction SilentlyContinue
    }
  }
  Restore-UninstallEntries $beforeRegistry
  Restore-Shortcuts
  if (Test-Path -LiteralPath $InstallRoot) {
    Remove-Item -LiteralPath $InstallRoot -Recurse -Force -ErrorAction SilentlyContinue
  }
  if (Test-Path -LiteralPath $BackupDir) {
    Remove-Item -LiteralPath $BackupDir -Recurse -Force -ErrorAction SilentlyContinue
  }
}