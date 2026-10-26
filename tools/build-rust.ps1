# Builds the runtime (CodVRuntime.dll, with iw4L's Modern Warfare 2 movement
# and weapons compiled in) and the MW2 importer (codv-mw2import.exe), runs the
# Rust tests and stages both in build/package.
param([ValidateSet('debug','release')][string]$Profile='release', [switch]$NoTests)
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
$Manifest = Join-Path $Root 'rust/Cargo.toml'
# iw4L's crates (movement, weapons, traces) compile from build/overlay/iw4l:
# the pinned upstream plus patches/iw4l, materialised the way SK8V builds its
# Skate overlay. Rebuilt only when the pin or a patch changes.
if (-not (Get-Command py -ErrorAction SilentlyContinue)) { throw 'The Python launcher (py) was not found. Install Python 3.11+ from https://www.python.org/downloads/.' }
& py (Join-Path $PSScriptRoot 'prepare_overlay.py')
if ($LASTEXITCODE -ne 0) { throw 'iw4L overlay failed (run BOOTSTRAP.bat to fetch upstream/iw4L)' }
# cargo reports progress on stderr; Windows PowerShell 5.1 would turn that into
# a terminating error under 'Stop'. Exit codes are checked instead.
$ErrorActionPreference = 'Continue'
if (-not $NoTests) {
    & cargo test --manifest-path $Manifest --workspace
    if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed' }
}
$cargoArgs = @('build', '--manifest-path', $Manifest, '-p', 'codv-runtime', '-p', 'codv-mw2data')
if ($Profile -eq 'release') { $cargoArgs += '--release' }
& cargo @cargoArgs
if ($LASTEXITCODE -ne 0) { throw 'Rust build failed' }
$ErrorActionPreference = 'Stop'
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root 'rust/target' }
$out = Join-Path $Root 'build/package'
New-Item -ItemType Directory -Force $out | Out-Null
$dll = Join-Path $targetDir "$Profile/CodVRuntime.dll"
if (-not (Test-Path -LiteralPath $dll)) { throw "Expected build output missing: $dll" }
Copy-Item -LiteralPath $dll -Destination (Join-Path $out 'CodVRuntime.dll') -Force
$importer = Join-Path $targetDir "$Profile/codv-mw2import.exe"
if (-not (Test-Path -LiteralPath $importer)) { throw "Expected build output missing: $importer" }
Copy-Item -LiteralPath $importer -Destination (Join-Path $out 'codv-mw2import.exe') -Force
# The setup's GTA archive tool (MW2 item in the weapon wheel): its own workspace on
# upstream/rpf-archive-rs (BOOTSTRAP.bat fetches it).
$gtapack = Join-Path $Root 'rust/codv-gtapack/Cargo.toml'
if (-not (Test-Path -LiteralPath (Join-Path $Root 'upstream/rpf-archive-rs/Cargo.toml'))) { throw 'upstream/rpf-archive-rs is missing: run BOOTSTRAP.bat' }
$ErrorActionPreference = 'Continue'
if (-not $NoTests) {
    & cargo test --manifest-path $gtapack --target-dir $targetDir
    if ($LASTEXITCODE -ne 0) { throw 'codv-gtapack tests failed' }
}
$packArgs = @('build', '--manifest-path', $gtapack, '--target-dir', $targetDir)
if ($Profile -eq 'release') { $packArgs += '--release' }
& cargo @packArgs
if ($LASTEXITCODE -ne 0) { throw 'codv-gtapack build failed' }
$ErrorActionPreference = 'Stop'
$tool = Join-Path $targetDir "$Profile/codv-gtapack.exe"
if (-not (Test-Path -LiteralPath $tool)) { throw "Expected build output missing: $tool" }
Copy-Item -LiteralPath $tool -Destination (Join-Path $out 'codv-gtapack.exe') -Force
Write-Host "Runtime -> $out"
