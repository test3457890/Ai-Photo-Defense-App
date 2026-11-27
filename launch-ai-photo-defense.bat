@echo off
setlocal
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0launch-ai-photo-defense.ps1" %*
if errorlevel 1 (
    echo.
    echo [ERROR] AI Photo Defense launcher exited with an error.
    pause
)
