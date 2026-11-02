# Creates the two Trispr Flow desktop shortcuts. Safe to re-run: existing ones are overwritten.
#   "Trispr Flow"     quick start: last debug build at once, rebuild in the background
#   "Trispr Flow Dev" npm run dev (hot reload), exits quietly when the app already runs
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File scripts\create-desktop-shortcuts.ps1

$repo = Split-Path -Parent $PSScriptRoot
$desktop = [Environment]::GetFolderPath('Desktop')
$icon = Join-Path $repo 'src-tauri\icons\icon.ico'
$wsh = New-Object -ComObject WScript.Shell

function New-TrisprShortcut([string]$name, [string]$script) {
    $path = Join-Path $desktop "$name.lnk"
    $lnk = $wsh.CreateShortcut($path)
    $lnk.TargetPath = Join-Path $env:WINDIR 'System32\wscript.exe'
    $lnk.Arguments = '"' + (Join-Path $repo $script) + '"'
    $lnk.WorkingDirectory = $repo
    $lnk.IconLocation = "$icon,0"
    $lnk.Description = $name
    $lnk.Save()

    # Read back from disk to show what was really saved.
    $saved = $wsh.CreateShortcut($path)
    "{0}`n  Target    {1}`n  Arguments {2}`n  WorkDir   {3}`n  Icon      {4}" -f $path, $saved.TargetPath, $saved.Arguments, $saved.WorkingDirectory, $saved.IconLocation
}

New-TrisprShortcut 'Trispr Flow' 'scripts\launch-quick.vbs'
New-TrisprShortcut 'Trispr Flow Dev' 'start-trispr-flow.vbs'
