# SPDX-License-Identifier: MIT
# check-all sources: app/commands/CellGridCommands.cpp app/commands/CreateCommands.cpp core/model/src/features/cell_grid.rs core/model/src/features/cell_grid app/framework/TestDriver.cpp tools/cli tools/ui-scripts/cell-grid.mitcad-ui tools/ui-scripts/cell-grid.json
# Cell Grid (mitcad#2238) in the build VM, as tools/ui-cell-grid-test.sh on Linux: the
# shared steps tools/ui-scripts/cell-grid.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/cell-grid.json). A rectangle with a circle inside sketched on a plate's top face, Cell Grid of hexagons on the rectangle's region, then edited to circles, with the cell counts and the volumes the script works out.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-cell-grid-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-cell-grid'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'cell-grid.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/cell-grid.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/cell-grid.mitcad-ui'
Ui-StartApp 'cell-grid' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'cell-grid driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "cell-grid driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Preview Cell Grid: ok, remark: 34 cells, open area 1884.47 mm' 'the hexagons counted'
Ui-ExpectLog 'Added cell grid (hexagon, whole edge cells) of 1 profile' 'the cell grid was added'
Ui-ExpectLog 'Preview Cell Grid: ok, remark: 34 cells, open area 1709.03 mm' 'the circles counted'
Ui-ExpectLog 'TestDriver: done' 'Cell Grid (mitcad#2238): every step of the script'
Ui-StopApp
Write-Host 'PASS: Cell Grid (mitcad#2238)'
