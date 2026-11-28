# SPDX-License-Identifier: MIT
# check-all sources: app/files app/MainWindowMcp.cpp app/framework/TestDriverCommands.cpp app/framework/TestDriverModel.cpp app/framework/TestDriverPointer.cpp
# check-all sources: core/model/src/file core/model-api/src/lib.rs tools/cli
# check-all sources: tools/ui-scripts/unreadable-cam-in-place.mitcad-ui tools/ui-scripts/edit-in-place-part.json tools/ui-scripts/edit-in-place-holder.json
# Edit in Place of a linked design whose CAM section this MitCad cannot
# read (mitcad#2587) in the build VM, as tools/ui-unreadable-cam-in-place-test.sh
# on Linux: the shared steps tools/ui-scripts/unreadable-cam-in-place.mitcad-ui
# on an assembly that links part.mitcad, a cube whose CAM has two setups
# with one id (made by mitcad-cli, the CAM added here). Edit in Place
# changes it, an agent's save (MCP) is refused and Save asks first:
#   1. Cancel: the part's file is as it was, byte for byte (Get-FileHash).
#   2. Save and Drop Them: written without the CAM, with the change.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-unreadable-cam-in-place-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-unreadable-cam-in-place'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$part = Join-Path $work 'part.mitcad'
$output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/edit-in-place-part.json') --save $part 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: the part was not made: $($output -join ' / ')" }
# A CAM section this MitCad cannot read: two setups with one id, as
# tools/ui-scripts/unreadable-cam-file.py adds it for Linux and macOS.
$text = [IO.File]::ReadAllText($part)
$open = $text.IndexOf('{')
$cam = '"cam": {"version": 1, "setups": [{"id": 1, "name": "Setup 1"}, {"id": 1, "name": "Setup 2"}]},'
[IO.File]::WriteAllText($part, $text.Substring(0, $open + 1) + $cam + $text.Substring($open + 1))
# The holder made in the part's folder: the link is part.mitcad.
Push-Location $work
try {
  $output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/edit-in-place-holder.json') --save (Join-Path $work 'holder.mitcad') 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: the holder was not made: $($output -join ' / ')" }
} finally {
  Pop-Location
}
$features = @((Get-Content -Raw $part | ConvertFrom-Json).features).Count
$before = (Get-FileHash $part -Algorithm SHA256).Hash

# The script with an answer to Save's question (its button, and the word
# the answer logs), to its end.
function Run-Answer([string]$Name, [string]$Answer, [string]$Answered) {
  Write-Host "--- $Name"
  $env:MITCAD_TEST_WORK = $work
  $env:MITCAD_TEST_STEP_MS = '100'
  $env:MITCAD_TEST_ANSWER = $Answer
  $env:MITCAD_TEST_ANSWERED = $Answered
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/unreadable-cam-in-place.mitcad-ui'
  Ui-StartApp "unreadable-cam-in-place-$Answered" @('--ui=qt', '--no-mcp', '--open', (Join-Path $work 'holder.mitcad'))
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail "${Name}: the driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${Name}: the driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: quit' "${Name}: Edit in Place, the agent's save refused, Save asked"
}

Run-Answer '1. Cancel' 'Cancel' 'cancelled'
if ((Get-FileHash $part -Algorithm SHA256).Hash -ne $before) { Ui-Fail "Cancel or the agent's save wrote the part's file" }
Write-Host "ok   the part's file is as it was, byte for byte"
# (At a line's start: the driver's trace of its steps quotes the script's lines.)
if ((Ui-LogText) -match '(?m)^Edit in Place: saved ') { Ui-Fail 'the part was saved' }

Run-Answer '2. Save and Drop Them' 'Save and Drop Them' 'save'
if ((Ui-LogText) -notmatch '(?m)^Edit in Place: saved .*/part[.]mitcad\r?$') { Ui-Fail 'the part was not saved' }
Write-Host 'ok   the part saved'
$saved = Get-Content -Raw $part | ConvertFrom-Json
if ($saved.PSObject.Properties.Name -contains 'cam') { Ui-Fail 'the saved part still has its CAM' }
if (@($saved.features).Count -ne $features + 1) { Ui-Fail "the saved part has $(@($saved.features).Count) features, not $($features + 1)" }
Write-Host 'ok   saved without the CAM, with the change'
Ui-StopApp
Write-Host 'PASS: Edit in Place of a design whose CAM cannot be read'
