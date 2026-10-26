# Builds CODV and installs it into the GTA V folder in one go:
#   INSTALL.bat                                  (GTA V and MW2 found automatically: Steam, Rockstar, Epic)
#   INSTALL.bat "D:\Games\Grand Theft Auto V"
#   INSTALL.bat -SkipBuild                       (install the last build again)
#   INSTALL.bat -Mw2 "D:\Games\Modern Warfare 2"  (your MW2: its weapons are imported)
# It checks the requirements (and the GTA folder) once, then runs
#   .\BOOTSTRAP.bat -SkipCheck
#   py .\tools\prepare.py --gta "<GTA folder>" --install
# When the GTA folder needs administrator rights it asks to restart the install
# step elevated.
param(
    # Your GTA V Legacy folder (empty: found by tools\gta_locate.py).
    [string]$Gta = '',
    # Your Modern Warfare 2 (PC) folder, with zone\english: its weapons are read at setup (empty: found on
    # Steam, else the folder a previous install imported from).
    [string]$Mw2 = '',
    [switch]$SkipBuild,
    [switch]$SkipCheck
)
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
$Tools = $PSScriptRoot
$onWindows = [Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT
# The .bat files pause when double-clicked; called from here they must not.
$env:CODV_NO_PAUSE = '1'

function Step([string]$Text) { Write-Host ''; Write-Host "== $Text" -ForegroundColor Cyan }

if (-not (Get-Command py -ErrorAction SilentlyContinue)) {
    throw 'The Python launcher (py) was not found. Install Python 3.11+ from https://www.python.org/downloads/ (it includes py).'
}
# The folders not given: found where Steam, the Rockstar launcher or Epic installed them.
if (-not $Gta -or -not $Mw2) {
    $found = & py (Join-Path $Tools 'gta_locate.py') | ConvertFrom-Json
    if (-not $Gta) { $Gta = $found.gta }
    if (-not $Mw2) { $Mw2 = $found.mw2 }
}
if (-not $Gta -or -not (Test-Path -LiteralPath (Join-Path $Gta 'GTA5.exe'))) {
    throw "GTA V Legacy was not found$(if ($Gta) { " in '$Gta'" }). Pass its folder: INSTALL.bat `"<GTA V folder>`""
}
if ($Mw2) { Write-Host "Modern Warfare 2: $Mw2" }
else { Write-Host 'Modern Warfare 2 not found on Steam: the folder a previous install imported from is used, or pass -Mw2 "<folder>".' -ForegroundColor Yellow }
$mw2Args = @()
$mw2Shown = ''
$mw2Elevated = ''
if ($Mw2) {
    $mw2Args = @('--mw2', $Mw2)
    $mw2Shown = "--mw2 `"$Mw2`" "
    $mw2Elevated = "-Mw2 `"$Mw2`" "
}

# 1. Requirements: the build tools, plus the GTA folder for the install.
if (-not $SkipCheck) {
    Step 'Checking requirements'
    $checkArgs = @{ Gta = $Gta }
    if ($SkipBuild) { $checkArgs.Offline = $true }
    & (Join-Path $Tools 'check-requirements.ps1') @checkArgs
    if ($LASTEXITCODE -ne 0) { throw 'Requirements missing (listed above). Fix them, or rerun with -SkipCheck to try anyway.' }
}

# 2. Build: BOOTSTRAP fetches what is missing (nothing after the first time) and builds.
if (-not $SkipBuild) {
    Step '.\BOOTSTRAP.bat -SkipCheck'
    Push-Location $Root
    try { & .\BOOTSTRAP.bat -SkipCheck } finally { Pop-Location }
    if ($LASTEXITCODE -ne 0) { throw "BOOTSTRAP.bat failed (exit $LASTEXITCODE); see the messages above." }
}
foreach ($f in 'CodVLegacy.asi', 'CodVRuntime.dll') {
    if (-not (Test-Path -LiteralPath (Join-Path $Root "build\package\$f"))) { throw "build\package\$f is missing: build first (drop -SkipBuild)." }
}

# 3. Install.
Step "py .\tools\prepare.py --gta `"$Gta`" $mw2Shown--install"
if (Get-Process -Name GTA5 -ErrorAction SilentlyContinue) { throw 'GTA V is running. Close it and run INSTALL.bat again.' }

# The GTA folder may need administrator rights (Program Files and similar).
$writable = $true
try {
    $probe = Join-Path $Gta ('.codv-write-' + [Guid]::NewGuid().ToString('N'))
    [IO.File]::WriteAllText($probe, '')
    Remove-Item -LiteralPath $probe -Force
} catch { $writable = $false }
if (-not $writable) {
    $isAdmin = $false
    if ($onWindows) {
        $isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
            [Security.Principal.WindowsBuiltInRole]::Administrator)
    }
    if ($isAdmin) { throw "Cannot write to '$Gta' even as administrator." }
    Write-Host "Writing to '$Gta' needs administrator rights; restarting the install step elevated..." -ForegroundColor Yellow
    $argLine = "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`" -Gta `"$Gta`" $mw2Elevated-SkipBuild -SkipCheck"
    $p = Start-Process -FilePath 'powershell.exe' -ArgumentList $argLine -Verb RunAs -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "The elevated install failed (exit $($p.ExitCode))." }
    Write-Host 'Installed (elevated).' -ForegroundColor Green
    exit 0
}

Push-Location $Root
try { & py .\tools\prepare.py --gta $Gta @mw2Args --install } finally { Pop-Location }
if ($LASTEXITCODE -ne 0) { throw "prepare.py failed (exit $LASTEXITCODE); see the messages above." }

Write-Host ''
Write-Host 'CODV installed. Start GTA V Legacy Story Mode (BattlEye off) and press F10 or Back + RB to switch CODV on.' -ForegroundColor Green
