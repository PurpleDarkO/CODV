# Checks everything BOOTSTRAP.bat needs before it starts, and says how to fix
# what is missing. Read-only: it installs, downloads and changes nothing.
#   CHECK.bat                                  (or)
#   powershell -ExecutionPolicy Bypass -File tools\check-requirements.ps1 [-Offline] [-Gta <GTA V folder>]
# -Offline skips the network checks; -Gta also checks the GTA folder for install.
# Exit code 0: ready to build. 1: something must be fixed first.
# Compatible with Windows PowerShell 5.1 (the one every Windows has) and 7.x.
param(
    [switch]$Offline,
    [string]$Gta
)
$ErrorActionPreference = 'Continue'
$Root = Split-Path -Parent $PSScriptRoot
$MinCMake = [version]'3.24'
$MinPython = [version]'3.11'
$TestedGta = '1.0.3889.0'
$script:Results = New-Object System.Collections.ArrayList

function Add-Result([string]$Status, [string]$Name, [string]$Detail, [string]$Fix = '') {
    [void]$script:Results.Add([pscustomobject]@{ Status = $Status; Name = $Name; Detail = $Detail; Fix = $Fix })
}

# Runs a native command outside the checkout (so rust-toolchain.toml cannot
# trigger a toolchain download) and returns its output lines (nothing on
# failure). Callers wrap it in @(...): PowerShell unrolls a one-line result.
function Invoke-Quiet([string]$Exe, [string[]]$Arguments) {
    $here = Get-Location
    try {
        Set-Location ([IO.Path]::GetTempPath())
        $out = & $Exe @Arguments 2>&1 | ForEach-Object { "$_" }
        if ($LASTEXITCODE -ne 0) { return }
        return $out
    } catch {
        return
    } finally {
        Set-Location $here
    }
}

function Get-VersionIn([string]$Text) {
    if ($Text -match '(\d+)\.(\d+)(\.(\d+))?') {
        $patch = if ($Matches[4]) { $Matches[4] } else { '0' }
        return [version]"$($Matches[1]).$($Matches[2]).$patch"
    }
    return $null
}

function Test-Url([string]$Name, [string]$Url, [hashtable]$Headers, [string]$Fix) {
    try {
        $params = @{ Uri = $Url; Method = 'Head'; UseBasicParsing = $true; TimeoutSec = 15; MaximumRedirection = 5; ErrorAction = 'Stop' }
        if ($Headers) { $params.Headers = $Headers }
        $r = Invoke-WebRequest @params
        Add-Result 'OK' $Name "reachable (HTTP $([int]$r.StatusCode))"
    } catch {
        $code = $null
        $resp = $_.Exception.Response
        if ($resp) { try { $code = [int]$resp.StatusCode } catch { $code = $null } }
        if ($code) {
            # The server answered: the network path works. A refusal may still block the download.
            if ($code -eq 405 -or $code -eq 404) { Add-Result 'OK' $Name "reachable (HTTP $code to a probe)" }
            else { Add-Result 'WARN' $Name "server answered HTTP $code" $Fix }
        } else {
            Add-Result 'FAIL' $Name "not reachable: $($_.Exception.Message)" $Fix
        }
    }
}

# ---------------------------------------------------------------- system
$onWindows = [Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT
if ($onWindows -and [Environment]::Is64BitOperatingSystem) {
    Add-Result 'OK' 'Windows x64' ([Environment]::OSVersion.VersionString)
} else {
    Add-Result 'FAIL' 'Windows x64' "this is $([Environment]::OSVersion.VersionString)" 'CODV builds and runs on 64-bit Windows only.'
}
Add-Result 'INFO' 'PowerShell' $PSVersionTable.PSVersion.ToString()

$layout = @('rust\Cargo.toml', 'host\CMakeLists.txt', 'CMakePresets.json', 'upstreams.lock.json', 'external.lock.json') |
    Where-Object { -not (Test-Path -LiteralPath (Join-Path $Root $_)) }
if ($layout) {
    Add-Result 'FAIL' 'Source checkout' "missing $($layout -join ', ')" 'Run this from a full clone of the CODV repository (a player release ZIP has no sources to build).'
} else {
    Add-Result 'OK' 'Source checkout' $Root
}

# Paths that break builds in practice.
if ($Root -match 'OneDrive') {
    Add-Result 'WARN' 'Checkout location' 'inside OneDrive' 'OneDrive locks and syncs build files mid-build; move the checkout to e.g. C:\dev\CODV.'
}
if ($Root.Length -gt 110) {
    Add-Result 'WARN' 'Path length' "$($Root.Length) characters" 'Cargo and MSVC create deep paths; a checkout path under ~110 characters avoids MAX_PATH errors.'
}
if ($Root -match '[^\x20-\x7E]') {
    Add-Result 'WARN' 'Path characters' 'contains non-ASCII characters' 'Some build tools mishandle accented paths; prefer a plain ASCII folder name.'
}

# Free space: the Rust target and CMake build trees need a few GB.
try {
    $drive = (Get-Item -LiteralPath $Root).PSDrive
    if ($drive -and $drive.Free) {
        $gb = [math]::Round($drive.Free / 1GB, 1)
        if ($gb -lt 1.5) { Add-Result 'FAIL' 'Free disk space' "$gb GB on $($drive.Name):" 'Free at least 4 GB on this drive.' }
        elseif ($gb -lt 4) { Add-Result 'WARN' 'Free disk space' "$gb GB on $($drive.Name):" 'The build may need up to ~4 GB.' }
        else { Add-Result 'OK' 'Free disk space' "$gb GB on $($drive.Name):" }
    }
} catch { }

# ---------------------------------------------------------------- tools
$git = Get-Command git -ErrorAction SilentlyContinue
if ($git) {
    $v = @(Invoke-Quiet $git.Source @('--version'))
    Add-Result 'OK' 'Git' $(if ($v) { $v[0] } else { $git.Source })
} else {
    Add-Result 'FAIL' 'Git' 'not found on PATH' 'Install Git for Windows: https://git-scm.com/download/win (reopen the terminal afterwards).'
}

# Rust: rustup installs the pinned toolchain from rust-toolchain.toml on first use.
$channel = $null
$tc = Join-Path $Root 'rust-toolchain.toml'
if ((Test-Path -LiteralPath $tc) -and ((Get-Content -LiteralPath $tc -Raw) -match 'channel\s*=\s*"([^"]+)"')) { $channel = $Matches[1] }
$rustup = Get-Command rustup -ErrorAction SilentlyContinue
$cargo = Get-Command cargo -ErrorAction SilentlyContinue
$needRustDownload = $false
if (-not $rustup -or -not $cargo) {
    Add-Result 'FAIL' 'Rust (rustup + cargo)' 'not found on PATH' 'Install Rust with rustup: https://rustup.rs (choose the default MSVC host), then reopen the terminal.'
} else {
    $show = @(Invoke-Quiet $rustup.Source @('show'))
    $hostLine = if ($show) { $show | Where-Object { $_ -match '^Default host:\s*(\S+)' } | Select-Object -First 1 } else { $null }
    $hostTriple = if ($hostLine -and $hostLine -match '^Default host:\s*(\S+)') { $Matches[1] } else { 'unknown' }
    if ($onWindows -and $hostTriple -ne 'x86_64-pc-windows-msvc') {
        Add-Result 'WARN' 'Rust host' $hostTriple 'CODV is built with the MSVC toolchain: rustup set default-host x86_64-pc-windows-msvc'
    } else {
        Add-Result 'OK' 'Rust host' $hostTriple
    }
    $list = @(Invoke-Quiet $rustup.Source @('toolchain', 'list'))
    if ($channel -and $list -and ($list | Where-Object { $_ -like "$channel-*" -or $_ -like "$channel *" -or $_ -eq $channel })) {
        Add-Result 'OK' "Rust $channel toolchain" 'installed'
    } elseif ($channel) {
        $needRustDownload = $true
        Add-Result 'INFO' "Rust $channel toolchain" 'not installed yet; rustup downloads it (~250 MB) on the first build'
    }
}

# Visual Studio 2022 with the C++ x64 tools (the CMake preset uses the "Visual Studio 17 2022" generator).
$pf86 = ${env:ProgramFiles(x86)}
$vswhere = if ($pf86) { Join-Path $pf86 'Microsoft Visual Studio\Installer\vswhere.exe' } else { $null }
$vsPath = $null
if ($onWindows -and $vswhere -and (Test-Path -LiteralPath $vswhere)) {
    $vc = 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64'
    $vsPath = @(Invoke-Quiet $vswhere @('-products', '*', '-version', '[17.0,18.0)', '-requires', $vc, '-latest', '-property', 'installationPath'))
    $vsPath = if ($vsPath) { $vsPath[0] } else { $null }
    if ($vsPath) {
        $ver = @(Invoke-Quiet $vswhere @('-products', '*', '-version', '[17.0,18.0)', '-requires', $vc, '-latest', '-property', 'catalog_productDisplayVersion'))
        Add-Result 'OK' 'Visual Studio 2022 C++' "$(if ($ver) { $ver[0] } else { '17.x' }) at $vsPath"
    } else {
        $any = @(Invoke-Quiet $vswhere @('-products', '*', '-latest', '-property', 'displayName'))
        $had = if ($any) { " (found: $($any[0]))" } else { '' }
        Add-Result 'FAIL' 'Visual Studio 2022 C++' "no VS 2022 with the C++ x64 tools$had" 'Install Visual Studio 2022 (Community is fine) or Build Tools 2022 with the "Desktop development with C++" workload.'
    }
} else {
    Add-Result 'FAIL' 'Visual Studio 2022 C++' 'Visual Studio Installer not found' 'Install Visual Studio 2022 (Community is fine) or Build Tools 2022 with the "Desktop development with C++" workload.'
}

# Windows SDK headers (part of the C++ workload; checked separately because it can be deselected).
$sdkOk = $false
if ($onWindows) {
    foreach ($key in 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows Kits\Installed Roots', 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots') {
        $kits = (Get-ItemProperty -Path $key -Name KitsRoot10 -ErrorAction SilentlyContinue).KitsRoot10
        if ($kits -and (Test-Path -LiteralPath (Join-Path $kits 'Include'))) {
            $hdr = Get-ChildItem -LiteralPath (Join-Path $kits 'Include') -Directory -ErrorAction SilentlyContinue |
                Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'um\Windows.h') } |
                Sort-Object Name -Descending | Select-Object -First 1
            if ($hdr) { Add-Result 'OK' 'Windows SDK' "$($hdr.Name)"; $sdkOk = $true; break }
        }
    }
}
if (-not $sdkOk) {
    Add-Result 'FAIL' 'Windows SDK' 'no Windows 10/11 SDK headers found' 'In the Visual Studio Installer, add a "Windows 11 SDK" (or Windows 10 SDK) component to the C++ workload.'
}

# CMake (build-host.ps1 calls cmake and ctest from PATH).
$cmake = Get-Command cmake -ErrorAction SilentlyContinue
$vsCmake = if ($vsPath) { Join-Path $vsPath 'Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin' } else { $null }
if ($cmake) {
    $v = Get-VersionIn ((Invoke-Quiet $cmake.Source @('--version')) -join ' ')
    if ($v -and $v -ge $MinCMake) { Add-Result 'OK' 'CMake' "$v" }
    else { Add-Result 'FAIL' 'CMake' "version $v (need $MinCMake+)" 'Install a current CMake: https://cmake.org/download/ (tick "Add CMake to the PATH").' }
    $ctest = Join-Path (Split-Path -Parent $cmake.Source) $(if ($onWindows) { 'ctest.exe' } else { 'ctest' })
    if (-not (Test-Path -LiteralPath $ctest) -and -not (Get-Command ctest -ErrorAction SilentlyContinue)) {
        Add-Result 'FAIL' 'CTest' 'not found next to cmake' 'Reinstall CMake (ctest ships with it); the build runs the host tests with it.'
    }
} elseif ($vsCmake -and (Test-Path -LiteralPath (Join-Path $vsCmake 'cmake.exe'))) {
    Add-Result 'FAIL' 'CMake' 'only the copy inside Visual Studio, not on PATH' "Add `"$vsCmake`" to PATH, or install CMake from https://cmake.org/download/ with `"Add CMake to the PATH`"."
} else {
    Add-Result 'FAIL' 'CMake' 'not found on PATH' 'Install CMake 3.24+: https://cmake.org/download/ (tick "Add CMake to the PATH").'
}

# Python 3.11+ (bootstrap prefers the py launcher, then python).
$pyFound = $null
foreach ($cand in @(@('py', '-3'), @('python'), @('python3'))) {
    $cmd = Get-Command $cand[0] -ErrorAction SilentlyContinue
    if (-not $cmd) { continue }
    $extra = @($cand | Select-Object -Skip 1)
    $out = @(Invoke-Quiet $cmd.Source ($extra + @('-c', 'import sys; print("%d.%d.%d" % sys.version_info[:3])')))
    if (-not $out) {
        if ($cmd.Source -match 'WindowsApps') {
            Add-Result 'WARN' "Python ($($cand -join ' '))" 'the Microsoft Store shortcut, not a real Python' 'Install Python from https://www.python.org/downloads/ or turn the alias off in Settings > Apps > App execution aliases.'
        }
        continue
    }
    $v = Get-VersionIn $out[-1]
    if ($v -and $v -ge $MinPython) { $pyFound = "$($cand -join ' ') $v"; break }
    Add-Result 'WARN' "Python ($($cand -join ' '))" "version $v is older than $MinPython"
}
if ($pyFound) { Add-Result 'OK' 'Python' $pyFound }
else { Add-Result 'FAIL' 'Python' "no Python $MinPython+ found" 'Install Python 3.11 or newer from https://www.python.org/downloads/ (tick "Add python.exe to PATH").' }

# ---------------------------------------------------------------- what bootstrap will fetch
$shvSdk = (Test-Path -LiteralPath (Join-Path $Root 'third_party\scripthookv\inc\main.h')) -and
          (Test-Path -LiteralPath (Join-Path $Root 'third_party\scripthookv\lib\ScriptHookV.lib'))
if ($shvSdk) { Add-Result 'OK' 'ScriptHookV SDK' 'already in third_party\scripthookv' }
else { Add-Result 'INFO' 'ScriptHookV SDK' 'not fetched yet; BOOTSTRAP.bat downloads it' }
if (Test-Path -LiteralPath (Join-Path $Root 'upstream\gta5-nativedb-data\natives.json')) { Add-Result 'OK' 'Native database' 'already in upstream\' }
else { Add-Result 'INFO' 'Native database' 'not fetched yet; BOOTSTRAP.bat clones it from GitHub' }

if (-not $Offline) {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    Test-Url 'GitHub' 'https://github.com/' $null 'BOOTSTRAP.bat clones the native database from GitHub; check your connection, proxy or firewall.'
    Test-Url 'crates.io' 'https://index.crates.io/config.json' $null 'Cargo resolves the workspace through crates.io; check your connection, proxy or firewall.'
    if ($needRustDownload) {
        Test-Url 'Rust downloads' 'https://static.rust-lang.org/dist/channel-rust-stable.toml.sha256' $null 'rustup downloads the pinned toolchain from static.rust-lang.org; check your connection, proxy or firewall.'
    }
    if (-not $shvSdk) {
        $lockPath = Join-Path $Root 'external.lock.json'
        if (Test-Path -LiteralPath $lockPath) {
            $shv = (Get-Content -LiteralPath $lockPath -Raw | ConvertFrom-Json).targets.scripthookv
            $headers = @{ 'User-Agent' = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)'; 'Referer' = $shv.page }
            Test-Url 'ScriptHookV SDK download' $shv.sdk_archive $headers ("If the automatic download is refused, download the SDK from $($shv.page) and put main.h, nativeCaller.h and types.h in third_party\scripthookv\inc and ScriptHookV.lib in third_party\scripthookv\lib.")
        }
    }
} else {
    Add-Result 'INFO' 'Network' 'skipped (-Offline)'
}

# ---------------------------------------------------------------- optional: the GTA folder for install
if ($Gta) {
    $exe = Join-Path $Gta 'GTA5.exe'
    if (Test-Path -LiteralPath $exe) {
        $fv = [Diagnostics.FileVersionInfo]::GetVersionInfo((Resolve-Path -LiteralPath $exe).Path)
        $ver = "$($fv.FileMajorPart).$($fv.FileMinorPart).$($fv.FileBuildPart).$($fv.FilePrivatePart)"
        if ($ver -eq $TestedGta) { Add-Result 'OK' 'GTA V Legacy' $ver }
        else { Add-Result 'WARN' 'GTA V Legacy' "$ver (tested: $TestedGta)" 'CODV uses natives only, so other Legacy builds may work; StrictVersion=1 in CodVLegacy.ini refuses them.' }
        foreach ($f in 'ScriptHookV.dll', 'dinput8.dll') {
            if (Test-Path -LiteralPath (Join-Path $Gta $f)) { Add-Result 'OK' $f 'present' }
            else { Add-Result 'FAIL' $f 'missing from the GTA folder' 'Install ScriptHookV for your GTA build: https://www.dev-c.com/gtav/scripthookv/' }
        }
        $probe = Join-Path $Gta ('.codv-check-' + [Guid]::NewGuid().ToString('N'))
        try { [IO.File]::WriteAllText($probe, ''); Remove-Item -LiteralPath $probe -Force; Add-Result 'OK' 'GTA folder writable' 'yes' }
        catch { Add-Result 'WARN' 'GTA folder writable' 'no' 'Run the install from an administrator PowerShell, or give your user write access to the GTA folder.' }
    } elseif (Test-Path -LiteralPath (Join-Path $Gta 'GTA5_Enhanced.exe')) {
        Add-Result 'FAIL' 'GTA V' 'this is GTA V Enhanced' 'CODV supports GTA V Legacy Story Mode only.'
    } else {
        Add-Result 'FAIL' 'GTA V' "no GTA5.exe in $Gta" 'Pass the folder that holds GTA5.exe.'
    }
}

# ---------------------------------------------------------------- report
$colors = @{ OK = 'Green'; WARN = 'Yellow'; FAIL = 'Red'; INFO = 'Cyan' }
Write-Host ''
Write-Host 'CODV build requirements' -ForegroundColor White
Write-Host ''
foreach ($r in $script:Results) {
    $tag = '[{0,-4}]' -f $r.Status
    Write-Host ('  {0} ' -f $tag) -ForegroundColor $colors[$r.Status] -NoNewline
    Write-Host ('{0,-26} {1}' -f $r.Name, $r.Detail)
    if ($r.Fix -and $r.Status -ne 'OK') { Write-Host ('         -> {0}' -f $r.Fix) -ForegroundColor DarkGray }
}
$fails = @($script:Results | Where-Object { $_.Status -eq 'FAIL' }).Count
$warns = @($script:Results | Where-Object { $_.Status -eq 'WARN' }).Count
Write-Host ''
if ($fails -gt 0) {
    Write-Host ("  {0} problem(s) must be fixed before building ({1} warning(s))." -f $fails, $warns) -ForegroundColor Red
    exit 1
}
if ($warns -gt 0) { Write-Host ("  Ready to build, with {0} warning(s) above. Next: BOOTSTRAP.bat" -f $warns) -ForegroundColor Yellow }
else { Write-Host '  Ready to build. Next: BOOTSTRAP.bat' -ForegroundColor Green }
exit 0
