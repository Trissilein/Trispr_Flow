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

echo Building Tauri installer...
echo.
echo This will:
echo   1. Run 'npm run build' (Vite frontend build)
echo   2. Compile Rust backend (cargo build --release)
echo   3. Create NSIS installer with CUDA and Vulkan backends
echo.
echo Estimated time: 30-60 seconds
echo.

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
echo Installer created:
echo %ROOT%\src-tauri\target\release\bundle\nsis\Trispr Flow_0.1.0_x64-setup.exe
echo.

REM Also copy to nsis folder for convenience
if not exist "%ROOT%\src-tauri\target\release\bundle\nsis\rebuild-installer.bat" (
    copy "%~f0" "%ROOT%\src-tauri\target\release\bundle\nsis\rebuild-installer.bat" >nul 2>&1
)

REM Open the folder in Explorer
start explorer "%ROOT%\src-tauri\target\release\bundle\nsis\"

echo Opened installer folder in Explorer.
echo.
echo Press any key to exit...
pause >nul

popd
endlocal
