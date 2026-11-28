# SPDX-License-Identifier: MIT
# check-all sources: app/files app/MainWindowMcp.cpp app/framework/TestDriverCommands.cpp app/framework/TestDriverModel.cpp
# check-all sources: core/model/src/file core/model-api/src/lib.rs core/model-api/src/classes.rs tools/cli
# check-all sources: tools/ui-scripts/unreadable-cam-recovery-change.mitcad-ui tools/ui-scripts/unreadable-cam-recovery.mitcad-ui tools/ui-scripts/edit-in-place-part.json
# A design whose CAM section this MitCad cannot read and with a field a
# newer MitCad wrote, recovered from an autosave (mitcad#2587) in the build
# VM, as tools/ui-unreadable-cam-recovery-test.sh on Linux: the autosaved
# text has neither, so the recovered design keeps them from its file and
# Save over it asks first, as for the design opened from it. The design is
# a cube made by mitcad-cli, its CAM and the newer field added here. Three
# runs of the application, as a script cannot start it again:
#   1. tools/ui-scripts/unreadable-cam-recovery-change.mitcad-ui: opened
#      with both warnings, changed, autosaved, the session killed.
#   2. tools/ui-scripts/unreadable-cam-recovery.mitcad-ui: recovered, it
#      warns again; an agent's save (MCP) is refused; Save asks, Cancel; the
#      file is as it was, byte for byte (Get-FileHash). Killed once it
#      autosaved.
#   3. The same script: recovered again; Save and Drop Them writes the file
#      without the CAM and the newer field, with the change.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-unreadable-cam-recovery-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-unreadable-cam-recovery'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'cam.mitcad'
$output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/edit-in-place-part.json') --save $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: the design was not made: $($output -join ' / ')" }
# A CAM section this MitCad cannot read (two setups with one id) and a
# field a newer MitCad wrote, as tools/ui-scripts/unreadable-cam-file.py
# adds them for Linux and macOS.
$text = [IO.File]::ReadAllText($design)
$open = $text.IndexOf('{')
$added = '"cam": {"version": 1, "setups": [{"id": 1, "name": "Setup 1"}, {"id": 1, "name": "Setup 2"}]},' +
  ' "future_notes": [{"id": "N1", "text": "Check the wall"}],'
[IO.File]::WriteAllText($design, $text.Substring(0, $open + 1) + $added + $text.Substring($open + 1))
$features = @((Get-Content -Raw $design | ConvertFrom-Json).features).Count
$before = (Get-FileHash $design -Algorithm SHA256).Hash
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
# Autosave every 2 s for the test (into the run's own folder, Ui-Init).
$env:MITCAD_TEST_AUTOSAVE_SECONDS = '2'

# The script to its end, which kills the application.
function Run-Script([string]$Name, [string]$Log, [string]$Script, [string[]]$Arguments, [switch]$Recovery) {
  Write-Host "--- $Name"
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$Script"
  $all = @('--ui=qt', '--no-mcp')
  if ($Arguments) { $all += $Arguments }
  Ui-StartApp $Log $all -Recovery:$Recovery
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail "${Name}: the driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${Name}: the driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: kill' "${Name}: the script ran to its end"
}

function Expect-Unchanged([string]$Writer) {
  if ((Get-FileHash $design -Algorithm SHA256).Hash -ne $before) { Ui-Fail "$Writer wrote the design's file" }
  Write-Host "ok   the design's file is as it was, byte for byte"
}

Run-Script '1. Opened, changed, autosaved, killed' 'recovery-change' 'unreadable-cam-recovery-change.mitcad-ui' @('--open', $design)
Expect-Unchanged 'the session'

$env:MITCAD_TEST_ANSWER = 'Cancel'
$env:MITCAD_TEST_ANSWERED = 'cancelled'
Run-Script "2. Recovered: the agent's save refused, Save asks, Cancel" 'recovery-cancel' 'unreadable-cam-recovery.mitcad-ui' @() -Recovery
Expect-Unchanged "Cancel or the agent's save"
# (At a line's start: the driver's trace of its steps quotes the script's lines.)
if ((Ui-LogText) -match '(?m)^Saved ') { Ui-Fail 'the recovered design was saved' }

$env:MITCAD_TEST_ANSWER = 'Save and Drop Them'
$env:MITCAD_TEST_ANSWERED = 'save'
Run-Script '3. Recovered again: Save and Drop Them' 'recovery-drop' 'unreadable-cam-recovery.mitcad-ui' @() -Recovery
if ((Ui-LogText) -notmatch '(?m)^Saved .*/cam[.]mitcad\r?$') { Ui-Fail "the recovered design was not saved over its file" }
Write-Host "ok   saved over the design's file"
$saved = Get-Content -Raw $design | ConvertFrom-Json
$names = $saved.PSObject.Properties.Name
if ($names -contains 'cam' -or $names -contains 'future_notes') { Ui-Fail 'the saved design still has its CAM or the newer field' }
if (@($saved.features).Count -ne $features + 1) { Ui-Fail "the saved design has $(@($saved.features).Count) features, not $($features + 1)" }
Write-Host 'ok   saved without the CAM and the newer field, with the change'
Ui-StopApp
Write-Host 'PASS: recovery of a design whose CAM cannot be read'
