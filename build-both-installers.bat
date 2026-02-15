@echo off
setlocal enabledelayedexpansion
REM ========================================
REM Trispr Flow - Automated Dual Installer Build
REM ========================================
REM Builds both CUDA and Vulkan installer variants automatically
REM No user interaction required - fully automated

echo.
echo ========================================
echo Trispr Flow - Dual Installer Builder
echo ========================================
echo.

REM ========================================
REM Step 1: Extract version from package.json
REM ========================================
echo [1/8] Detecting version...
for /f "tokens=2 delims=:, " %%a in ('findstr /C:"\"version\"" package.json') do (
    set VERSION_RAW=%%a
)
REM Remove quotes
set VERSION=%VERSION_RAW:"=%
echo Found version: %VERSION%
echo.

REM ========================================
REM Step 2: Verify CUDA DLLs are present
REM ========================================
echo [2/8] Verifying CUDA runtime libraries...
set DLL_MISSING=0

if not exist "src-tauri\bin\cuda\cublas64_13.dll" (
    echo   ERROR: cublas64_13.dll not found!
    set DLL_MISSING=1
)
if not exist "src-tauri\bin\cuda\cudart64_13.dll" (
    echo   ERROR: cudart64_13.dll not found!
    set DLL_MISSING=1
)

if %DLL_MISSING%==1 (
    echo.
    echo CRITICAL ERROR: CUDA runtime DLLs are missing!
    echo Please copy them from: C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.0\bin\x64\
    echo.
    echo Required files:
    echo   - cublas64_13.dll
    echo   - cudart64_13.dll
    echo.
    echo Note: cublasLt64_13.dll is no longer required (removed to reduce size by 460MB)
    echo.
    pause
    exit /b 1
)

echo   ✓ cublas64_13.dll
echo   ✓ cudart64_13.dll
echo All CUDA DLLs present.
echo.

REM ========================================
REM Step 3: Clean previous builds
REM ========================================
echo [3/8] Cleaning previous builds...
if exist "src-tauri\target\release\bundle\nsis" (
    rmdir /s /q "src-tauri\target\release\bundle\nsis" 2>nul
    if errorlevel 1 (
        echo WARNING: Could not delete old builds in target dir
    ) else (
        echo   ✓ Removed old NSIS installers from target
    )
)

REM Clean and create installers directory in project root
if exist "installers" (
    del /q "installers\*.exe" 2>nul
    echo   ✓ Cleaned installers directory
) else (
    mkdir "installers"
    echo   ✓ Created installers directory
)
echo.

REM ========================================
REM Step 4: Build frontend
REM ========================================
echo [4/8] Building frontend (TypeScript + Vite)...
call npm run build
if errorlevel 1 (
    echo.
    echo ERROR: Frontend build failed!
    echo Check the npm build output above for errors.
    pause
    exit /b 1
)
echo   ✓ Frontend built successfully
echo.

REM ========================================
REM Step 5: Build CUDA Edition (Full)
REM ========================================
echo [5/8] Building CUDA Edition (Complete)...
echo   Backend: CUDA + Vulkan
echo   Size: ~110 MB (optimized CUDA runtime, cublasLt removed)
echo   Config: tauri.conf.json
echo.
call npm run tauri build -- --config src-tauri/tauri.conf.json
if errorlevel 1 (
    echo.
    echo ERROR: CUDA edition build failed!
    echo Check npm/tauri output above for errors.
    pause
    exit /b 1
)
echo   ✓ CUDA edition compiled
echo.

REM ========================================
REM Step 6: Move CUDA installer to installers/
REM ========================================
echo [6/8] Moving CUDA installer...
set CUDA_SOURCE=src-tauri\target\release\bundle\nsis\Trispr Flow_%VERSION%_x64-setup.exe
set CUDA_FILENAME=Trispr-Flow-v%VERSION%-CUDA-Setup.exe
set CUDA_TARGET=installers\%CUDA_FILENAME%

if exist "%CUDA_SOURCE%" (
    move "%CUDA_SOURCE%" "%CUDA_TARGET%" >nul
    if errorlevel 1 (
        echo   ERROR: Failed to move CUDA installer
        pause
        exit /b 1
    )
    echo   ✓ Moved to: installers\%CUDA_FILENAME%
) else (
    echo   ERROR: CUDA installer not found at: %CUDA_SOURCE%
    pause
    exit /b 1
)
echo.

REM ========================================
REM Step 7: Build Vulkan Edition (Lite)
REM ========================================
echo [7/8] Building Vulkan Edition (Lite)...
echo   Backend: Vulkan only
echo   Size: ~200 MB (no CUDA runtime)
echo   Config: tauri.conf.vulkan.json
echo.
call npm run tauri build -- --config src-tauri/tauri.conf.vulkan.json
if errorlevel 1 (
    echo.
    echo ERROR: Vulkan edition build failed!
    echo Check npm/tauri output above for errors.
    pause
    exit /b 1
)
echo   ✓ Vulkan edition compiled
echo.

REM ========================================
REM Step 8: Move Vulkan installer to installers/
REM ========================================
echo [8/8] Moving Vulkan installer...
set VULKAN_SOURCE=src-tauri\target\release\bundle\nsis\Trispr Flow_%VERSION%_x64-setup.exe
set VULKAN_FILENAME=Trispr-Flow-v%VERSION%-Vulkan-Setup.exe
set VULKAN_TARGET=installers\%VULKAN_FILENAME%

if exist "%VULKAN_SOURCE%" (
    move "%VULKAN_SOURCE%" "%VULKAN_TARGET%" >nul
    if errorlevel 1 (
        echo   ERROR: Failed to move Vulkan installer
        pause
        exit /b 1
    )
    echo   ✓ Moved to: installers\%VULKAN_FILENAME%
) else (
    echo   ERROR: Vulkan installer not found at: %VULKAN_SOURCE%
    pause
    exit /b 1
)
echo.

REM ========================================
REM Build Summary
REM ========================================
echo ========================================
echo ✓ Build Complete!
echo ========================================
echo.
echo Output directory:
echo   %CD%\installers\
echo.
echo Installers created:
echo.

REM Get file sizes and display
if exist "%CUDA_TARGET%" (
    for %%A in ("%CUDA_TARGET%") do (
        set SIZE=%%~zA
        set /a SIZE_MB=!SIZE! / 1048576
        echo   CUDA Edition (Full):
        echo     File: %CUDA_FILENAME%
        echo     Size: !SIZE_MB! MB
        echo     Backends: NVIDIA CUDA + Vulkan
        echo.
    )
) else (
    echo   ERROR: CUDA installer missing!
)

if exist "%VULKAN_TARGET%" (
    for %%A in ("%VULKAN_TARGET%") do (
        set SIZE=%%~zA
        set /a SIZE_MB=!SIZE! / 1048576
        echo   Vulkan Edition (Lite):
        echo     File: %VULKAN_FILENAME%
        echo     Size: !SIZE_MB! MB
        echo     Backends: Vulkan only
        echo.
    )
) else (
    echo   ERROR: Vulkan installer missing!
)

echo ========================================
echo.
echo Next steps:
echo   1. Test both installers on a clean system
echo   2. Verify CUDA edition on NVIDIA hardware
echo   3. Verify Vulkan edition on AMD/Intel hardware
echo   4. Upload both to release page
echo.
echo ========================================

endlocal
