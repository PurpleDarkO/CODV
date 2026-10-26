@echo off
rem Builds CODV and installs it into GTA V (GTA V and MW2 found automatically: Steam, Rockstar, Epic).
rem   INSTALL.bat ["<GTA V folder>"] [-Mw2 "<MW2 folder>"] [-SkipBuild] [-SkipCheck]
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\build-and-install.ps1" %*
set CODV_RC=%errorlevel%
rem Double-clicked from Explorer: keep the window open so the result can be read.
if not defined CODV_NO_PAUSE echo %cmdcmdline% | find /i "/c" >nul && pause
exit /b %CODV_RC%
