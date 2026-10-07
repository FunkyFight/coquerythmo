# Publie une release de Coquerythmo sur GitHub (FunkyFight/coquerythmo-releases)
# avec le zip portable Windows, l'installateur et VERSION_NOTES.md comme notes.
#
# Usage : powershell -ExecutionPolicy Bypass -File publish.ps1 [version] [-ResetToken]
#
# Sans version, celle de Cargo.toml est utilisee. Le jeton GitHub n'est demande
# qu'une seule fois : il est ensuite conserve chiffre (DPAPI, lisible uniquement
# par cet utilisateur Windows) dans %APPDATA%\coquerythmo-publish. La variable
# d'environnement COQUERYTHMO_GITHUB_TOKEN, si elle existe, a la priorite.
param(
    [string]$Version,
    [switch]$ResetToken
)

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

$Repo = 'FunkyFight/coquerythmo-releases'
$TokenFile = Join-Path $env:APPDATA 'coquerythmo-publish\github-token'

[Net.ServicePointManager]::SecurityProtocol =
    [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
Add-Type -AssemblyName System.Net.Http

if (-not $Version) {
    $match = Select-String -Path Cargo.toml -Pattern '^version = "([^"]+)"' | Select-Object -First 1
    if (-not $match) { throw 'Version introuvable dans Cargo.toml.' }
    $Version = $match.Matches[0].Groups[1].Value
}
$Tag = "v$Version"

$Assets = @(
    "target\release\coquerythmo-$Tag-windows-portable.zip",
    'target\release\Output\Coquerythmo-Installer.exe'
)
foreach ($asset in $Assets) {
    if (-not (Test-Path $asset)) { throw "Fichier introuvable : $asset" }
}
# Un installateur plus ancien que l'exe installerait une version precedente :
# il faut le recompiler (build_installer.ps1, lance par package.bat).
$exe = Get-Item 'target\release\coquerythmo.exe'
$installer = Get-Item 'target\release\Output\Coquerythmo-Installer.exe'
if ($installer.LastWriteTime -lt $exe.LastWriteTime) {
    throw ("Installateur perime ({0:g}) : plus ancien que coquerythmo.exe ({1:g}). " +
        "Lance build_installer.ps1 $Version.") -f $installer.LastWriteTime, $exe.LastWriteTime
}

$Notes = Get-Content VERSION_NOTES.md -Raw -Encoding UTF8

Write-Host ''
Write-Host "Release $Tag -> https://github.com/$Repo" -ForegroundColor Cyan
foreach ($asset in $Assets) {
    Write-Host ("  {0} ({1:N1} Mo)" -f $asset, ((Get-Item $asset).Length / 1MB))
}
Write-Host '  Notes : VERSION_NOTES.md'
if ($Notes -notmatch "^\s*#\s*$([regex]::Escape($Version))\s*\r?\n") {
    Write-Host "Attention : VERSION_NOTES.md ne commence pas par '# $Version'." -ForegroundColor Yellow
}
Write-Host 'Les utilisateurs recevront la mise a jour des la publication.' -ForegroundColor Yellow
if ((Read-Host 'Publier ? (o/N)') -notmatch '^[oOyY]') {
    Write-Host 'Publication annulee.'
    exit 0
}

# --- Jeton -------------------------------------------------------------------

function Read-SavedToken {
    if (-not (Test-Path $TokenFile)) { return $null }
    try {
        $secure = (Get-Content $TokenFile -Raw).Trim() | ConvertTo-SecureString
        $bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
        try { return [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr) }
        finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr) }
    } catch {
        return $null
    }
}

function Request-Token {
    Write-Host ''
    Write-Host 'Jeton GitHub requis (demande une seule fois).' -ForegroundColor Cyan
    Write-Host 'Cree un fine-grained token sur https://github.com/settings/personal-access-tokens/new :'
    Write-Host "  - Repository access : Only select repositories > $Repo"
    Write-Host '  - Repository permissions > Contents : Read and write'
    Write-Host '  - Expiration : la plus longue possible'
    $secure = Read-Host 'Colle le jeton' -AsSecureString
    if ($secure.Length -eq 0) { throw 'Aucun jeton fourni.' }
    New-Item -ItemType Directory -Force (Split-Path $TokenFile) | Out-Null
    $secure | ConvertFrom-SecureString | Set-Content $TokenFile
    return Read-SavedToken
}

# --- API GitHub --------------------------------------------------------------

$Client = New-Object System.Net.Http.HttpClient
$Client.Timeout = [TimeSpan]::FromHours(1)
$Client.DefaultRequestHeaders.UserAgent.ParseAdd('coquerythmo-publish')
$Client.DefaultRequestHeaders.Accept.ParseAdd('application/vnd.github+json')
$Client.DefaultRequestHeaders.Add('X-GitHub-Api-Version', '2022-11-28')

function Set-Token([string]$Token) {
    $Client.DefaultRequestHeaders.Authorization =
        New-Object System.Net.Http.Headers.AuthenticationHeaderValue('Bearer', $Token)
}

function Send-Request([string]$Method, [string]$Url, $Content = $null) {
    $request = New-Object System.Net.Http.HttpRequestMessage(
        (New-Object System.Net.Http.HttpMethod $Method), $Url)
    if ($Content) { $request.Content = $Content }
    $response = $Client.SendAsync($request).GetAwaiter().GetResult()
    $text = $response.Content.ReadAsStringAsync().GetAwaiter().GetResult()
    [pscustomobject]@{
        Status = [int]$response.StatusCode
        Ok     = $response.IsSuccessStatusCode
        Json   = if ($text) { $text | ConvertFrom-Json } else { $null }
    }
}

function Invoke-GitHub([string]$Method, [string]$Path, $Body = $null) {
    $content = $null
    if ($null -ne $Body) {
        $json = $Body | ConvertTo-Json -Compress -Depth 5
        $content = New-Object System.Net.Http.StringContent($json, [Text.Encoding]::UTF8, 'application/json')
    }
    $result = Send-Request $Method "https://api.github.com/repos/$Repo$Path" $content
    if (-not $result.Ok) {
        throw "GitHub a repondu $($result.Status) a $Method $Path : $($result.Json.message)"
    }
    $result.Json
}

function Send-Asset($Release, [string]$Path) {
    $name = Split-Path $Path -Leaf
    $existing = $Release.assets | Where-Object { $_.name -eq $name }
    foreach ($asset in $existing) {
        Write-Host "Suppression de l'ancien $name..."
        Invoke-GitHub DELETE "/releases/assets/$($asset.id)" | Out-Null
    }
    Write-Host ("Envoi de {0} ({1:N1} Mo)..." -f $name, ((Get-Item $Path).Length / 1MB))
    $stream = [IO.File]::OpenRead((Resolve-Path $Path).Path)
    try {
        $content = New-Object System.Net.Http.StreamContent($stream)
        $content.Headers.ContentType =
            New-Object System.Net.Http.Headers.MediaTypeHeaderValue('application/octet-stream')
        $url = "https://uploads.github.com/repos/$Repo/releases/$($Release.id)/assets?name=$([Uri]::EscapeDataString($name))"
        $result = Send-Request POST $url $content
        if (-not $result.Ok) {
            throw "Envoi de $name refuse ($($result.Status)) : $($result.Json.message)"
        }
    } finally {
        $stream.Dispose()
    }
}

# --- Publication -------------------------------------------------------------

$token = $env:COQUERYTHMO_GITHUB_TOKEN
if (-not $token) {
    if ($ResetToken -and (Test-Path $TokenFile)) { Remove-Item $TokenFile }
    $token = Read-SavedToken
    if (-not $token) { $token = Request-Token }
}
Set-Token $token
if ((Send-Request GET "https://api.github.com/repos/$Repo").Status -eq 401) {
    if ($env:COQUERYTHMO_GITHUB_TOKEN) { throw 'COQUERYTHMO_GITHUB_TOKEN est invalide ou expire.' }
    Write-Host 'Le jeton enregistre est invalide ou expire.' -ForegroundColor Yellow
    Remove-Item $TokenFile -ErrorAction SilentlyContinue
    Set-Token (Request-Token)
}

# Les brouillons n'apparaissent pas dans /releases/tags/{tag} : on cherche dans
# la liste pour reprendre une publication interrompue.
$release = Invoke-GitHub GET '/releases?per_page=100' | Where-Object { $_.tag_name -eq $Tag } |
    Select-Object -First 1
if ($release) {
    Write-Host "La release $Tag existe deja : notes mises a jour, fichiers remplaces."
    $release = Invoke-GitHub PATCH "/releases/$($release.id)" @{ body = $Notes }
} else {
    # Creee en brouillon pour que l'updater ne voie jamais une release sans zip.
    $release = Invoke-GitHub POST '/releases' @{
        tag_name = $Tag
        name     = "Coquerythmo $Tag"
        body     = $Notes
        draft    = $true
    }
}

foreach ($asset in $Assets) {
    Send-Asset $release $asset
}

if ($release.draft) {
    $release = Invoke-GitHub PATCH "/releases/$($release.id)" @{ draft = $false; make_latest = 'true' }
}

Write-Host ''
Write-Host "Publie : $($release.html_url)" -ForegroundColor Green
