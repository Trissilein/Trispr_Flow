@echo off
setlocal
cd /d "%~dp0"

where powershell >nul 2>&1
if errorlevel 1 (
  echo ERROR: PowerShell not found. Please install PowerShell 5.1+.
  exit /b 1
)

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\first-run.ps1" %*
set "EXIT_CODE=%ERRORLEVEL%"

if not "%EXIT_CODE%"=="0" (
  echo.
  echo First run bootstrap failed with exit code %EXIT_CODE%.
  exit /b %EXIT_CODE%
)

echo.
echo First run bootstrap completed.
exit /b 0
