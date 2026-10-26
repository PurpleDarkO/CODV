# Fetches the pinned ScriptHookV SDK (headers + import library) from its
# publisher into third_party/scripthookv. Never committed (external.lock.json).
param([switch]$Force)
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
$Lock = (Get-Content (Join-Path $Root 'external.lock.json') -Raw | ConvertFrom-Json).targets.scripthookv
$Dest = Join-Path $Root 'third_party/scripthookv'
$ReceiptDir = Join-Path $Root 'receipts'
$Receipt = Join-Path $ReceiptDir 'scripthookv.json'
if ((Test-Path (Join-Path $Dest 'inc/main.h')) -and (Test-Path (Join-Path $Dest 'lib/ScriptHookV.lib')) -and -not $Force) {
    Write-Host 'ScriptHookV SDK already present. Use -Force to refresh.'
    exit 0
}
Remove-Item -Recurse -Force $Dest -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Dest,(Join-Path $Dest 'inc'),(Join-Path $Dest 'lib'),$ReceiptDir | Out-Null
$temp = Join-Path ([IO.Path]::GetTempPath()) ('codv-shv-' + [Guid]::NewGuid())
New-Item -ItemType Directory -Force -Path $temp | Out-Null
try {
    # dev-c.com refuses requests without a browser-like agent and referer.
    $headers = @{ 'User-Agent'='Mozilla/5.0 (Windows NT 10.0; Win64; x64)'; 'Referer'=$Lock.page }
    $sdkZip = Join-Path $temp 'sdk.zip'
    Invoke-WebRequest -Uri $Lock.sdk_archive -Headers $headers -OutFile $sdkZip
    $sdkHash = (Get-FileHash $sdkZip -Algorithm SHA256).Hash.ToLowerInvariant()
    Expand-Archive $sdkZip (Join-Path $temp 'sdk') -Force
    foreach ($f in 'main.h','nativeCaller.h','types.h') {
        $src = Get-ChildItem (Join-Path $temp 'sdk') -Filter $f -Recurse | Select-Object -First 1
        if (-not $src) { throw "SDK missing $f" }
        Copy-Item $src.FullName (Join-Path $Dest "inc/$f")
    }
    $lib = Get-ChildItem (Join-Path $temp 'sdk') -Filter 'ScriptHookV.lib' -Recurse | Select-Object -First 1
    if (-not $lib) { throw 'SDK missing ScriptHookV.lib' }
    Copy-Item $lib.FullName (Join-Path $Dest 'lib/ScriptHookV.lib')
    @{
        fetched_utc = (Get-Date).ToUniversalTime().ToString('o')
        declared_version = $Lock.version
        sdk_url = $Lock.sdk_archive
        sdk_sha256 = $sdkHash
    } | ConvertTo-Json | Set-Content $Receipt -Encoding UTF8
    Write-Host "Fetched ScriptHookV SDK for $($Lock.version). Receipt: $Receipt"
} finally {
    Remove-Item -Recurse -Force $temp -ErrorAction SilentlyContinue
}
