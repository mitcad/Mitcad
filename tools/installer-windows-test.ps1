# SPDX-License-Identifier: MIT
# Tests the Windows installer in the build VM, in both of its scopes
# (mitcad#118): just for the current user (the default: HKCU, no
# elevation) and for all users (/AllUsers: HKLM, Program Files when no
# folder is given). For each it installs the NSIS installer of a build
# directory silently into a folder of its own and checks the registration
# of that scope (the uninstall entry, installation.ini, the .mitcad file
# type, the Start menu shortcut) and none in the other scope.
#   0. the installer runs as the user: its manifest asks for no elevation
#      (asInvoker), so a per-user installation and its updates have no UAC
#      prompt; only /AllUsers elevates (which session 0 cannot show: the
#      test runs with administrator rights).
#   1. just for the current user, the installer's files: executables, Qt
#      and OCCT DLLs, the Qt platform plugin, the Visual C++ runtime, the
#      licences; no Mesa, no PDB files. The installed mitcad-updater does
#      not run an installer whose SHA-256 is not the one it is given, or
#      without one, and logs the reason as its last line (mitcad#603).
#   2. mitcad-cli runs a script of the repository; with the toolchain (Qt,
#      vcpkg, Rust, Visual Studio) removed from PATH from here on.
#   3. mitcad.exe --version loads Qt with its plugins. The render worker
#      (mitcad#51): mitcad-render.exe, Open Image Denoise's DLLs and the
#      renderer's licences are installed, and the worker renders its test
#      scene headless. A build with Cycles' CUDA device (mitcad#202) has
#      every GPU kernel of the build folder in bin\cycles\lib and cuew's
#      licence; a build configured with MITCAD_RENDER_REQUIRED_DEVICES
#      naming CUDA (a release) must have them. The installed worker lists
#      its devices (the CPU at least: the CUDA driver is loaded at run
#      time, and a VM has no NVIDIA GPU).
#   4. with -MesaDir: mitcad.exe --demo --screenshot renders the demo block
#      (Mesa is copied next to the installed copy, as a VM has no GPU).
#   5. with -MesaDir, the automatic update (mitcad#9) in each scope: the
#      installed Mitcad, which takes itself for version 0.0.0
#      (MITCAD_TEST_VERSION), finds this installer as the release in a
#      manifest signed with a key made for the test (mitcad-release of the
#      build directory) and served over HTTPS from 127.0.0.1
#      (tools/update-test-server.py, Python) with a certificate made for the
#      test (openssl), which only its update requests trust
#      (MITCAD_UPDATE_TEST_CA). Just for the current user also: the
#      administrator's policy turns checks off, and a tampered download is
#      refused and deleted. Then Install downloads the release into a
#      private folder of a random name and verifies it, the changed design
#      is saved when asked, Mitcad quits,
#      mitcad-updater runs the installer silently over the installation in
#      its scope and starts Mitcad again, which records the update as
#      installed; the installation is still registered in its scope only.
#      For all users, before that: a silent install with only /D= of the
#      installation's folder (what an earlier release's updater runs) keeps
#      it for all users. Only a test build reads the tests' variables
#      (the CMake option MITCAD_UPDATE_TEST_HOOKS in the build's
#      CMakeCache.txt, mitcad#921); for all users its update elevates
#      nothing, since the test has administrator rights (an update from a
#      test's source never asks for them). A build without the option (a
#      release's configuration) is checked instead to ignore them: its
#      own version, GitHub's addresses, the built-in key only, no request
#      to the test's server.
#   6. the uninstaller removes the files and every registry key of its
#      scope.
# The installer registers the .mitcad file type under HKCU and HKLM and
# uninstalls an earlier installation in each scope; run the test only in
# the VM.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\installer-windows-test.ps1
#          [-Build build\dev] [-MesaDir C:\dev\mesa\bin] [-Out dir] [-NoUpdate]
#          [-Scope both|user|all] [-SkipStale]
# -SkipStale (the ctest app.installer-windows): exit 77, skipped, when the
# build directory has no installer newer than its mitcad.exe.
param(
  [string]$Build = (Join-Path $PSScriptRoot '..\build\dev'),
  [string]$MesaDir = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-installer-test'),
  [switch]$NoUpdate,
  [ValidateSet('both', 'user', 'all')]
  [string]$Scope = 'both',
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
# Whether the build reads the update tests' environment variables (the
# CMake option MITCAD_UPDATE_TEST_HOOKS, mitcad#921): only then does step 5
# update; a build without it is checked to ignore them.
$buildCache = Join-Path $Build 'CMakeCache.txt'
$script:UpdateTestHooks = (Test-Path $buildCache) -and
  (Select-String -Path $buildCache -Pattern '^MITCAD_UPDATE_TEST_HOOKS:BOOL=(ON|1|TRUE|YES|Y)\s*$' -Quiet)
Write-Host "Update test hooks: $(if ($script:UpdateTestHooks) { 'on' } else { 'off (step 5 checks that the variables are ignored)' })"

# The registry of each scope. The installer is 32-bit, so HKLM's keys of
# its own are under WOW6432Node; HKLM\Software\Classes is shared.
$scopeName = @{ user = 'just for the current user'; all = 'for all users' }
$scopeLog = @{ user = 'for the current user'; all = 'for all users' }
$hive = @{ user = 'HKCU:'; all = 'HKLM:' }
$uninstallKeys = @{
  user = @('HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Mitcad')
  all  = @('HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Mitcad',
           'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Mitcad')
}
$vendorKeys = @{
  user = @('HKCU:\Software\MitCad developers')
  all  = @('HKLM:\Software\WOW6432Node\MitCad developers', 'HKLM:\Software\MitCad developers')
}
$startMenu = @{
  user = [Environment]::GetFolderPath('Programs')
  all  = [Environment]::GetFolderPath('CommonPrograms')
}

# 0. The manifest (a resource of the installer's stub, at its start).
$stream = [IO.File]::OpenRead($installer.FullName)
try {
  $head = New-Object byte[] ([Math]::Min(4MB, $stream.Length))
  $read = $stream.Read($head, 0, $head.Length)
} finally { $stream.Close() }
$headText = [Text.Encoding]::ASCII.GetString($head, 0, $read)
if ($headText -match 'requestedExecutionLevel\s+level=.asInvoker.') { Pass 'the installer runs as the user (asInvoker)' }
elseif ($headText -match 'requestedExecutionLevel\s+level=.(\w+).') { Fail "the installer asks for $($Matches[1])" }
else { Fail 'no requestedExecutionLevel in the installer' }

# A registration an interrupted run left behind, whose uninstaller is gone,
# goes first (the installer would pass it over, but the checks below look
# at the registry).
foreach ($key in $uninstallKeys.user + $uninstallKeys.all) {
  $uninstall = (Get-ItemProperty $key -ErrorAction SilentlyContinue).UninstallString
  if ($uninstall -and -not (Test-Path $uninstall.Trim('"'))) {
    Remove-Item -Recurse $key
    Write-Host "note: removed the registration of a missing installation ($uninstall)"
  }
}
if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }
New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path

# The update test's tools, found before PATH is cut down in step 2.
$python = Get-Command python.exe -All -ErrorAction SilentlyContinue | Where-Object { $_.Source -notlike '*WindowsApps*' } |
  Select-Object -First 1 -ExpandProperty Source
$openssl = @((Get-Command openssl.exe -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty Source),
             "$env:ProgramFiles\Git\mingw64\bin\openssl.exe", "$env:ProgramFiles\Git\usr\bin\openssl.exe") |
  Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1

function Get-Registration([string]$In) {
  foreach ($key in $uninstallKeys[$In]) {
    $entry = Get-ItemProperty $key -ErrorAction SilentlyContinue
    if ($entry) { return $entry }
  }
  return $null
}

function Get-Default([string]$Key) {
  $item = Get-ItemProperty $Key -ErrorAction SilentlyContinue
  if ($item) { return $item.'(default)' }
  return $null
}

# The installation in $install is registered in $In and not in the other
# scope.
function Test-Registered([string]$In, [string]$Install, [string]$When) {
  $other = if ($In -eq 'user') { 'all' } else { 'user' }
  $entry = Get-Registration $In
  if ($entry -and $entry.InstallLocation -eq $Install) {
    Pass "${When}: registered $($scopeName[$In]) ($($entry.DisplayName))"
  } else {
    Fail "${When}: no uninstall entry $($scopeName[$In]) for $Install (found: '$($entry.InstallLocation)')"
  }
  $otherEntry = Get-Registration $other
  if ($otherEntry -and $otherEntry.InstallLocation -eq $Install) { Fail "${When}: registered $($scopeName[$other]) too" }
  else { Pass "${When}: not registered $($scopeName[$other])" }
  $scopeFile = Join-Path $Install 'installation.ini'
  if ((Test-Path $scopeFile) -and (Get-Content $scopeFile -Raw) -match "Scope=$In\b") { Pass "${When}: installation.ini says $In" }
  else { Fail "${When}: installation.ini does not say Scope=$In" }
  $classes = "$($hive[$In])\Software\Classes"
  $type = Get-Default "$classes\.mitcad"
  $command = Get-Default "$classes\Mitcad.Project\shell\open\command"
  if ($type -eq 'Mitcad.Project' -and $command -and $command.Contains("$Install\bin\mitcad.exe")) {
    Pass "${When}: .mitcad opens with $command ($($hive[$In]))"
  } else { Fail "${When}: the .mitcad file type in $classes is '$type', '$command'" }
  if ($In -eq 'user' -and (Test-Path 'HKLM:\Software\Classes\Mitcad.Project')) {
    $allCommand = Get-Default 'HKLM:\Software\Classes\Mitcad.Project\shell\open\command'
    if ($allCommand -and $allCommand.Contains("$Install\bin\mitcad.exe")) { Fail "${When}: .mitcad registered for all users too" }
  }
  $shortcut = Join-Path $startMenu[$In] 'MitCad\MitCad.lnk'
  if (Test-Path $shortcut) { Pass "${When}: Start menu shortcut $shortcut" } else { Fail "${When}: no $shortcut" }
}

# 6. After the uninstall: no files and none of the scope's keys.
function Test-Unregistered([string]$In, [string]$Install) {
  if (Test-Path (Join-Path $Install 'bin')) { Fail 'bin remains after the uninstall' } else { Pass 'bin removed' }
  if (Test-Path (Join-Path $Install 'installation.ini')) { Fail 'installation.ini remains' } else { Pass 'installation.ini removed' }
  $classes = "$($hive[$In])\Software\Classes"
  $left = @($uninstallKeys[$In] + $vendorKeys[$In] + "$classes\.mitcad", "$classes\Mitcad.Project" |
    Where-Object { Test-Path $_ })
  if ($left.Count -eq 0) { Pass "no registry key of the installation left in $($hive[$In])" }
  else { Fail "left in the registry: $($left -join ', ')" }
  $folder = Join-Path $startMenu[$In] 'MitCad'
  if (Test-Path $folder) { Fail "the Start menu folder $folder remains" } else { Pass 'Start menu folder removed' }
}

# 1.-4. The installation's files and programs (just for the current user).
function Test-Programs([string]$Install) {
  $bin = Join-Path $Install 'bin'
  # 1. Files (Qt's plugins are in plugins, which bin\qt.conf points to).
  $required = @('mitcad.exe', 'mitcad-cli.exe', 'mitcad-updater.exe', 'Qt6Core.dll', 'Qt6Gui.dll', 'Qt6Widgets.dll',
                'Qt6OpenGL.dll', 'Qt6OpenGLWidgets.dll', 'Qt6Svg.dll', 'Qt6Network.dll',
                '..\plugins\platforms\qwindows.dll', '..\plugins\tls\qschannelbackend.dll',
                'TKernel.dll', 'TKService.dll', 'TKOpenGl.dll', 'vcruntime140.dll', 'msvcp140.dll')
  foreach ($file in $required) {
    if (Test-Path (Join-Path $bin $file)) { Pass "bin\$file" } else { Fail "bin\$file is missing" }
  }
  foreach ($file in 'LICENSE', 'THIRD-PARTY-NOTICES.txt', 'Uninstall.exe', 'licenses\droid-sans\LICENSE.txt',
                    'licenses\qtkeychain\COPYING') {
    if (Test-Path (Join-Path $Install $file)) { Pass $file } else { Fail "$file is missing" }
  }
  # The render worker (every build has it) with its libraries and licences.
  foreach ($file in 'bin\mitcad-render.exe', 'bin\OpenImageDenoise.dll', 'bin\OpenImageDenoise_core.dll',
                    'bin\OpenImageDenoise_device_cpu.dll', 'licenses\cycles\LICENSE',
                    'licenses\openimagedenoise\LICENSE.txt') {
    if (Test-Path (Join-Path $Install $file)) { Pass $file } else { Fail "$file is missing" }
  }
  # The GPU kernels: every one of the build folder's (cycles\lib next to
  # its mitcad-render.exe), and some when the build requires CUDA.
  $builtWorker = Find-Built 'mitcad-render.exe', 'app\mitcad-render.exe'
  # Open Image Denoise's other device modules next to it (CUDA's with -Cuda).
  foreach ($module in @(if ($builtWorker) {
      Get-ChildItem (Split-Path $builtWorker) -Filter 'OpenImageDenoise_device_*.dll' |
        Where-Object { $_.Name -ne 'OpenImageDenoise_device_cpu.dll' }
    })) {
    if (Test-Path (Join-Path $bin $module.Name)) { Pass "bin\$($module.Name)" } else { Fail "bin\$($module.Name) is missing" }
  }
  $builtKernels = @(if ($builtWorker) {
      Get-ChildItem (Join-Path (Split-Path $builtWorker) 'cycles\lib') -File -Filter '*.zst' -ErrorAction SilentlyContinue
    })
  $cache = Join-Path $Build 'CMakeCache.txt'
  $requireCuda = (Test-Path $cache) -and
    (Select-String -Path $cache -Pattern '^MITCAD_RENDER_REQUIRED_DEVICES:[A-Z]+=.*\bCUDA\b' -Quiet)
  $installedKernels = Join-Path $bin 'cycles\lib'
  $missingKernels = @($builtKernels | Where-Object { -not (Test-Path (Join-Path $installedKernels $_.Name)) })
  $cubins = @($builtKernels | Where-Object { $_.Name -like 'kernel_sm_*.cubin.zst' })
  if ($requireCuda -and $cubins.Count -eq 0) { Fail 'the build requires CUDA but has no CUDA kernels' }
  if ($builtKernels.Count -gt 0) {
    if ($missingKernels.Count -eq 0) {
      $size = ($builtKernels | Measure-Object -Sum Length).Sum
      Pass "$($builtKernels.Count) GPU kernels in bin\cycles\lib ($([math]::Round($size / 1MB, 1)) MB)"
    } else { Fail "GPU kernels missing from bin\cycles\lib: $($missingKernels.Name -join ', ')" }
    if (Test-Path (Join-Path $Install 'licenses\cycles\cuew-license.txt')) { Pass 'licenses\cycles\cuew-license.txt' }
    else { Fail 'licenses\cycles\cuew-license.txt is missing' }
  } else {
    Pass 'no GPU kernels (a build without GPU devices)'
  }
  $licenseFiles = @(Get-ChildItem (Join-Path $Install 'licenses\vcpkg') -Recurse -Filter copyright -ErrorAction SilentlyContinue)
  if ($licenseFiles.Count -gt 0) { Pass "$($licenseFiles.Count) vcpkg licence texts" } else { Fail 'no vcpkg licence texts' }
  foreach ($pattern in 'opengl32.dll', 'libgallium_wgl.dll', 'opengl32sw.dll', '*.pdb') {
    $found = @(Get-ChildItem $Install -Recurse -Filter $pattern -ErrorAction SilentlyContinue)
    if ($found.Count -eq 0) { Pass "no $pattern" } else { Fail "$pattern must not be installed: $($found[0].FullName)" }
  }

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
  # The render worker with the installation's libraries only (no OpenGL).
  $renderLog = Join-Path $Out 'render.log'
  $proc = Start-Process -FilePath (Join-Path $bin 'mitcad-render.exe') `
    -ArgumentList '--bench', '64x48', '--samples', '2', '--output', "`"$(Join-Path $Out 'render.png')`"" `
    -RedirectStandardOutput $renderLog -RedirectStandardError (Join-Path $Out 'render-err.log') -Wait -PassThru
  $renderText = (Get-Content $renderLog, (Join-Path $Out 'render-err.log') -Raw -ErrorAction SilentlyContinue) -join ''
  if ($proc.ExitCode -eq 0 -and $renderText -match 'render-bench 64x48 samples 2') { Pass 'mitcad-render.exe --bench 64x48' }
  else { Fail "mitcad-render.exe exit code $($proc.ExitCode) (see $renderLog)" }
  # Its devices: a CUDA build starts without an NVIDIA driver too.
  $devicesOut = Join-Path $Out 'devices.json'
  $proc = Start-Process -FilePath (Join-Path $bin 'mitcad-render.exe') -ArgumentList '--list-devices' `
    -RedirectStandardOutput $devicesOut -RedirectStandardError (Join-Path $Out 'devices-err.log') -Wait -PassThru
  $devicesText = if (Test-Path $devicesOut) { (Get-Content $devicesOut -Raw) } else { '' }
  $types = @(try { ($devicesText | ConvertFrom-Json).devices | ForEach-Object { $_.type } } catch { })
  if ($proc.ExitCode -eq 0 -and $types -contains 'CPU') { Pass "mitcad-render.exe --list-devices: $($types -join ', ')" }
  else { Fail "mitcad-render.exe --list-devices exit code $($proc.ExitCode), output '$devicesText'" }

  # 4. Rendering, when a software OpenGL is given (Mesa next to the
  # installed copy, also for the update test; removed before the uninstall).
  if ($MesaDir) {
    Copy-Item (Join-Path $MesaDir 'opengl32.dll'), (Join-Path $MesaDir 'libgallium_wgl.dll') $bin
    $shot = Join-Path $Out 'demo.png'
    $proc = Start-Process -FilePath (Join-Path $bin 'mitcad.exe') `
      -ArgumentList '--demo', '--no-recovery', '--screenshot', "`"$shot`"" -Wait -PassThru
    if ($proc.ExitCode -eq 0 -and (Test-Path $shot) -and (Get-Item $shot).Length -gt 5000) {
      Pass "mitcad.exe --demo --screenshot ($((Get-Item $shot).Length) bytes)"
    } else { Fail "mitcad.exe --screenshot exit code $($proc.ExitCode)" }
  }
}

# 1b. The installed mitcad-updater checks the installer again right before
# it runs it (mitcad#603): given a SHA-256 that is not the installer's, one
# not written as the manifest writes it, or none, it does not run it (the
# installation folder it is given is never made) and logs why.
function Test-UpdaterCheck([string]$Install, [string]$Work) {
  New-Item -ItemType Directory -Force $Work | Out-Null
  $updater = Join-Path $Work 'mitcad-updater.exe'
  $copy = Join-Path $Work $installer.Name
  Copy-Item (Join-Path $Install 'bin\mitcad-updater.exe') $updater
  Copy-Item $installer.FullName $copy
  $target = Join-Path $Work 'never'
  $restart = Join-Path $Work 'none.exe'
  $log = Join-Path $Work 'updater.log'
  $hash = (Get-FileHash -Algorithm SHA256 $copy).Hash.ToLowerInvariant()
  $wrong = $(if ($hash.StartsWith('0')) { '1' } else { '0' }) + $hash.Substring(1)
  $cases = @(
    @{ Name = "a SHA-256 that is not the installer's"; Hash = @('--sha256', $wrong); Code = 1
       Text = 'The installer is not the verified download' },
    @{ Name = 'an upper-case SHA-256'; Hash = @('--sha256', $hash.ToUpperInvariant()); Code = 2; Text = 'No SHA-256' },
    @{ Name = 'no SHA-256'; Hash = @(); Code = 2; Text = 'No SHA-256' })
  foreach ($case in $cases) {
    Remove-Item $log -ErrorAction SilentlyContinue
    $arguments = @('--installer', "`"$copy`"") + $case.Hash +
                 @('--dir', "`"$target`"", '--scope', 'user', '--restart', "`"$restart`"", '--log', "`"$log`"")
    $proc = Start-Process -FilePath $updater -ArgumentList $arguments -Wait -PassThru
    $text = Get-Content $log -Raw -ErrorAction SilentlyContinue
    if ($proc.ExitCode -eq $case.Code -and $text -and $text.Contains($case.Text) -and -not (Test-Path $target)) {
      Pass "mitcad-updater does not run the installer with $($case.Name)"
    } else {
      Fail "mitcad-updater with $($case.Name): exit code $($proc.ExitCode), installed: $(Test-Path $target), log: $text"
    }
  }
  # The reason is the log's last line, which the next start reports (here a
  # program that starts and ends at once stands for Mitcad, in a folder
  # that exists, so that the start succeeds).
  Remove-Item $log -ErrorAction SilentlyContinue
  $stand = Join-Path $env:SystemRoot 'System32\hostname.exe'
  $proc = Start-Process -FilePath $updater -Wait -PassThru -ArgumentList (
    @('--installer', "`"$copy`"", '--sha256', $wrong, '--dir', "`"$Work`"", '--scope', 'user',
      '--restart', "`"$stand`"", '--log', "`"$log`""))
  $last = @(Get-Content $log -ErrorAction SilentlyContinue) | Where-Object { $_.Trim() } | Select-Object -Last 1
  if ($last -and $last.StartsWith('The installer is not the verified download')) { Pass 'the reason is the last line' }
  else { Fail "the log's last line is '$last'" }
}

# 5. The automatic update of the installation in $Install, installed in
# scope $In (the app's window is driven with tools/ui-windows-lib.ps1).
# -Full adds the administrator's policy and a tampered download.
function Test-Update([string]$In, [string]$Install, [string]$Work, [switch]$Full) {
  $tool = Find-Built 'mitcad-release.exe', 'core\mitcad-release.exe'
  if (-not $python -or -not $openssl -or -not $tool) {
    Fail "the update test needs python.exe, openssl.exe and the build's mitcad-release.exe (or -NoUpdate)"
    return
  }
  $bin = Join-Path $Install 'bin'
  $version = if ($installer.Name -match '^mitcad-(.+)-windows-x64\.exe$') { $Matches[1] } else { '' }
  $www = Join-Path $Work 'www'
  New-Item -ItemType Directory -Force (Join-Path $www 'good'), (Join-Path $www 'tampered') | Out-Null

  # A certificate for 127.0.0.1, made for this run, and the server.
  $cert = Join-Path $Work 'cert.pem'
  $key = Join-Path $Work 'key.pem'
  $proc = Start-Process -FilePath $openssl -Wait -PassThru -RedirectStandardError (Join-Path $Work 'openssl.log') `
    -ArgumentList 'req', '-x509', '-newkey', 'ec', '-pkeyopt', 'ec_paramgen_curve:prime256v1', '-nodes', '-days', '2',
                  '-subj', '/CN=localhost', '-addext', 'subjectAltName=IP:127.0.0.1,DNS:localhost',
                  '-keyout', "`"$key`"", '-out', "`"$cert`""
  if ($proc.ExitCode -ne 0 -or -not (Test-Path $cert)) { Fail "openssl made no certificate (see $Work\openssl.log)"; return }
  $portFile = Join-Path $Work 'port'
  $script:ServerLog = Join-Path $Work 'server.log'
  $server = Start-Process -FilePath $python -PassThru -RedirectStandardError $script:ServerLog `
    -ArgumentList "`"$(Join-Path $PSScriptRoot 'update-test-server.py')`"", "`"$www`"", "`"$cert`"", "`"$key`"",
                  "`"$(Join-Path $Work 'requests.log')`"", "`"$portFile`""
  try {
    for ($i = 0; $i -lt 100 -and -not (Test-Path $portFile); $i++) { Start-Sleep -Milliseconds 100 }
    if (-not (Test-Path $portFile)) { Fail "the HTTPS server did not start (see $Work\server.log)"; return }
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
    $script:UiOut = $Work
    $script:UiSettings = Join-Path $Work 'settings'
    $script:UiAutosave = Join-Path $Work 'autosave'
    $script:UiResults = Join-Path $Work 'results'
    $script:UiThumbnails = Join-Path $Work 'thumbnails'
    $ini = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'
    $env:MITCAD_UPDATE_TEST_KEY = $public
    $env:MITCAD_UPDATE_TEST_CA = $cert
    $env:MITCAD_TEST_VERSION = '0.0.0'
    $requests = Join-Path $Work 'requests.log'

    if (-not $script:UpdateTestHooks) {
      # A build without MITCAD_UPDATE_TEST_HOOKS (a release's configuration,
      # mitcad#921): the installed MitCad ignores the tests' variables. Its
      # check would go to GitHub, so checks stay off (Ui-StartApp without
      # -Updates); the source is logged anyway.
      Remove-Item -Recurse -Force $script:UiSettings -ErrorAction SilentlyContinue
      $env:MITCAD_UPDATE_URL = "$base/good/update-manifest.json"
      Ui-StartApp 'update-ignored'
      Ui-ExpectLog "Updates: Mitcad $version for windows-x64, installed in " 'its own version, not MITCAD_TEST_VERSION'
      Ui-ExpectLog ('Update source: manifest at https://github.com/mitcad/Mitcad/releases/latest/download/update-manifest.json, ' +
                    'releases at https://api.github.com/repos/mitcad/Mitcad/releases?per_page=20, ') `
        "GitHub's addresses, not MITCAD_UPDATE_URL"
      Ui-ExpectLog ('Update source: this build ignores MITCAD_UPDATE_URL, MITCAD_UPDATE_TEST_KEY, MITCAD_UPDATE_TEST_CA, ' +
                    'MITCAD_TEST_VERSION (MITCAD_UPDATE_TEST_HOOKS off)') "the update tests' variables are ignored"
      $logText = "$(Ui-LogText)"
      if ($logText.Contains('and the test key') -or $logText.Contains('test certificate')) {
        Fail "the installed MitCad took the test's key or certificate"
      } else { Pass 'the built-in key only, no test certificate' }
      Start-Sleep -Seconds 4
      if (@(Get-Content $requests -ErrorAction SilentlyContinue).Count -eq 0) { Pass "no request to the test's server" }
      else { Fail "requests to the test's server: $(Get-Content $requests -Raw)" }
      Ui-StopApp
      Write-Host 'note the update itself needs a test build (-DMITCAD_UPDATE_TEST_HOOKS=ON)'
      return
    }

    if ($Full) {
      # The administrator's policy in the registry: no request, not even
      # for Check for Updates (F8 in the test's settings: menus do not open
      # in session 0). The policy goes again at once.
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
      # The download's private folder (mitcad#603) is named in the log.
      $download = Ui-LastMatch '^.*Update rejected: .*\(deleted (.+)\)\s*$'
      if (-not $download) { Fail 'the log does not name the deleted download' }
      elseif (Test-Path $download) { Fail "the tampered download is left: $download" }
      else { Pass 'and deleted' }
      Ui-StopApp
    }

    # The release: a changed design open, saved when asked; the installer
    # puts back the file removed here.
    $design = Join-Path $Work 'block.mitcad'
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
    Ui-ExpectLog "\Mitcad $($scopeLog[$In])" "the installation is $($scopeName[$In])"
    # A test build reads the tests' variables; for all users the update
    # elevates nothing, as the test runs with administrator rights
    # (mitcad#921: a test's source never asks for them).
    Ui-ExpectLog 'Update source: this test build reads MITCAD_UPDATE_URL, ' "a test build reads the update tests' variables"
    Ui-ExpectLog '[Release Notes, Install, Skip This Version, Later]' 'the start-up check offers Install' 30
    Ui-ExpectLog 'Update notice Install at ' 'the notice places Install'
    # The question to save is titled "MitCad"; hidden windows of the app
    # (the 3D view's offscreen surfaces) take the application name: it is
    # the new one.
    $hidden = Ui-Windows '^MitCad$'
    Ui-ClickLogged 'Update notice Install'
    Ui-ExpectLog 'Update verified: ' 'the download is verified' 120
    # A private folder of a random name (mitcad#603), not the fixed
    # %TEMP%\mitcad-update of earlier releases.
    $verified = Ui-LastMatch '^.*Update verified: (.+?)\s*$'
    $folder = if ($verified) { Split-Path -Leaf (Split-Path -Parent $verified) } else { '' }
    if ($folder -match '^mitcad-update-.+$') { Pass "the download is in a private folder ($folder)" }
    else { Fail "the download is not in a private folder: '$verified'" }
    Ui-ExpectLog "Update staged: 0.0.0 -> $version" 'the update waits for Mitcad to quit'
    Ui-FocusDialog '^MitCad$' -Except $hidden
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
    Test-Registered $In $Install 'after the update'
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
    if ($script:UiProcess -and -not $script:UiProcess.HasExited) {
      Write-Host '     windows:'
      [UiInput]::Describe($script:UiProcess.Id) -split "`r?`n" | Where-Object { $_ } | ForEach-Object { Write-Host "       $_" }
    }
    Ui-LogLines | Select-Object -Last 40 | ForEach-Object { Write-Host "     $_" }
    # What the test's HTTPS server reported (errors of its requests).
    if ($script:ServerLog -and (Test-Path $script:ServerLog) -and (Get-Item $script:ServerLog).Length -gt 0) {
      Write-Host '     server:'
      Get-Content $script:ServerLog | Select-Object -Last 30 | ForEach-Object { Write-Host "       $_" }
    }
    Ui-StopApp
    $script:failures++
    throw 'the update test failed'
  }
}

# One scope: install, check, update, uninstall.
function Test-Scope([string]$In) {
  Write-Host "--- $($scopeName[$In])"
  $root = Join-Path $Out $In
  New-Item -ItemType Directory -Force $root | Out-Null
  $install = Join-Path $root 'Mitcad'
  $bin = Join-Path $install 'bin'

  # NSIS: /S is silent, /D sets the folder and must be the last argument,
  # without quotes. Without /AllUsers the installation is just for the
  # current user.
  $arguments = @('/S')
  if ($In -eq 'all') { $arguments += '/AllUsers' }
  $arguments += "/D=$install"
  $proc = Start-Process -FilePath $installer.FullName -ArgumentList $arguments -Wait -PassThru
  if ($proc.ExitCode -eq 0) { Pass "silent install $($scopeName[$In]) ($($arguments -join ' '))" }
  else { Fail "installer exit code $($proc.ExitCode)"; return }
  Test-Registered $In $install 'install'

  if ($In -eq 'user') {
    Test-Programs $install
    Test-UpdaterCheck $install (Join-Path $root 'updater-check')
  } else {
    # What an earlier release's updater runs: /S /D=<its folder>, no scope.
    $proc = Start-Process -FilePath $installer.FullName -ArgumentList '/S', "/D=$install" -Wait -PassThru
    if ($proc.ExitCode -eq 0) { Pass 'silent install with only /D= of the installation for all users' }
    else { Fail "installer (only /D=) exit code $($proc.ExitCode)" }
    Test-Registered $In $install 'reinstalled with only /D='
    if ($MesaDir) { Copy-Item (Join-Path $MesaDir 'opengl32.dll'), (Join-Path $MesaDir 'libgallium_wgl.dll') $bin }
  }

  if ($MesaDir -and -not $NoUpdate) {
    $work = Join-Path $root 'update'
    New-Item -ItemType Directory -Force $work | Out-Null
    try { Test-Update $In $install $work -Full:($In -eq 'user') }
    catch { if ($_.Exception.Message -ne 'the update test failed') { throw } }
    Get-Process -Name mitcad -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$install\*" } |
      Stop-Process -Force -ErrorAction SilentlyContinue
  }

  # 6. Uninstall. The test's own Mesa copy is removed first (the uninstaller
  # does not know it). NSIS copies the uninstaller elsewhere unless _?= says
  # where the installation is.
  if ($MesaDir) {
    Remove-Item (Join-Path $bin 'opengl32.dll'), (Join-Path $bin 'libgallium_wgl.dll') -ErrorAction SilentlyContinue
  }
  $proc = Start-Process -FilePath (Join-Path $install 'Uninstall.exe') -ArgumentList '/S', "_?=$install" -Wait -PassThru
  if ($proc.ExitCode -eq 0) { Pass 'silent uninstall' } else { Fail "uninstaller exit code $($proc.ExitCode)" }
  Test-Unregistered $In $install
}

$scopes = if ($Scope -eq 'both') { @('user', 'all') } else { @($Scope) }
foreach ($one in $scopes) { Test-Scope $one }

if ($failures -gt 0) { Write-Host "$failures check(s) failed"; exit 1 }
Write-Host 'Installer test passed'
