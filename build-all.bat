@echo off
setlocal
cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\build-release.ps1" %*
rem 等价于 scripts\build-all.bat
exit /b %ERRORLEVEL%
