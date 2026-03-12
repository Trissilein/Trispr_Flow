; NSIS Script for Trispr Flow Unified Installer
; This script packages both Vulkan and CUDA builds and allows user selection.

!include "MUI2.nsh"
!include "StrFunc.nsh" ; Required for StrStr. Download from: https://nsis.sourceforge.io/StrFunc_plug-in

; --- Basic Info ---
Name "Trispr Flow 0.7.1"
OutFile "..\Trispr-Flow-Unified_0.7.1_x64-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\Trispr Flow"
RequestExecutionLevel user ; Can be 'user' if symlinks are not created by installer

; --- Modern UI Configuration ---
!define MUI_ABORTWARNING
!define MUI_ICON "..\src-tauri\icons\icon.ico"
!define MUI_UNICON "..\src-tauri\icons\icon.ico"

; --- Page Order ---
!insertmacro MUI_PAGE_WELCOME
Page custom VariantSelectionPageCreate VariantSelectionPageLeave
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "German"

; --- Global Variables ---
Var IsCudaDetected
Var RadioVulkan
Var RadioCuda

; --- Installer Sections ---
Section "Vulkan Runtime (Core)" SecVulkan
    SectionIn RO ; Read-only, always included as a base
    SetOutPath "$INSTDIR"
    
    ; Install all Vulkan files
    File /r "..\dist\vulkan\*.*"
SectionEnd

Section "NVIDIA CUDA Runtime (Optional)" SecCuda
    SetOutPath "$INSTDIR"
    
    ; Install all CUDA files, overwriting Vulkan where necessary
    File /r "..\dist\cuda\*.*"
SectionEnd

; --- Functions ---
Function .onInit
    ; Default to Vulkan
    StrCpy $IsCudaDetected 0
    
    ; Heuristic to detect an NVIDIA GPU. A more robust check might use a plugin.
    ReadRegStr $0 HKLM "SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0000" "DriverDesc"
    IfErrors done_check
    
    ; Check if "NVIDIA" is in the driver description string
    ${StrStr} $1 $0 "NVIDIA"
    StrCmp $1 "" done_check
    
    ; NVIDIA GPU found, set flag to pre-select CUDA
    StrCpy $IsCudaDetected 1
    
done_check:
    ; Pre-select the CUDA section if an NVIDIA card was detected
    ${If} $IsCudaDetected == 1
        !insertmacro SelectSection ${SecCuda}
    ${Else}
        !insertmacro UnselectSection ${SecCuda}
    ${EndIf}
FunctionEnd

Function VariantSelectionPageCreate
    !insertmacro MUI_HEADER_TEXT "Komponenten auswählen" "Wählen Sie die für Ihre Hardware optimierte Version."
    nsDialogs::Create 1018
    Pop $0

    ${NSD_CreateRadioButton} 10u 30u 80% 10u "Standard (Vulkan) - Kompatibel mit AMD, Intel & NVIDIA."
    Pop $RadioVulkan
    
    ${NSD_CreateRadioButton} 10u 50u 80% 10u "NVIDIA (CUDA) - Beste Performance auf NVIDIA-Karten."
    Pop $RadioCuda

    ${If} $IsCudaDetected == 1
        ${NSD_Check} $RadioCuda
    ${Else}
        ${NSD_Check} $RadioVulkan
    ${EndIf}
    
    nsDialogs::Show
FunctionEnd

Function VariantSelectionPageLeave
    ${NSD_GetState} $RadioCuda $0
    ${If} $0 == ${BST_CHECKED}
        ; User selected CUDA, ensure section is selected
        !insertmacro SelectSection ${SecCuda}
    ${Else}
        ; User selected Vulkan, ensure CUDA section is unselected
        !insertmacro UnselectSection ${SecCuda}
    ${EndIf}
FunctionEnd

Section "Uninstall"
    Delete "$INSTDIR\*.*"
    Delete "$INSTDIR\uninstall.exe"
    RMDir /r "$INSTDIR"
    DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Trispr Flow"
SectionEnd