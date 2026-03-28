@echo off
setlocal
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\windows\upload-release-assets.ps1" %*
exit /b %ERRORLEVEL%
