# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MeshToolsCommands.cpp core/model/src/mesh_tools core/model/src/features/mesh_tools.rs tools/cli
# Mesh tools for reverse engineering (mitcad#1871) in the build VM: the
# shared steps tools/ui-scripts/mesh-tools.mitcad-ui (also run by
# tools/ui-mesh-tools-test.sh and tools/ui-macos-mesh-tools-test.sh) on the
# design mitcad-cli makes (tools/ui-scripts/mesh-tools.json, run in the work
# folder, where it writes its STL file): each panel on a cylinder's mesh.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-mesh-tools-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-mesh-tools'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'mesh-tools.mitcad'
Push-Location $work
try {
  & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/mesh-tools.json') --save $design
  $made = $LASTEXITCODE
} finally {
  Pop-Location
}
if ($made -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/mesh-tools.mitcad-ui'
Ui-StartApp 'mesh-tools' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(600000)) { Ui-Fail 'mesh tools driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "mesh tools driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'each mesh tool panel made its body'
Ui-StopApp
Write-Host 'PASS: the mesh tools panels make their bodies'
