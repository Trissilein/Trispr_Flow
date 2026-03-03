@echo off
setlocal
REM Trispr Flow Installer Rebuild Script
REM Builds the NSIS installer and opens the output folder

REM Always run from the repo root (directory of this script)
for %%I in ("%~dp0.") do set "ROOT=%%~fI"
pushd "%ROOT%" >nul 2>&1
if %ERRORLEVEL% neq 0 (
    echo ERROR: Failed to change directory to repo root: %ROOT%
    pause
    exit /b 1
)

echo ========================================
echo Trispr Flow Installer Rebuild
echo ========================================
echo.

REM Check if we're in the right location
if not exist "%ROOT%\package.json" (
    echo ERROR: Cannot find package.json - not in project root!
    echo Current path: %ROOT%
    pause
    popd
    exit /b 1
)

if not exist "%ROOT%\src-tauri\Cargo.toml" (
    echo ERROR: Cannot find src-tauri\Cargo.toml - project structure invalid!
    pause
    popd
    exit /b 1
)
set "NSIS_OUT_DIR=%ROOT%\src-tauri\target\release\bundle\nsis"

echo Building Tauri installer...
echo.
echo This will:
echo   1. Run 'npm run build' (Vite frontend build)
echo   2. Compile Rust backend (cargo build --release)
echo   3. Create NSIS installer with CUDA and Vulkan backends
echo.
echo Estimated time: 30-60 seconds
echo.

REM Try to bundle quantize.exe if available
if not exist "%ROOT%\src-tauri\bin" (
    mkdir "%ROOT%\src-tauri\bin" >nul 2>&1
)

set "WHISPER_ROOT_HINT="
if defined TRISPR_WHISPER_ROOT set "WHISPER_ROOT_HINT=%TRISPR_WHISPER_ROOT%"
if not defined WHISPER_ROOT_HINT if defined WHISPER_ROOT set "WHISPER_ROOT_HINT=%WHISPER_ROOT%"
if not defined WHISPER_ROOT_HINT set "WHISPER_ROOT_HINT=%ROOT%\..\whisper.cpp"

set "QUANTIZE_SRC="
for %%P in (
    "%WHISPER_ROOT_HINT%\build-cpu\bin\Release\whisper-quantize.exe"
    "%WHISPER_ROOT_HINT%\build\bin\Release\whisper-quantize.exe"
    "%WHISPER_ROOT_HINT%\build-cuda\bin\Release\whisper-quantize.exe"
    "%WHISPER_ROOT_HINT%\build-vulkan\bin\Release\whisper-quantize.exe"
    "%WHISPER_ROOT_HINT%\build\bin\Release\quantize.exe"
    "%WHISPER_ROOT_HINT%\build\bin\quantize.exe"
    "%WHISPER_ROOT_HINT%\build-cpu\bin\Release\quantize.exe"
    "%WHISPER_ROOT_HINT%\build-cpu\bin\quantize.exe"
    "%WHISPER_ROOT_HINT%\build-cuda\bin\Release\quantize.exe"
    "%WHISPER_ROOT_HINT%\build-cuda\bin\quantize.exe"
    "%WHISPER_ROOT_HINT%\build-vulkan\bin\Release\quantize.exe"
    "%WHISPER_ROOT_HINT%\build-vulkan\bin\quantize.exe"
) do (
    if exist "%%~P" (
        set "QUANTIZE_SRC=%%~P"
    )
)

if defined QUANTIZE_SRC (
    echo Bundling quantize.exe from:
    echo   %QUANTIZE_SRC%
    copy /Y "%QUANTIZE_SRC%" "%ROOT%\src-tauri\bin\quantize.exe" >nul 2>&1
) else (
    echo WARNING: quantize.exe not found. Optimize button will be unavailable in installer.
)

REM Run the build command
call npm run tauri build

if %ERRORLEVEL% neq 0 (
    echo.
    echo ========================================
    echo BUILD FAILED!
    echo ========================================
    echo.
    echo Check the error messages above for details.
    pause
    popd
    exit /b %ERRORLEVEL%
)

echo.
echo ========================================
echo BUILD SUCCESSFUL!
echo ========================================
echo.
set "INSTALLER_PATH="
for /f "delims=" %%F in ('dir /b /a:-d /o-d "%NSIS_OUT_DIR%\Trispr Flow_*_x64-setup.exe" 2^>nul') do (
    if not defined INSTALLER_PATH set "INSTALLER_PATH=%NSIS_OUT_DIR%\%%F"
)

echo Installer created:
if defined INSTALLER_PATH (
    echo %INSTALLER_PATH%
) else (
    echo WARNING: Installer file not found in %NSIS_OUT_DIR%
)
echo.

REM Also copy to nsis folder for convenience
if not exist "%NSIS_OUT_DIR%\rebuild-installer.bat" (
    copy "%~f0" "%NSIS_OUT_DIR%\rebuild-installer.bat" >nul 2>&1
)

REM Open the folder in Explorer
start explorer "%NSIS_OUT_DIR%\"

echo Opened installer folder in Explorer.
echo.
echo Press any key to exit...
pause >nul

popd
endlocal
