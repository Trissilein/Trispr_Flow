@echo off
setlocal enabledelayedexpansion

REM ========================================
REM Trispr Flow - Automated Triple Installer Build
REM ========================================
REM Builds CUDA, Vulkan, and CUDA+Analysis installer variants.

set "ROOT=%~dp0"
pushd "%ROOT%" >nul 2>&1
if not "!ERRORLEVEL!"=="0" (
    echo ERROR: Failed to switch to repo root: %ROOT%
    exit /b 1
)

echo.
echo ========================================
echo Trispr Flow - Triple Installer Builder
echo ========================================
echo.

echo [1/11] Detecting version...
for /f "tokens=2 delims=:, " %%a in ('findstr /C:"\"version\"" package.json') do (
    set VERSION_RAW=%%a
)
set "VERSION=%VERSION_RAW:"=%"
for /f %%i in ('powershell -NoProfile -Command "Get-Date -Format dd.MM."') do set "BUILDDATE=%%i"
for /f %%i in ('powershell -NoProfile -Command "Get-Date -Format HH.mm"') do set "BUILDTIME=%%i"
set "BUILDSTAMP=%BUILDDATE%-%BUILDTIME%"
echo Found version: %VERSION%
echo Build stamp: %BUILDSTAMP%
echo.

echo [2/11] Verifying config version consistency...
call :verify_version "src-tauri\tauri.conf.json"
if not "!ERRORLEVEL!"=="0" goto :fail
call :verify_version "src-tauri\tauri.conf.vulkan.json"
if not "!ERRORLEVEL!"=="0" goto :fail
call :verify_version "src-tauri\tauri.conf.cuda.analysis.json"
if not "!ERRORLEVEL!"=="0" goto :fail
echo.

echo [3/11] Verifying CUDA runtime libraries...
set DLL_MISSING=0
if not exist "src-tauri\bin\cuda\cublas64_13.dll" (
    echo   ERROR: cublas64_13.dll not found!
    set DLL_MISSING=1
)
if not exist "src-tauri\bin\cuda\cudart64_13.dll" (
    echo   ERROR: cudart64_13.dll not found!
    set DLL_MISSING=1
)
if "!DLL_MISSING!"=="1" (
    echo.
    echo CRITICAL ERROR: CUDA runtime DLLs are missing.
    goto :fail
)
echo   OK: CUDA DLLs found
echo.

echo [4/11] Preparing output directories...
if not exist "installers" mkdir "installers"
if exist "src-tauri\target\release\bundle\nsis" (
    rmdir /s /q "src-tauri\target\release\bundle\nsis" 2>nul
)
echo   OK: Output directories ready
echo.

echo [5/11] Building frontend...
call npm run build
if not "!ERRORLEVEL!"=="0" goto :fail
echo   OK: Frontend build successful
echo.

echo [6/11] Building CUDA installer...
call :build_variant "CUDA" "src-tauri/tauri.conf.json" "CUDA"
if not "!ERRORLEVEL!"=="0" goto :fail
set "CUDA_TARGET=!LAST_TARGET!"
echo.

echo [7/11] Building Vulkan installer...
call :build_variant "VULKAN" "src-tauri/tauri.conf.vulkan.json" "VULKAN"
if not "!ERRORLEVEL!"=="0" goto :fail
set "VULKAN_TARGET=!LAST_TARGET!"
echo.

echo [8/11] Checking bundled Analysis installer gate...
set "ANALYSIS_SETUP=installers\Trispr-Analysis-Setup.exe"
if not exist "%ANALYSIS_SETUP%" (
    echo ERROR: Missing required file for CUDA+Analysis variant:
    echo   %ANALYSIS_SETUP%
    echo Place the Analysis installer there and rerun this script.
    goto :fail
)
echo   OK: Found %ANALYSIS_SETUP%
echo.

echo [9/11] Building CUDA+Analysis installer...
call :build_variant "CUDA+Analysis" "src-tauri/tauri.conf.cuda.analysis.json" "CUDA-ANALYSIS"
if not "!ERRORLEVEL!"=="0" goto :fail
set "CUDA_ANALYSIS_TARGET=!LAST_TARGET!"
echo.

echo [10/11] Build summary...
call :print_file_info "CUDA" "!CUDA_TARGET!"
call :print_file_info "VULKAN" "!VULKAN_TARGET!"
call :print_file_info "CUDA-ANALYSIS" "!CUDA_ANALYSIS_TARGET!"
echo.

echo [11/11] Done.
echo ========================================
echo Build Complete
echo ========================================
echo.
echo Output directory:
echo   %CD%\installers\
echo.

popd
endlocal
exit /b 0

:verify_version
set "CFG=%~1"
for /f "tokens=2 delims=:, " %%a in ('findstr /C:"\"version\"" "%CFG%"') do (
    set CFG_VERSION_RAW=%%a
)
set "CFG_VERSION=%CFG_VERSION_RAW:"=%"
if /I not "%CFG_VERSION%"=="%VERSION%" (
    echo ERROR: Version mismatch in %CFG%
    echo   package.json: %VERSION%
    echo   config:       %CFG_VERSION%
    exit /b 1
)
echo   OK: %CFG% = %CFG_VERSION%
exit /b 0

:build_variant
set "VARIANT_LABEL=%~1"
set "CONFIG_PATH=%~2"
set "VARIANT_KEY=%~3"
set "SOURCE=src-tauri\target\release\bundle\nsis\Trispr Flow_%VERSION%_x64-setup.exe"
set "TARGET_NAME=TrsprFlw.v%VERSION%.%VARIANT_KEY%-%BUILDSTAMP%.exe"

call npm run tauri build -- --config %CONFIG_PATH%
if not "!ERRORLEVEL!"=="0" (
    echo ERROR: Build failed for %VARIANT_LABEL%
    exit /b 1
)

if not exist "%SOURCE%" (
    echo ERROR: Installer not found after %VARIANT_LABEL% build:
    echo   %SOURCE%
    exit /b 1
)

if exist "installers\!TARGET_NAME!" (
    set /a SUFFIX=!RANDOM! %% 100
    if !SUFFIX! lss 10 set "SUFFIX=0!SUFFIX!"
    set "TARGET_NAME=TrsprFlw.v%VERSION%.%VARIANT_KEY%-%BUILDSTAMP%-!SUFFIX!.exe"
)

move "%SOURCE%" "installers\!TARGET_NAME!" >nul
if not "!ERRORLEVEL!"=="0" (
    echo ERROR: Failed to move installer for %VARIANT_LABEL%
    exit /b 1
)

echo   OK: %VARIANT_LABEL% -> installers\!TARGET_NAME!
set "LAST_TARGET=installers\!TARGET_NAME!"
exit /b 0

:print_file_info
set "LABEL=%~1"
set "FILE=%~2"
if exist "%FILE%" (
    for %%A in ("%FILE%") do (
        set SIZE=%%~zA
        set /a SIZE_MB=!SIZE! / 1048576
        echo   %LABEL%: %%~nxA ^(!SIZE_MB! MB^)
    )
) else (
    echo   %LABEL%: MISSING
)
exit /b 0

:fail
echo.
echo ========================================
echo BUILD FAILED
echo ========================================
popd
endlocal
exit /b 1
