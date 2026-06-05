@echo off
setlocal
cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\build-release.ps1" -SetupOnly %*
exit /b %ERRORLEVEL%
