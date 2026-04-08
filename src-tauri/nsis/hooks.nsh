; Trispr Flow NSIS Installer Hooks
; Wizard: Hardware/Variant info, Component selection, First-run config, Summary.
; This file is included at the top level of the installer script by Tauri.

!include "nsDialogs.nsh"
!include "LogicLib.nsh"
!include "WinMessages.nsh"

; Load variant define written by build-installers.bat
; Use ${__FILEDIR__} to resolve relative to hooks.nsh directory, not installer.nsi temp dir
!include "${__FILEDIR__}\variant-define.nsh"

; Fallback to "vulkan" if variant-define.nsh was not found
!ifndef TRISPR_VARIANT
  !define TRISPR_VARIANT "vulkan"
!endif

; Variant display: uppercase version for UI
!if "${TRISPR_VARIANT}" == "vulkan"
  !define TRISPR_VARIANT_DISPLAY "VULKAN"
!else if "${TRISPR_VARIANT}" == "cuda-lite"
  !define TRISPR_VARIANT_DISPLAY "CUDA (lite)"
!else if "${TRISPR_VARIANT}" == "cuda-complete"
  !define TRISPR_VARIANT_DISPLAY "CUDA (complete, offline)"
!else
  !define TRISPR_VARIANT_DISPLAY "${TRISPR_VARIANT}"
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
Var VoiceLabel
Var VoiceExtraLabel
Var VoiceExtraInput
Var CheckVoiceThorsten
Var CheckVoiceThorstenEmotional
Var CheckVoiceAlan
Var CheckVoiceAlba
Var CheckVoiceCori
Var ComponentFFmpegSelected
Var ComponentPiperSelected
Var ComponentVoiceChoice
Var ComponentVoiceExtraKeys
Var ComponentVoiceSelectedCount

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

; Page 4: Capture Mode
Var CaptureModeDialog

; Page 5: Summary
Var SummaryDialog
Var SummaryLabel
Var SummaryFont

; =====================================================================
; Custom page declarations (inserted between Tauri's reinstall page
; and the directory selector page)
; =====================================================================
Page custom HardwareVariantPage HardwareVariantPageLeave
Page custom ComponentsPage ComponentsPageLeave
Page custom FirstRunConfigPage FirstRunConfigPageLeave
Page custom CaptureModeConfigPage CaptureModeConfigPageLeave
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

  ; Variant text in uppercase
  !if "${TRISPR_VARIANT}" == "vulkan"
    StrCpy $R0 "VULKAN"
  !else if "${TRISPR_VARIANT}" == "cuda-lite"
    StrCpy $R0 "CUDA (lite)"
  !else if "${TRISPR_VARIANT}" == "cuda-complete"
    StrCpy $R0 "CUDA (complete, offline)"
  !else
    StrCpy $R0 "${TRISPR_VARIANT}"
  !endif
  ${NSD_CreateLabel} 0 76u 100% 14u "$R0"
  Pop $0

  ; CUDA hint for NVIDIA GPU on Vulkan variant
  !if "${TRISPR_VARIANT}" == "vulkan"
    StrCmp $DetectedGpuStr "Keine NVIDIA GPU erkannt" no_cuda_hint
      ${NSD_CreateLabel} 0 92u 100% 20u "Hinweis: Für NVIDIA GPUs steht ein optimierter CUDA-Installer zur Verfügung (cuda-lite)."
      Pop $0
      SetCtlColors $0 "ff8800" transparent
    no_cuda_hint:
  !endif

  ${NSD_CreateLabel} 0 118u 100% 30u "Im nächsten Schritt kannst du auswählen, welche zusätzlichen Komponenten heruntergeladen werden sollen."
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
  ${NSD_CreateLabel} 0 24u 100% 18u "Offline-Variante: FFmpeg und Piper Runtime sind bereits enthalten."
  Pop $0
  ${NSD_CreateLabel} 0 42u 100% 12u "Zusätzliche Piper-Stimmen können optional online nachgeladen werden."
  Pop $0
!else
  ${NSD_CreateLabel} 0 24u 100% 12u "Audio-Encoder (wird heruntergeladen bei Installation):"
  Pop $0

  ${NSD_CreateCheckbox} 10u 37u 100% 13u "FFmpeg ~84 MB - benötigt für OPUS-Aufnahme"
  Pop $CheckFFmpeg
  ${NSD_SetState} $CheckFFmpeg ${BST_CHECKED}

  ${NSD_CreateLabel} 0 54u 100% 12u "Sprachausgabe:"
  Pop $0

  ${NSD_CreateRadioButton} 10u 67u 100% 12u "Windows Natural Voice (empfohlen)"
  Pop $RadioTtsNatural
  ${NSD_AddStyle} $RadioTtsNatural ${WS_GROUP}
  ${NSD_SetState} $RadioTtsNatural ${BST_CHECKED}

  ${NSD_CreateRadioButton} 10u 80u 100% 12u "Windows Sprachausgabe (SAPI, immer verfügbar)"
  Pop $RadioTtsSapi

  ${NSD_CreateCheckbox} 10u 95u 100% 12u "Piper KI-Stimme (lokal, ~28 MB Runtime-Download)"
  Pop $CheckPiperRuntime
  ${NSD_OnClick} $CheckPiperRuntime ComponentsPiperToggle

  ${NSD_CreateLabel} 22u 108u 95% 16u "Bei deaktiviertem Piper bleiben Voice-Packs inaktiv."
  Pop $0
!endif

  ${NSD_CreateLabel} 0 126u 100% 12u "Piper Voice Packs (kuratierte Auswahl, mindestens medium):"
  Pop $VoiceLabel

  ${NSD_CreateCheckbox} 10u 139u 100% 11u "de_DE-thorsten-medium (~53 MB, maennlich, Default)"
  Pop $CheckVoiceThorsten
  ${NSD_SetState} $CheckVoiceThorsten ${BST_CHECKED}

  ${NSD_CreateCheckbox} 10u 151u 100% 11u "de_DE-thorsten_emotional-medium (~77 MB, 8 styles)"
  Pop $CheckVoiceThorstenEmotional

  ${NSD_CreateCheckbox} 10u 163u 100% 11u "en_GB-alan-medium (~56 MB, maennlich)"
  Pop $CheckVoiceAlan

  ${NSD_CreateCheckbox} 10u 175u 100% 11u "en_GB-alba-medium (~57 MB, weiblich)"
  Pop $CheckVoiceAlba

  ${NSD_CreateCheckbox} 10u 187u 100% 11u "en_GB-cori-high (~81 MB, weiblich)"
  Pop $CheckVoiceCori

  ${NSD_CreateLabel} 0 196u 100% 10u "Weitere Voice-Keys (optional, eine Zeile pro Key):"
  Pop $VoiceExtraLabel

  ${NSD_CreateText} 10u 206u 100% 22u ""
  Pop $VoiceExtraInput
  ${NSD_AddStyle} $VoiceExtraInput ${ES_MULTILINE}|${ES_AUTOVSCROLL}|${WS_VSCROLL}
  SendMessage $VoiceExtraInput ${EM_SETLIMITTEXT} 2048 0

!if "${TRISPR_VARIANT}" == "cuda-complete"
  StrCpy $0 "1"
  Call SetVoiceControlsState
!else
  Call ComponentsPiperToggle
!endif

  nsDialogs::Show
FunctionEnd

Function SetVoiceControlsState
  ${If} $0 == "1"
    EnableWindow $VoiceLabel 1
    EnableWindow $CheckVoiceThorsten 1
    EnableWindow $CheckVoiceThorstenEmotional 1
    EnableWindow $CheckVoiceAlan 1
    EnableWindow $CheckVoiceAlba 1
    EnableWindow $CheckVoiceCori 1
    EnableWindow $VoiceExtraLabel 1
    EnableWindow $VoiceExtraInput 1
  ${Else}
    EnableWindow $VoiceLabel 0
    EnableWindow $CheckVoiceThorsten 0
    EnableWindow $CheckVoiceThorstenEmotional 0
    EnableWindow $CheckVoiceAlan 0
    EnableWindow $CheckVoiceAlba 0
    EnableWindow $CheckVoiceCori 0
    EnableWindow $VoiceExtraLabel 0
    EnableWindow $VoiceExtraInput 0
  ${EndIf}
FunctionEnd

Function ComponentsPiperToggle
  ${NSD_GetState} $CheckPiperRuntime $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $0 "1"
  ${Else}
    StrCpy $0 "0"
  ${EndIf}
  Call SetVoiceControlsState
FunctionEnd

Function CountNonEmptyLines
  ; In:  $0 = potentially multiline string
  ; Out: $1 = number of non-empty lines
  StrCpy $1 "0"
  StrCpy $2 "0"
  StrCpy $3 ""
  CountLinesLoop:
    StrCpy $4 $0 1 $2
    StrCmp $4 "" CountLinesDone
    StrCmp $4 "$\r" CountLinesNext
    StrCmp $4 "$\n" CountLinesBreak
    StrCpy $3 "$3$4"
    Goto CountLinesNext
  CountLinesBreak:
    StrCmp $3 "" CountLinesReset
    IntOp $1 $1 + 1
  CountLinesReset:
    StrCpy $3 ""
  CountLinesNext:
    IntOp $2 $2 + 1
    Goto CountLinesLoop
  CountLinesDone:
    StrCmp $3 "" +2
    IntOp $1 $1 + 1
FunctionEnd

Function ComponentsPageLeave
!if "${TRISPR_VARIANT}" == "cuda-complete"
  ; All components bundled in offline installer
  StrCpy $ComponentFFmpegSelected "0"
  StrCpy $ComponentPiperSelected "1"
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

  StrCpy $ComponentVoiceChoice ""
  StrCpy $ComponentVoiceExtraKeys ""
  StrCpy $ComponentVoiceSelectedCount "0"

  ${If} $ComponentPiperSelected == "1"
    ${NSD_GetState} $CheckVoiceThorsten $0
    ${If} $0 == ${BST_CHECKED}
      StrCpy $ComponentVoiceChoice "de_DE-thorsten-medium"
      IntOp $ComponentVoiceSelectedCount $ComponentVoiceSelectedCount + 1
    ${EndIf}

    ${NSD_GetState} $CheckVoiceThorstenEmotional $0
    ${If} $0 == ${BST_CHECKED}
      ${If} $ComponentVoiceChoice == ""
        StrCpy $ComponentVoiceChoice "de_DE-thorsten_emotional-medium"
      ${Else}
        StrCpy $ComponentVoiceChoice "$ComponentVoiceChoice$\r$\nde_DE-thorsten_emotional-medium"
      ${EndIf}
      IntOp $ComponentVoiceSelectedCount $ComponentVoiceSelectedCount + 1
    ${EndIf}

    ${NSD_GetState} $CheckVoiceAlan $0
    ${If} $0 == ${BST_CHECKED}
      ${If} $ComponentVoiceChoice == ""
        StrCpy $ComponentVoiceChoice "en_GB-alan-medium"
      ${Else}
        StrCpy $ComponentVoiceChoice "$ComponentVoiceChoice$\r$\nen_GB-alan-medium"
      ${EndIf}
      IntOp $ComponentVoiceSelectedCount $ComponentVoiceSelectedCount + 1
    ${EndIf}

    ${NSD_GetState} $CheckVoiceAlba $0
    ${If} $0 == ${BST_CHECKED}
      ${If} $ComponentVoiceChoice == ""
        StrCpy $ComponentVoiceChoice "en_GB-alba-medium"
      ${Else}
        StrCpy $ComponentVoiceChoice "$ComponentVoiceChoice$\r$\nen_GB-alba-medium"
      ${EndIf}
      IntOp $ComponentVoiceSelectedCount $ComponentVoiceSelectedCount + 1
    ${EndIf}

    ${NSD_GetState} $CheckVoiceCori $0
    ${If} $0 == ${BST_CHECKED}
      ${If} $ComponentVoiceChoice == ""
        StrCpy $ComponentVoiceChoice "en_GB-cori-high"
      ${Else}
        StrCpy $ComponentVoiceChoice "$ComponentVoiceChoice$\r$\nen_GB-cori-high"
      ${EndIf}
      IntOp $ComponentVoiceSelectedCount $ComponentVoiceSelectedCount + 1
    ${EndIf}

    ${NSD_GetText} $VoiceExtraInput $ComponentVoiceExtraKeys
    StrCpy $0 $ComponentVoiceExtraKeys
    Call CountNonEmptyLines
    IntOp $ComponentVoiceSelectedCount $ComponentVoiceSelectedCount + $1
  ${EndIf}
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

  ${NSD_CreateLabel} 22u 54u 95% 36u "Verbessert Transkripte mit lokalem KI-Modell (Ollama): Satzzeichen, Rephrasing, Prompt-Stile (präzise, Meeting, casual). Erfordert installiertes Ollama + Modell-Download beim ersten Start."
  Pop $0

  ${NSD_CreateLabel} 0 96u 100% 12u "Overlay-Stil (jederzeit in den App-Einstellungen änderbar):"
  Pop $0

  ${NSD_CreateRadioButton} 10u 110u 100% 13u "HAL 9000 — pulsierender Kreis"
  Pop $RadioHal
  ${NSD_SetState} $RadioHal ${BST_CHECKED}

  ${NSD_CreateRadioButton} 10u 125u 100% 13u "KITT — Leuchtbalken"
  Pop $RadioKitt

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
FunctionEnd

; =====================================================================
; Page 4: Capture Mode Configuration
; =====================================================================

Function CaptureModeConfigPage
  nsDialogs::Create 1018
  Pop $CaptureModeDialog
  ${If} $CaptureModeDialog == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 20u "Aufnahme-Modus"
  Pop $0
  CreateFont $1 "Segoe UI" 11 700
  SendMessage $0 ${WM_SETFONT} $1 1

  ${NSD_CreateLabel} 0 26u 100% 12u "Wie soll die Aufnahme ausgelöst werden?"
  Pop $0

  ${NSD_CreateRadioButton} 10u 42u 100% 13u "Push-to-Talk (PTT) — empfohlen"
  Pop $RadioPtt
  ${NSD_AddStyle} $RadioPtt ${WS_GROUP}
  ${NSD_SetState} $RadioPtt ${BST_CHECKED}

  ${NSD_CreateRadioButton} 10u 57u 100% 13u "Voice Activation — automatische Aufnahme bei Sprache"
  Pop $RadioVad

  nsDialogs::Show
FunctionEnd

Function CaptureModeConfigPageLeave
  ${NSD_GetState} $RadioPtt $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $CaptureModeChoice "ptt"
  ${Else}
    StrCpy $CaptureModeChoice "vad"
  ${EndIf}
FunctionEnd

; =====================================================================
; Page 5: Summary (read-only confirmation)
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

  StrCpy $R0 "GPU:              $DetectedGpuStr$\r$\nVariante:         ${TRISPR_VARIANT_DISPLAY}$\r$\n"

!if "${TRISPR_VARIANT}" == "cuda-complete"
  StrCpy $R0 "$R0FFmpeg:           Enthalten (Offline)$\r$\n"
  StrCpy $R0 "$R0Piper TTS:        Enthalten (Offline)$\r$\n"
  StrCpy $R0 "$R0Stimmen:          $ComponentVoiceSelectedCount gewaehlt / Default: de_DE-thorsten-medium$\r$\n"
!else
  ${If} $ComponentFFmpegSelected == "1"
    StrCpy $R0 "$R0FFmpeg:           Download ~84 MB$\r$\n"
  ${Else}
    StrCpy $R0 "$R0FFmpeg:           Nein$\r$\n"
  ${EndIf}
  ${If} $ComponentPiperSelected == "1"
    StrCpy $R0 "$R0Piper TTS:        Download ~28 MB$\r$\n"
    StrCpy $R0 "$R0Stimmen:          $ComponentVoiceSelectedCount gewaehlt / Default: de_DE-thorsten-medium$\r$\n"
  ${Else}
    StrCpy $R0 "$R0Piper TTS:        Nein$\r$\n"
    StrCpy $R0 "$R0Stimmen:          0 gewaehlt / Default: de_DE-thorsten-medium$\r$\n"
  ${EndIf}
!endif

  ${If} $AIRefinementSelected == "1"
    StrCpy $R0 "$R0KI-Verfeinerung:  Aktiviert (Ollama benötigt)$\r$\n"
  ${Else}
    StrCpy $R0 "$R0KI-Verfeinerung:  Deaktiviert$\r$\n"
  ${EndIf}
  StrCpy $R0 "$R0Overlay:          $OverlayStyleChoice$\r$\nAufnahme:         $CaptureModeChoice$\r$\n"

  ; TTS summary line
!if "${TRISPR_VARIANT}" != "cuda-complete"
  ${If} $TtsProviderChoice == "natural"
    StrCpy $R0 "$R0Sprachausgabe:    Windows Natural Voice"
  ${Else}
    StrCpy $R0 "$R0Sprachausgabe:    Windows SAPI"
  ${EndIf}
  ${If} $ComponentPiperSelected == "1"
    StrCpy $R0 "$R0 (+ Piper Fallback)"
  ${EndIf}
!endif

  ${NSD_CreateLabel} 0 26u 100% 150u "$R0"
  Pop $SummaryLabel
  CreateFont $SummaryFont "Consolas" 9 400
  SendMessage $SummaryLabel ${WM_SETFONT} $SummaryFont 1

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
      FileWrite $0 '"voice_output_settings":{"enabled":false,"default_provider":"windows_natural","fallback_provider":"windows_native","voice_id_windows":"","voice_id_local":"","rate":1.0,"volume":1.0,"output_policy":"agent_replies_only","output_device":"default","piper_binary_path":"","piper_model_path":"de_DE-thorsten-medium","piper_model_dir":""},'
    ${Else}
      FileWrite $0 '"voice_output_settings":{"enabled":false,"default_provider":"windows_natural","fallback_provider":"windows_native","voice_id_windows":"","voice_id_local":"","rate":1.0,"volume":1.0,"output_policy":"agent_replies_only","output_device":"default","piper_binary_path":"","piper_model_path":"","piper_model_dir":""},'
    ${EndIf}
  ${Else}
    ; Windows SAPI with optional Piper fallback
    ${If} $ComponentPiperSelected == "1"
      FileWrite $0 '"voice_output_settings":{"enabled":false,"default_provider":"windows_native","fallback_provider":"local_custom","voice_id_windows":"","voice_id_local":"de_DE-thorsten-medium","rate":1.0,"volume":1.0,"output_policy":"agent_replies_only","output_device":"default","piper_binary_path":"","piper_model_path":"de_DE-thorsten-medium","piper_model_dir":""},'
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
  ; Core runtime downloads (skipped for cuda-complete: already bundled)
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

  ; --- Piper voice packs (all variants, optional/best-effort) ---
!if "${TRISPR_VARIANT}" == "cuda-complete"
  StrCpy $R7 "1"
!else
  StrCpy $R7 $ComponentPiperSelected
!endif
  ${If} $R7 == "1"
    IfFileExists "$INSTDIR\resources\bin\piper\piper.exe" PiperVoicesRuntimeReady PiperVoicesRuntimeMissing

    PiperVoicesRuntimeMissing:
      MessageBox MB_OK|MB_ICONEXCLAMATION "Piper Runtime wurde nicht gefunden.$\r$\nVoice-Packs konnten nicht heruntergeladen werden."
      Goto PiperVoicesDone

    PiperVoicesRuntimeReady:
      CreateDirectory "$INSTDIR\resources\bin\piper\voices"
      DetailPrint "Verarbeite Piper Voice-Packs..."

      FileOpen $0 "$TEMP\trispr-piper-selected-voices.txt" w
      FileWrite $0 "$ComponentVoiceChoice"
      FileClose $0

      FileOpen $0 "$TEMP\trispr-piper-extra-voices.txt" w
      FileWrite $0 "$ComponentVoiceExtraKeys"
      FileClose $0

      Delete "$TEMP\trispr-piper-invalid-keys.txt"
      Delete "$TEMP\trispr-piper-failed-keys.txt"

      File "/nonfatal" "/oname=$PLUGINSDIR\download-piper-voices.ps1" "${__FILEDIR__}\download-piper-voices.ps1"

      ${If} ${FileExists} "$PLUGINSDIR\download-piper-voices.ps1"
        nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -File "$PLUGINSDIR\download-piper-voices.ps1" -SelectedFile "$TEMP\trispr-piper-selected-voices.txt" -ExtraFile "$TEMP\trispr-piper-extra-voices.txt" -VoicesDir "$INSTDIR\resources\bin\piper\voices" -InvalidOut "$TEMP\trispr-piper-invalid-keys.txt" -FailedOut "$TEMP\trispr-piper-failed-keys.txt"'
        Pop $0
        ${If} $0 != 0
          MessageBox MB_OK|MB_ICONEXCLAMATION "Voice-Pack Download fehlgeschlagen. Installation wird fortgesetzt."
        ${EndIf}
      ${EndIf}

      nsExec::ExecToStack 'powershell -NoProfile -Command "if(Test-Path \"$TEMP\trispr-piper-invalid-keys.txt\"){Get-Content -Path \"$TEMP\trispr-piper-invalid-keys.txt\" -Raw}"'
      Pop $0
      Pop $1
      ${If} $1 != ""
        MessageBox MB_OK|MB_ICONEXCLAMATION "Ungueltige Piper Voice-Keys wurden uebersprungen:$\r$\n$1"
      ${EndIf}

      nsExec::ExecToStack 'powershell -NoProfile -Command "if(Test-Path \"$TEMP\trispr-piper-failed-keys.txt\"){Get-Content -Path \"$TEMP\trispr-piper-failed-keys.txt\" -Raw}"'
      Pop $0
      Pop $1
      ${If} $1 != ""
        MessageBox MB_OK|MB_ICONEXCLAMATION "Folgende Voice-Keys konnten nicht geladen werden:$\r$\n$1$\r$\nDie Installation wurde trotzdem abgeschlossen."
      ${EndIf}

      Delete "$TEMP\trispr-piper-selected-voices.txt"
      Delete "$TEMP\trispr-piper-extra-voices.txt"
      Delete "$TEMP\trispr-piper-invalid-keys.txt"
      Delete "$TEMP\trispr-piper-failed-keys.txt"
  PiperVoicesDone:
  ${EndIf}

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
