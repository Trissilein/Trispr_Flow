; Trispr Flow NSIS Installer Hooks - Vulkan Only Edition
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

; Variables for the VibeVoice opt-in page
Var VibeVoiceDialog
Var VibeVoiceLabel
Var VibeVoiceRadioYes
Var VibeVoiceRadioNo
Var VibeVoiceChoice

; --- Custom page declarations (top-level, included before MUI pages) ---
Page custom InstallModePage InstallModePageLeave
Page custom OverlayStylePage OverlayStylePageLeave
Page custom CaptureModePage CaptureModePageLeave
Page custom VibeVoicePage VibeVoicePageLeave

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
; =====================================================================
; Page 4: Speaker Diarization (VibeVoice) Opt-In
; =====================================================================

Function VibeVoicePage
  nsDialogs::Create 1018
  Pop $VibeVoiceDialog
  ${If} $VibeVoiceDialog == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 72u "Speaker Diarization identifies WHO said WHAT in recordings.$\r$\n$\r$\nThis feature uses VibeVoice-ASR and requires Python 3.11 or newer.$\r$\nPython is NOT included in this installer to keep download size small.$\r$\n$\r$\nDo you want to use Speaker Diarization?"
  Pop $VibeVoiceLabel

  ${NSD_CreateRadioButton} 10u 82u 100% 14u "Yes — I will install Python if needed"
  Pop $VibeVoiceRadioYes

  ${NSD_CreateRadioButton} 10u 100u 100% 14u "No — I only need standard transcription"
  Pop $VibeVoiceRadioNo
  ${NSD_SetState} $VibeVoiceRadioNo ${BST_CHECKED}

  nsDialogs::Show
FunctionEnd

Function VibeVoicePageLeave
  ${NSD_GetState} $VibeVoiceRadioYes $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $VibeVoiceChoice "yes"

    ; Check if Python 3.x is available
    nsExec::ExecToStack 'python --version'
    Pop $1  ; exit code
    Pop $2  ; output ("Python 3.x.x" or error)

    ${If} $1 != 0
      ; Python not found — offer to open download page
      MessageBox MB_YESNO|MB_ICONINFORMATION \
        "Python was not found on your system.$\r$\n$\r$\nSpeaker Diarization requires Python 3.11 or newer.$\r$\n$\r$\nClick Yes to open the Python download page in your browser.$\r$\nYou can install it after this setup completes.$\r$\n$\r$\nClick No to continue without installing Python now." \
        IDYES OpenPythonDownload IDNO SkipPythonDownload

      OpenPythonDownload:
        ExecShell "open" "https://www.python.org/downloads/"

      SkipPythonDownload:
    ${Else}
      ; Python found — inform user about next step
      ${If} $2 != ""
        MessageBox MB_OK|MB_ICONINFORMATION \
          "Python found: $2$\r$\n$\r$\nAfter installation, run this command to install VibeVoice dependencies:$\r$\n  pip install -r sidecar\vibevoice-asr\requirements.txt"
      ${EndIf}
    ${EndIf}
  ${Else}
    StrCpy $VibeVoiceChoice "no"
  ${EndIf}
FunctionEnd

; Post-install: write settings (Vulkan-only, no GPU backend cleanup)
; =====================================================================

!macro NSIS_HOOK_POSTINSTALL
  ; Check install mode - only proceed if user selected "install"
  ${If} $InstallModeChoice == "uninstall"
    ; User selected uninstall, skip post-install configuration
    Goto SkipPostInstall
  ${EndIf}

  ; Write initial settings.json with all required fields and defaults.
  ; GPU backend is hardcoded to "vulkan" in this edition.
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
  FileWrite $0 '"model":"whisper-large-v3-turbo",'
  FileWrite $0 '"cloud_fallback":false,'
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
  FileWrite $0 '"overlay_kitt_min_width":20.0,'
  FileWrite $0 '"overlay_kitt_max_width":700.0,'
  FileWrite $0 '"overlay_kitt_height":13.0,'
  FileWrite $0 '"hallucination_filter_enabled":true,'
  FileWrite $0 '"hallucination_rms_threshold":0.3,'
  FileWrite $0 '"hallucination_max_duration_ms":2000,'
  FileWrite $0 '"hallucination_max_words":5,'
  FileWrite $0 '"hallucination_max_chars":50'
  FileWrite $0 '}'
  FileClose $0

  ; Vulkan-only edition: No GPU backend cleanup needed (only Vulkan is bundled)

  ; Create models directory for future use (app will download model on first start)
  CreateDirectory "$APPDATA\com.trispr.flow\models"

  SkipPostInstall:
!macroend
