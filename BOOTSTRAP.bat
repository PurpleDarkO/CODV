@echo off
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\bootstrap.ps1" %*
set CODV_RC=%errorlevel%
rem Double-clicked from Explorer: keep the window open so the result can be read.
if not defined CODV_NO_PAUSE echo %cmdcmdline% | find /i "/c" >nul && pause
exit /b %CODV_RC%
