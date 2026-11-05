; Trispr Flow NSIS Installer Hooks
; Wizard: System info, Startup options, Recording mode, Summary.
; This file is included at the top level of the installer script by Tauri.
; Variants: "mini" (app only, setup wizard downloads the speech engine) and
;           "full" (bundles bin\cuda + bin\vulkan).

!include "nsDialogs.nsh"
!include "LogicLib.nsh"
!include "WinMessages.nsh"

; Load variant define written by scripts/generate-tauri-variant-config.mjs (build-installers.bat).
; Use ${__FILEDIR__} to resolve relative to hooks.nsh directory, not installer.nsi temp dir
!include /NONFATAL "${__FILEDIR__}\variant-define.nsh"

; Fallback to "full" (matches the base tauri.conf.json resource list) if the define was not generated
!ifndef TRISPR_VARIANT
  !define TRISPR_VARIANT "full"
!endif

!if "${TRISPR_VARIANT}" == "mini"
  !define TRISPR_VARIANT_DISPLAY "Mini"
  !define TRISPR_VARIANT_NOTE "Compact installer. The speech engine is not bundled."
!else
  !define TRISPR_VARIANT_DISPLAY "Full (CUDA + Vulkan)"
  !define TRISPR_VARIANT_NOTE "Both speech engines (CUDA and Vulkan) are bundled. The app picks the best one for your GPU."
!endif

!define TRISPR_NO_NVIDIA "No NVIDIA GPU detected"

; =====================================================================
; Variable declarations
; =====================================================================

; Page 1: Hardware/Variant info
Var HardwareDialog
Var DetectedGpuStr


; Page 2: Startup options
Var FirstRunDialog
Var CheckAIRefinement
Var RadioHal
Var RadioKitt
Var RadioPtt
Var RadioVad
Var AIRefinementSelected
Var OverlayStyleChoice
Var CaptureModeChoice

; Page 3: Recording mode
Var CaptureModeDialog

; Page 4: Summary
Var SummaryDialog
Var SummaryLabel
Var SummaryFont

; =====================================================================
; Custom page declarations (inserted between Tauri's reinstall page
; and the directory selector page)
; =====================================================================
Page custom HardwareVariantPage HardwareVariantPageLeave
Page custom FirstRunConfigPage FirstRunConfigPageLeave
Page custom CaptureModeConfigPage CaptureModeConfigPageLeave
Page custom HardwareSummaryPage

; =====================================================================
; GPU detection (informational): first NVIDIA display adapter in the registry.
; Also called from the post-install hook, because silent installs skip the pages.
; =====================================================================

Function DetectNvidiaGpu
  StrCpy $DetectedGpuStr "${TRISPR_NO_NVIDIA}"
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
FunctionEnd

; =====================================================================
; Page 1: System Info
; =====================================================================

Function HardwareVariantPage
  Call DetectNvidiaGpu

  nsDialogs::Create 1018
  Pop $HardwareDialog
  ${If} $HardwareDialog == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 16u "System detection"
  Pop $0
  SetCtlColors $0 "" 0xffffff
  CreateFont $1 "Segoe UI" 11 700
  SendMessage $0 ${WM_SETFONT} $1 1

  ${NSD_CreateLabel} 0 24u 100% 10u "Detected GPU:"
  Pop $0
  ${NSD_CreateLabel} 0 35u 100% 10u "$DetectedGpuStr"
  Pop $0

  ${NSD_CreateLabel} 0 54u 100% 10u "Installer variant:"
  Pop $0
  ${NSD_CreateLabel} 0 65u 100% 10u "${TRISPR_VARIANT_DISPLAY}"
  Pop $0
  ${NSD_CreateLabel} 0 78u 100% 20u "${TRISPR_VARIANT_NOTE}"
  Pop $0

  ${NSD_CreateLabel} 0 104u 100% 20u "The next steps set your startup options."
  Pop $0

  nsDialogs::Show
FunctionEnd

Function HardwareVariantPageLeave
  ; Nothing to capture - informational page only
FunctionEnd

; =====================================================================
; Page 2: Startup Options
; =====================================================================

Function FirstRunConfigPage
  nsDialogs::Create 1018
  Pop $FirstRunDialog
  ${If} $FirstRunDialog == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 16u "Startup options"
  Pop $0
  CreateFont $1 "Segoe UI" 11 700
  SendMessage $0 ${WM_SETFONT} $1 1

  ${NSD_CreateLabel} 0 20u 100% 10u "AI refinement (local, managed Ollama runtime):"
  Pop $0

  ${NSD_CreateCheckbox} 10u 32u 100% 12u "Enable AI refinement"
  Pop $CheckAIRefinement
  ; Default: checked (restored from the earlier choice when the user navigates back).
  ${If} $AIRefinementSelected != "0"
    ${NSD_SetState} $CheckAIRefinement ${BST_CHECKED}
  ${EndIf}

  ${NSD_CreateLabel} 22u 46u 95% 34u "Fixes punctuation and wording with a local AI model (prompt styles: precise, meeting, casual). The setup wizard downloads the runtime and model (several GB) on first start; you can skip it there."
  Pop $0

  ${NSD_CreateLabel} 0 84u 100% 10u "Overlay style (changeable later in the app settings):"
  Pop $0

  ${NSD_CreateRadioButton} 10u 96u 100% 12u "HAL 9000 - pulsing circle"
  Pop $RadioHal
  ${NSD_SetState} $RadioHal ${BST_CHECKED}

  ${NSD_CreateRadioButton} 10u 110u 100% 12u "KITT - light bar"
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
; Page 3: Recording Mode
; =====================================================================

Function CaptureModeConfigPage
  nsDialogs::Create 1018
  Pop $CaptureModeDialog
  ${If} $CaptureModeDialog == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 16u "Recording mode"
  Pop $0
  CreateFont $1 "Segoe UI" 11 700
  SendMessage $0 ${WM_SETFONT} $1 1

  ${NSD_CreateLabel} 0 24u 100% 10u "How should recording be triggered?"
  Pop $0

  ${NSD_CreateRadioButton} 10u 40u 100% 12u "Push-to-talk (PTT) - recommended"
  Pop $RadioPtt
  ${NSD_AddStyle} $RadioPtt ${WS_GROUP}
  ${NSD_SetState} $RadioPtt ${BST_CHECKED}

  ${NSD_CreateRadioButton} 10u 55u 100% 12u "Voice activation - records automatically when you speak"
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
; Page 4: Summary (read-only confirmation)
; =====================================================================

Function HardwareSummaryPage
  nsDialogs::Create 1018
  Pop $SummaryDialog
  ${If} $SummaryDialog == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 16u "Summary"
  Pop $0
  CreateFont $1 "Segoe UI" 11 700
  SendMessage $0 ${WM_SETFONT} $1 1

  StrCpy $R0 "GPU:            $DetectedGpuStr$\r$\nInstaller:      ${TRISPR_VARIANT_DISPLAY}$\r$\n"

  ${If} $DetectedGpuStr != "${TRISPR_NO_NVIDIA}"
    StrCpy $R0 "$R0GPU mode:       High performance (NVIDIA)$\r$\n"
  ${EndIf}

  ${If} $AIRefinementSelected == "1"
    StrCpy $R0 "$R0AI refinement:  On (downloads on first start)$\r$\n"
  ${Else}
    StrCpy $R0 "$R0AI refinement:  Off$\r$\n"
  ${EndIf}
  StrCpy $R0 "$R0Overlay:        $OverlayStyleChoice$\r$\nRecording:      $CaptureModeChoice$\r$\n"

  ${NSD_CreateLabel} 0 20u 100% 56u "$R0"
  Pop $SummaryLabel
  CreateFont $SummaryFont "Consolas" 9 400
  SendMessage $SummaryLabel ${WM_SETFONT} $SummaryFont 1

  !if "${TRISPR_VARIANT}" == "mini"
    ${NSD_CreateLabel} 0 80u 100% 20u "The setup wizard downloads the speech engine and models on first start."
    Pop $0
  !endif

  ${NSD_CreateLabel} 0 104u 100% 10u "Click 'Install' to start."
  Pop $0

  ${NSD_CreateLabel} 0 115u 100% 10u "Logs: %LOCALAPPDATA%\Trispr Flow\logs\"
  Pop $0

  nsDialogs::Show
FunctionEnd

; =====================================================================
; Post-install: write settings and GPU preferences
; =====================================================================

!macro NSIS_HOOK_POSTINSTALL
  ; Silent installs (/S) skip the custom pages: apply the same defaults the pages preselect.
  ${If} $DetectedGpuStr == ""
    Call DetectNvidiaGpu
  ${EndIf}
  ${If} $AIRefinementSelected == ""
    StrCpy $AIRefinementSelected "1"
  ${EndIf}
  ${If} $OverlayStyleChoice == ""
    StrCpy $OverlayStyleChoice "dot"
  ${EndIf}
  ${If} $CaptureModeChoice == ""
    StrCpy $CaptureModeChoice "ptt"
  ${EndIf}

  ; Resolve %LOCALAPPDATA% - matches Rust paths.rs which uses LOCALAPPDATA\Trispr Flow
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
  FileWrite $0 '"topic_keywords":{"technical":["code","coding","debug","debugging","bug","error","stacktrace","exception","function","variable","api","endpoint","database","sql","query","schema","deploy","deployment","build","compile","performance","latency","memory","thread","integration","schnittstelle","fehler","datenbank","abfrage","bereitstellung","leistung","speicher","konfiguration","version","docker","kubernetes"],"meeting":["meeting","agenda","minutes","action","action item","deadline","owner","follow-up","stakeholder","alignment","decision","next step","roadmap","priority","milestone","planning","sync","standup","retrospective","workshop","besprechung","termin","protokoll","entscheidung","naechster schritt","prioritaet","meilenstein","planung","abstimmung","aufgabe","verantwortlich","rueckmeldung","review"],"personal":["personal","note","reminder","todo","to-do","follow-up","errand","appointment","family","health","habit","journal","private","vacation","shopping","budget","finance","bank","insurance","medicine","persoenlich","erinnerung","notiz","einkauf","urlaub","arzt","haushalt","konto","rechnung","gesundheit","routine","privat","aufraeumen"]},'
  FileWrite $0 '"model":"whisper-large-v3-turbo",'

  ; AI Refinement + module_settings - conditional on wizard choice
  ${If} $AIRefinementSelected == "1"
    FileWrite $0 '"module_settings":{"enabled_modules":["ai_refinement"],"consented_permissions":{},"module_overrides":{}},'
    FileWrite $0 '"ai_fallback":{"enabled":true,"provider":"ollama","preserve_source_language":true,"model":"gemma4:e4b","temperature":0.3,"max_tokens":4000,"custom_prompt_enabled":false,"custom_prompt":"Refine this voice transcription: fix punctuation, capitalization, and obvious errors. Keep the original meaning. Output only the refined text.","use_default_prompt":true},'
  ${Else}
    FileWrite $0 '"module_settings":{"enabled_modules":[],"consented_permissions":{},"module_overrides":{}},'
    FileWrite $0 '"ai_fallback":{"enabled":false,"provider":"ollama","preserve_source_language":true,"model":"","temperature":0.3,"max_tokens":4000,"custom_prompt_enabled":false,"custom_prompt":"Refine this voice transcription: fix punctuation, capitalization, and obvious errors. Keep the original meaning. Output only the refined text.","use_default_prompt":true},'
  ${EndIf}

  FileWrite $0 '"setup":{"local_ai_wizard_completed":false,"local_ai_wizard_pending":true},'
  FileWrite $0 '"audio_cues":true,'
  FileWrite $0 '"audio_cues_volume":0.3,'
  FileWrite $0 '"ptt_use_vad":false,'
  FileWrite $0 '"vad_threshold":0.01,'
  FileWrite $0 '"vad_threshold_start":0.01,'
  FileWrite $0 '"vad_threshold_sustain":0.005,'
  FileWrite $0 '"vad_silence_ms":500,'
  FileWrite $0 '"transcribe_enabled":true,'
  FileWrite $0 '"transcribe_hotkey":"CommandOrControl+Shift+T",'
  FileWrite $0 '"transcribe_output_device":"default",'
  FileWrite $0 '"transcribe_vad_mode":false,'
  FileWrite $0 '"transcribe_vad_threshold":0.04,'
  FileWrite $0 '"transcribe_vad_silence_ms":900,'
  FileWrite $0 '"transcribe_batch_interval_ms":8000,'
  FileWrite $0 '"transcribe_chunk_overlap_ms":1000,'
  FileWrite $0 '"transcribe_input_gain_db":0.0,'
  FileWrite $0 '"mic_input_gain_db":0.0,'
  FileWrite $0 '"capture_enabled":true,'
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
  FileWrite $0 '"whisper_gpu_layers":35'
  FileWrite $0 '}'
  FileClose $0

  SettingsExists:
  ; Create models directory
  CreateDirectory "$R8\models"

  ; ----------------------------------------------------------------
  ; GPU Preferences: Force NVIDIA dGPU on hybrid/Optimus systems
  ; ----------------------------------------------------------------
  ; Set Windows DirectX GPU preference to "High Performance" for all whisper binaries
  ; This ensures NVIDIA dGPU is used on hybrid graphics systems instead of Intel iGPU
  ${If} $DetectedGpuStr != "${TRISPR_NO_NVIDIA}"
    WriteRegStr HKCU "Software\Microsoft\DirectX\UserGpuPreferences" "$INSTDIR\bin\cuda\whisper-cli.exe" "GpuPreference=2;"
    WriteRegStr HKCU "Software\Microsoft\DirectX\UserGpuPreferences" "$INSTDIR\bin\cuda\whisper-server.exe" "GpuPreference=2;"
    WriteRegStr HKCU "Software\Microsoft\DirectX\UserGpuPreferences" "$INSTDIR\bin\vulkan\whisper-cli.exe" "GpuPreference=2;"
    WriteRegStr HKCU "Software\Microsoft\DirectX\UserGpuPreferences" "$INSTDIR\bin\vulkan\whisper-server.exe" "GpuPreference=2;"
  ${EndIf}
!macroend
