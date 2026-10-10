# SPDX-License-Identifier: MIT
# The Windows build of tools/dev-env/build-cycles.sh: fetches and builds the
# Cycles render worker's libraries that vcpkg has no port for (MITCAD_RENDER,
# docs/rendering.md), Cycles itself (Apache-2.0) and Open Image Denoise
# (Apache-2.0, built with Intel's ISPC compiler, BSD-3-Clause, which is only
# a build tool). Everything else comes from vcpkg through the manifest's
# "render" feature. Cycles has its CPU device only so far (mitcad#51).
#
# Usage, in the Visual Studio x64 environment (cl, Ninja):
#   tools\dev-env\msvc.cmd powershell -ExecutionPolicy Bypass -File tools\dev-env\build-cycles.ps1 [-Prefix dir]
#
# The prefix (default: MITCAD_RENDER_DEPS, else C:\dev\mitcad-render-deps)
# gets the same layout as on Linux:
#   downloads\  src\  build\       the pinned sources and their builds
#   vcpkg_installed\               the render feature's vcpkg libraries
#   install\oidn\                  Open Image Denoise (DLLs in bin\)
#   install\cycles\                Cycles' headers, static libraries and
#                                  mitcad-cycles.cmake for Mitcad's build
# Configure Mitcad with -DMITCAD_RENDER=ON -DMITCAD_RENDER_DEPS=<prefix>, in
# a release configuration (RelWithDebInfo or Release): the libraries are
# release builds, which a Debug build's runtime library does not match.
#
# Pins: tools/dev-env/versions.sh, read from there. Re-running it is cheap:
# finished steps are skipped (delete the prefix's build\ and install\
# folders, by their literal paths, to build again).
#
# Environment: MITCAD_RENDER_JOBS (parallel compile jobs, default 12),
# VCPKG_ROOT (default C:\dev\vcpkg).
param([string]$Prefix = '')

$ErrorActionPreference = 'Stop'
$Repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path

# The pins: the KEY=value lines of versions.sh.
$Pins = @{}
foreach ($line in Get-Content (Join-Path $PSScriptRoot 'versions.sh')) {
  if ($line -match '^([A-Z0-9_]+)="?([^"]*)"?$') { $Pins[$Matches[1]] = $Matches[2] }
}

if (-not $Prefix) { $Prefix = if ($env:MITCAD_RENDER_DEPS) { $env:MITCAD_RENDER_DEPS } else { 'C:\dev\mitcad-render-deps' } }
$Jobs = if ($env:MITCAD_RENDER_JOBS) { $env:MITCAD_RENDER_JOBS } else { '12' }
$VcpkgRoot = if ($env:VCPKG_ROOT) { $env:VCPKG_ROOT } else { 'C:\dev\vcpkg' }
$Triplet = 'x64-windows'
New-Item -ItemType Directory -Force $Prefix | Out-Null
$Prefix = (Resolve-Path $Prefix).Path
$Downloads = Join-Path $Prefix 'downloads'
$Src = Join-Path $Prefix 'src'
$Build = Join-Path $Prefix 'build'
$Install = Join-Path $Prefix 'install'
$VcpkgInstalled = Join-Path $Prefix 'vcpkg_installed'
foreach ($dir in $Downloads, $Src, $Build, $Install) { New-Item -ItemType Directory -Force $dir | Out-Null }

if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue) -or -not (Get-Command ninja.exe -ErrorAction SilentlyContinue)) {
  throw 'Run this in the Visual Studio x64 environment with Ninja (tools\dev-env\msvc.cmd).'
}

# A native command; a failure stops the script. Its output goes to $Log
# when given (and its end is shown on a failure).
function Invoke-Native([string]$Program, [string[]]$Arguments, [string]$Log = '') {
  # Windows PowerShell turns a native command's stderr into errors, which
  # would stop the script at the first warning: only the exit code counts.
  $ErrorActionPreference = 'Continue'
  if ($Log) {
    & $Program @Arguments *> $Log
  } else {
    & $Program @Arguments
  }
  if ($LASTEXITCODE -ne 0) {
    if ($Log) { Get-Content $Log -Tail 40 | Write-Host }
    throw "$Program failed ($LASTEXITCODE)"
  }
}

# A download checked against its SHA-256 pin.
function Get-Pinned([string]$Url, [string]$File, [string]$Sha256) {
  $path = Join-Path $Downloads $File
  if (-not (Test-Path $path)) {
    Write-Host "   downloading $Url"
    $ProgressPreference = 'SilentlyContinue'
    Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile "$path.part"
    Move-Item "$path.part" $path
  }
  $actual = (Get-FileHash -Algorithm SHA256 $path).Hash.ToLowerInvariant()
  if ($actual -ne $Sha256) { throw "$File does not match its SHA-256 pin $Sha256 ($actual)" }
  return $path
}

Write-Host "== vcpkg: the manifest's render feature ($Triplet)"
# The same manifest and baseline as Mitcad's own build, so the libraries
# Cycles is compiled against are the ones Mitcad links.
Invoke-Native (Join-Path $VcpkgRoot 'vcpkg.exe') @('install', "--x-manifest-root=$Repo", '--x-feature=render',
  "--x-install-root=$VcpkgInstalled", '--triplet', $Triplet, '--no-print-usage') (Join-Path $Prefix 'vcpkg.log')
$Deps = Join-Path $VcpkgInstalled $Triplet
$DepsCMake = $Deps -replace '\\', '/'

Write-Host "== ISPC $($Pins.ISPC_VERSION) (build tool)"
$ispcName = "ispc-v$($Pins.ISPC_VERSION)-windows"
$ispcZip = Get-Pinned "https://github.com/ispc/ispc/releases/download/v$($Pins.ISPC_VERSION)/$ispcName.zip" `
  "$ispcName.zip" $Pins.ISPC_WINDOWS_SHA256
$ispc = Join-Path $Src "$ispcName\bin\ispc.exe"
if (-not (Test-Path $ispc)) { Expand-Archive -Path $ispcZip -DestinationPath $Src -Force }
if (-not (Test-Path $ispc)) { throw "No $ispc in ISPC's archive" }

Write-Host "== Open Image Denoise $($Pins.OIDN_VERSION)"
$oidnArchive = Get-Pinned "https://github.com/RenderKit/oidn/releases/download/v$($Pins.OIDN_VERSION)/oidn-$($Pins.OIDN_VERSION).src.tar.gz" `
  "oidn-$($Pins.OIDN_VERSION).src.tar.gz" $Pins.OIDN_SRC_SHA256
$oidnSrc = Join-Path $Src "oidn-$($Pins.OIDN_VERSION)"
if (-not (Test-Path (Join-Path $oidnSrc 'CMakeLists.txt'))) {
  # Windows' own tar (bsdtar).
  Invoke-Native "$env:SystemRoot\System32\tar.exe" @('-xzf', $oidnArchive, '-C', $Src)
}
$oidnInstall = Join-Path $Install 'oidn'
$oidnDone = Join-Path $oidnInstall ".done-$($Pins.OIDN_VERSION)"
if (-not (Test-Path $oidnDone)) {
  # The CPU device, the ray tracing filter's weights only (the lightmap
  # filter is for baking); no example or test programs.
  Remove-Item -Force (Join-Path $oidnInstall '.done-*') -ErrorAction SilentlyContinue
  $oidnBuild = Join-Path $Build 'oidn'
  Invoke-Native cmake.exe @('-S', $oidnSrc, '-B', $oidnBuild, '-G', 'Ninja', '-DCMAKE_BUILD_TYPE=Release',
    "-DCMAKE_INSTALL_PREFIX=$oidnInstall", "-DCMAKE_PREFIX_PATH=$DepsCMake", "-DISPC_EXECUTABLE=$ispc",
    '-DOIDN_DEVICE_CPU=ON', '-DOIDN_DEVICE_CUDA=OFF', '-DOIDN_DEVICE_HIP=OFF', '-DOIDN_DEVICE_SYCL=OFF',
    '-DOIDN_DEVICE_METAL=OFF', '-DOIDN_FILTER_RT=ON', '-DOIDN_FILTER_RTLIGHTMAP=OFF', '-DOIDN_APPS=OFF',
    '-DOIDN_INSTALL_DEPENDENCIES=OFF') (Join-Path $Prefix 'oidn-configure.log')
  Invoke-Native cmake.exe @('--build', $oidnBuild, '--parallel', $Jobs)
  Invoke-Native cmake.exe @('--install', $oidnBuild) (Join-Path $Prefix 'oidn-install.log')
  New-Item -ItemType File -Force $oidnDone | Out-Null
}

Write-Host "== Cycles $($Pins.CYCLES_VERSION)"
$cyclesSrc = Join-Path $Src "cycles-$($Pins.CYCLES_VERSION)"
if (-not (Test-Path (Join-Path $cyclesSrc '.git'))) {
  Invoke-Native git.exe @('clone', '--quiet', '--depth', '1', '--branch', "v$($Pins.CYCLES_VERSION)",
    'https://projects.blender.org/blender/cycles.git', $cyclesSrc)
}
$head = (& git.exe -C $cyclesSrc rev-parse HEAD).Trim()
if ($head -ne $Pins.CYCLES_COMMIT) { throw "Cycles v$($Pins.CYCLES_VERSION) is not at the pinned commit $($Pins.CYCLES_COMMIT)" }
$cyclesInstall = Join-Path $Install 'cycles'
$cyclesBuild = Join-Path $Build 'cycles'
$cyclesDone = Join-Path $cyclesInstall ".done-$($Pins.CYCLES_VERSION)"
if (-not (Test-Path $cyclesDone)) {
  # The CPU device with Embree and Open Image Denoise; no OSL, USD,
  # Alembic, OpenVDB or OpenSubdiv, no GPU devices, no precompiled
  # libraries (vcpkg's instead). The standalone program is built too: it
  # renders Cycles' XML scenes, a check of the build without Mitcad.
  $oidnCMake = $oidnInstall -replace '\\', '/'
  Invoke-Native cmake.exe @('-S', $cyclesSrc, '-B', $cyclesBuild, '-G', 'Ninja', '-DCMAKE_BUILD_TYPE=Release',
    "-DCMAKE_PREFIX_PATH=$DepsCMake;$oidnCMake", "-DCMAKE_INSTALL_PREFIX=$cyclesBuild/install",
    '-DCMAKE_EXPORT_COMPILE_COMMANDS=ON', '-DWITH_LIBS_PRECOMPILED=OFF',
    '-DWITH_CYCLES_EMBREE=ON', '-DWITH_CYCLES_OPENIMAGEDENOISE=ON',
    '-DWITH_CYCLES_ALEMBIC=OFF', '-DWITH_CYCLES_OPENSUBDIV=OFF', '-DWITH_CYCLES_OPENVDB=OFF',
    '-DWITH_CYCLES_NANOVDB=OFF', '-DWITH_CYCLES_OSL=OFF', '-DWITH_CYCLES_USD=OFF',
    '-DWITH_CYCLES_HYDRA_RENDER_DELEGATE=OFF', '-DWITH_CYCLES_LOGGING=OFF',
    '-DWITH_CYCLES_PATH_GUIDING=OFF', '-DWITH_CYCLES_STANDALONE_GUI=OFF',
    '-DWITH_CYCLES_DEVICE_CUDA=OFF', '-DWITH_CYCLES_DEVICE_OPTIX=OFF', '-DWITH_CYCLES_DEVICE_HIP=OFF',
    '-DWITH_CYCLES_DEVICE_HIPRT=OFF', '-DWITH_CYCLES_DEVICE_METAL=OFF', '-DWITH_CYCLES_DEVICE_ONEAPI=OFF',
    "-DEMBREE_ROOT_DIR=$DepsCMake", "-DTBB_ROOT_DIR=$DepsCMake", "-DOPENIMAGEDENOISE_ROOT_DIR=$oidnCMake",
    # Cycles looks for tbb.lib; oneTBB's Windows library carries its
    # interface version.
    "-DTBB_LIBRARY=$DepsCMake/lib/tbb12.lib",
    "-DZSTD_ROOT_DIR=$DepsCMake", "-DPUGIXML_ROOT_DIR=$DepsCMake") (Join-Path $Prefix 'cycles-configure.log')
  $started = Get-Date
  Invoke-Native cmake.exe @('--build', $cyclesBuild, '--parallel', $Jobs)
  Write-Host ("   built in {0:0} s" -f ((Get-Date) - $started).TotalSeconds)

  # Install for Mitcad: the headers in Cycles' own layout (src\ is the
  # include root), the static libraries, and the compile definitions the
  # libraries were built with, which the headers depend on.
  if (Test-Path $cyclesInstall) { Remove-Item -Recurse -Force $cyclesInstall }
  foreach ($part in @(@('src', ''), @('third_party', 'third_party'))) {
    $from = Join-Path $cyclesSrc $part[0]
    $to = Join-Path (Join-Path $cyclesInstall 'include') $part[1]
    Get-ChildItem -Path $from -Recurse -File -Filter '*.h' | ForEach-Object {
      $target = Join-Path $to $_.FullName.Substring($from.Length + 1)
      New-Item -ItemType Directory -Force (Split-Path $target) | Out-Null
      Copy-Item $_.FullName $target
    }
  }
  $lib = Join-Path $cyclesInstall 'lib'
  New-Item -ItemType Directory -Force $lib | Out-Null
  Get-ChildItem -Path $cyclesBuild -Recurse -File -Include 'cycles_*.lib', 'extern_*.lib' |
    ForEach-Object { Copy-Item $_.FullName $lib }
  Copy-Item (Join-Path $cyclesSrc 'LICENSE') (Join-Path $cyclesInstall 'LICENSE')
  Set-Content -Encoding ascii (Join-Path $cyclesInstall 'devices.txt') 'cpu'
  New-Item -ItemType File -Force $cyclesDone | Out-Null
}
# The licence texts of the code Cycles bundles (its SPDX lines name them),
# for the installer's notices (cmake/Packaging.cmake).
$licenses = Join-Path $cyclesInstall 'licenses'
New-Item -ItemType Directory -Force $licenses | Out-Null
Copy-Item (Join-Path $cyclesSrc 'src\doc\license\*license*.txt') $licenses

# What Mitcad's build needs to know of the Cycles build (cmake/Render.cmake):
# the definitions and the instruction set and floating point options that
# Cycles' host code was compiled with (its headers' inline functions use
# them), and its libraries.
$commands = Get-Content -Raw (Join-Path $cyclesBuild 'compile_commands.json') | ConvertFrom-Json
$entry = $commands | Where-Object { ($_.file -replace '\\', '/') -like '*src/session/session.cpp' } | Select-Object -First 1
if (-not $entry) { throw 'session.cpp is not in Cycles'' compile_commands.json' }
# The command's arguments: quoted parts kept together, quotes removed.
$arguments = [regex]::Matches($entry.command, '(?:[^\s"]+|"(?:\\.|[^"\\])*")+') | ForEach-Object {
  ($_.Value -replace '(?<!\\)"', '') -replace '\\"', '"'
}
$definitions = $arguments | Where-Object { $_ -match '^[-/]D.' } | ForEach-Object { $_.Substring(2) } | Sort-Object -Unique
$options = $arguments | Where-Object { $_ -match '^[-/](arch|fp):' }
# CMake's quoted argument: backslashes and quotes escaped.
function Quote([string]$Text) { '"' + ($Text -replace '\\', '\\' -replace '"', '\"') + '"' }
$out = New-Object System.Collections.Generic.List[string]
$out.Add('# SPDX-License-Identifier: MIT')
$out.Add('# Written by tools/dev-env/build-cycles.ps1: the Cycles build, for cmake/Render.cmake.')
$out.Add("set(MITCAD_CYCLES_ROOT $(Quote ($cyclesInstall -replace '\\', '/')))")
$out.Add("set(MITCAD_OIDN_ROOT $(Quote ($oidnInstall -replace '\\', '/')))")
$out.Add('set(MITCAD_CYCLES_DEFINITIONS')
foreach ($d in $definitions) { $out.Add("  $(Quote $d)") }
$out.Add(')')
$out.Add('set(MITCAD_CYCLES_OPTIONS')
foreach ($o in $options) { $out.Add("  $(Quote $o)") }
$out.Add(')')
$out.Add('set(MITCAD_CYCLES_DEVICES CPU)')
$out.Add('set(MITCAD_CYCLES_KERNELS)')
$out.Add('set(MITCAD_CYCLES_LIBRARIES')
foreach ($l in Get-ChildItem (Join-Path $cyclesInstall 'lib') -Filter '*.lib' | Sort-Object Name) {
  $out.Add("  `"`${MITCAD_CYCLES_ROOT}/lib/$($l.Name)`"")
}
$out.Add(')')
Set-Content -Encoding ascii (Join-Path $cyclesInstall 'mitcad-cycles.cmake') $out

Write-Host "== Check: Cycles' standalone program renders a test scene"
$check = Join-Path $Build 'check'
New-Item -ItemType Directory -Force $check | Out-Null
$env:Path = (Join-Path $Deps 'bin') + ';' + (Join-Path $oidnInstall 'bin') + ';' + $env:Path
Invoke-Native (Join-Path $cyclesBuild 'bin\cycles.exe') @('--background', '--quiet', '--samples', '4',
  '--width', '64', '--height', '48', '--output', (Join-Path $check 'scene.png'),
  (Join-Path $cyclesSrc 'examples\scene_cube_surface.xml'))
if ((Get-Item (Join-Path $check 'scene.png')).Length -eq 0) { throw 'The test scene rendered an empty image' }
Write-Host "Done: configure Mitcad with -DMITCAD_RENDER=ON -DMITCAD_RENDER_DEPS=$Prefix"
