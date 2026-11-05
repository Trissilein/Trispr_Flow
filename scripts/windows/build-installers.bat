@echo off
setlocal enabledelayedexpansion

REM ========================================
REM Trispr Flow - Installer Builder
REM ========================================
REM Variants (default: mini full):
REM   mini - app + quantize only; the setup wizard downloads the speech engine and models on first start
REM   full - app + quantize + bundled whisper runtimes (bin\cuda + bin\vulkan, fetched from the pinned manifest)
REM Output: installers\TrsprFlw.v<version>.<variant>.exe
REM Env:
REM   CARGO_TARGET_DIR                    honored when looking for the built NSIS bundle
REM   TRISPR_SKIP_INSTALL_VALIDATION=1    skip the silent test install of each built installer

for %%I in ("%~dp0..\..") do set "ROOT=%%~fI"
pushd "%ROOT%" >nul 2>&1
if not "!ERRORLEVEL!"=="0" (
    echo ERROR: Failed to switch to repo root: %ROOT%
    exit /b 1
)

set "VARIANTS=%*"
if "%VARIANTS%"=="" set "VARIANTS=mini full"

set "TARGET_DIR=src-tauri\target"
if defined CARGO_TARGET_DIR set "TARGET_DIR=%CARGO_TARGET_DIR%"

echo.
echo ========================================
echo Trispr Flow - Installer Builder
echo ========================================
echo.
echo Variants: %VARIANTS%
echo.

echo [1/5] Detecting version...
for /f "tokens=2 delims=:, " %%a in ('findstr /C:"\"version\"" package.json') do (
    set VERSION_RAW=%%a
)
set "VERSION=%VERSION_RAW:"=%"
echo Found version: %VERSION%
echo.

echo [2/5] Preparing output directories...
if not exist "installers" mkdir "installers"
if exist "%TARGET_DIR%\release\bundle\nsis" (
    rmdir /s /q "%TARGET_DIR%\release\bundle\nsis" 2>nul
)
echo   OK: Output directories ready
echo.

echo [3/5] Building frontend...
call npm run build
if not "!ERRORLEVEL!"=="0" goto :fail
echo   OK: Frontend build successful
echo.

echo [4/5] Building variants...
for %%V in (%VARIANTS%) do (
    call :build_variant %%V
    if not "!ERRORLEVEL!"=="0" goto :fail
)
echo.

echo [5/5] Build summary...
for %%V in (%VARIANTS%) do (
    for %%A in ("installers\TrsprFlw.v%VERSION%.%%V.exe") do (
        set SIZE=%%~zA
        set /a SIZE_MB=!SIZE! / 1048576
        echo   %%~nxA ^(!SIZE_MB! MB^)
    )
)
echo.
echo ========================================
echo Output directory:
echo   %CD%\installers\
echo ========================================
echo.

popd
endlocal
exit /b 0

:build_variant
set "VARIANT=%~1"
if /i not "%VARIANT%"=="mini" if /i not "%VARIANT%"=="full" (
    echo ERROR: Unknown variant '%~1'. Allowed: mini, full
    exit /b 1
)

set "CFG=src-tauri\tauri.conf.variant.%VARIANT%.json"
echo   - variant=%VARIANT%

if /i "%VARIANT%"=="full" (
    node scripts\fetch-whisper-runtime.mjs --variants cuda,vulkan
    if not "!ERRORLEVEL!"=="0" (
        echo ERROR: Whisper runtime fetch/validation failed.
        exit /b 1
    )
)

REM Writes the variant tauri config and src-tauri\nsis\variant-define.nsh (read by hooks.nsh).
node scripts\generate-tauri-variant-config.mjs --variant %VARIANT% --out %CFG%
if not "!ERRORLEVEL!"=="0" (
    echo ERROR: Failed to generate Tauri config for variant %VARIANT%.
    exit /b 1
)

set "BASE_CFG=src-tauri\tauri.conf.json"
set "BASE_BAK=src-tauri\tauri.conf.base.bak.json"
copy /y "%BASE_CFG%" "%BASE_BAK%" >nul
if not "!ERRORLEVEL!"=="0" (
    echo ERROR: Failed to backup base tauri.conf.json.
    if exist "%CFG%" del /q "%CFG%" >nul 2>&1
    exit /b 1
)

copy /y "%CFG%" "%BASE_CFG%" >nul
if not "!ERRORLEVEL!"=="0" (
    echo ERROR: Failed to activate variant tauri config for %VARIANT%.
    copy /y "%BASE_BAK%" "%BASE_CFG%" >nul 2>&1
    del /q "%BASE_BAK%" >nul 2>&1
    if exist "%CFG%" del /q "%CFG%" >nul 2>&1
    exit /b 1
)

call npm run tauri build
set "BUILD_RESULT=!ERRORLEVEL!"

copy /y "%BASE_BAK%" "%BASE_CFG%" >nul 2>&1
del /q "%BASE_BAK%" >nul 2>&1
if exist "%CFG%" del /q "%CFG%" >nul 2>&1
if not "!BUILD_RESULT!"=="0" (
    echo ERROR: Build failed for variant %VARIANT%.
    exit /b 1
)

set "SOURCE=%TARGET_DIR%\release\bundle\nsis\Trispr Flow_%VERSION%_x64-setup.exe"
if not exist "%SOURCE%" (
    echo ERROR: Installer not found after variant build: %SOURCE%
    exit /b 1
)

set "TARGET_NAME=TrsprFlw.v%VERSION%.%VARIANT%.exe"
move /y "%SOURCE%" "installers\!TARGET_NAME!" >nul
if not "!ERRORLEVEL!"=="0" (
    echo ERROR: Failed to move variant installer: !TARGET_NAME!
    exit /b 1
)

if not defined TRISPR_SKIP_INSTALL_VALIDATION (
    pwsh -NoProfile -ExecutionPolicy Bypass -File "scripts\windows\validate-installed-installer.ps1" -InstallerPath "installers\!TARGET_NAME!" -Variant %VARIANT%
    if not "!ERRORLEVEL!"=="0" (
        echo ERROR: Installed installer validation failed for !TARGET_NAME!.
        exit /b 1
    )
)

echo     OK -^> installers\!TARGET_NAME!
exit /b 0

:fail
echo.
echo ========================================
echo BUILD FAILED
echo ========================================
popd
endlocal
exit /b 1
