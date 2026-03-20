@echo off
setlocal
call "%~dp0scripts\windows\build-quantize.bat" %*
exit /b %ERRORLEVEL%
