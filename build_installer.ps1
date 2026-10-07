# Compile l'installateur Windows (installer.iss) avec Inno Setup 6.
# Produit target\release\Output\Coquerythmo-Installer.exe, publie par publish.ps1.
#
# Usage : powershell -ExecutionPolicy Bypass -File build_installer.ps1 [version]
#
# Sans version, celle de Cargo.toml est utilisee. ISCC.exe est cherche dans
# INNO_SETUP_ISCC, le PATH, le registre puis les dossiers d'installation usuels.
param([string]$Version)

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

if (-not $Version) {
    $match = Select-String -Path Cargo.toml -Pattern '^version = "([^"]+)"' | Select-Object -First 1
    if (-not $match) { throw 'Version introuvable dans Cargo.toml.' }
    $Version = $match.Matches[0].Groups[1].Value
}

function Find-Iscc {
    if ($env:INNO_SETUP_ISCC -and (Test-Path $env:INNO_SETUP_ISCC)) { return $env:INNO_SETUP_ISCC }
    $command = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    $keys = @(
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Inno Setup 6_is1',
        'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Inno Setup 6_is1',
        'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Inno Setup 6_is1'
    )
    foreach ($key in $keys) {
        $location = (Get-ItemProperty $key -ErrorAction SilentlyContinue).InstallLocation
        if ($location -and (Test-Path (Join-Path $location 'ISCC.exe'))) { return Join-Path $location 'ISCC.exe' }
    }
    foreach ($dir in @(${env:ProgramFiles(x86)}, $env:ProgramFiles, "$env:LOCALAPPDATA\Programs")) {
        if (-not $dir) { continue }
        $candidate = Join-Path $dir 'Inno Setup 6\ISCC.exe'
        if (Test-Path $candidate) { return $candidate }
    }
    throw 'ISCC.exe (Inno Setup 6) introuvable. Installe Inno Setup ou definis INNO_SETUP_ISCC.'
}

$iscc = Find-Iscc
$output = 'target\release\Output\Coquerythmo-Installer.exe'
# Supprime l'ancien installateur : en cas d'echec, publish.ps1 ne peut pas
# publier par erreur celui d'une version precedente.
Remove-Item $output -ErrorAction SilentlyContinue

Write-Host "Compilation de l'installateur $Version avec $iscc"
& $iscc /Q "/DMyAppVersion=$Version" installer.iss
if ($LASTEXITCODE -ne 0) { throw "ISCC a echoue (code $LASTEXITCODE)." }
if (-not (Test-Path $output)) { throw "Installateur introuvable apres compilation : $output" }
Write-Host ("  {0} ({1:N1} Mo)" -f $output, ((Get-Item $output).Length / 1MB))
