; Trispr Flow NSIS Installer Hooks
; Wizard: Hardware/Variant info, Component selection, First-run config, Summary.
; This file is included at the top level of the installer script by Tauri.

!include "nsDialogs.nsh"
!include "LogicLib.nsh"

; Compile-time variant define injected by build-installers.bat
; Fallback to "vulkan" if building outside the standard pipeline
!ifndef TRISPR_VARIANT
  !define TRISPR_VARIANT "vulkan"
!endif

; =====================================================================
; Variable declarations
; =====================================================================

; Page 1: Hardware/Variant info
Var HardwareDialog
Var DetectedGpuStr

; Page 2: Components
Var ComponentsDialog
Var CheckFFmpeg
Var CheckPiperRuntime
Var RadioVoiceDE
Var RadioVoiceEN
Var RadioVoiceBoth
Var ComponentFFmpegSelected
Var ComponentPiperSelected
Var ComponentVoiceChoice

; Page 3: First-run config
Var FirstRunDialog
Var CheckAIRefinement
Var RadioHal
Var RadioKitt
Var RadioPtt
Var RadioVad
Var RadioTtsNatural
Var RadioTtsSapi
Var AIRefinementSelected
Var OverlayStyleChoice
Var CaptureModeChoice
Var TtsProviderChoice

; Page 4: Summary
Var SummaryDialog
Var SummaryLabel

; =====================================================================
; Custom page declarations (inserted between Tauri's reinstall page
; and the directory selector page)
; =====================================================================
Page custom HardwareVariantPage HardwareVariantPageLeave
Page custom ComponentsPage ComponentsPageLeave
Page custom FirstRunConfigPage FirstRunConfigPageLeave
Page custom HardwareSummaryPage

; =====================================================================
; Page 1: Hardware / Variant Info
; =====================================================================

Function HardwareVariantPage
  ; Detect NVIDIA GPU via registry
  StrCpy $DetectedGpuStr "Keine NVIDIA GPU erkannt"
  StrCpy $0 0
  GpuScanLoop:
    IntFmt $1 "%04i" $0
    ReadRegStr $2 HKLM "SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\$1" "DriverDesc"
    ${If} $2 == ""
      IntCmp $0 9 GpuScanDone GpuScanDone +1
      IntOp $0 $0 + 1
      Goto GpuScanLoop
    ${EndIf}
    ${If} $2 != ""
      StrCpy $3 $2 6
      ${If} $3 == "NVIDIA"
        StrCpy $DetectedGpuStr "NVIDIA: $2"
        Goto GpuScanDone
      ${EndIf}
    ${EndIf}
    IntOp $0 $0 + 1
    IntCmp $0 10 GpuScanDone GpuScanLoop GpuScanDone
  GpuScanDone:

  nsDialogs::Create 1018
  Pop $HardwareDialog
  ${If} $HardwareDialog == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 20u "System-Erkennung"
  Pop $0
  SetCtlColors $0 "" 0xffffff
  CreateFont $1 "Segoe UI" 11 700
  SendMessage $0 ${WM_SETFONT} $1 1

  ${NSD_CreateLabel} 0 26u 100% 14u "Erkannte GPU:"
  Pop $0
  ${NSD_CreateLabel} 0 40u 100% 14u "$DetectedGpuStr"
  Pop $0

  ${NSD_CreateLabel} 0 62u 100% 14u "Diese Installer-Variante:"
  Pop $0
  ${NSD_CreateLabel} 0 76u 100% 14u "${TRISPR_VARIANT}"
  Pop $0

  ${NSD_CreateLabel} 0 100u 100% 30u "Im nächsten Schritt kannst du auswählen, welche zusätzlichen Komponenten heruntergeladen werden sollen."
  Pop $0

  nsDialogs::Show
FunctionEnd

Function HardwareVariantPageLeave
  ; Nothing to capture — informational page only
FunctionEnd

; =====================================================================
; Page 2: Component Selection
; =====================================================================

Function ComponentsPage
  nsDialogs::Create 1018
  Pop $ComponentsDialog
  ${If} $ComponentsDialog == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 20u "Komponenten"
  Pop $0
  CreateFont $1 "Segoe UI" 11 700
  SendMessage $0 ${WM_SETFONT} $1 1

!if "${TRISPR_VARIANT}" == "cuda-complete"
  ${NSD_CreateLabel} 0 26u 100% 20u "Offline-Variante: FFmpeg und Piper sind bereits enthalten. Kein Download nötig."
  Pop $0
!else
  ${NSD_CreateLabel} 0 26u 100% 14u "Audio-Encoder (wird heruntergeladen bei Installation):"
  Pop $0

  ${NSD_CreateCheckbox} 10u 42u 100% 14u "FFmpeg ~84 MB — benötigt für OPUS-Aufnahme"
  Pop $CheckFFmpeg
  ${NSD_SetState} $CheckFFmpeg ${BST_CHECKED}

  ${NSD_CreateLabel} 0 62u 100% 14u "Sprachausgabe:"
  Pop $0

  ${NSD_CreateRadioButton} 10u 78u 100% 13u "Windows Natural Voice (empfohlen)"
  Pop $RadioTtsNatural
  ${NSD_AddStyle} $RadioTtsNatural ${WS_GROUP}
  ${NSD_SetState} $RadioTtsNatural ${BST_CHECKED}

  ${NSD_CreateRadioButton} 10u 93u 100% 13u "Windows Sprachausgabe (SAPI, immer verfügbar)"
  Pop $RadioTtsSapi

  ${NSD_CreateCheckbox} 10u 112u 100% 13u "Piper KI-Stimme (lokal, ~28 MB Download)"
  Pop $CheckPiperRuntime
!endif

  nsDialogs::Show
FunctionEnd

Function ComponentsPageLeave
!if "${TRISPR_VARIANT}" == "cuda-complete"
  ; All components bundled in offline installer
  StrCpy $ComponentFFmpegSelected "0"
  StrCpy $ComponentPiperSelected "0"
  StrCpy $TtsProviderChoice "native"
!else
  ${NSD_GetState} $CheckFFmpeg $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $ComponentFFmpegSelected "1"
  ${Else}
    StrCpy $ComponentFFmpegSelected "0"
  ${EndIf}

  ${NSD_GetState} $CheckPiperRuntime $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $ComponentPiperSelected "1"
  ${Else}
    StrCpy $ComponentPiperSelected "0"
  ${EndIf}

  ; TTS Provider selection
  ${NSD_GetState} $RadioTtsNatural $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $TtsProviderChoice "natural"
  ${Else}
    StrCpy $TtsProviderChoice "native"
  ${EndIf}
!endif
FunctionEnd

; =====================================================================
; Page 3: First-run Configuration
; =====================================================================

Function FirstRunConfigPage
  nsDialogs::Create 1018
  Pop $FirstRunDialog
  ${If} $FirstRunDialog == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 20u "Startverhalten"
  Pop $0
  CreateFont $1 "Segoe UI" 11 700
  SendMessage $0 ${WM_SETFONT} $1 1

  ${NSD_CreateLabel} 0 26u 100% 12u "KI-Verfeinerung (benötigt lokales Ollama):"
  Pop $0

  ${NSD_CreateCheckbox} 10u 40u 100% 13u "KI-Verfeinerung direkt aktivieren"
  Pop $CheckAIRefinement
  ; Default: unchecked (Ollama must be installed separately)

  ${NSD_CreateLabel} 0 62u 100% 12u "Overlay-Stil (jederzeit in den App-Einstellungen änderbar):"
  Pop $0

  ${NSD_CreateRadioButton} 10u 76u 100% 13u "HAL 9000 — pulsierender Kreis"
  Pop $RadioHal
  ${NSD_SetState} $RadioHal ${BST_CHECKED}

  ${NSD_CreateRadioButton} 10u 91u 100% 13u "KITT — Leuchtbalken"
  Pop $RadioKitt

  ${NSD_CreateLabel} 0 112u 100% 12u "Aufnahme-Modus:"
  Pop $0

  ${NSD_CreateRadioButton} 10u 126u 100% 13u "Push-to-Talk (PTT) — empfohlen"
  Pop $RadioPtt
  ${NSD_AddStyle} $RadioPtt ${WS_GROUP}
  ${NSD_SetState} $RadioPtt ${BST_CHECKED}

  ${NSD_CreateRadioButton} 10u 141u 100% 13u "Spracherkennung (VAD) — automatisch"
  Pop $RadioVad

  nsDialogs::Show
FunctionEnd

Function FirstRunConfigPageLeave
  ${NSD_GetState} $CheckAIRefinement $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $AIRefinementSelected "1"
  ${Else}
    StrCpy $AIRefinementSelected "0"
  ${EndIf}

  ${NSD_GetState} $RadioHal $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $OverlayStyleChoice "dot"
  ${Else}
    StrCpy $OverlayStyleChoice "kitt"
  ${EndIf}

  ${NSD_GetState} $RadioPtt $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $CaptureModeChoice "ptt"
  ${Else}
    StrCpy $CaptureModeChoice "vad"
  ${EndIf}
FunctionEnd

; =====================================================================
; Page 4: Summary (read-only confirmation)
; =====================================================================

Function HardwareSummaryPage
  nsDialogs::Create 1018
  Pop $SummaryDialog
  ${If} $SummaryDialog == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 20u "Zusammenfassung"
  Pop $0
  CreateFont $1 "Segoe UI" 11 700
  SendMessage $0 ${WM_SETFONT} $1 1

  StrCpy $R0 "GPU:$\t$\t$DetectedGpuStr$\r$\nVariante:$\t${TRISPR_VARIANT}$\r$\n"

!if "${TRISPR_VARIANT}" == "cuda-complete"
  StrCpy $R0 "$R0FFmpeg:$\t$\tEnthalten (Offline)$\r$\n"
  StrCpy $R0 "$R0Piper TTS:$\t$\tEnthalten (Offline)$\r$\n"
!else
  ${If} $ComponentFFmpegSelected == "1"
    StrCpy $R0 "$R0FFmpeg:$\t$\tDownload ~84 MB$\r$\n"
  ${Else}
    StrCpy $R0 "$R0FFmpeg:$\t$\tNein$\r$\n"
  ${EndIf}
  ${If} $ComponentPiperSelected == "1"
    StrCpy $R0 "$R0Piper TTS:$\t$\tDownload ~28 MB$\r$\n"
    StrCpy $R0 "$R0Stimme:$\t$\t$ComponentVoiceChoice (Download beim 1. TTS-Aufruf)$\r$\n"
  ${Else}
    StrCpy $R0 "$R0Piper TTS:$\t$\tNein$\r$\n"
  ${EndIf}
!endif

  ${If} $AIRefinementSelected == "1"
    StrCpy $R0 "$R0KI-Verfeinerung: Aktiviert (Ollama benötigt)$\r$\n"
  ${Else}
    StrCpy $R0 "$R0KI-Verfeinerung: Deaktiviert$\r$\n"
  ${EndIf}
  StrCpy $R0 "$R0Overlay:$\t$\t$OverlayStyleChoice$\r$\nAufnahme:$\t$\t$CaptureModeChoice$\r$\n"

  ; TTS summary line
!if "${TRISPR_VARIANT}" != "cuda-complete"
  ${If} $TtsProviderChoice == "natural"
    StrCpy $R0 "$R0Sprachausgabe:$\t$\tWindows Natural Voice"
  ${Else}
    StrCpy $R0 "$R0Sprachausgabe:$\t$\tWindows SAPI"
  ${EndIf}
  ${If} $ComponentPiperSelected == "1"
    StrCpy $R0 "$R0 (+ Piper Fallback)"
  ${EndIf}
!endif

  ${NSD_CreateLabel} 0 26u 100% 130u "$R0"
  Pop $SummaryLabel

  ${NSD_CreateLabel} 0 160u 100% 20u "Klick auf 'Installieren' zum Starten."
  Pop $0

  ${NSD_CreateLabel} 0 185u 100% 14u "Logs findest du später unter: %LOCALAPPDATA%\Trispr Flow\logs\"
  Pop $0

  nsDialogs::Show
FunctionEnd

; =====================================================================
; Post-install: write settings and on-demand downloads
; =====================================================================

!macro NSIS_HOOK_POSTINSTALL
  ; Resolve %LOCALAPPDATA% — matches Rust paths.rs which uses LOCALAPPDATA\Trispr Flow
  ExpandEnvStrings $R9 "%LOCALAPPDATA%"
  StrCpy $R8 "$R9\Trispr Flow"

  CreateDirectory "$R8"

  ; Only write settings.json on fresh install (preserve existing user config on upgrade)
  IfFileExists "$R8\settings.json" SettingsExists

  FileOpen $0 "$R8\settings.json" w
  FileWrite $0 '{'
  FileWrite $0 '"mode":"$CaptureModeChoice",'
  FileWrite $0 '"hotkey_ptt":"CommandOrControl+Shift+Space",'
  FileWrite $0 '"hotkey_toggle":"CommandOrControl+Shift+M",'
  FileWrite $0 '"input_device":"default",'
  FileWrite $0 '"language_mode":"auto",'
  FileWrite $0 '"product_mode":"transcribe",'
  FileWrite $0 '"topic_keywords":{"technical":["code","coding","debug","debugging","bug","error","stacktrace","exception","function","variable","api","endpoint","database","sql","query","schema","deploy","deployment","build","compile","performance","latency","memory","thread","integration","schnittstelle","fehler","datenbank","abfrage","bereitstellung","leistung","speicher","konfiguration","version","docker","kubernetes"],"meeting":["meeting","agenda","minutes","action","action item","deadline","owner","follow-up","stakeholder","alignment","decision","next step","roadmap","priority","milestone","planning","sync","standup","retrospective","workshop","besprechung","termin","protokoll","entscheidung","naechster schritt","prioritaet","meilenstein","planung","abstimmung","aufgabe","verantwortlich","rueckmeldung","review"],"personal":["personal","note","reminder","todo","to-do","follow-up","errand","appointment","family","health","habit","journal","private","vacation","shopping","budget","finance","bank","insurance","medicine","persoenlich","erinnerung","notiz","einkauf","urlaub","arzt","haushalt","konto","rechnung","gesundheit","routine","privat","aufraeumen"]},'
  FileWrite $0 '"model":"whisper-large-v3-turbo",'
  FileWrite $0 '"cloud_fallback":false,'

  ; AI Refinement + module_settings — conditional on wizard choice
  ${If} $AIRefinementSelected == "1"
    FileWrite $0 '"module_settings":{"enabled_modules":["ai_refinement"],"consented_permissions":{},"module_overrides":{}},'
    FileWrite $0 '"ai_fallback":{"enabled":true,"provider":"ollama","fallback_provider":null,"execution_mode":"local_primary","strict_local_mode":true,"preserve_source_language":true,"model":"qwen3.5:4b","temperature":0.3,"max_tokens":4000,"custom_prompt_enabled":false,"custom_prompt":"Refine this voice transcription: fix punctuation, capitalization, and obvious errors. Keep the original meaning. Output only the refined text.","use_default_prompt":true},'
  ${Else}
    FileWrite $0 '"module_settings":{"enabled_modules":[],"consented_permissions":{},"module_overrides":{}},'
    FileWrite $0 '"ai_fallback":{"enabled":false,"provider":"ollama","fallback_provider":null,"execution_mode":"local_primary","strict_local_mode":true,"preserve_source_language":true,"model":"","temperature":0.3,"max_tokens":4000,"custom_prompt_enabled":false,"custom_prompt":"Refine this voice transcription: fix punctuation, capitalization, and obvious errors. Keep the original meaning. Output only the refined text.","use_default_prompt":true},'
  ${EndIf}

  FileWrite $0 '"setup":{"local_ai_wizard_completed":false,"local_ai_wizard_pending":true,"ollama_remote_expert_opt_in":false},'
  FileWrite $0 '"audio_cues":true,'
  FileWrite $0 '"audio_cues_volume":0.3,'
  FileWrite $0 '"ptt_use_vad":false,'
  FileWrite $0 '"vad_threshold":0.01,'
  FileWrite $0 '"vad_threshold_start":0.01,'
  FileWrite $0 '"vad_threshold_sustain":0.005,'
  FileWrite $0 '"vad_silence_ms":500,'
  FileWrite $0 '"transcribe_enabled":false,'
  FileWrite $0 '"transcribe_hotkey":"CommandOrControl+Shift+T",'
  FileWrite $0 '"transcribe_output_device":"default",'
  FileWrite $0 '"transcribe_vad_mode":false,'
  FileWrite $0 '"transcribe_vad_threshold":0.04,'
  FileWrite $0 '"transcribe_vad_silence_ms":900,'
  FileWrite $0 '"transcribe_batch_interval_ms":8000,'
  FileWrite $0 '"transcribe_chunk_overlap_ms":1000,'
  FileWrite $0 '"transcribe_input_gain_db":0.0,'
  FileWrite $0 '"mic_input_gain_db":0.0,'
  FileWrite $0 '"capture_enabled":false,'
  FileWrite $0 '"model_source":"default",'
  FileWrite $0 '"model_custom_url":"",'
  FileWrite $0 '"model_storage_dir":"",'
  FileWrite $0 '"overlay_color":"#ff3d2e",'
  FileWrite $0 '"overlay_min_radius":16.0,'
  FileWrite $0 '"overlay_max_radius":64.0,'
  FileWrite $0 '"overlay_rise_ms":20,'
  FileWrite $0 '"overlay_fall_ms":400,'
  FileWrite $0 '"overlay_opacity_inactive":0.1,'
  FileWrite $0 '"overlay_opacity_active":0.97,'
  FileWrite $0 '"overlay_kitt_color":"#ff3d2e",'
  FileWrite $0 '"overlay_kitt_rise_ms":20,'
  FileWrite $0 '"overlay_kitt_fall_ms":800,'
  FileWrite $0 '"overlay_kitt_opacity_inactive":0.1,'
  FileWrite $0 '"overlay_kitt_opacity_active":1.0,'
  FileWrite $0 '"overlay_pos_x":50.0,'
  FileWrite $0 '"overlay_pos_y":90.0,'
  FileWrite $0 '"overlay_kitt_pos_x":50.0,'
  FileWrite $0 '"overlay_kitt_pos_y":90.0,'
  FileWrite $0 '"overlay_style":"$OverlayStyleChoice",'
  FileWrite $0 '"overlay_refining_indicator_enabled":true,'
  FileWrite $0 '"overlay_refining_indicator_preset":"standard",'
  FileWrite $0 '"overlay_kitt_min_width":20.0,'
  FileWrite $0 '"overlay_kitt_max_width":700.0,'
  FileWrite $0 '"overlay_kitt_height":13.0,'
  FileWrite $0 '"hallucination_filter_enabled":true,'
  FileWrite $0 '"hallucination_rms_threshold":0.3,'
  FileWrite $0 '"hallucination_max_duration_ms":2000,'
  FileWrite $0 '"hallucination_max_words":5,'
  FileWrite $0 '"hallucination_max_chars":50,'
  FileWrite $0 '"workflow_agent":{"enabled":false,"wakewords":["trispr","hey trispr","trispr agent"],"intent_keywords":{"gdd_generate_publish":["gdd","game design document","design document","designdokument","publish","confluence","draft","generate","create gdd","erstelle gdd","erstellen","veroeffentlichen","posten","session","meeting","interview","minutes","zusammenfassung","dokument","doc","spec","gameplay","feature"]},"model":"qwen3.5:4b","temperature":0.2,"max_tokens":512,"session_gap_minutes":20,"max_candidates":3},'
  FileWrite $0 '"vision_input_settings":{"enabled":false,"fps":2,"source_scope":"all_monitors","max_width":1280,"jpeg_quality":75,"ram_buffer_seconds":30,"all_monitors_default":true},'

  ; Voice output settings — conditional on wizard TTS selection
  ${If} $TtsProviderChoice == "natural"
    ; Windows Natural Voice (preferred) with SAPI fallback
    ${If} $ComponentPiperSelected == "1"
      FileWrite $0 '"voice_output_settings":{"enabled":false,"default_provider":"windows_natural","fallback_provider":"windows_native","voice_id_windows":"","voice_id_local":"","rate":1.0,"volume":1.0,"output_policy":"agent_replies_only","output_device":"default","piper_binary_path":"","piper_model_path":"","piper_model_dir":""},'
    ${Else}
      FileWrite $0 '"voice_output_settings":{"enabled":false,"default_provider":"windows_natural","fallback_provider":"windows_native","voice_id_windows":"","voice_id_local":"","rate":1.0,"volume":1.0,"output_policy":"agent_replies_only","output_device":"default","piper_binary_path":"","piper_model_path":"","piper_model_dir":""},'
    ${EndIf}
  ${Else}
    ; Windows SAPI with optional Piper fallback
    ${If} $ComponentPiperSelected == "1"
      FileWrite $0 '"voice_output_settings":{"enabled":false,"default_provider":"windows_native","fallback_provider":"local_custom","voice_id_windows":"","voice_id_local":"de_DE-thorsten-medium","rate":1.0,"volume":1.0,"output_policy":"agent_replies_only","output_device":"default","piper_binary_path":"","piper_model_path":"","piper_model_dir":""},'
    ${Else}
      FileWrite $0 '"voice_output_settings":{"enabled":false,"default_provider":"windows_native","fallback_provider":"windows_native","voice_id_windows":"","voice_id_local":"","rate":1.0,"volume":1.0,"output_policy":"agent_replies_only","output_device":"default","piper_binary_path":"","piper_model_path":"","piper_model_dir":""},'
    ${EndIf}
  ${EndIf}

  FileWrite $0 '"whisper_gpu_layers":35'
  FileWrite $0 '}'
  FileClose $0

  SettingsExists:
  ; Create models directory
  CreateDirectory "$R8\models"

  ; ----------------------------------------------------------------
  ; On-demand downloads (skipped for cuda-complete: already bundled)
  ; ----------------------------------------------------------------

!if "${TRISPR_VARIANT}" != "cuda-complete"

  ; --- FFmpeg download (via PowerShell for better reliability) ---
  ${If} $ComponentFFmpegSelected == "1"
    CreateDirectory "$INSTDIR\resources\bin\ffmpeg"
    DetailPrint "Lade FFmpeg herunter..."
    nsExec::ExecToStack 'powershell -NoProfile -Command "Try{$ProgressPreference=\"SilentlyContinue\";Invoke-WebRequest -Uri \"https://github.com/GyanD/codexffmpeg/releases/download/7.1.1/ffmpeg-7.1.1-essentials_build.zip\" -OutFile \"$TEMP\trispr-ffmpeg.zip\" -UseBasicParsing}Catch{Exit 1}"'
    Pop $0
    ${If} $0 != 0
      MessageBox MB_OK|MB_ICONEXCLAMATION "FFmpeg konnte nicht heruntergeladen werden.$\r$\nOPUS-Aufnahme wird nicht verfügbar sein.$\r$\nManuelle Installation: ffmpeg.exe nach$\r$\n$INSTDIR\resources\bin\ffmpeg\ kopieren."
      Goto FFmpegDownloadDone
    ${EndIf}
    DetailPrint "Entpacke FFmpeg..."
    nsExec::ExecToStack 'powershell -NoProfile -Command "Expand-Archive -Path \"$TEMP\trispr-ffmpeg.zip\" -DestinationPath \"$TEMP\trispr-ffmpeg-ext\" -Force; Get-ChildItem -Path \"$TEMP\trispr-ffmpeg-ext\" -Filter ffmpeg.exe -Recurse | Where-Object { $_.Directory.Name -eq $\"bin$\" } | Select-Object -First 1 | Copy-Item -Destination \"$INSTDIR\resources\bin\ffmpeg\ffmpeg.exe\""'
    Pop $0
    Pop $1
    ; SHA256 verify
    nsExec::ExecToStack 'powershell -NoProfile -Command "(Get-FileHash -Path \"$INSTDIR\resources\bin\ffmpeg\ffmpeg.exe\" -Algorithm SHA256).Hash.ToLower()"'
    Pop $0
    Pop $2
    StrCmp $2 "b90225987bdd042cca09a1efb5e34e9848f2d1dbf5fbcd388753a44145522997" FFmpegHashOK
      MessageBox MB_OK|MB_ICONEXCLAMATION "FFmpeg-Prüfsumme ungültig — Datei möglicherweise beschädigt.$\r$\nBitte FFmpeg manuell installieren."
      Delete "$INSTDIR\resources\bin\ffmpeg\ffmpeg.exe"
      Goto FFmpegCleanup
    FFmpegHashOK:
      DetailPrint "FFmpeg OK (SHA256 verifiziert, libopus enthalten)"
    FFmpegCleanup:
      Delete "$TEMP\trispr-ffmpeg.zip"
      RMDir /r "$TEMP\trispr-ffmpeg-ext"
    FFmpegDownloadDone:
  ${EndIf}

  ; --- Piper TTS Runtime download (via PowerShell for better reliability) ---
  ${If} $ComponentPiperSelected == "1"
    CreateDirectory "$INSTDIR\resources\bin\piper"
    DetailPrint "Lade Piper TTS Runtime herunter..."
    nsExec::ExecToStack 'powershell -NoProfile -Command "Try{$ProgressPreference=\"SilentlyContinue\";Invoke-WebRequest -Uri \"https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip\" -OutFile \"$TEMP\trispr-piper.zip\" -UseBasicParsing}Catch{Exit 1}"'
    Pop $0
    ${If} $0 != 0
      MessageBox MB_OK|MB_ICONEXCLAMATION "Piper TTS Runtime konnte nicht heruntergeladen werden.$\r$\nSprachausgabe wird nicht verfügbar sein."
      Goto PiperDownloadDone
    ${EndIf}
    DetailPrint "Entpacke Piper TTS Runtime..."
    nsExec::ExecToStack 'powershell -NoProfile -Command "$files=@(\"piper.exe\",\"onnxruntime.dll\",\"onnxruntime_providers_shared.dll\",\"espeak-ng.dll\",\"piper_phonemize.dll\",\"libtashkeel_model.ort\"); $zip=[System.IO.Compression.ZipFile]::OpenRead(\"$TEMP\trispr-piper.zip\"); foreach($e in $zip.Entries){$n=$e.Name; if($files -contains $n -or $e.FullName -match \"espeak-ng-data\"){$out=\"$INSTDIR\resources\bin\piper\\\"+($e.FullName -replace \"^piper/\",\"\"); $dir=[System.IO.Path]::GetDirectoryName($out); if(-not(Test-Path $dir)){New-Item -ItemType Directory -Path $dir -Force|Out-Null}; if(-not $e.FullName.EndsWith(\"/\")){[System.IO.Compression.ZipFileExtensions]::ExtractToFile($e,$out,$true)}}}; $zip.Dispose()"'
    Pop $0
    Pop $1
    DetailPrint "Piper TTS Runtime installiert"
    Delete "$TEMP\trispr-piper.zip"
    PiperDownloadDone:
  ${EndIf}

!endif

  ; ----------------------------------------------------------------
  ; Ollama auto-install + model pull (when AI Refinement selected)
  ; ----------------------------------------------------------------
  ${If} $AIRefinementSelected == "1"
    ; Check if Ollama is already installed
    nsExec::ExecToStack 'powershell -NoProfile -Command "if(Test-Path \"$env:LOCALAPPDATA\Programs\Ollama\ollama.exe\"){Write-Output \"installed\"}else{Write-Output \"missing\"}"'
    Pop $0
    Pop $1
    StrCmp $1 "installed" OllamaAlreadyInstalled

    ; Download Ollama installer
    DetailPrint "Lade Ollama herunter (~80 MB)..."
    nsExec::ExecToLog 'powershell -NoProfile -Command "Try{$ProgressPreference=\"SilentlyContinue\";Invoke-WebRequest -Uri \"https://ollama.com/download/OllamaSetup.exe\" -OutFile \"$TEMP\OllamaSetup.exe\" -UseBasicParsing;Exit 0}Catch{Write-Error $_.Exception.Message;Exit 1}"'
    Pop $0
    ${If} $0 != 0
      MessageBox MB_OK|MB_ICONEXCLAMATION "Ollama konnte nicht heruntergeladen werden.$\r$\nManuelle Installation: ollama.com$\r$\nKI-Verfeinerung wird ohne Ollama nicht funktionieren."
      Goto OllamaDone
    ${EndIf}

    ; Silent install
    DetailPrint "Installiere Ollama..."
    nsExec::ExecToLog '"$TEMP\OllamaSetup.exe" /S'
    Pop $0
    ${If} $0 != 0
      MessageBox MB_OK|MB_ICONEXCLAMATION "Ollama-Installation fehlgeschlagen.$\r$\nManuelle Installation: ollama.com"
      Delete "$TEMP\OllamaSetup.exe"
      Goto OllamaDone
    ${EndIf}
    Delete "$TEMP\OllamaSetup.exe"

    OllamaAlreadyInstalled:
    ; Start Ollama serve in background, then pull the model
    DetailPrint "Starte Ollama und lade qwen3.5:4b Modell (~400 MB)..."
    nsExec::ExecToLog 'powershell -NoProfile -Command "Start-Process -FilePath \"$env:LOCALAPPDATA\Programs\Ollama\ollama.exe\" -ArgumentList \"serve\" -WindowStyle Hidden; Start-Sleep -Seconds 5; $env:OLLAMA_HOST=\"http://127.0.0.1:11434\"; & \"$env:LOCALAPPDATA\Programs\Ollama\ollama.exe\" pull qwen3.5:4b"'
    Pop $0
    ${If} $0 != 0
      MessageBox MB_OK|MB_ICONEXCLAMATION "Ollama-Modell konnte nicht heruntergeladen werden.$\r$\nDu kannst es später manuell laden:$\r$\nollama pull qwen3.5:4b"
    ${Else}
      DetailPrint "Ollama + qwen3.5:4b erfolgreich installiert"
    ${EndIf}

    OllamaDone:
  ${EndIf}

  ; Keep both CUDA and Vulkan runtime folders on disk.
  ; Rationale:
  ; - Hybrid GPU/Optimus systems can report CUDA availability late.
  ; - quantize.exe may need runtime DLLs from either backend folder.
  ; Runtime backend selection is handled by app diagnostics/settings.

  Goto SkipPostInstall
  SkipPostInstall:
!macroend
