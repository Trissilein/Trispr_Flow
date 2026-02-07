; Trispr Flow NSIS Installer Hooks
; Adds custom pages for overlay style and GPU backend selection.
; This file is included at the top level of the installer script by Tauri.

!include "nsDialogs.nsh"
!include "LogicLib.nsh"

; Variables for the overlay style page
Var OverlayStyleDialog
Var OverlayStyleLabel
Var OverlayRadioHal
Var OverlayRadioKitt
Var OverlayStyleChoice

; Variables for the GPU backend page
Var GpuDialog
Var GpuLabel
Var GpuRadioCuda
Var GpuRadioVulkan
Var GpuBackendChoice

; --- Custom page declarations (top-level, included before MUI pages) ---
Page custom OverlayStylePage OverlayStylePageLeave
Page custom GpuBackendPage GpuBackendPageLeave

; =====================================================================
; Page 1: Overlay Style Selection
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
; Page 2: GPU Backend Selection
; =====================================================================

Function GpuBackendPage
  nsDialogs::Create 1018
  Pop $GpuDialog
  ${If} $GpuDialog == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 48u "Choose the GPU backend for speech recognition.$\r$\n$\r$\nCUDA is fastest on NVIDIA GPUs.$\r$\nVulkan works on both AMD and NVIDIA GPUs."
  Pop $GpuLabel

  ${NSD_CreateRadioButton} 10u 58u 100% 14u "NVIDIA CUDA (recommended for NVIDIA GPUs)"
  Pop $GpuRadioCuda
  ${NSD_SetState} $GpuRadioCuda ${BST_CHECKED}

  ${NSD_CreateRadioButton} 10u 76u 100% 14u "Vulkan (AMD, Intel, or NVIDIA GPUs)"
  Pop $GpuRadioVulkan

  nsDialogs::Show
FunctionEnd

Function GpuBackendPageLeave
  ${NSD_GetState} $GpuRadioCuda $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $GpuBackendChoice "cuda"
  ${Else}
    StrCpy $GpuBackendChoice "vulkan"
  ${EndIf}
FunctionEnd

; =====================================================================
; Post-install: write settings + clean up unused GPU backend
; =====================================================================

!macro NSIS_HOOK_POSTINSTALL
  ; Write initial settings.json with all required fields and defaults.
  ; Only overlay_style is overridden by user choice; other fields use defaults.
  CreateDirectory "$APPDATA\com.trispr.flow"
  FileOpen $0 "$APPDATA\com.trispr.flow\settings.json" w
  FileWrite $0 '{'
  FileWrite $0 '"mode":"ptt",'
  FileWrite $0 '"hotkey_ptt":"CommandOrControl+Shift+Space",'
  FileWrite $0 '"hotkey_toggle":"CommandOrControl+Shift+M",'
  FileWrite $0 '"input_device":"default",'
  FileWrite $0 '"language_mode":"auto",'
  FileWrite $0 '"model":"whisper-large-v3",'
  FileWrite $0 '"cloud_fallback":false,'
  FileWrite $0 '"audio_cues":true,'
  FileWrite $0 '"audio_cues_volume":0.3,'
  FileWrite $0 '"ptt_use_vad":false,'
  FileWrite $0 '"vad_threshold":0.01,'
  FileWrite $0 '"vad_threshold_start":0.01,'
  FileWrite $0 '"vad_threshold_sustain":0.005,'
  FileWrite $0 '"vad_silence_ms":500,'
  FileWrite $0 '"transcribe_enabled":false,'
  FileWrite $0 '"transcribe_hotkey":"CommandOrControl+Shift+O",'
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
  FileWrite $0 '"overlay_min_radius":8.0,'
  FileWrite $0 '"overlay_max_radius":24.0,'
  FileWrite $0 '"overlay_rise_ms":80,'
  FileWrite $0 '"overlay_fall_ms":160,'
  FileWrite $0 '"overlay_opacity_inactive":0.2,'
  FileWrite $0 '"overlay_opacity_active":0.8,'
  FileWrite $0 '"overlay_kitt_color":"#ff3d2e",'
  FileWrite $0 '"overlay_kitt_rise_ms":80,'
  FileWrite $0 '"overlay_kitt_fall_ms":160,'
  FileWrite $0 '"overlay_kitt_opacity_inactive":0.2,'
  FileWrite $0 '"overlay_kitt_opacity_active":0.8,'
  FileWrite $0 '"overlay_pos_x":50.0,'
  FileWrite $0 '"overlay_pos_y":90.0,'
  FileWrite $0 '"overlay_kitt_pos_x":50.0,'
  FileWrite $0 '"overlay_kitt_pos_y":90.0,'
  FileWrite $0 '"overlay_style":"$OverlayStyleChoice",'
  FileWrite $0 '"overlay_kitt_min_width":20.0,'
  FileWrite $0 '"overlay_kitt_max_width":200.0,'
  FileWrite $0 '"overlay_kitt_height":20.0,'
  FileWrite $0 '"hallucination_filter_enabled":true,'
  FileWrite $0 '"hallucination_rms_threshold":0.3,'
  FileWrite $0 '"hallucination_max_duration_ms":2000,'
  FileWrite $0 '"hallucination_max_words":5,'
  FileWrite $0 '"hallucination_max_chars":50'
  FileWrite $0 '}'
  FileClose $0

  ; Remove the GPU backend that was NOT chosen to save disk space.
  ; Both backends are bundled in the installer, but only the selected one stays.
  ${If} $GpuBackendChoice == "cuda"
    ; Keep cuda, remove vulkan
    RMDir /r "$INSTDIR\bin\vulkan"
  ${Else}
    ; Keep vulkan, remove cuda
    RMDir /r "$INSTDIR\bin\cuda"
  ${EndIf}
!macroend
