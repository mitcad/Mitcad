# SPDX-License-Identifier: MIT
# Tests the Windows installer in the build VM: installs the NSIS installer
# of a build directory silently into a folder of its own, checks that the
# files are all there, starts the installed programs with the toolchain
# (Qt, vcpkg, Rust, Visual Studio) removed from PATH, and uninstalls.
#   1. the installer's files: executables, Qt and OCCT DLLs, the Qt platform
#      plugin, the Visual C++ runtime, the licences; no Mesa, no PDB files.
#   2. mitcad-cli runs a script of the repository.
#   3. mitcad.exe --version loads Qt with its plugins.
#   4. with -MesaDir: mitcad.exe --demo --screenshot renders the demo block
#      (Mesa is copied next to the installed copy, as a VM has no GPU).
#   5. with -MesaDir, the automatic update (mitcad#9): the installed Mitcad,
#      which takes itself for version 0.0.0 (MITCAD_TEST_VERSION), finds
#      this installer as the release in a manifest signed with a key made
#      for the test (mitcad-release of the build directory) and served
#      over HTTPS from 127.0.0.1 (tools/update-test-server.py, Python) with
#      a certificate made for the test (openssl), which only its update
#      requests trust (MITCAD_UPDATE_TEST_CA). A tampered download is
#      refused and deleted; then Install downloads and verifies the
#      release, the changed design is saved when asked, Mitcad quits,
#      mitcad-updater runs the installer silently over the installation and
#      starts Mitcad again, which records the update as installed.
#   6. the uninstaller removes the folder.
# The installer registers the .mitcad file type under HKLM; run the test only
# in the VM.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\installer-windows-test.ps1
#          [-Build build\dev] [-MesaDir C:\dev\mesa\bin] [-Out dir] [-NoUpdate]
#          [-SkipStale]
# -SkipStale (the ctest app.installer-windows): exit 77, skipped, when the
# build directory has no installer newer than its mitcad.exe.
param(
  [string]$Build = (Join-Path $PSScriptRoot '..\build\dev'),
  [string]$MesaDir = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-installer-test'),
  [switch]$NoUpdate,
  [switch]$SkipStale)

$ErrorActionPreference = 'Stop'
$Build = (Resolve-Path $Build).Path
$failures = 0
function Pass([string]$Message) { Write-Host "ok   $Message" }
function Fail([string]$Message) { Write-Host "FAIL: $Message"; $script:failures++ }

$installer = Get-ChildItem $Build -Filter 'mitcad-*-windows-x64.exe' |
  Sort-Object LastWriteTime -Descending | Select-Object -First 1
# The build's executables: in the build directory (the Windows presets), or
# in app\ and core\.
function Find-Built([string[]]$Names) {
  $Names | ForEach-Object { Join-Path $Build $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
$built = Find-Built 'mitcad.exe', 'app\mitcad.exe' | ForEach-Object { Get-Item $_ }
if ($SkipStale -and (-not $installer -or ($built -and $installer.LastWriteTime -lt $built.LastWriteTime))) {
  Write-Host "SKIP: no installer of this build in $Build; run: cmake --build --preset dev --target package"
  exit 77
}
if (-not $installer) {
  Write-Host "FAIL: no mitcad-*-windows-x64.exe in $Build; run: cmake --build --preset dev --target package"
  exit 1
}
Write-Host "Installer: $($installer.FullName) ($([math]::Round($installer.Length / 1MB, 1)) MB)"

# A registration an interrupted run left behind, whose uninstaller is gone,
# would stop the installer at "Uninstall failed" (a message box even when
# silent): it goes first.
foreach ($key in 'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Mitcad',
                 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Mitcad') {
  $uninstall = (Get-ItemProperty $key -ErrorAction SilentlyContinue).UninstallString
  if ($uninstall -and -not (Test-Path $uninstall.Trim('"'))) {
    Remove-Item $key
    Write-Host "note: removed the registration of a missing installation ($uninstall)"
  }
}
if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }
New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$install = Join-Path $Out 'Mitcad'

# NSIS: /S is silent, /D sets the folder and must be the last argument,
# without quotes.
$proc = Start-Process -FilePath $installer.FullName -ArgumentList '/S', "/D=$install" -Wait -PassThru
if ($proc.ExitCode -eq 0) { Pass 'silent install' } else { Fail "installer exit code $($proc.ExitCode)"; exit 1 }

# 1. Files (Qt's plugins are in plugins, which binqt.conf points to).
$bin = Join-Path $install 'bin'
$required = @('mitcad.exe', 'mitcad-cli.exe', 'mitcad-updater.exe', 'Qt6Core.dll', 'Qt6Gui.dll', 'Qt6Widgets.dll',
              'Qt6OpenGL.dll', 'Qt6OpenGLWidgets.dll', 'Qt6Svg.dll', 'Qt6Network.dll',
              '..\plugins\platforms\qwindows.dll', '..\plugins\tls\qschannelbackend.dll',
              'TKernel.dll', 'TKService.dll', 'TKOpenGl.dll', 'vcruntime140.dll', 'msvcp140.dll')
foreach ($file in $required) {
  if (Test-Path (Join-Path $bin $file)) { Pass "bin\$file" } else { Fail "bin\$file is missing" }
}
foreach ($file in 'LICENSE', 'THIRD-PARTY-NOTICES.txt', 'Uninstall.exe', 'licenses\droid-sans\LICENSE.txt') {
  if (Test-Path (Join-Path $install $file)) { Pass $file } else { Fail "$file is missing" }
}
$licenseFiles = @(Get-ChildItem (Join-Path $install 'licenses\vcpkg') -Recurse -Filter copyright -ErrorAction SilentlyContinue)
if ($licenseFiles.Count -gt 0) { Pass "$($licenseFiles.Count) vcpkg licence texts" } else { Fail 'no vcpkg licence texts' }
foreach ($pattern in 'opengl32.dll', 'libgallium_wgl.dll', 'opengl32sw.dll', '*.pdb') {
  $found = @(Get-ChildItem $install -Recurse -Filter $pattern -ErrorAction SilentlyContinue)
  if ($found.Count -eq 0) { Pass "no $pattern" } else { Fail "$pattern must not be installed: $($found[0].FullName)" }
}

# The update test's tools, found before PATH is cut down below.
$python = Get-Command python.exe -All -ErrorAction SilentlyContinue | Where-Object { $_.Source -notlike '*WindowsApps*' } |
  Select-Object -First 1 -ExpandProperty Source
$openssl = @((Get-Command openssl.exe -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty Source),
             "$env:ProgramFiles\Git\mingw64\bin\openssl.exe", "$env:ProgramFiles\Git\usr\bin\openssl.exe") |
  Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1

# Run with a PATH of the system's only, so that a DLL missing from the
# installation is not found in the development toolchain.
$env:Path = "$env:SystemRoot\system32;$env:SystemRoot"
$env:MITCAD_SETTINGS_DIR = Join-Path $Out 'settings'
# No update checks but in step 5's, against its own server.
$env:MITCAD_NO_UPDATE_CHECK = '1'
$env:QT_PLUGIN_PATH = ''
$env:QT_QPA_PLATFORM_PLUGIN_PATH = ''

# 2. mitcad-cli.
$script = Join-Path $PSScriptRoot 'cli\tests\block_fillet_chamfer.json'
& (Join-Path $bin 'mitcad-cli.exe') run $script *> (Join-Path $Out 'cli.log')
if ($LASTEXITCODE -eq 0) { Pass 'mitcad-cli run block_fillet_chamfer.json' }
else { Fail "mitcad-cli exit code $LASTEXITCODE (see $Out\cli.log)" }

# 3. mitcad.exe --version: Qt and its platform plugin load.
$versionOut = Join-Path $Out 'version.txt'
$proc = Start-Process -FilePath (Join-Path $bin 'mitcad.exe') -ArgumentList '--version' `
  -RedirectStandardOutput $versionOut -RedirectStandardError (Join-Path $Out 'version-err.txt') -Wait -PassThru
$versionText = if (Test-Path $versionOut) { (Get-Content $versionOut -Raw) } else { '' }
if ($proc.ExitCode -eq 0 -and $versionText -match 'Mitcad') { Pass "mitcad.exe --version: $($versionText.Trim())" }
else { Fail "mitcad.exe --version exit code $($proc.ExitCode), output '$versionText'" }

# 4. Rendering, when a software OpenGL is given.
if ($MesaDir) {
  Copy-Item (Join-Path $MesaDir 'opengl32.dll'), (Join-Path $MesaDir 'libgallium_wgl.dll') $bin
  $shot = Join-Path $Out 'demo.png'
  $proc = Start-Process -FilePath (Join-Path $bin 'mitcad.exe') `
    -ArgumentList '--demo', '--no-recovery', '--screenshot', "`"$shot`"" -Wait -PassThru
  if ($proc.ExitCode -eq 0 -and (Test-Path $shot) -and (Get-Item $shot).Length -gt 5000) {
    Pass "mitcad.exe --demo --screenshot ($((Get-Item $shot).Length) bytes)"
  } else { Fail "mitcad.exe --screenshot exit code $($proc.ExitCode)" }
}

# 5. The automatic update, when a software OpenGL is given (the app's window
# is driven with tools/ui-windows-lib.ps1).
function Test-Update {
  $tool = Find-Built 'mitcad-release.exe', 'core\mitcad-release.exe'
  if (-not $python -or -not $openssl -or -not $tool) {
    Fail "the update test needs python.exe, openssl.exe and the build's mitcad-release.exe (or -NoUpdate)"
    return
  }
  $version = if ($installer.Name -match '^mitcad-(.+)-windows-x64\.exe$') { $Matches[1] } else { '' }
  $work = Join-Path $Out 'update'
  $www = Join-Path $work 'www'
  New-Item -ItemType Directory -Force (Join-Path $www 'good'), (Join-Path $www 'tampered') | Out-Null

  # A certificate for 127.0.0.1, made for this run, and the server.
  $cert = Join-Path $work 'cert.pem'
  $key = Join-Path $work 'key.pem'
  $proc = Start-Process -FilePath $openssl -Wait -PassThru -RedirectStandardError (Join-Path $work 'openssl.log') `
    -ArgumentList 'req', '-x509', '-newkey', 'ec', '-pkeyopt', 'ec_paramgen_curve:prime256v1', '-nodes', '-days', '2',
                  '-subj', '/CN=localhost', '-addext', 'subjectAltName=IP:127.0.0.1,DNS:localhost',
                  '-keyout', "`"$key`"", '-out', "`"$cert`""
  if ($proc.ExitCode -ne 0 -or -not (Test-Path $cert)) { Fail "openssl made no certificate (see $work\openssl.log)"; return }
  $portFile = Join-Path $work 'port'
  $server = Start-Process -FilePath $python -PassThru -RedirectStandardError (Join-Path $work 'server.log') `
    -ArgumentList "`"$(Join-Path $PSScriptRoot 'update-test-server.py')`"", "`"$www`"", "`"$cert`"", "`"$key`"",
                  "`"$(Join-Path $work 'requests.log')`"", "`"$portFile`""
  try {
    for ($i = 0; $i -lt 100 -and -not (Test-Path $portFile); $i++) { Start-Sleep -Milliseconds 100 }
    if (-not (Test-Path $portFile)) { Fail "the HTTPS server did not start (see $work\server.log)"; return }
    $base = "https://127.0.0.1:$((Get-Content $portFile).Trim())"

    # A release key made for this run; the release is this installer, and
    # tampered\ serves it changed in one byte under the same manifest.
    $bytes = New-Object byte[] 32
    [Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($bytes)
    $env:MITCAD_RELEASE_KEY = ($bytes | ForEach-Object { $_.ToString('x2') }) -join ''
    $public = (& $tool public-key | Out-String).Trim()
    foreach ($folder in 'good', 'tampered') {
      $copy = Join-Path $www "$folder\$($installer.Name)"
      Copy-Item $installer.FullName $copy
      & $tool manifest --version $version --date 2026-10-06 --notes "$base/$folder/notes.html" `
        --asset windows-x64 $copy "$base/$folder/$($installer.Name)" --out (Join-Path $www "$folder\update-manifest.json") |
        Out-Null
      if ($LASTEXITCODE -ne 0) { Fail "mitcad-release could not sign $folder"; return }
    }
    Remove-Item Env:MITCAD_RELEASE_KEY
    $stream = [IO.File]::Open((Join-Path $www "tampered\$($installer.Name)"), 'Open', 'ReadWrite')
    $stream.Position = [int64]($stream.Length / 2)
    $byte = $stream.ReadByte()
    $stream.Position = $stream.Position - 1
    $stream.WriteByte($byte -bxor 1)
    $stream.Close()
    Pass "release $version signed with a test key, served at $base"

    $script:UiApp = Join-Path $bin 'mitcad.exe'
    $script:UiOut = $work
    $script:UiSettings = Join-Path $work 'settings'
    $script:UiAutosave = Join-Path $work 'autosave'
    $script:UiResults = Join-Path $work 'results'
    $script:UiThumbnails = Join-Path $work 'thumbnails'
    $ini = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'
    $env:MITCAD_UPDATE_TEST_KEY = $public
    $env:MITCAD_UPDATE_TEST_CA = $cert
    $env:MITCAD_TEST_VERSION = '0.0.0'
    $download = Join-Path $env:TEMP "mitcad-update\$($installer.Name)"
    $requests = Join-Path $work 'requests.log'

    # The administrator's policy in the registry: no request, not even for
    # Check for Updates (F8 in the test's settings: menus do not open in
    # session 0). The policy goes again at once.
    $policies = 'HKLM:\SOFTWARE\Policies\Mitcad'
    New-Item -Force $policies, "$policies\Mitcad" | Out-Null
    New-ItemProperty -Force -Path "$policies\Mitcad" -Name DisableUpdateCheck -PropertyType DWord -Value 1 | Out-Null
    try {
      New-Item -ItemType Directory -Force (Join-Path $script:UiSettings 'Mitcad') | Out-Null
      Set-Content -Encoding ascii $ini '[shortcuts]', 'help.check_updates=F8'
      $env:MITCAD_UPDATE_URL = "$base/good/update-manifest.json"
      Ui-StartApp 'update-policy' -Updates
      Ui-ExpectLog 'Update checks are turned off by the administrator' 'the registry policy turns checks off'
      Ui-Step 'check for updates (F8)' { Ui-Key 'F8' }
      Ui-ExpectLog 'Update notice (message): Update checks are turned off by your administrator.' 'Check for Updates says so'
      Start-Sleep -Seconds 4
      if (@(Get-Content $requests -ErrorAction SilentlyContinue).Count -eq 0) { Pass 'no request' }
      else { Fail "requests with checks off: $(Get-Content $requests -Raw)" }
      Ui-StopApp
    } finally {
      Remove-Item -Recurse -Force $policies
    }

    # A tampered download: refused and deleted.
    Remove-Item -Recurse -Force $script:UiSettings -ErrorAction SilentlyContinue
    $env:MITCAD_UPDATE_URL = "$base/tampered/update-manifest.json"
    Ui-StartApp 'update-tampered' -Updates
    Ui-ExpectLog 'Updates: Mitcad 0.0.0 for windows-x64, installed in ' 'the installation can update itself'
    Ui-ExpectLog '[Release Notes, Install, Skip This Version, Later]' 'the start-up check offers Install' 30
    Ui-ExpectLog 'Update notice Install at ' 'the notice places Install'
    Ui-Step 'install (click)' { Ui-ClickLogged 'Update notice Install' }
    Ui-ExpectLog "Update rejected: the download is not the release's: its SHA-256 differs from the release's" `
      'a tampered download is refused' 120
    if (Test-Path $download) { Fail 'the tampered download is left' } else { Pass 'and deleted' }
    Ui-StopApp

    # The release: a changed design open, saved when asked; the installer
    # puts back the file removed here.
    $design = Join-Path $work 'block.mitcad'
    @'
{
  "format": "mitcad",
  "version": 1,
  "parameters": [
    { "name": "d1", "value": 60.0 }, { "name": "d2", "value": 40.0 }, { "name": "d3", "value": 20.0 }
  ],
  "features": [
    { "type": "sketch", "name": "Sketch1", "shapes": [
      { "type": "rectangle", "corner": [0.0, 0.0], "width": "d1", "height": "d2" } ] },
    { "type": "extrude", "name": "Extrude1", "sketch": "Sketch1", "profile": 0,
      "distance": "d3", "operation": "new_body" }
  ]
}
'@ | Set-Content -Encoding ascii $design
    Remove-Item (Join-Path $bin 'mitcad-cli.exe')
    Remove-Item -Recurse -Force $script:UiSettings -ErrorAction SilentlyContinue
    $env:MITCAD_UPDATE_URL = "$base/good/update-manifest.json"
    Ui-StartApp 'update' @('--open', $design, '--set', 'd3=35') -Updates
    Ui-ExpectLog '[Release Notes, Install, Skip This Version, Later]' 'the start-up check offers Install' 30
    Ui-ExpectLog 'Update notice Install at ' 'the notice places Install'
    Ui-ClickLogged 'Update notice Install'
    Ui-ExpectLog 'Update verified: ' 'the download is verified' 120
    Ui-ExpectLog "Update staged: 0.0.0 -> $version" 'the update waits for Mitcad to quit'
    Ui-FocusDialog '^Mitcad$'
    Ui-Key 'Return' # Save, the default
    if (-not $script:UiProcess.WaitForExit(60000)) { Ui-Fail 'Mitcad did not quit' }
    Pass 'asked to save, saved, quit'
    $result = $null
    for ($i = 0; $i -lt 300 -and -not $result; $i++) {
      Start-Sleep -Seconds 1
      $result = Select-String -Path $ini -Pattern '^lastResult=(.*)' -ErrorAction SilentlyContinue |
        ForEach-Object { $_.Matches[0].Groups[1].Value.Trim('"') }
    }
    $restarted = @(Get-Process -Name mitcad -ErrorAction SilentlyContinue |
      Where-Object { $_.Path -eq (Join-Path $bin 'mitcad.exe') })
    if ($result -eq "installed 0.0.0 -> $version") { Pass "Mitcad started again: $result" }
    else { Fail "the update was not recorded as installed: '$result'" }
    if ($restarted.Count -gt 0) { Pass 'the new Mitcad runs' } else { Fail 'no Mitcad runs after the update' }
    $restarted | Stop-Process -Force -ErrorAction SilentlyContinue
    $restarted | ForEach-Object { $_.WaitForExit(10000) | Out-Null }
    if (Test-Path (Join-Path $bin 'mitcad-cli.exe')) { Pass 'the installer ran over the installation' }
    else { Fail 'bin\mitcad-cli.exe is missing: the installer did not run' }
    $d3 = ((Get-Content $design -Raw | ConvertFrom-Json).parameters | Where-Object { $_.name -eq 'd3' }).value
    if ($d3 -eq 35) { Pass 'the design was saved' } else { Fail "the design was not saved: d3 = $d3" }
  } finally {
    foreach ($variable in 'MITCAD_UPDATE_URL', 'MITCAD_UPDATE_TEST_KEY', 'MITCAD_UPDATE_TEST_CA', 'MITCAD_TEST_VERSION',
                          'MITCAD_RELEASE_KEY') {
      Remove-Item "Env:$variable" -ErrorAction SilentlyContinue
    }
    $env:MITCAD_NO_UPDATE_CHECK = '1'
    if ($server -and -not $server.HasExited) { $server.Kill() }
    Remove-Item -Recurse -Force $www -ErrorAction SilentlyContinue
  }
}
if ($MesaDir -and -not $NoUpdate) {
  # The UI helpers: [UiInput] posts input to the app's window in session 0.
  . (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
  # A failed UI step ends the update test, not the whole test: the
  # installation is still uninstalled below.
  function Ui-Fail([string]$Message) {
    Write-Host "FAIL: $Message"
    Ui-LogLines | Select-Object -Last 40 | ForEach-Object { Write-Host "     $_" }
    Ui-StopApp
    $script:failures++
    throw 'the update test failed'
  }
  try { Test-Update } catch { if ($_.Exception.Message -ne 'the update test failed') { throw } }
  Get-Process -Name mitcad -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$install\*" } |
    Stop-Process -Force -ErrorAction SilentlyContinue
}

# 6. Uninstall. The test's own Mesa copy is removed first (the uninstaller
# does not know it). NSIS copies
# at once unless _?= says where the installation is.
if ($MesaDir) { Remove-Item (Join-Path $bin "opengl32.dll"), (Join-Path $bin "libgallium_wgl.dll") }
$proc = Start-Process -FilePath (Join-Path $install 'Uninstall.exe') -ArgumentList '/S', "_?=$install" -Wait -PassThru
if ($proc.ExitCode -eq 0) { Pass 'silent uninstall' } else { Fail "uninstaller exit code $($proc.ExitCode)" }
if (Test-Path $bin) { Fail 'bin remains after the uninstall' } else { Pass 'bin removed' }
if (Test-Path 'Registry::HKEY_CLASSES_ROOT\.mitcad') { Fail '.mitcad is still registered' } else { Pass '.mitcad file type removed' }

if ($failures -gt 0) { Write-Host "$failures check(s) failed"; exit 1 }
Write-Host 'Installer test passed'
