# SPDX-License-Identifier: MIT
# The web browser component's package for a release on Windows (mitcad#1579,
# docs/updates.md "The web browser component"); the same steps as
# tools/browser-package.sh: the fingerprint of what it is built from, the
# latest release's package kept when it is unchanged, else the package built
# (MITCAD_BROWSER_COMPONENT on, a version of the build's time) and zipped.
# Run in the MSVC environment (tools\dev-env\msvc.cmd).
#
#   powershell -ExecutionPolicy Bypass -File tools\browser-package.ps1 -Build <build dir>
#              [-Latest <latest update-manifest.json>] [-Key <release public key>]
#
# Writes <build dir>\browser-package.txt: `windows-x64 ABI VERSION CEF
# FINGERPRINT FILE` for a new package (the release adds `--browser <line>
# <url>` to mitcad-release manifest), or `unchanged`.
param([Parameter(Mandatory = $true)][string]$Build, [string]$Latest = '', [string]$Key = '')

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$versions = Get-Content (Join-Path $repo 'tools\dev-env\versions.sh')
$cef = (($versions | Where-Object { $_ -match '^CEF_VERSION=' } | Select-Object -First 1) -replace '^CEF_VERSION=', '').Trim()
$header = Get-Content (Join-Path $repo 'app\webbrowser\ComponentBrowserHost.hpp') -Raw
if ($header -notmatch 'constexpr int kComponentBrowserAbi = (\d+);') { throw 'no kComponentBrowserAbi in ComponentBrowserHost.hpp' }
$abi = $Matches[1]
$platform = 'windows-x64'

function Sha256Text([string]$Text) {
  $bytes = [Text.Encoding]::UTF8.GetBytes($Text)
  $hash = [Security.Cryptography.SHA256]::Create().ComputeHash($bytes)
  return -join ($hash | ForEach-Object { $_.ToString('x2') })
}
# The same text browser-package.sh hashes: paths relative to the
# repository with '/', sorted by their bytes, each file's own SHA-256.
$lines = New-Object System.Collections.Generic.List[string]
$lines.Add("cef $cef")
$lines.Add("abi $abi")
$files = @()
foreach ($part in 'app\webbrowser', 'packaging\browser') {
  $files += Get-ChildItem -Recurse -File (Join-Path $repo $part)
}
$files += Get-Item (Join-Path $repo 'app\files\FileFormats.cpp'), (Join-Path $repo 'app\files\FileFormats.hpp')
$relative = $files | ForEach-Object { $_.FullName.Substring($repo.Length + 1).Replace('\', '/') }
$sorted = [string[]]$relative
[Array]::Sort($sorted, [StringComparer]::Ordinal)
foreach ($path in $sorted) {
  $hash = (Get-FileHash -Algorithm SHA256 (Join-Path $repo $path)).Hash.ToLowerInvariant()
  $lines.Add("file $path $hash")
}
$fingerprint = Sha256Text (($lines -join "`n") + "`n")
Write-Host "browser-package.ps1: $platform, interface $abi, CEF $cef, fingerprint $fingerprint"

$result = Join-Path $Build 'browser-package.txt'
if ($Latest -and (Test-Path $Latest)) {
  # The build's own tool at the build folder's top (not a recursive search:
  # it would follow build\dev\cargo's junction and could pick cargo's test
  # builds' copy).
  $tool = Join-Path $Build 'mitcad-release.exe'
  if (-not (Test-Path $tool)) { throw "no mitcad-release.exe in $Build" }
  $listed = & $tool browser --manifest $Latest --key $Key --platform $platform --abi $abi |
    Where-Object { $_ -match '^fingerprint ' } | ForEach-Object { $_ -replace '^fingerprint ', '' }
  if ($LASTEXITCODE -ne 0) { throw 'mitcad-release browser failed' }
  if ($listed -eq $fingerprint) {
    Write-Host "browser-package.ps1: unchanged since the latest release's package: kept"
    Set-Content -Encoding ascii $result 'unchanged'
    exit 0
  }
}

$now = (Get-Date).ToUniversalTime()
$version = '{0}.{1}.{2}' -f $now.Year, ($now.Month * 100 + $now.Day), ($now.Hour * 100 + $now.Minute)
Write-Host "browser-package.ps1: building the package $version"
& cmake -S $repo -B $Build -DMITCAD_BROWSER_COMPONENT=ON "-DMITCAD_BROWSER_VERSION=$version" | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'configure failed' }
& cmake --build $Build --target browser-package
if ($LASTEXITCODE -ne 0) { throw 'the package did not build' }
$file = Join-Path $Build "mitcad-browser-$version-$platform.zip"
if (-not (Test-Path $file)) { throw "no $file" }
Set-Content -Encoding ascii $result "$platform $abi $version $cef $fingerprint $file"
Write-Host "browser-package.ps1: $file ($((Get-Item $file).Length) bytes)"
