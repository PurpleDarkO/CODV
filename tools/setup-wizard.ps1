# CODV setup window: picks the GTA V folder and the player's own Modern Warfare 2 (PC), then runs
# tools/prepare.py --install with a progress bar. Adapted from SK8V's setup window (Apache-2.0).
# Windows PowerShell 5.1 + WinForms, nothing to install; the release's "CODV Setup.cmd" starts it.
# ASCII only: 5.1 reads a BOM-less script as ANSI.
# -Folder/-Mw2Dir/-Start: the restart as administrator carries the choices over and goes straight on.
# (Not -Gta/-Mw2: PowerShell names ignore case, and those are the window's controls.)
param([string]$Folder = '', [string]$Mw2Dir = '', [switch]$Start)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
[Windows.Forms.Application]::EnableVisualStyles()
$Root = Split-Path -Parent $PSScriptRoot
$Self = $PSCommandPath

# ---- finding things ----
# The copy tools\get-python.ps1 downloaded, else a Python 3.11+ on PATH (not the Microsoft Store stub), else
# the py launcher: the words that start it, or $null. Setup needs only Python's standard library.
function Find-Python {
  $bundled = Join-Path $Root 'python\python.exe'
  if (Test-Path $bundled) { return , @($bundled) }
  $ErrorActionPreference = 'Continue'  # under Stop, 5.1 turns the probe's redirected stderr into an exception
  $probe = 'import sys, ctypes, hashlib, json, zlib; sys.exit(sys.version_info < (3, 11))'
  $found = Get-Command python.exe -CommandType Application -All -ErrorAction SilentlyContinue |
    Where-Object { $_.Source -notmatch '\\WindowsApps\\' } | ForEach-Object Source |
    Where-Object { & $_ -c $probe 2>$null; $LASTEXITCODE -eq 0 } | Select-Object -First 1
  if ($found) { return , @($found) }
  $py = Get-Command py.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($py) {
    & $py.Source -3 -c $probe 2>$null
    if ($LASTEXITCODE -eq 0) { return , @($py.Source, '-3') }
  }
  $null
}

# Steam's library folders (the registry's Steam path and libraryfolders.vdf).
function Get-SteamLibraries {
  $libs = @()
  foreach ($k in 'HKCU:\Software\Valve\Steam', 'HKLM:\SOFTWARE\WOW6432Node\Valve\Steam') {
    $p = Get-ItemProperty $k -ErrorAction SilentlyContinue
    foreach ($v in $p.SteamPath, $p.InstallPath) { if ($v) { $libs += ($v -replace '/', '\') } }
  }
  foreach ($root in @($libs)) {
    $vdf = Join-Path $root 'steamapps\libraryfolders.vdf'
    if (Test-Path $vdf) {
      foreach ($m in [regex]::Matches((Get-Content $vdf -Raw), '"path"\s+"([^"]+)"')) { $libs += ($m.Groups[1].Value -replace '\\\\', '\') }
    }
  }
  $libs | Where-Object { Test-Path (Join-Path $_ 'steamapps') } | Select-Object -Unique
}

# A Steam app's install folder (its appmanifest's installdir) holding $marker, or ''.
function Find-SteamApp([int[]]$ids, [string]$marker) {
  foreach ($lib in Get-SteamLibraries) {
    foreach ($id in $ids) {
      $acf = Join-Path $lib "steamapps\appmanifest_$id.acf"
      if (-not (Test-Path $acf)) { continue }
      $m = [regex]::Match((Get-Content $acf -Raw), '"installdir"\s+"([^"]+)"')
      if (-not $m.Success) { continue }
      $dir = Join-Path $lib ("steamapps\common\" + $m.Groups[1].Value)
      if (Test-Path (Join-Path $dir $marker)) { return $dir }
    }
  }
  ''
}

function Find-Gta {
  $keys = 'HKLM:\SOFTWARE\WOW6432Node\Rockstar Games\Grand Theft Auto V',
          'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Steam App 271590',
          'HKLM:\SOFTWARE\WOW6432Node\Rockstar Games\GTAV'
  foreach ($k in $keys) {
    $p = Get-ItemProperty $k -ErrorAction SilentlyContinue
    foreach ($v in $p.InstallFolder, $p.InstallLocation) {
      if ($v -and (Test-Path (Join-Path $v 'GTA5.exe'))) { return $v }
    }
  }
  Find-SteamApp @(271590) 'GTA5.exe'
}

# Modern Warfare 2 (2009): the single-player and multiplayer apps share one folder.
function Find-Mw2 { Find-SteamApp @(10190, 10180) 'zone\english\common_mp.ff' }
function Test-Mw2([string]$dir) { $dir -and (Test-Path (Join-Path $dir 'zone\english\common_mp.ff')) }

# ---- look: CODV's banner (near black, a green glow) ----
$rgb = { param($r, $g, $b) [Drawing.Color]::FromArgb($r, $g, $b) }
$ground = & $rgb 12 13 12; $panel = & $rgb 6 7 6; $field = & $rgb 24 27 23; $line = & $rgb 40 45 39
$ink = & $rgb 232 236 230; $muted = & $rgb 138 147 134; $accent = & $rgb 95 212 59; $onAccent = & $rgb 10 11 10
function New-Font([float]$size, [string]$style = 'Regular', [string]$family = 'Segoe UI') { New-Object Drawing.Font($family, $size, [Drawing.FontStyle]$style) }
function Add([string]$type, [hashtable]$props, $parent = $form) {
  $c = New-Object "Windows.Forms.$type" -Property $props
  $parent.Controls.Add($c)
  $c
}
function Set-Flat($button, $back, $fore) {
  $button.FlatStyle = 'Flat'; $button.BackColor = $back; $button.ForeColor = $fore
  $button.FlatAppearance.BorderColor = $back; $button.Cursor = [Windows.Forms.Cursors]::Hand
}

$form = New-Object Windows.Forms.Form -Property @{
  Text = 'CODV Setup'; ClientSize = '640,460'; StartPosition = 'CenterScreen'; FormBorderStyle = 'FixedSingle'
  MaximizeBox = $false; Font = (New-Font 9.5); BackColor = $ground; ForeColor = $ink
}
$header = Add Panel @{ Location = '0,0'; Size = '640,96'; BackColor = $panel }
[void](Add Label @{ Text = 'CODV'; Location = '20,12'; AutoSize = $true; Font = (New-Font 26 'Bold' 'Consolas'); ForeColor = $accent } $header)
[void](Add Label @{ Text = 'MODERN WARFARE 2 GAMEPLAY IN GTA V LEGACY'; Location = '24,62'; AutoSize = $true
  Font = (New-Font 8.75 'Regular' 'Consolas'); ForeColor = $muted } $header)
[void](Add Panel @{ Dock = 'Bottom'; Height = 3; BackColor = $accent } $header)

$y = 118
function Add-Row($label, $value) {
  [void](Add Label @{ Text = $label; Location = "24,$($script:y + 4)"; Size = '150,22' })
  $t = Add TextBox @{ Text = $value; Location = "174,$script:y"; Size = '336,26'; BackColor = $field; ForeColor = $ink; BorderStyle = 'FixedSingle' }
  $b = Add Button @{ Text = 'Browse...'; Location = "518,$($script:y - 1)"; Size = '98,28' }
  Set-Flat $b $field $ink; $b.FlatAppearance.BorderColor = $line
  $b.Add_Click({ $d = New-Object Windows.Forms.FolderBrowserDialog
                 if ($d.ShowDialog() -eq 'OK') { $t.Text = $d.SelectedPath } }.GetNewClosure())
  $script:y += 38
  $t
}
$gta = Add-Row 'GTA V Legacy folder' $(if ($Folder) { $Folder } else { Find-Gta })
$mw2 = Add-Row 'Modern Warfare 2 folder' $(if ($Mw2Dir) { $Mw2Dir } else { Find-Mw2 })
[void](Add Label @{ Location = "24,$y"; Size = '592,66'; ForeColor = $muted; Font = (New-Font 8.75)
  Text = ("Needs GTA V Legacy 1.0.3889.0 with ScriptHookV and your own Call of Duty: Modern Warfare 2 (2009, PC). " +
          "Setup imports MW2's weapons, view models, animation, sounds and HUD from your copy into the GTA folder's CODV " +
          "folder (about 300 MB); nothing of MW2 is shared. Close GTA V first. If setup stops, press Install again: it " +
          "picks up where it left off.") })
$y += 76

# Progress: the current step, a bar, percent and elapsed time. The result panel covers it at the end.
$status = Add Label @{ Text = 'Ready when you are.'; Location = "24,$y"; Size = '592,22'; Font = (New-Font 10 'Bold') }
$bar = Add Panel @{ Location = "24,$($y + 28)"; Size = '592,8'; BackColor = $line }
$fill = Add Panel @{ Location = '0,0'; Size = '0,8'; BackColor = $accent } $bar
$meta = Add Label @{ Location = "24,$($y + 42)"; Size = '592,20'; ForeColor = $muted; Font = (New-Font 8.75 'Regular' 'Consolas') }
$result = Add Panel @{ Location = "24,$($y - 4)"; Size = '592,70'; Visible = $false }
$resultTitle = Add Label @{ Location = '14,8'; Size = '570,26'; Font = (New-Font 12 'Bold') } $result
$resultText = Add Label @{ Location = '14,36'; Size = '570,32'; Font = (New-Font 9) } $result
$y += 84

$details = Add LinkLabel @{ Text = 'Show details'; Location = "24,$($y + 9)"; AutoSize = $true; LinkColor = $muted; ActiveLinkColor = $accent }
$install = Add Button @{ Text = 'Install'; Location = "396,$y"; Size = '120,36'; Font = (New-Font 10 'Bold') }
Set-Flat $install $accent $onAccent
$close = Add Button @{ Text = 'Close'; Location = "526,$y"; Size = '90,36' }
Set-Flat $close $field $ink; $close.FlatAppearance.BorderColor = $line
$close.Add_Click({ $form.Close() })
$y += 50
$log = Add TextBox @{ Multiline = $true; ScrollBars = 'Vertical'; ReadOnly = $true; Location = "24,$y"; Size = '592,200'
  Font = (New-Object Drawing.Font('Consolas', 8.5)); BackColor = $panel; ForeColor = $ink; BorderStyle = 'FixedSingle'; Visible = $false }
function Show-Details([bool]$show) {
  $log.Visible = $show
  $details.Text = $(if ($show) { 'Hide details' } else { 'Show details' })
  $form.ClientSize = New-Object Drawing.Size(640, ($log.Top + $(if ($show) { 216 } else { 0 })))
}
Show-Details $false
$details.Add_LinkClicked({ Show-Details (-not $log.Visible) })

function Show-Result([bool]$ok, [string]$title, [string]$text) {
  $result.BackColor = $(if ($ok) { & $rgb 18 38 16 } else { & $rgb 46 18 16 })
  $resultTitle.ForeColor = $(if ($ok) { $accent } else { & $rgb 255 120 100 })
  $resultText.ForeColor = $ink
  $resultTitle.Text = $title; $resultText.Text = $text
  $status.Visible = $bar.Visible = $meta.Visible = $false
  $result.Visible = $true
}
function Set-Running([bool]$running) {
  $install.Enabled = -not $running
  $install.Text = $(if ($running) { 'Installing...' } else { 'Install' })
  Set-Flat $install $(if ($running) { $line } else { $accent }) $(if ($running) { $muted } else { $onAccent })
}
function Set-Progress([int]$percent) { $fill.Width = [int]($bar.Width * [Math]::Min(100, $percent) / 100); $script:percent = $percent }

# ---- running a step ----
# The tool writes to a log file the timer tails (PowerShell event handlers would not run while the window is open).
# prepare.py prints '== <stage>: <what it does>' as each stage starts and '@progress <percent>' as each ends.
$logFile = Join-Path $env:TEMP 'codv-setup.log'
$script:proc = $null
$script:then = $null  # what starts when the running step succeeds
$script:read = 0
$script:partial = ''
$script:lastLine = ''
$script:percent = 0
$script:withMw2 = $true
$timer = New-Object Windows.Forms.Timer -Property @{ Interval = 250 }
$timer.Add_Tick({
  if (-not $script:proc) { return }
  try {
    $fs = [IO.File]::Open($logFile, 'Open', 'Read', 'ReadWrite')
    try {
      [void]$fs.Seek($script:read, 'Begin')
      $buf = New-Object byte[] ($fs.Length - $script:read)
      $n = $fs.Read($buf, 0, $buf.Length)
      $script:read += $n
      $lines = ($script:partial + [Text.Encoding]::UTF8.GetString($buf, 0, $n)) -split "`r?`n"
      $script:partial = $lines[-1]
      foreach ($line in $lines[0..($lines.Count - 2)]) {
        if ($line -match '^@progress (\d+)') { Set-Progress ([int]$Matches[1]); continue }
        if ($line -match '^== [^:]+: (.+)' -and $Matches[1] -ne 'up to date') { $status.Text = $Matches[1] + '...' }
        if ($line.Trim()) { $script:lastLine = $line.Trim() }
        $log.AppendText($line + "`r`n")
      }
    } finally { $fs.Dispose() }
  } catch [IO.FileNotFoundException] {}
  $elapsed = (Get-Date) - $script:started
  $meta.Text = '{0}%   |   {1:m\:ss} elapsed' -f $script:percent, $elapsed
  if ($script:proc.HasExited) {
    $code, $then = $script:proc.ExitCode, $script:then
    $script:proc, $script:then = $null, $null
    if ($code -eq 0 -and $then) { & $then; return }
    Set-Running $false
    if ($code -eq 0) {
      Set-Progress 100
      $status.Text = 'Done'
      $how = "Start GTA V Story Mode with BattlEye off. Pick MW2 in the weapon wheel (or press F10 / Back + RB); + (or Back + LB) opens the CODV menu."
      if (-not $script:withMw2) { $how += ' Without MW2, CODV runs its interim gunplay only.' }
      Show-Result $true 'CODV is installed' ("$how Took {0:m\:ss}." -f $elapsed)
      $install.Visible = $false
      $close.Text = 'Finish'; Set-Flat $close $accent $onAccent; $close.Font = (New-Font 10 'Bold')
    } else {
      $status.Text = 'Setup stopped'
      Show-Result $false 'Setup stopped' ("$($script:lastLine)`nFix that and press Install to pick up where it left off.")
      $install.Text = 'Try again'
      Show-Details $true
    }
  }
})
$timer.Start()

# Runs a command (each word quoted) with its output in the log file the timer shows; $then runs if it succeeds.
function Start-Logged([string[]]$words, [scriptblock]$then) {
  Remove-Item $logFile -ErrorAction SilentlyContinue
  $script:read = 0; $script:partial = ''; $script:then = $then
  # a trailing backslash would escape the closing quote (F:\ -> F:)
  $command = ($words | ForEach-Object { '"' + ([string]$_).TrimEnd('\') + '"' }) -join ' '
  $script:proc = Start-Process cmd.exe -ArgumentList "/c `"$command > `"$logFile`" 2>&1`"" -WorkingDirectory $Root `
    -WindowStyle Hidden -PassThru
  [void]$script:proc.Handle  # keeps ExitCode readable after the process exits
  Set-Running $true
}

# A real write (as setup_engine.writable): the folder's permissions, not its read-only flag.
function Test-Writable([string]$dir) {
  $probe = Join-Path $dir ".codv-write-test-$PID"
  try { [IO.File]::WriteAllBytes($probe, @()); Remove-Item -LiteralPath $probe; $true } catch { $false }
}

function Start-Prepare {
  $py = Find-Python
  if (-not $py) { Show-Result $false 'No usable Python' 'Setup needs Python 3.11 or newer.'; Set-Running $false; return }
  $env:PYTHONIOENCODING = 'utf-8'
  # The downloaded copy ignores the user's site-packages and stays in its own folder (PYTHONHOME): a
  # registered Python elsewhere would otherwise lend it its own Lib and DLLs.
  $bundled = $py[0] -eq (Join-Path $Root 'python\python.exe')
  $env:PYTHONNOUSERSITE = $(if ($bundled) { '1' } else { $null })
  if ($bundled) { $env:PYTHONHOME = Join-Path $Root 'python'; $env:PYTHONPATH = $null }
  $status.Text = 'Starting...'
  $words = @($py) + @('-B', '-u', (Join-Path $Root 'tools\prepare.py'), '--gta', $gta.Text)
  if ($script:withMw2) { $words += @('--mw2', $mw2.Text) }
  Start-Logged ($words + @('--install'))
}

$install.Add_Click({
  if (-not (Test-Path (Join-Path $gta.Text 'GTA5.exe'))) { [void][Windows.Forms.MessageBox]::Show('Pick the folder that holds GTA5.exe.', 'CODV Setup'); return }
  $script:withMw2 = Test-Mw2 $mw2.Text
  if (-not $script:withMw2) {
    $ask = "No Modern Warfare 2 found there (it needs zone\english\common_mp.ff).`n`n" +
           "Install without it? CODV then runs only its interim gunplay: no MW2 movement, weapons, HUD or killstreaks. " +
           "You can run setup again later with your MW2 folder."
    if ([Windows.Forms.MessageBox]::Show($ask, 'CODV Setup', 'YesNo', 'Warning') -ne 'Yes') { return }
  }
  $result.Visible = $false
  $status.Visible = $bar.Visible = $meta.Visible = $true
  Set-Progress 0
  $script:started = Get-Date
  if (-not (Test-Writable $gta.Text)) {
    $ask = "Windows only lets administrators change this GTA V folder (a GTA download or repair sets that).`n`n" +
           "Restart setup as administrator? Your choices carry over."
    if ([Windows.Forms.MessageBox]::Show($ask, 'CODV Setup', 'YesNo', 'Question') -ne 'Yes') { return }
    $words = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $Self, '-Folder', $gta.Text)
    if ($script:withMw2) { $words += @('-Mw2Dir', $mw2.Text) }
    $words = ($words + @('-Start')) | ForEach-Object { '"' + ([string]$_).TrimEnd('\') + '"' }
    try { # not -WindowStyle Hidden: Windows applies that to this window's first show as well
      Start-Process powershell.exe -Verb RunAs -ArgumentList ($words -join ' ')
      $form.Close()
    } catch {} # the Windows prompt was declined: stay here
    return
  }
  if (Find-Python) { Start-Prepare; return }
  $ask = "Setup runs on Python 3.11 or newer, which this PC does not have.`n`n" +
         "Download the official embeddable Python now? About 11 MB from python.org, checked against its published " +
         "hash and kept in this setup folder. Nothing is installed on your PC."
  if ([Windows.Forms.MessageBox]::Show($ask, 'CODV Setup', 'YesNo', 'Question') -ne 'Yes') { return }
  $status.Text = 'Downloading Python...'
  Start-Logged @('powershell', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $Root 'tools\get-python.ps1')) { Start-Prepare }
})

# Closing mid-run stops the tools (cmd, Python and the importer under it); Install resumes later.
$form.Add_FormClosing({
  if (-not $script:proc -or $script:proc.HasExited) { return }
  $q = "Setup is still running. Stop it? Pressing Install later picks up where it stopped."
  if ([Windows.Forms.MessageBox]::Show($q, 'CODV Setup', 'YesNo', 'Warning') -ne 'Yes') { $_.Cancel = $true; return }
  Start-Process taskkill.exe -ArgumentList "/T /F /PID $($script:proc.Id)" -WindowStyle Hidden -Wait
})
if ($Start) { $form.Add_Shown({ $install.PerformClick() }) }
[void]$form.ShowDialog()
