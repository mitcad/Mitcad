# SPDX-License-Identifier: MIT
# check-all sources: app/files app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model/src/exchange core/model/src/exchange.rs core/model-api/src/exchange.rs
# URDF export for ROS 2 (mitcad#2100) in the build VM, as
# tools/ui-urdf-test.sh on Linux: the shared input script
# tools/ui-scripts/urdf.mitcad-ui on a mechanism of three components
# (tools/ui-scripts/urdf.json): File > Export to robot.urdf writes the
# package folder robot\ beside it: urdf\robot.urdf (four links, three
# joints), a binary STL per body in meshes\, package.xml and
# CMakeLists.txt.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-urdf-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-urdf'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'robot.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/urdf.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'URDF design fixture creation failed' }

# The script types the paths with forward slashes, as the log shows them.
$env:MITCAD_TEST_WORK = $work -replace '\\', '/'
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/urdf.mitcad-ui'
Ui-StartApp 'urdf' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'URDF export driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "URDF export driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'File > Export writes the URDF package folder'
$urdf = [IO.File]::ReadAllText((Join-Path $work 'robot/urdf/robot.urdf'))
if (([regex]::Matches($urdf, '<link name=')).Count -ne 4) { Ui-Fail 'robot.urdf has not 4 links' }
if (([regex]::Matches($urdf, '<joint name=')).Count -ne 3) { Ui-Fail 'robot.urdf has not 3 joints' }
if (@(Get-ChildItem (Join-Path $work 'robot/meshes') -Filter '*.stl').Count -ne 3) {
  Ui-Fail 'the package has not 3 meshes'
}
if ([IO.File]::ReadAllText((Join-Path $work 'robot/package.xml')) -notmatch '<name>robot</name>') {
  Ui-Fail 'package.xml does not name the package robot'
}
Write-Host 'ok   robot\ holds robot.urdf (4 links, 3 joints), 3 meshes, package.xml and CMakeLists.txt'
Ui-StopApp
Write-Host 'PASS: URDF export'
