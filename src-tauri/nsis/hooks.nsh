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
  ; Write the chosen overlay style
  CreateDirectory "$APPDATA\com.trispr.flow"
  FileOpen $0 "$APPDATA\com.trispr.flow\settings.json" w
  FileWrite $0 '{"overlay_style":"$OverlayStyleChoice"}'
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
