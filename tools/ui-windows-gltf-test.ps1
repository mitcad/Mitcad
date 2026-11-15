# SPDX-License-Identifier: MIT
# check-all sources: app/web app/files app/render/Renderer.hpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/gltf core/model/src/exchange core/model/src/exchange.rs core/model-api/src/exchange.rs
# check-all sources: app/browser/AppearanceDialog.cpp app/framework/Appearances.cpp
# glTF export and import (mitcad#1826) in the build VM, as
# tools/ui-gltf-test.sh on Linux: the shared input scripts
# tools/ui-scripts/gltf.mitcad-ui and gltf-open.mitcad-ui on a design of two
# components (tools/ui-scripts/gltf.json): File > Export to robot.glb and
# robot.gltf (its buffer and texture image beside it), File > Import of
# both; MitCad's own reader reads both files back with mitcad-cli
# (tools/ui-scripts/gltf-read.json: node names, placements within 1e-6 m,
# triangles, colours, the .glb imported as components); --open of the .glb
# a new document of its two components, whose textured appearance the
# Appearances window shows by its texture coordinates, Projection Box and
# Texture coordinates picked there (mitcad#1874).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-gltf-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-gltf'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'robot.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/gltf.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'glTF design fixture creation failed' }

# The scripts type the paths with forward slashes, as the log shows them.
$env:MITCAD_TEST_WORK = $work -replace '\\', '/'
$env:MITCAD_TEST_STEP_MS = '100'

# Runs the application with a shared script and waits for its driver.
function Run-Script([string]$Name, [string]$Script, [string]$Open, [string]$What) {
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$Script"
  Ui-StartApp $Name @('--ui=qt', '--no-mcp', '--open', $Open)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail "$What driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "$What driver failed (exit code $exitCode)"
  }
}

Run-Script 'gltf' 'gltf.mitcad-ui' $design 'glTF export and import'
Ui-ExpectLog 'TestDriver: done' 'File > Export writes .glb and .gltf; File > Import reads both'
$glb = [IO.File]::ReadAllBytes((Join-Path $work 'robot.glb'))
if ($glb.Length -lt 4 -or [Text.Encoding]::ASCII.GetString($glb, 0, 4) -ne 'glTF') {
  Ui-Fail 'robot.glb is no binary glTF file'
}
if ([IO.File]::ReadAllText((Join-Path $work 'robot.gltf')) -notmatch '"uri": *"robot\.bin"') {
  Ui-Fail 'robot.gltf does not refer to robot.bin'
}
Write-Host 'ok   robot.glb binary, robot.gltf refers to its buffer beside it'

# Read back in the folder the files are in (the script's paths are theirs).
Push-Location $work
try {
  $output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/gltf-read.json') 2>&1
  $status = $LASTEXITCODE
} finally {
  Pop-Location
}
if ($status -ne 0) { Ui-Fail "mitcad-cli: the files read back: $($output -join ' / ')" }
Write-Host 'ok   both files read back: names, placements, triangles, colours; the .glb imported as components'

Run-Script 'gltf-open' 'gltf-open.mitcad-ui' (Join-Path $work 'robot.glb') 'glTF open'
Ui-ExpectLog 'TestDriver: done' 'File > Open of the .glb: a new document of its two components'
Ui-StopApp
Write-Host 'PASS: glTF export and import'
