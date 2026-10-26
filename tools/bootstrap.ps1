# Checks the requirements, fetches the pinned sources and builds everything a
# release needs: the Rust runtime and tools, then the ASI. Later rebuilds: BUILD.bat.
#   powershell -ExecutionPolicy Bypass -File tools\bootstrap.ps1 [-SkipCheck]
param([switch]$SkipCheck)
$ErrorActionPreference = 'Stop'
if (-not $SkipCheck) {
    & (Join-Path $PSScriptRoot 'check-requirements.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'Requirements missing (listed above). Fix them, or rerun BOOTSTRAP.bat -SkipCheck to try anyway.' }
}
& (Join-Path $PSScriptRoot 'fetch-upstreams.ps1')
& (Join-Path $PSScriptRoot 'fetch-scripthookv.ps1')
$check = Join-Path $PSScriptRoot 'check-natives.py'
if (-not (Get-Command py -ErrorAction SilentlyContinue)) { throw 'The Python launcher (py) was not found. Install Python 3.11+ from https://www.python.org/downloads/.' }
& py $check
if ($LASTEXITCODE -ne 0) { throw 'host/src/natives.h does not match the pinned nativedb' }
& (Join-Path $PSScriptRoot 'build-rust.ps1')
& (Join-Path $PSScriptRoot 'build-host.ps1')
Write-Host 'Built. Next: py tools\build-release.py --version <version>'
