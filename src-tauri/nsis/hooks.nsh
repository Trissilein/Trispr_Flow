; Trispr Flow NSIS Installer Hooks
; Adds custom pages for install/uninstall selection, overlay style, and capture mode.
; This file is included at the top level of the installer script by Tauri.

!include "nsDialogs.nsh"
!include "LogicLib.nsh"

; Variables for the install/uninstall page
Var InstallModeDialog
Var InstallModeLabel
Var InstallModeRadioInstall
Var InstallModeRadioUninstall
Var InstallModeChoice

; Variables for the overlay style page
Var OverlayStyleDialog
Var OverlayStyleLabel
Var OverlayRadioHal
Var OverlayRadioKitt
Var OverlayStyleChoice

; Variables for the capture mode page
Var CaptureModeDialog
Var CaptureModeLabel
Var CaptureModeRadioPtt
Var CaptureModeRadioVad
Var CaptureModeChoice

; Variables for the GPU optimization page
Var GpuOptimizeDialog
Var GpuOptimizeLabel
Var GpuOptimizeRadioAuto
Var GpuOptimizeRadioManual
Var GpuOptimizeChoice
Var GpuLayersText
Var GpuLayersInput

; --- Custom page declarations (top-level, included before MUI pages) ---
Page custom InstallModePage InstallModePageLeave
Page custom OverlayStylePage OverlayStylePageLeave
Page custom CaptureModePage CaptureModePageLeave
Page custom GpuOptimizePage GpuOptimizePageLeave

; =====================================================================
; Page 1: Install/Uninstall Mode Selection
; =====================================================================

Function InstallModePage
  nsDialogs::Create 1018
  Pop $InstallModeDialog
  ${If} $InstallModeDialog == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 36u "Choose whether to install or uninstall Trispr Flow."
  Pop $InstallModeLabel

  ${NSD_CreateRadioButton} 10u 46u 100% 14u "Install Trispr Flow"
  Pop $InstallModeRadioInstall
  ${NSD_SetState} $InstallModeRadioInstall ${BST_CHECKED}

  ${NSD_CreateRadioButton} 10u 64u 100% 14u "Uninstall Trispr Flow"
  Pop $InstallModeRadioUninstall

  nsDialogs::Show
FunctionEnd

Function InstallModePageLeave
  ${NSD_GetState} $InstallModeRadioInstall $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $InstallModeChoice "install"
  ${Else}
    StrCpy $InstallModeChoice "uninstall"
    ; User chose uninstall - run uninstaller and quit installer
    MessageBox MB_YESNO "Do you want to uninstall Trispr Flow?" IDYES DoUninstall
    Abort  ; Cancel if user clicks No

    DoUninstall:
      ; Check if uninstaller exists
      IfFileExists "$INSTDIR\uninstall.exe" +1 NoUninstaller
        ExecWait '"$INSTDIR\uninstall.exe" _?=$INSTDIR'
        Quit
      NoUninstaller:
        MessageBox MB_OK|MB_ICONEXCLAMATION "Uninstaller not found. The application may not be installed."
        Quit
  ${EndIf}
FunctionEnd

; =====================================================================
; Page 2: Overlay Style Selection
; =====================================================================

Function OverlayStylePage
  nsDialogs::Create 1018
  Pop $OverlayStyleDialog
  ${If} $OverlayStyleDialog == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 36u "Choose your preferred overlay style.$\r$\n$\r$\nYou can change this later in the app settings."
  Pop $OverlayStyleLabel

  ${NSD_CreateRadioButton} 10u 46u 100% 14u "HAL 9000 by Space:2001 (pulsing circle)"
  Pop $OverlayRadioHal
  ${NSD_SetState} $OverlayRadioHal ${BST_CHECKED}

  ${NSD_CreateRadioButton} 10u 64u 100% 14u "KITT by Dox (expanding bar)"
  Pop $OverlayRadioKitt

  nsDialogs::Show
FunctionEnd

Function OverlayStylePageLeave
  ${NSD_GetState} $OverlayRadioHal $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $OverlayStyleChoice "dot"
  ${Else}
    StrCpy $OverlayStyleChoice "kitt"
  ${EndIf}
FunctionEnd

; =====================================================================
; Page 3: Capture Mode Selection
; =====================================================================

Function CaptureModePage
  nsDialogs::Create 1018
  Pop $CaptureModeDialog
  ${If} $CaptureModeDialog == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 60u "Choose how you want to activate voice recording.$\r$\nPush-to-Talk: Hold a hotkey to record$\r$\nVoice Activation: Automatic recording when you speak$\r$\n$\r$\nYou can change this later in the app settings."
  Pop $CaptureModeLabel

  ${NSD_CreateRadioButton} 10u 70u 100% 14u "Push-to-Talk (PTT) - recommended"
  Pop $CaptureModeRadioPtt
  ${NSD_SetState} $CaptureModeRadioPtt ${BST_CHECKED}

  ${NSD_CreateRadioButton} 10u 88u 100% 14u "Voice Activation (VAD) - automatic"
  Pop $CaptureModeRadioVad

  nsDialogs::Show
FunctionEnd

Function CaptureModePageLeave
  ${NSD_GetState} $CaptureModeRadioPtt $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $CaptureModeChoice "ptt"
  ${Else}
    StrCpy $CaptureModeChoice "vad"
  ${EndIf}
FunctionEnd

; =====================================================================
; Page 4: GPU Optimization for Whisper (CUDA)
; =====================================================================

Function GpuOptimizePage
  nsDialogs::Create 1018
  Pop $GpuOptimizeDialog
  ${If} $GpuOptimizeDialog == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 60u "GPU acceleration speeds up speech transcription.$\r$\n$\r$\nAuto: Uses default GPU settings (recommended for most users)$\r$\nManual: Specify GPU layers for advanced users$\r$\n$\r$\nYou can change this later in the app settings."
  Pop $GpuOptimizeLabel

  ${NSD_CreateRadioButton} 10u 70u 100% 14u "Auto (default GPU optimization)"
  Pop $GpuOptimizeRadioAuto
  ${NSD_SetState} $GpuOptimizeRadioAuto ${BST_CHECKED}

  ${NSD_CreateRadioButton} 10u 88u 100% 14u "Manual (specify GPU layers):"
  Pop $GpuOptimizeRadioManual

  ${NSD_CreateText} 30u 106u 40u 12u "35"
  Pop $GpuLayersInput

  nsDialogs::Show
FunctionEnd

Function GpuOptimizePageLeave
  ${NSD_GetState} $GpuOptimizeRadioAuto $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $GpuOptimizeChoice "auto"
  ${Else}
    StrCpy $GpuOptimizeChoice "manual"
    ${NSD_GetText} $GpuLayersInput $GpuLayersText
  ${EndIf}
FunctionEnd

; =====================================================================
; Post-install: write settings and environment
; =====================================================================

!macro NSIS_HOOK_POSTINSTALL
  ; Check install mode - only proceed if user selected "install"
  ${If} $InstallModeChoice == "uninstall"
    ; User selected uninstall, skip post-install configuration
    Goto SkipPostInstall
  ${EndIf}

  ; Write initial settings.json with all required fields and defaults.
  ; Only overlay_style is overridden by user choice; other fields use defaults.
  CreateDirectory "$APPDATA\com.trispr.flow"
  ; Delete old settings.json to ensure fresh defaults on reinstalls/updates
  Delete "$APPDATA\com.trispr.flow\settings.json"
  FileOpen $0 "$APPDATA\com.trispr.flow\settings.json" w
  FileWrite $0 '{'
  FileWrite $0 '"mode":"$CaptureModeChoice",'
  FileWrite $0 '"hotkey_ptt":"CommandOrControl+Shift+Space",'
  FileWrite $0 '"hotkey_toggle":"CommandOrControl+Shift+M",'
  FileWrite $0 '"input_device":"default",'
  FileWrite $0 '"language_mode":"auto",'
  FileWrite $0 '"topic_keywords":{"technical":["code","coding","debug","debugging","bug","error","stacktrace","exception","function","variable","api","endpoint","database","sql","query","schema","deploy","deployment","build","compile","performance","latency","memory","thread","integration","schnittstelle","fehler","datenbank","abfrage","bereitstellung","leistung","speicher","konfiguration","version","docker","kubernetes"],"meeting":["meeting","agenda","minutes","action","action item","deadline","owner","follow-up","stakeholder","alignment","decision","next step","roadmap","priority","milestone","planning","sync","standup","retrospective","workshop","besprechung","termin","protokoll","entscheidung","naechster schritt","prioritaet","meilenstein","planung","abstimmung","aufgabe","verantwortlich","rueckmeldung","review"],"personal":["personal","note","reminder","todo","to-do","follow-up","errand","appointment","family","health","habit","journal","private","vacation","shopping","budget","finance","bank","insurance","medicine","persoenlich","erinnerung","notiz","einkauf","urlaub","arzt","haushalt","konto","rechnung","gesundheit","routine","privat","aufraeumen"]},'
  FileWrite $0 '"model":"whisper-large-v3-turbo",'
  FileWrite $0 '"cloud_fallback":false,'
  FileWrite $0 '"ai_fallback":{"enabled":false,"provider":"ollama","fallback_provider":null,"execution_mode":"local_primary","strict_local_mode":true,"preserve_source_language":true,"model":"","temperature":0.3,"max_tokens":4000,"custom_prompt_enabled":false,"custom_prompt":"Refine this voice transcription: fix punctuation, capitalization, and obvious errors. Keep the original meaning. Output only the refined text.","use_default_prompt":true},'
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
  FileWrite $0 '"workflow_agent":{"enabled":false,"wakewords":["trispr","hey trispr","trispr agent"],"intent_keywords":{"gdd_generate_publish":["gdd","game design document","design document","designdokument","publish","confluence","draft","generate","create gdd","erstelle gdd","erstellen","veroeffentlichen","posten","session","meeting","interview","minutes","zusammenfassung","dokument","doc","spec","gameplay","feature"]},"model":"qwen3:4b","temperature":0.2,"max_tokens":512,"session_gap_minutes":20,"max_candidates":3},'
  FileWrite $0 '"vision_input_settings":{"enabled":false,"fps":2,"source_scope":"all_monitors","max_width":1280,"jpeg_quality":75,"ram_buffer_seconds":30,"all_monitors_default":true},'
  FileWrite $0 '"voice_output_settings":{"enabled":false,"default_provider":"windows_native","fallback_provider":"local_custom","voice_id_windows":"","voice_id_local":"","rate":1.0,"volume":1.0,"output_policy":"agent_replies_only"},'
  ; Determine GPU layers setting based on user choice
  ${If} $GpuOptimizeChoice == "manual"
    FileWrite $0 '"whisper_gpu_layers":$GpuLayersText'
  ${Else}
    FileWrite $0 '"whisper_gpu_layers":35'
  ${EndIf}
  FileWrite $0 '}'
  FileClose $0

  ; Create models directory for future use (app will download model on first start)
  CreateDirectory "$APPDATA\com.trispr.flow\models"

  ; Set GPU environment variable based on user choice
  ${If} $GpuOptimizeChoice == "auto"
    ; Auto mode: set to 35 (reasonable default for Turing/Ampere GPUs)
    WriteRegExpandStr HKCU "Environment" "TRISPR_WHISPER_GPU_LAYERS" "35"
  ${ElseIf} $GpuOptimizeChoice == "manual"
    ${If} $GpuLayersText != ""
      WriteRegExpandStr HKCU "Environment" "TRISPR_WHISPER_GPU_LAYERS" "$GpuLayersText"
    ${Else}
      WriteRegExpandStr HKCU "Environment" "TRISPR_WHISPER_GPU_LAYERS" "35"
    ${EndIf}
  ${EndIf}

  SkipPostInstall:
!macroend
