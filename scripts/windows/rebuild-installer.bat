@echo off
setlocal

for %%I in ("%~dp0..\..") do set "ROOT=%%~fI"
pushd "%ROOT%" >nul 2>&1
if %ERRORLEVEL% neq 0 (
    echo ERROR: Failed to change directory to repo root: %ROOT%
    exit /b 1
)

call "%~dp0build-installers.bat" %*
set "BUILD_RESULT=%ERRORLEVEL%"

if "%BUILD_RESULT%"=="0" (
    start explorer "%ROOT%\installers"
)

popd
endlocal
exit /b %BUILD_RESULT%
