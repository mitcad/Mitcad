# SPDX-License-Identifier: MIT
# Builds Mesa's software OpenGL driver (llvmpipe) from source for running
# Mitcad in a Windows VM without a GPU, where Windows offers only OpenGL 1.1.
# Development and testing only: the DLLs are never part of a release.
#
# Mesa and its dependencies (LLVM, zlib, zstd, DirectX headers) are built
# with the vcpkg ports of the pinned baseline, which check every source
# archive against a SHA512. The vcpkg manifest and triplet are in mesa\; the
# manifest turns off LLVM's default features (Clang, LLD, all targets), which
# only the root manifest can do, so LLVM is built for x86 only. Mesa's build
# scripts also need the Python packages mako, PyYAML and setuptools, which
# vcpkg installs from PyPI into its own Python (without pinned versions).
# Needs Visual Studio with the C++ tools and the vcpkg checkout of
# setup-windows.ps1. The first run builds LLVM: about 20 minutes on the
# 16-core VM and 15 GB of temporary disk space; later runs take the packages
# from vcpkg's binary cache.
#
# Result: <Out>\bin with opengl32.dll and libgallium_wgl.dll (LLVM linked in
# statically), the licences, the port versions and SHA256 checksums.
# Use it with: cmake --preset dev -DMITCAD_MESA_DIR=C:/dev/mesa/bin
#
# Usage: powershell -ExecutionPolicy Bypass -File setup-mesa-windows.ps1 [-Out C:\dev\mesa] [-Jobs 4]
param(
  [string]$Out = 'C:\dev\mesa',
  [string]$Vcpkg = 'C:\dev\vcpkg',
  # Parallel compile jobs; 0 = one per GB of memory, at most one per core.
  [int]$Jobs = 0)

$ErrorActionPreference = 'Stop'
# Must match "builtin-baseline" in mesa\vcpkg.json and in vcpkg.json.
$VcpkgCommit = '3aea538b2bb21a586502c67b00eb474fdd2e3098'
$Triplet = 'x64-windows-mesa'
$Manifest = Join-Path $PSScriptRoot 'mesa'

function Invoke-Checked([scriptblock]$Command) {
  & $Command
  if ($LASTEXITCODE -ne 0) { throw "Command failed ($LASTEXITCODE): $Command" }
}

# sshd sessions keep the PATH of the service's start-up.
$env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' +
            [Environment]::GetEnvironmentVariable('Path', 'User')

$vcpkgExe = Join-Path $Vcpkg 'vcpkg.exe'
if (-not (Test-Path $vcpkgExe)) { throw "vcpkg not found at $Vcpkg; run setup-windows.ps1 first" }
$head = (& git -C $Vcpkg rev-parse HEAD).Trim()
if ($head -ne $VcpkgCommit) { throw "vcpkg at $Vcpkg is at $head, expected $VcpkgCommit; run setup-windows.ps1" }

# The LLVM port needs Visual Studio's ATL headers (vcpkg port "atl").
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$vsPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vsPath) { throw 'Visual Studio with the C++ x64 tools was not found' }
$atlPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.ATL -property installationPath
if (-not $atlPath) {
  Write-Host '== Visual Studio component C++ ATL (needs an administrator)'
  $installer = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\setup.exe'
  $process = Start-Process -Wait -PassThru -FilePath $installer -ArgumentList @(
    'modify', '--installPath', "`"$vsPath`"", '--add', 'Microsoft.VisualStudio.Component.VC.ATL',
    '--quiet', '--norestart')
  if ($process.ExitCode -notin 0, 3010) { throw "Visual Studio Installer failed ($($process.ExitCode))" }
}

if ($Jobs -le 0) {
  $system = Get-CimInstance Win32_ComputerSystem
  $memoryGb = [math]::Floor($system.TotalPhysicalMemory / 1GB)
  $Jobs = [math]::Max(2, [math]::Min($system.NumberOfLogicalProcessors, $memoryGb))
}
# Linking the LLVM tools needs about 1 GB per job.
$env:VCPKG_MAX_CONCURRENCY = "$Jobs"
Write-Host "== Mesa with vcpkg ($Triplet, $Jobs jobs)"
$installRoot = Join-Path $Out 'installed'
# Own build and package trees: vcpkg locks its build tree while it builds,
# and LLVM would block Mitcad's own configure runs for hours.
Invoke-Checked { & $vcpkgExe install "--x-manifest-root=$Manifest" "--x-install-root=$installRoot" `
  "--x-buildtrees-root=$(Join-Path $Out 'buildtrees')" "--x-packages-root=$(Join-Path $Out 'packages')" `
  "--overlay-triplets=$Manifest" "--triplet=$Triplet" "--host-triplet=$Triplet" `
  --clean-after-build --disable-metrics }

Write-Host "== $Out\bin"
$installed = Join-Path $installRoot $Triplet
$bin = Join-Path $Out 'bin'
if (Test-Path $bin) { Remove-Item -Recurse -Force $bin }
New-Item -ItemType Directory -Force (Join-Path $bin 'licenses') | Out-Null
foreach ($dll in 'opengl32.dll', 'libgallium_wgl.dll') {
  if (-not (Test-Path (Join-Path $installed "bin\$dll"))) { throw "The Mesa build did not produce $dll" }
}

# The LLVM port also installs DLLs of its tools (LLVM-C, LTO, Remarks); Mesa
# does not use them.
foreach ($dll in 'opengl32.dll', 'libgallium_wgl.dll') { Copy-Item (Join-Path $installed "bin\$dll") $bin }
foreach ($port in 'mesa', 'llvm', 'zlib', 'zstd', 'directx-headers') {
  Copy-Item (Join-Path $installed "share\$port\copyright") (Join-Path $bin "licenses\$port.txt")
}
& $vcpkgExe list "--x-manifest-root=$Manifest" "--x-install-root=$installRoot" |
  Set-Content -Encoding ascii (Join-Path $bin 'VERSIONS.txt')
Get-ChildItem (Join-Path $bin '*.dll') | Get-FileHash -Algorithm SHA256 |
  ForEach-Object { '{0}  {1}' -f $_.Hash.ToLower(), (Split-Path -Leaf $_.Path) } |
  Set-Content -Encoding ascii (Join-Path $bin 'SHA256SUMS.txt')
Get-Content (Join-Path $bin 'VERSIONS.txt'), (Join-Path $bin 'SHA256SUMS.txt')

Write-Host 'Done. Configure Mitcad with it, e.g.:'
Write-Host "  tools\dev-env\msvc.cmd cmake --preset dev -DMITCAD_MESA_DIR=$($bin -replace '\\', '/')"
