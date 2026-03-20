$PiperDir = 'C:\GIT\Trispr_Flow\src-tauri\bin\piper'
New-Item -ItemType Directory -Force -Path $PiperDir | Out-Null

Write-Host '== Downloading Piper binary =='
$ZipUrl = 'https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip'
$ZipPath = Join-Path $env:TEMP 'piper.zip'
Write-Host 'Downloading from: ' $ZipUrl
Invoke-WebRequest -Uri $ZipUrl -OutFile $ZipPath -UseBasicParsing

Write-Host 'Extracting...'
$ExtractDir = Join-Path $env:TEMP 'piper_extract'
if (Test-Path $ExtractDir) { Remove-Item -Recurse -Force $ExtractDir }
Expand-Archive -Path $ZipPath -DestinationPath $ExtractDir -Force

Write-Host 'Copying files...'
$PiperSource = Join-Path $ExtractDir 'piper'
Copy-Item (Join-Path $PiperSource 'piper.exe') $PiperDir -Force
Copy-Item (Join-Path $PiperSource 'onnxruntime.dll') $PiperDir -Force
Copy-Item (Join-Path $PiperSource 'onnxruntime_providers_shared.dll') $PiperDir -Force
Copy-Item -Path (Join-Path $PiperSource 'espeak-ng-data') -Destination $PiperDir -Recurse -Force

Write-Host 'OK: Piper setup complete!'
ls (Join-Path $PiperDir 'piper.exe')
