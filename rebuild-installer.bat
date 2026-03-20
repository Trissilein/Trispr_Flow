@echo off
setlocal
call "%~dp0scripts\windows\rebuild-installer.bat" %*
exit /b %ERRORLEVEL%
