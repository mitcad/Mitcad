# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CutTargetsPanel.cpp app/browser core/model/src/assembly_cut.rs core/model/src/features/extrude_scope.rs tools/cli tools/ui-scripts/cut-linked-assembly.mitcad-ui
# Extrude Cut, Revolve Cut and Hole in an assembly on a linked part
# (mitcad#2131, mitcad#2160) in the build VM, as
# tools/ui-cut-linked-assembly-test.sh on Linux: the shared steps
# tools/ui-scripts/cut-linked-assembly.mitcad-ui on a holder linking
# plate.mitcad, both made by mitcad-cli: the Affected components list of
# each panel lists the linked occurrence, checked it is machined in this
# assembly only, OK cuts it; the saved holder checked with
# cut-linked-assembly-check.json and the part's file as it was written.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-cut-linked-assembly-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-cut-linked-assembly'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$plate = Join-Path $work 'plate.mitcad'
$holder = Join-Path $work 'holder.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/cut-linked-assembly-part.json') --save $plate
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the part was not made' }
# The holder made in the part's folder: the link is plate.mitcad.
Push-Location $work
try {
  & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/cut-linked-assembly-holder.json') --save $holder
  if ($LASTEXITCODE -ne 0) { Ui-Fail 'the holder was not made' }
} finally {
  Pop-Location
}
$written = (Get-FileHash $plate -Algorithm SHA256).Hash
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$name = 'cut-linked-assembly'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$name.mitcad-ui"
Ui-StartApp $name @('--ui=qt', '--no-mcp', '--no-recovery', '--open', $holder)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(420000)) { Ui-Fail "${name}: driver timed out" }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "${name}: driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' "${name}: the shared steps"
Ui-StopApp
Push-Location $work
try {
  & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/cut-linked-assembly-check.json') --open $holder
  if ($LASTEXITCODE -ne 0) { Ui-Fail "${name}: the saved holder does not pass cut-linked-assembly-check.json" }
} finally {
  Pop-Location
}
if ((Get-FileHash $plate -Algorithm SHA256).Hash -ne $written) { Ui-Fail "${name}: plate.mitcad changed" }
Write-Host 'PASS: the linked occurrence listed, checked, machined in this assembly only by Extrude, Revolve and Hole; its design file as it was'
