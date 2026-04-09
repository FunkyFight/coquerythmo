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
echo === Creating zip: coquerythmo-v%VERSION%-windows-portable.zip ===
powershell -Command "Compress-Archive -Force -Path 'target\release\coquerythmo.exe','target\release\updater.exe','target\release\ffmpeg.exe','target\release\ffplay.exe','target\release\ffprobe.exe' -DestinationPath 'target\release\coquerythmo-v%VERSION%-windows-portable.zip'"
if errorlevel 1 (
    echo Zip creation failed!
    pause
    exit /b 1
)

echo.
echo === Done! ===
echo   target\release\coquerythmo-v%VERSION%-windows-portable.zip
pause
