# SPDX-License-Identifier: MIT
# Installs ccache for Windows builds: CMakeLists.txt then compiles C++
# through it (MITCAD_COMPILER_CACHE), with MSVC's debug information in the
# object files (/Z7). A build tool only; nothing of it is linked or shipped.
#
# ccache comes from its GitHub release, pinned to a version and checked
# against the SHA-256 digest GitHub publishes for the release asset, and is
# unpacked into <Dir> (default C:\dev\ccache), which goes on the machine's
# PATH. Its cache is the user's (%LOCALAPPDATA%\ccache), shared by every
# checkout folder: CMake hashes the paths inside a checkout relative to it,
# so a new checkout builds its C++ mostly from the cache. The cache's size
# limit is set in the user's ccache configuration; `ccache -s` shows the
# hits, `ccache -C` empties it.
#
# Usage: powershell -ExecutionPolicy Bypass -File setup-ccache-windows.ps1 [-Dir C:\dev\ccache] [-MaxSize 2G]
# Run as an administrator (the machine's PATH), as the user who builds.
param(
  [string]$Dir = 'C:\dev\ccache',
  [string]$MaxSize = '2G')

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$Version = '4.14.1'
$Sha256 = '6219f3865ca59aec41ee4b678df171d5d35855ecb2b6dbbbd20690b3a68af7b4'
$Name = "ccache-$Version-windows-x86_64"
$Url = "https://github.com/ccache/ccache/releases/download/v$Version/$Name.zip"

$exe = Join-Path $Dir 'ccache.exe'
$installed = if (Test-Path $exe) { & $exe --version | Select-Object -First 1 } else { '' }
if ($installed -ne "ccache version $Version") {
  Write-Host "== ccache $Version from $Url"
  $zip = Join-Path $env:TEMP "$Name.zip"
  $unpacked = Join-Path $env:TEMP $Name
  Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $zip
  $actual = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLowerInvariant()
  if ($actual -ne $Sha256) {
    Remove-Item -Force $zip
    throw "ccache: the download's SHA-256 is $actual, the release publishes $Sha256"
  }
  Write-Host "SHA-256 $actual matches the release"
  if (Test-Path $unpacked) { Remove-Item -Recurse -Force $unpacked }
  Expand-Archive -Path $zip -DestinationPath $env:TEMP
  if (-not (Test-Path (Join-Path $unpacked 'ccache.exe'))) { throw "ccache: no $Name\ccache.exe in the archive" }
  if (Test-Path $Dir) { Remove-Item -Recurse -Force $Dir }
  Move-Item $unpacked $Dir
  Remove-Item -Force $zip
}

$machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
if (($machinePath -split ';') -notcontains $Dir) {
  [Environment]::SetEnvironmentVariable('Path', $machinePath.TrimEnd(';') + ";$Dir", 'Machine')
  Write-Host "$Dir added to the machine's PATH"
}

& $exe --set-config "max_size=$MaxSize"
if ($LASTEXITCODE -ne 0) { throw 'ccache --set-config failed' }
& $exe --version | Select-Object -First 1
& $exe --show-config | Select-String -Pattern '\) (cache_dir|max_size) '
