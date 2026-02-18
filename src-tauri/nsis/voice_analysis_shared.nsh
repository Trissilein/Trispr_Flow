; Shared Voice Analysis NSIS helpers (used by CUDA and Vulkan installer variants)

; Variables for the VibeVoice opt-in page
Var VibeVoiceDialog
Var VibeVoiceLabel
Var VibeVoiceRadioYes
Var VibeVoiceRadioNo
Var VibeVoiceChoice

!define TRISPR_VIBEVOICE_SETUP_SCRIPT "$INSTDIR\resources\sidecar\vibevoice-asr\setup-vibevoice.ps1"

; =====================================================================
; Voice Analysis (VibeVoice) Opt-In Page
; =====================================================================

Function VibeVoicePage
  nsDialogs::Create 1018
  Pop $VibeVoiceDialog
  ${If} $VibeVoiceDialog == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 72u "Voice Analysis identifies WHO said WHAT in recordings.$\r$\n$\r$\nThis feature uses VibeVoice-ASR and requires Python 3.11 or newer.$\r$\nPython is NOT included in this installer to keep download size small.$\r$\n$\r$\nDo you want to enable Voice Analysis?"
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

    ; Check whether Python is available on PATH (version checks happen in setup-vibevoice.ps1)
    nsExec::ExecToStack 'python --version'
    Pop $1 ; exit code
    Pop $2 ; output

    ${If} $1 != 0
      MessageBox MB_YESNO|MB_ICONINFORMATION "Python was not found on your system.$\r$\n$\r$\nVoice Analysis requires Python 3.11 or newer.$\r$\n$\r$\nClick Yes to open the Python download page in your browser.$\r$\nYou can install it after this setup completes.$\r$\n$\r$\nClick No to continue without installing Python now." IDYES VibeVoiceOpenPythonDownload IDNO VibeVoiceSkipPythonDownload
      VibeVoiceOpenPythonDownload:
        ExecShell "open" "https://www.python.org/downloads/"
      VibeVoiceSkipPythonDownload:
    ${Else}
      ${If} $2 != ""
        MessageBox MB_OK|MB_ICONINFORMATION "Python found: $2$\r$\n$\r$\nAfter installation, run this command to install Voice Analysis dependencies (no Git required):$\r$\n  powershell -NoProfile -ExecutionPolicy Bypass -File $\"${TRISPR_VIBEVOICE_SETUP_SCRIPT}$\""
      ${EndIf}
    ${EndIf}
  ${Else}
    StrCpy $VibeVoiceChoice "no"
  ${EndIf}
FunctionEnd

Function VibeVoiceRunOptionalSetup
  MessageBox MB_OK|MB_ICONINFORMATION "Trispr Flow will now install optional Voice Analysis dependencies.$\r$\n$\r$\nThis can take a few minutes."

  IfFileExists "${TRISPR_VIBEVOICE_SETUP_SCRIPT}" VibeVoiceRunSetup VibeVoiceMissingSetupScript

  VibeVoiceRunSetup:
    nsExec::ExecToStack '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -ExecutionPolicy Bypass -File "${TRISPR_VIBEVOICE_SETUP_SCRIPT}"'
    Pop $0 ; exit code
    Pop $1 ; script output (last line)
    ${If} $0 != 0
      MessageBox MB_OK|MB_ICONEXCLAMATION "Voice Analysis setup did not complete automatically (exit code: $0).$\r$\n$\r$\nLast output:$\r$\n$1$\r$\n$\r$\nYou can run it later with:$\r$\n  powershell -NoProfile -ExecutionPolicy Bypass -File $\"${TRISPR_VIBEVOICE_SETUP_SCRIPT}$\""
    ${EndIf}
    Return

  VibeVoiceMissingSetupScript:
    MessageBox MB_OK|MB_ICONEXCLAMATION "Voice Analysis setup script was not found in the installation.$\r$\n$\r$\nRun manual setup from:$\r$\n  ${TRISPR_VIBEVOICE_SETUP_SCRIPT}"
FunctionEnd

!macro TRISPR_VIBEVOICE_POSTINSTALL
  ${If} $VibeVoiceChoice == "yes"
    Call VibeVoiceRunOptionalSetup
  ${EndIf}
!macroend
