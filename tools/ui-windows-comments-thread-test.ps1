# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowComments.cpp app/view/Comments.cpp app/view/CommentsWindow.cpp
# check-all sources: app/browser/BrowserController.cpp core/model/src/document core/model-api/src tools/cli
# A comment thread with an agent (mitcad#2010) in the build VM, as the
# second part of tools/ui-comments-test.sh on Linux: the shared steps
# tools/ui-scripts/comments-thread.mitcad-ui on an empty design
# (tools/ui-scripts/comments-empty.json): Add Comment with Whole Design
# and the browser root row's Add Comment..., an agent's long reply shown in
# full in the Comments panel's details pane, the comment's row showing
# the whole thread, "Open, waiting for your answer" until the user
# answers, and Reply... showing the reply it answers.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-comments-thread-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-comments-thread'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'empty.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/comments-empty.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the empty design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/comments-thread.mitcad-ui'
Ui-StartApp 'comments-thread' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'the comment thread driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "the comment thread driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Comment C1: open, user, Design: Plan a wall bracket' 'Add Comment with Whole Design in an empty design'
Ui-ExpectLog 'Comments status C1: Open, waiting for your answer' 'waiting for your answer after the agent''s question'
Ui-ExpectLog 'Comment dialog answers: Reply 1 by claude-code, ' 'Reply... shows the reply it answers'
Ui-ExpectLog 'TestDriver: done' 'the comment thread''s steps'
Ui-StopApp
Write-Host 'PASS: comment thread'
