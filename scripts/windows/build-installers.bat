@echo off
setlocal enabledelayedexpansion

REM ========================================
REM Trispr Flow - Multi-Variant Installer Builder
REM ========================================
REM Variants:
REM   - vulkan       (no CUDA runtime payload)
REM   - cuda-lite    (CUDA without cublasLt64_13.dll, Vulkan included)
REM   - cuda-complete(CUDA + cublasLt64_13.dll, Vulkan included)

for %%I in ("%~dp0..\..") do set "ROOT=%%~fI"
pushd "%ROOT%" >nul 2>&1
if not "!ERRORLEVEL!"=="0" (
    echo ERROR: Failed to switch to repo root: %ROOT%
    exit /b 1
)

set "VARIANTS=%*"
if "%VARIANTS%"=="" set "VARIANTS=vulkan cuda-lite cuda-complete"

echo.
echo ========================================
echo Trispr Flow - Multi-Variant Builder
echo ========================================
echo.
echo Variants: %VARIANTS%
echo.

echo [1/7] Detecting version...
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

echo [2/7] Preparing output directories...
if not exist "installers" mkdir "installers"
if exist "src-tauri\target\release\bundle\nsis" (
    rmdir /s /q "src-tauri\target\release\bundle\nsis" 2>nul
)
echo   OK: Output directories ready
echo.

echo [3/7] Building frontend...
call npm run build
if not "!ERRORLEVEL!"=="0" goto :fail
echo   OK: Frontend build successful
echo.

echo [4/6] Building variants...
for %%V in (%VARIANTS%) do (
    call :build_variant %%V
    if not "!ERRORLEVEL!"=="0" goto :fail
)
echo.

echo [5/6] Build summary...
for %%V in (%BUILT_INSTALLERS%) do (
    if exist "%%~fV" (
        for %%A in ("%%~fV") do (
            set SIZE=%%~zA
            set /a SIZE_MB=!SIZE! / 1048576
            echo   %%~nxA ^(!SIZE_MB! MB^)
        )
    )
)
echo.

echo [6/6] Completed
echo ========================================
echo Output directory:
echo   %CD%\installers\
echo ========================================
echo.

popd
endlocal
exit /b 0

:build_variant
set "RAW_VARIANT=%~1"
set "VARIANT=%RAW_VARIANT%"
if /i "%VARIANT%"=="vulkan-only" set "VARIANT=vulkan"
if /i "%VARIANT%"=="unified" set "VARIANT=cuda-complete"

if /i not "%VARIANT%"=="vulkan" if /i not "%VARIANT%"=="cuda-lite" if /i not "%VARIANT%"=="cuda-complete" (
    echo ERROR: Unknown variant '%RAW_VARIANT%'. Allowed: vulkan, cuda-lite, cuda-complete
    exit /b 1
)

set "CFG=src-tauri\tauri.conf.variant.%VARIANT%.json"
echo   - variant=%VARIANT%

node scripts\generate-tauri-variant-config.mjs --variant %VARIANT% --out %CFG%
if not "!ERRORLEVEL!"=="0" (
    echo ERROR: Failed to generate Tauri config for variant %VARIANT%.
    exit /b 1
)

node scripts\validate-whisper-runtime.mjs --variant %VARIANT%
if not "!ERRORLEVEL!"=="0" (
    echo ERROR: Whisper runtime validation failed for variant %VARIANT%.
    del /q "%CFG%" >nul 2>&1
    exit /b 1
)

REM --- FFmpeg + Piper only bundled in cuda-complete (offline installer) ---
if /i "%VARIANT%"=="cuda-complete" (
    echo   [setup] Ensuring FFmpeg runtime for cuda-complete...
    powershell -NoProfile -ExecutionPolicy Bypass -File "scripts\setup-ffmpeg.ps1"
    if not "!ERRORLEVEL!"=="0" (
        echo ERROR: FFmpeg setup failed for cuda-complete.
        del /q "%CFG%" >nul 2>&1
        exit /b 1
    )
    echo   [setup] Ensuring Piper runtime for cuda-complete...
    powershell -NoProfile -ExecutionPolicy Bypass -File "scripts\setup-piper.ps1"
    if not "!ERRORLEVEL!"=="0" (
        echo ERROR: Piper setup failed for cuda-complete.
        del /q "%CFG%" >nul 2>&1
        exit /b 1
    )
) else (
    echo   [skip] FFmpeg/Piper bundling skipped for %VARIANT% ^(on-demand download^)
)

REM --- Inject compile-time variant define for hooks.nsh ---
echo !define TRISPR_VARIANT "%VARIANT%" > "src-tauri\nsis\variant-define.nsh"

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

set "SOURCE=src-tauri\target\release\bundle\nsis\Trispr Flow_%VERSION%_x64-setup.exe"
if not exist "%SOURCE%" (
    echo ERROR: Installer not found after variant build: %SOURCE%
    exit /b 1
)

set "LABEL=%VARIANT%"
if /i "%VARIANT%"=="vulkan" set "LABEL=vulkan-only"
set "TARGET_NAME=TrsprFlw.v%VERSION%.!LABEL!-%BUILDSTAMP%.exe"
if exist "installers\!TARGET_NAME!" (
    set /a SUFFIX=!RANDOM! %% 100
    if !SUFFIX! lss 10 set "SUFFIX=0!SUFFIX!"
    set "TARGET_NAME=TrsprFlw.v%VERSION%.!LABEL!-%BUILDSTAMP%-!SUFFIX!.exe"
)

move "%SOURCE%" "installers\!TARGET_NAME!" >nul
if not "!ERRORLEVEL!"=="0" (
    echo ERROR: Failed to move variant installer: !TARGET_NAME!
    exit /b 1
)

echo     OK -^> installers\!TARGET_NAME!
set "BUILT_INSTALLERS=!BUILT_INSTALLERS! installers\!TARGET_NAME!"
exit /b 0

:fail
echo.
echo ========================================
echo BUILD FAILED
echo ========================================
popd
endlocal
exit /b 1
