@echo off
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\build-rust.ps1"
if errorlevel 1 goto done
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\build-host.ps1"
:done
set CODV_RC=%errorlevel%
rem Double-clicked from Explorer: keep the window open so the result can be read.
if not defined CODV_NO_PAUSE echo %cmdcmdline% | find /i "/c" >nul && pause
exit /b %CODV_RC%
