# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowScripts.cpp app/framework/MacroRecorder.cpp app/framework/DocumentSession.cpp
# check-all sources: core/model-api/src/script.rs core/model-api/src/script_export.rs
# check-all sources: tools/ui-scripts/macro-record.mitcad-ui
# Macros and scripts (mitcad#2245) in the build VM, as
# tools/ui-macro-record-test.sh on Linux: the shared steps
# tools/ui-scripts/macro-record.mitcad-ui in an empty design: Record Macro,
# a sketch, an extrusion and a fillet, Stop Recording and Save Macro; the
# steps undone and Run Script of the saved file, which builds the filleted
# block again as one undo step; File > Export Timeline as Script writes the
# timeline. The macro and the exported script are then read here.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-macro-record-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-macro-record'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# The script types the paths with forward slashes, as the log shows them.
$env:MITCAD_TEST_WORK = $work -replace '\\', '/'
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/macro-record.mitcad-ui'
Ui-StartApp 'macro-record' @('--ui=qt', '--no-mcp')
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'macro recording driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "macro recording driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Macro: recorded add_feature (4 steps)' 'the sketch, the extrusion and the fillet recorded'
Ui-ExpectLog 'Script: ran ' 'Run Script built the block again'
Ui-ExpectLog 'Undo: Run fillet-block' 'the script is one undo step'
$macro = [IO.File]::ReadAllText((Join-Path $work 'fillet-block.json'))
if (-not ($macro.Contains('Macro recorded in MitCad') -and $macro.Contains('"sketch.create"') -and $macro.Contains('"fillet"'))) {
  Ui-Fail 'the macro lacks its comment, the sketch or the fillet'
}
Write-Host 'ok   the macro holds its comment, the sketch and the fillet'
$timeline = [IO.File]::ReadAllText((Join-Path $work 'block-timeline.json'))
if (-not ($timeline.Contains('"add_feature"') -and $timeline.Contains('"fillet"') -and $timeline.Contains('"expect"'))) {
  Ui-Fail 'the exported script lacks add_feature, the fillet or its expectations'
}
Write-Host 'ok   the exported script adds the features and checks the bodies'
Ui-ExpectLog 'TestDriver: done' 'a macro recorded, saved and run; the timeline exported as a script'
Ui-StopApp
Write-Host 'PASS: macros and scripts'
