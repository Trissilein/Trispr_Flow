; Shared optional Trispr Analysis Tool installer flow.
; This file is used by the CUDA+Analysis installer variant only.

Var AnalysisToolDialog
Var AnalysisToolLabel
Var AnalysisToolRadioYes
Var AnalysisToolRadioNo
Var AnalysisToolChoice

Function AnalysisToolPage
  nsDialogs::Create 1018
  Pop $AnalysisToolDialog
  ${If} $AnalysisToolDialog == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 78u "Trispr Analysis is an optional external app for speaker-aware Voice Analysis.$\r$\n$\r$\nThis installer bundle includes a local Trispr Analysis setup file.$\r$\n$\r$\nDo you want to install Trispr Analysis now?"
  Pop $AnalysisToolLabel

  ${NSD_CreateRadioButton} 10u 88u 100% 14u "Yes - install Trispr Analysis (optional)"
  Pop $AnalysisToolRadioYes

  ${NSD_CreateRadioButton} 10u 106u 100% 14u "No - install Trispr Flow only"
  Pop $AnalysisToolRadioNo
  ${NSD_SetState} $AnalysisToolRadioNo ${BST_CHECKED}

  nsDialogs::Show
FunctionEnd

Function AnalysisToolPageLeave
  ${NSD_GetState} $AnalysisToolRadioYes $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $AnalysisToolChoice "yes"
  ${Else}
    StrCpy $AnalysisToolChoice "no"
  ${EndIf}
FunctionEnd

Function InstallAnalysisToolOptional
  StrCpy $0 "$INSTDIR\resources\analysis-installer\Trispr-Analysis-Setup.exe"

  IfFileExists "$0" FoundInstaller MissingInstaller

  FoundInstaller:
    DetailPrint "Running bundled Trispr Analysis installer (silent)"
    ExecWait '"$0" /S' $1
    ${If} $1 != 0
      MessageBox MB_OK|MB_ICONEXCLAMATION "Trispr Analysis installation failed (exit code: $1).$\r$\n$\r$\nTrispr Flow installation will continue."
      Return
    ${EndIf}
    MessageBox MB_OK|MB_ICONINFORMATION "Trispr Analysis installed successfully."
    Return

  MissingInstaller:
    MessageBox MB_OK|MB_ICONEXCLAMATION "Bundled Trispr Analysis installer not found at:$\r$\n$0$\r$\n$\r$\nTrispr Flow installation will continue."
    Return
FunctionEnd

!macro TRISPR_ANALYSIS_TOOL_POSTINSTALL
  ${If} $AnalysisToolChoice == "yes"
    Call InstallAnalysisToolOptional
  ${EndIf}
!macroend
