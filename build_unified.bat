@echo off
setlocal
call "%~dp0scripts\windows\build_unified.bat" %*
exit /b %ERRORLEVEL%
