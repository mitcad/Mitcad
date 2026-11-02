# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp app/OcctViewer.cpp app/framework/TestDriver.cpp
# check-all sources: core/sketch core/model core/model-base core/model-api core/ffi core/cpp geometry/src/sketch_ref.cpp tools/cli
param(
  [string]$App,
  [string]$Fixture,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-face-profiles'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
& $Fixture --write-fixture $work (Join-Path $PSScriptRoot 'ui-scripts')
if ($LASTEXITCODE -ne 0) { Ui-Fail 'face profile fixture creation failed' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
foreach ($scenario in @('face-bounded-profiles', 'face-bounded-whole-face', 'face-bounded-open-face')) {
  $design = if ($scenario -eq 'face-bounded-profiles') {
    'face-profiles'
  } elseif ($scenario -eq 'face-bounded-open-face') {
    'open-face-profiles'
  } else { 'whole-face' }
  $env:MITCAD_TEST_INPUT = Join-Path $work "$scenario.mitcad-ui"
  Ui-StartApp $scenario @('--ui=qt', '--no-mcp', '--open', (Join-Path $work "$design.mitcad"))
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail "$scenario driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "$scenario driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "$scenario completed"
  Ui-StopApp
}
Write-Host 'PASS: solid/open face-bounded and whole-face profile picking'
