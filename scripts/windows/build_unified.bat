@echo off
setlocal enabledelayedexpansion

REM ========================================
REM Trispr Flow - Unified Custom Installer Build
REM ========================================
REM Builds a unified installer containing both CUDA and Vulkan endpoints
REM and automatically removes the unused set upon installation.

for %%I in ("%~dp0..\..") do set "ROOT=%%~fI"
pushd "%ROOT%" >nul 2>&1
if not "!ERRORLEVEL!"=="0" (
    echo ERROR: Failed to switch to repo root: %ROOT%
    exit /b 1
)

echo.
echo ========================================
echo Trispr Flow - Unified Installer Builder
echo ========================================
echo.

echo [1/5] Detecting version...
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

echo [2/5] Preparing output directories...
if not exist "installers" mkdir "installers"
if exist "src-tauri\target\release\bundle\nsis" (
    rmdir /s /q "src-tauri\target\release\bundle\nsis" 2>nul
)
echo   OK: Output directories ready
echo.

echo [3/5] Building frontend...
call npm run build
if not "!ERRORLEVEL!"=="0" goto :fail
echo   OK: Frontend build successful
echo.

echo [4/6] Ensuring FFmpeg runtime...
powershell -NoProfile -ExecutionPolicy Bypass -File "scripts\setup-ffmpeg.ps1"
if not "!ERRORLEVEL!"=="0" (
    echo ERROR: FFmpeg setup failed.
    goto :fail
)
echo   OK: FFmpeg ready
echo.

echo [5/6] Building Unified Installer...
set "SOURCE=src-tauri\target\release\bundle\nsis\Trispr Flow_%VERSION%_x64-setup.exe"
set "TARGET_NAME=TrsprFlw.v%VERSION%.unified-%BUILDSTAMP%.exe"

call npm run tauri build
set "BUILD_RESULT=!ERRORLEVEL!"

if not "!BUILD_RESULT!"=="0" (
    echo ERROR: Build failed for Unified Installer
    exit /b 1
)

if not exist "%SOURCE%" (
    echo ERROR: Installer not found after Unified build:
    echo   %SOURCE%
    exit /b 1
)

if exist "installers\!TARGET_NAME!" (
    set /a SUFFIX=!RANDOM! %% 100
    if !SUFFIX! lss 10 set "SUFFIX=0!SUFFIX!"
    set "TARGET_NAME=TrsprFlw.v%VERSION%.unified-%BUILDSTAMP%-!SUFFIX!.exe"
)

move "%SOURCE%" "installers\!TARGET_NAME!" >nul
if not "!ERRORLEVEL!"=="0" (
    echo ERROR: Failed to move Unified installer
    exit /b 1
)

set "LAST_TARGET=installers\!TARGET_NAME!"
echo   OK: UNIFIED -^> !LAST_TARGET!
echo.

echo [6/6] Build summary...
if exist "!LAST_TARGET!" (
    for %%A in ("!LAST_TARGET!") do (
        set SIZE=%%~zA
        set /a SIZE_MB=!SIZE! / 1048576
        echo   UNIFIED: %%~nxA ^(!SIZE_MB! MB^)
    )
) else (
    echo   UNIFIED: MISSING
)
echo.

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

:fail
echo.
echo ========================================
echo BUILD FAILED
echo ========================================
popd
endlocal
exit /b 1
