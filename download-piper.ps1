$PiperDir = 'C:\GIT\Trispr_Flow\src-tauri\bin\piper'
$VoicesDir = Join-Path $PiperDir 'voices'
New-Item -ItemType Directory -Force -Path $PiperDir, $VoicesDir | Out-Null

Write-Host '== Downloading Piper binary =='
$ZipUrl = 'https://github.com/rhasspy/piper/releases/download/v1.2.0/piper_windows_amd64.zip'
$ZipPath = Join-Path $env:TEMP 'piper.zip'
Write-Host 'Downloading...'
Invoke-WebRequest -Uri $ZipUrl -OutFile $ZipPath -UseBasicParsing -ErrorAction Stop
Write-Host 'Extracting...'
Expand-Archive -Path $ZipPath -DestinationPath $env:TEMP -Force
$ExtractedPiper = Join-Path $env:TEMP 'piper'
Copy-Item (Join-Path $ExtractedPiper 'piper.exe') $PiperDir -Force
Copy-Item (Join-Path $ExtractedPiper 'onnxruntime.dll') $PiperDir -Force
Copy-Item (Join-Path $ExtractedPiper 'onnxruntime_providers_shared.dll') $PiperDir -Force
Copy-Item -Path (Join-Path $ExtractedPiper 'espeak-ng-data') -Destination $PiperDir -Recurse -Force
Write-Host 'OK: Piper binary extracted'

Write-Host ''
Write-Host '== Downloading voice models =='
$HfBase = 'https://huggingface.co/rhasspy/piper-voices/resolve/main'

@(
  @{Name='de_DE-thorsten-medium'; Path='de/de_DE/thorsten/medium'},
  @{Name='en_US-amy-medium'; Path='en/en_US/amy/medium'}
) | ForEach-Object {
  $name = $_.Name
  $path = $_.Path
  $onnxUrl = "$HfBase/$path/$name.onnx?download=true"
  $jsonUrl = "$HfBase/$path/$name.onnx.json?download=true"

  Write-Host "Downloading $name.onnx..."
  Invoke-WebRequest -Uri $onnxUrl -OutFile (Join-Path $VoicesDir "$name.onnx") -UseBasicParsing
  Invoke-WebRequest -Uri $jsonUrl -OutFile (Join-Path $VoicesDir "$name.onnx.json") -UseBasicParsing
  Write-Host "OK: $name"
}

Write-Host ''
Write-Host 'Piper setup complete!'
ls $VoicesDir
