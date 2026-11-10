# SPDX-License-Identifier: MIT
# Fetches the Chromium Embedded Framework's binary distribution pinned in
# versions.sh (CEF_VERSION, the "minimal" distribution, windows64), checks
# its SHA-1 (CEF_WINDOWS64_SHA1, else the digest cef-builds publishes beside
# the archive), and unpacks it into C:\dev\cef\<CEF_VERSION>\; prints the
# folder, which the build takes as MITCAD_CEF_ROOT (Insert > Component from
# Web's web browser component, mitcad#1579, docs/development.md "The web
# browser component"). Reads the pins from versions.sh itself. Running it
# again with the folder there does nothing.
# Usage: powershell -ExecutionPolicy Bypass -File fetch-cef.ps1 [-PrintRoot]
param([switch]$PrintRoot, [string]$Root = 'C:\dev\cef')

$ErrorActionPreference = 'Stop'
$versions = Get-Content (Join-Path $PSScriptRoot 'versions.sh')
function Pin([string]$Name) {
  $line = $versions | Where-Object { $_ -match "^$Name=" } | Select-Object -First 1
  if ($null -eq $line) { return '' }
  return ($line -replace "^$Name=", '').Trim()
}
$version = Pin 'CEF_VERSION'
$pin = Pin 'CEF_WINDOWS64_SHA1'
$name = "cef_binary_${version}_windows64_minimal"
$folder = Join-Path (Join-Path $Root $version) $name
if ($PrintRoot -or (Test-Path (Join-Path $folder 'cmake\FindCEF.cmake'))) {
  Write-Output $folder
  exit 0
}

$encoded = $name -replace '\+', '%2B'
$url = "https://cef-builds.spotifycdn.com/$encoded.tar.bz2"
$work = Join-Path (Join-Path $Root $version) 'download'
New-Item -ItemType Directory -Force $work | Out-Null
$archive = Join-Path $work "$name.tar.bz2"
Write-Host "== CEF $version (windows64) from $url"
Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $archive
$actual = (Get-FileHash -Algorithm SHA1 $archive).Hash.ToLowerInvariant()
if ($pin) {
  $expected = $pin.ToLowerInvariant()
} else {
  $expected = ((Invoke-WebRequest -UseBasicParsing -Uri "$url.sha1").Content | Out-String).Trim().ToLowerInvariant()
  Write-Host "fetch-cef.ps1: CEF_WINDOWS64_SHA1 is not pinned in versions.sh; the published digest is $expected"
}
if ($actual -ne $expected) {
  Remove-Item -Force $archive
  throw "fetch-cef.ps1: $archive has SHA-1 $actual, not $expected"
}
$partial = "$folder.partial"
if (Test-Path $partial) { Remove-Item -Recurse -Force $partial }
New-Item -ItemType Directory -Force $partial | Out-Null
# Windows' own tar (bsdtar) reads bzip2.
& "$env:SystemRoot\System32\tar.exe" -xjf $archive -C $partial --strip-components=1
if ($LASTEXITCODE -ne 0) { throw "fetch-cef.ps1: tar failed ($LASTEXITCODE)" }
Move-Item $partial $folder
Remove-Item -Force $archive
Write-Output $folder
