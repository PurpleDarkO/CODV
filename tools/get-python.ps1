# Downloads the Python setup runs on into <release>\python: the official
# embeddable Python from python.org, checked against its published SHA-256.
# CODV's setup is standard library only, so nothing else is fetched. About
# 11 MB; nothing is installed system-wide. The setup window asks first.
#   powershell -ExecutionPolicy Bypass -File tools\get-python.ps1
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
$Target = Join-Path $Root 'python'
$Pin = @{ Url = 'https://www.python.org/ftp/python/3.13.15/python-3.13.15-embed-amd64.zip'
          Sha = 'd1f04d990aee1253d8569e8e5104e30fa9f5fa830899f14843448872d936a2cf' }

Add-Type -AssemblyName System.IO.Compression.FileSystem
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$stage = "$Target.download"
if (Test-Path $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
New-Item -ItemType Directory $stage | Out-Null
$zip = Join-Path $stage 'python.zip'
Write-Host "== python: Downloading $([IO.Path]::GetFileName($Pin.Url))"
(New-Object Net.WebClient).DownloadFile($Pin.Url, $zip)
$sha = (Get-FileHash -Algorithm SHA256 -LiteralPath $zip).Hash.ToLowerInvariant()
if ($sha -ne $Pin.Sha) { throw "python.zip does not match its published SHA-256 (got $sha); nothing was installed" }
[IO.Compression.ZipFile]::ExtractToDirectory($zip, "$stage\py")
Remove-Item -LiteralPath $zip
# Without its ._pth the embeddable interpreter puts the script's folder on
# sys.path (the tools import each other from tools\).
Remove-Item "$stage\py\python*._pth"
$env:PYTHONHOME = "$stage\py"; $env:PYTHONPATH = $null
$check = & "$stage\py\python.exe" -s -c "import ctypes, json, sys; print(sys.version.split()[0])" 2>&1
if ($LASTEXITCODE) { throw "the downloaded Python cannot start: $check" }
if (Test-Path $Target) { Remove-Item -LiteralPath $Target -Recurse -Force }
Move-Item -LiteralPath "$stage\py" $Target
Remove-Item -LiteralPath $stage -Recurse -Force
Write-Host "Python $check ready in $Target"
