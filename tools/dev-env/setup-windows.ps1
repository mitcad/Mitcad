# SPDX-License-Identifier: MIT
# Installs the Mitcad Windows toolchain with pinned versions, ccache too
# (setup-ccache-windows.ps1), and writes CMakeUserPresets.json for a
# checkout. Meant for the isolated Windows build VM; run as an
# administrator. Visual Studio 2022 or newer with the C++ desktop tools
# must already be installed.
#
# Usage: powershell -ExecutionPolicy Bypass -File setup-windows.ps1 [-Repo C:\dev\src\mitcad]
param([string]$Repo = 'C:\dev\src\mitcad')

$ErrorActionPreference = 'Stop'
# The pins below are mirrored in versions.sh for the bash setup scripts.
$QtVersion = '6.12.0'
# aqtinstall 3.3.0 cannot install Qt 6.11+ on Windows; the fix (PR #1000)
# is unreleased, so a pinned commit is used on every platform.
$AqtSpec = 'aqtinstall @ git+https://github.com/miurahr/aqtinstall@076e1659807d0b362a3ed684d54c2e9c775eb9c7'
$CargoDenyVersion = '0.20.2'
# Must match "builtin-baseline" in vcpkg.json.
$VcpkgCommit = '3aea538b2bb21a586502c67b00eb474fdd2e3098'
$Root = 'C:\dev'

function Refresh-Path {
  $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' +
              [Environment]::GetEnvironmentVariable('Path', 'User')
}

function Invoke-Checked([scriptblock]$Command) {
  & $Command
  if ($LASTEXITCODE -ne 0) { throw "Command failed ($LASTEXITCODE): $Command" }
}

Write-Host '== winget packages'
foreach ($id in 'Git.Git', 'Kitware.CMake', 'Ninja-build.Ninja', 'Rustlang.Rustup', 'Python.Python.3.12', 'NSIS.NSIS') {
  winget list --id $id -e --accept-source-agreements *> $null
  if ($LASTEXITCODE -ne 0) {
    Invoke-Checked { winget install --id $id -e --source winget --silent --accept-package-agreements --accept-source-agreements }
  }
}
# The MQTT broker of the live updates' tests (mitcad#89), a test tool: the
# tests start their own (C:\Program Files\mosquitto) and skip without it.
# Its installer adds a Windows service, which is not needed.
winget list --id EclipseFoundation.Mosquitto -e --accept-source-agreements *> $null
if ($LASTEXITCODE -ne 0) {
  Invoke-Checked { winget install --id EclipseFoundation.Mosquitto -e --source winget --silent --accept-package-agreements --accept-source-agreements }
}
if (Get-Service mosquitto -ErrorAction SilentlyContinue) {
  Stop-Service mosquitto -ErrorAction SilentlyContinue
  Set-Service mosquitto -StartupType Disabled
}
# The compiler cache for the C++ compiles, shared by the checkouts.
& (Join-Path $PSScriptRoot 'setup-ccache-windows.ps1')
Refresh-Path

# The toolchain pinned in rust-toolchain.toml, with its components, so that
# rustup has nothing to download during builds and tests.
$toolchainFile = Get-Content (Join-Path $PSScriptRoot '..\..\rust-toolchain.toml')
$RustToolchain = ($toolchainFile | Select-String '^channel *= *"(.*)"').Matches[0].Groups[1].Value
$RustComponents = ($toolchainFile | Select-String '^components *= *\[(.*)\]').Matches[0].Groups[1].Value -replace '[" ]', ''
Write-Host "== Rust $RustToolchain ($RustComponents)"
# rustup writes progress to stderr, which Windows PowerShell would turn into
# an error under ErrorActionPreference = Stop; run it through cmd instead.
Invoke-Checked { cmd /c "rustup toolchain install $RustToolchain --profile minimal --component $RustComponents --no-self-update 2>&1" }
cmd /c 'rustup default >nul 2>&1'
if ($LASTEXITCODE -ne 0) { Invoke-Checked { cmd /c "rustup default $RustToolchain 2>&1" } }
Invoke-Checked { rustc "+$RustToolchain" --version }

Write-Host "== cargo-deny $CargoDenyVersion"
# Look the binary up directly: in Windows PowerShell a native command's stderr
# becomes a terminating error under ErrorActionPreference = Stop.
$denyExe = Get-Command cargo-deny -ErrorAction SilentlyContinue
$denyVersion = if ($denyExe) { & $denyExe.Source --version } else { '' }
if ($denyVersion -notmatch [regex]::Escape($CargoDenyVersion)) {
  Invoke-Checked { cmd /c "cargo +$RustToolchain install cargo-deny --version $CargoDenyVersion --locked 2>&1" }
}

Write-Host "== vcpkg at $VcpkgCommit"
$vcpkg = Join-Path $Root 'vcpkg'
if (-not (Test-Path (Join-Path $vcpkg '.git'))) {
  Invoke-Checked { git clone --quiet https://github.com/microsoft/vcpkg $vcpkg }
}
Invoke-Checked { git -C $vcpkg fetch --quiet origin }
Invoke-Checked { git -C $vcpkg checkout --quiet $VcpkgCommit }
Invoke-Checked { & (Join-Path $vcpkg 'bootstrap-vcpkg.bat') -disableMetrics }

Write-Host "== Qt $QtVersion"
$qtDir = Join-Path $Root "Qt\$QtVersion\msvc2022_64"
if (-not (Test-Path $qtDir)) {
  $aqt = Join-Path $Root 'tools\aqt'
  # The py launcher avoids the Microsoft Store alias for python.exe.
  Invoke-Checked { py -3.12 -m venv $aqt }
  Invoke-Checked { & (Join-Path $aqt 'Scripts\pip.exe') install --quiet $AqtSpec }
  Push-Location $env:TEMP
  try {
    Invoke-Checked { & (Join-Path $aqt 'Scripts\aqt.exe') install-qt windows desktop $QtVersion win64_msvc2022_64 -O (Join-Path $Root 'Qt') }
  } finally { Pop-Location }
}

Write-Host '== CMakeUserPresets.json'
New-Item -ItemType Directory -Force $Repo | Out-Null
$presets = Join-Path $Repo 'CMakeUserPresets.json'
if (-not (Test-Path $presets)) {
  $qtPath = $qtDir -replace '\\', '/'
  $vcpkgPath = $vcpkg -replace '\\', '/'
  @"
{
  "version": 6,
  "configurePresets": [
    {
      "name": "dev",
      "displayName": "Windows (local toolchain)",
      "inherits": "windows-release",
      "environment": {
        "VCPKG_ROOT": "$vcpkgPath",
        "QT_ROOT_DIR": "$qtPath"
      }
    }
  ],
  "buildPresets": [{ "name": "dev", "configurePreset": "dev" }],
  "testPresets": [
    { "name": "dev", "configurePreset": "dev", "output": { "outputOnFailure": true } }
  ]
}
"@ | Set-Content -Encoding ascii $presets
}

Write-Host 'Done. Build from a shell with the MSVC environment, e.g.:'
Write-Host '  tools\dev-env\msvc.cmd cmake --preset dev'
Write-Host 'To run the app without a GPU, build Mesa with setup-mesa-windows.ps1.'
