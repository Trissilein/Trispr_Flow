@echo off
setlocal
call "%~dp0scripts\windows\FIRST_RUN.bat" %*
exit /b %ERRORLEVEL%
