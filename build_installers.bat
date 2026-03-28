@echo off
setlocal
call "%~dp0scripts\windows\build-installers.bat" %*
exit /b %ERRORLEVEL%
