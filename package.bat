@echo off
setlocal enabledelayedexpansion

set /p VERSION="Version (ex: 1.1.0): "
if "%VERSION%"=="" (
    echo Version requise.
    pause
    exit /b 1
)

echo.
echo === Setting version to %VERSION% ===
powershell -File set_version.ps1 %VERSION%

echo.
echo === Building coquerythmo (release) ===
cargo build --release
if errorlevel 1 (
    echo Build coquerythmo failed!
    pause
    exit /b 1
)

echo.
echo === Building updater (release) ===
cd updater
cargo build --release
if errorlevel 1 (
    echo Build updater failed!
    pause
    exit /b 1
)
cd ..

echo.
echo === Copying updater.exe to release folder ===
copy /Y updater\target\release\updater.exe target\release\updater.exe

echo.
echo === Done! ===
echo   target\release\coquerythmo.exe
echo   target\release\updater.exe
echo   Version: %VERSION%
pause
