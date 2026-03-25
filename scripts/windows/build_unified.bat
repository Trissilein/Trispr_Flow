@echo off
setlocal
REM Backward-compatible entrypoint:
REM "build_unified" now builds all maintained installer variants.
call "%~dp0build-installers.bat" %*
exit /b %ERRORLEVEL%
