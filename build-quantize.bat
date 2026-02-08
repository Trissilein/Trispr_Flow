@echo off
setlocal enabledelayedexpansion
REM Build quantize.exe from whisper.cpp
REM This script configures and builds quantize.exe for CPU (universal compatibility)

echo ========================================
echo Building quantize.exe from whisper.cpp
echo ========================================
echo.

REM Check if whisper.cpp exists
if not exist "..\whisper.cpp" (
    echo ERROR: whisper.cpp repository not found at ..\whisper.cpp
    echo Please clone it first: git clone https://github.com/ggerganov/whisper.cpp.git
    pause
    exit /b 1
)

REM Find CMake (in PATH or standard installation locations)
set "CMAKE_EXE=cmake"
where cmake >nul 2>&1
if %ERRORLEVEL% neq 0 (
    echo CMake not in PATH, searching in standard locations...
    if exist "C:\Program Files\CMake\bin\cmake.exe" (
        set "CMAKE_EXE=C:\Program Files\CMake\bin\cmake.exe"
        echo Found CMake at: !CMAKE_EXE!
    ) else if exist "C:\Program Files (x86)\CMake\bin\cmake.exe" (
        set "CMAKE_EXE=C:\Program Files (x86)\CMake\bin\cmake.exe"
        echo Found CMake at: !CMAKE_EXE!
    ) else (
        echo ERROR: CMake not found
        echo.
        echo Please install CMake first:
        echo   winget install --id Kitware.CMake -e
        echo.
        echo After installation, restart this script.
        pause
        exit /b 1
    )
) else (
    echo CMake found in PATH
)

echo CMake version:
"%CMAKE_EXE%" --version | findstr /C:"version"
echo.

REM Configure CMake for CPU build (universal compatibility)
echo Configuring whisper.cpp for CPU build...
"%CMAKE_EXE%" -S ..\whisper.cpp -B ..\whisper.cpp\build-cpu ^
    -DGGML_SHARED=OFF ^
    -DBUILD_SHARED_LIBS=OFF ^
    -DGGML_CUDA=OFF ^
    -DGGML_VULKAN=OFF ^
    -DWHISPER_BUILD_TESTS=OFF ^
    -DWHISPER_BUILD_EXAMPLES=ON

if %ERRORLEVEL% neq 0 (
    echo ERROR: CMake configuration failed
    pause
    exit /b %ERRORLEVEL%
)

echo.
echo Building whisper.cpp (Release configuration)...
echo This may take a few minutes on first build...
"%CMAKE_EXE%" --build ..\whisper.cpp\build-cpu --config Release

if %ERRORLEVEL% neq 0 (
    echo ERROR: Build failed
    pause
    exit /b %ERRORLEVEL%
)

echo.
echo ========================================
echo Build successful!
echo ========================================
echo.

REM Find the built quantize.exe (may be named whisper-quantize.exe)
set "QUANTIZE_SRC="
for %%P in (
    "..\whisper.cpp\build-cpu\bin\Release\whisper-quantize.exe"
    "..\whisper.cpp\build-cpu\bin\Release\quantize.exe"
    "..\whisper.cpp\build-cpu\bin\whisper-quantize.exe"
    "..\whisper.cpp\build-cpu\examples\quantize\Release\quantize.exe"
    "..\whisper.cpp\build-cpu\examples\Release\quantize.exe"
) do (
    if exist "%%~P" (
        set "QUANTIZE_SRC=%%~P"
    )
)

if not defined QUANTIZE_SRC (
    echo ERROR: Built quantize.exe not found in expected locations
    echo Please check ..\whisper.cpp\build-cpu\ directory
    pause
    exit /b 1
)

echo Found quantize.exe at: %QUANTIZE_SRC%
echo.

REM Copy to Trispr Flow bin directory
if not exist "src-tauri\bin" (
    mkdir "src-tauri\bin"
)

echo Copying to src-tauri\bin\quantize.exe...
copy /Y "%QUANTIZE_SRC%" "src-tauri\bin\quantize.exe"

if %ERRORLEVEL% neq 0 (
    echo ERROR: Failed to copy quantize.exe
    pause
    exit /b %ERRORLEVEL%
)

echo.
echo ========================================
echo SUCCESS!
echo ========================================
echo.
echo quantize.exe is now ready at:
echo   %~dp0src-tauri\bin\quantize.exe
echo.
echo You can now use the Optimize button in Trispr Flow.
echo.
pause
endlocal
