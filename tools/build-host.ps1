# Builds CodVLegacy.asi with CMake + MSVC, runs the host model tests and
# stages the ASI in build/package.
param([ValidateSet('Debug','Release')][string]$Config='Release', [switch]$NoTests)
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path (Join-Path $Root 'third_party/scripthookv/inc/main.h'))) { throw 'Run tools/fetch-scripthookv.ps1 first.' }
Push-Location $Root
try {
    & cmake --preset vs2022-x64
    if ($LASTEXITCODE -ne 0) { throw 'CMake configure failed' }
    & cmake --build (Join-Path $Root 'build/host') --config $Config
    if ($LASTEXITCODE -ne 0) { throw 'Host build failed' }
    if (-not $NoTests) {
        & ctest --test-dir (Join-Path $Root 'build/host') -C $Config --output-on-failure
        if ($LASTEXITCODE -ne 0) { throw 'Host tests failed' }
    }
} finally { Pop-Location }
$asi = Join-Path $Root "build/host/host/$Config/CodVLegacy.asi"
if (-not (Test-Path -LiteralPath $asi)) { throw "CodVLegacy.asi not found after build: $asi" }
$out = Join-Path $Root 'build/package'
New-Item -ItemType Directory -Force $out | Out-Null
Copy-Item -LiteralPath $asi -Destination (Join-Path $out 'CodVLegacy.asi') -Force
Write-Host "Host package -> $out"
