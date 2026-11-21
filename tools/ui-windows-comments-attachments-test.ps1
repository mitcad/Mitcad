# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowComments.cpp app/view/Comments.cpp app/view/CommentsWindow.cpp app/view/CommentImages.cpp app/view/CommentMarkup.cpp app/framework/TestDriver.cpp core/model/src/document core/model-api/src tools/cli tools/ui-scripts/comments-attachments.mitcad-ui tools/ui-scripts/comments-attachments-reopen.mitcad-ui
# Images on comments and replies (mitcad#2011) in the build VM, as
# tools/ui-comments-attachments-test.sh on Linux: the shared steps
# tools/ui-scripts/comments-attachments.mitcad-ui on the block mitcad-cli
# makes (tools/ui-scripts/comments-attachments.json), then
# comments-attachments-reopen.mitcad-ui on the saved design. Capture and
# Annotate... from Add Comment's panel (a rectangle over the view, a line,
# an arc and a rectangle, one undone, attached), Reply... with Attach
# Image..., the thumbnails and the viewer; opened again, the images are
# still there.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-comments-attachments-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-comments-attachments'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'comments-attachments.mitcad'
# The image file the reply attaches: a 128 x 128 PNG.
Copy-Item -Force (Join-Path $PSScriptRoot '../app/branding/png/mitcad-128.png') (Join-Path $work 'photo.png')
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/comments-attachments.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
foreach ($name in @('comments-attachments', 'comments-attachments-reopen')) {
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$name.mitcad-ui"
  Ui-StartApp $name @('--ui=qt', '--no-mcp', '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail "${name}: driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${name}: driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "${name}: the shared steps"
  if ($name -eq 'comments-attachments') {
    Ui-ExpectLog 'Markup attached: capture.png' 'the capture marked up and attached'
    Ui-ExpectLog 'Comment C1 reply 1 image A2: photo.png, image/png, 128 x 128' 'the reply with its image'
    Ui-ExpectLog 'Comment image viewer opened: Comment 1: capture.png' 'the viewer opened from a thumbnail'
  } else {
    Ui-ExpectLog 'Comments details images: C1: A1 capture.png, A2 photo.png' 'opened again: both images'
  }
  Ui-StopApp
  if ($name -eq 'comments-attachments') {
    $saved = Get-Content -Raw -Encoding UTF8 $design | ConvertFrom-Json
    $images = @($saved.comment_attachments)
    if ($images.Count -ne 2 -or $images[0].comment -ne 'C1' -or $images[1].reply -ne 1 -or $images[1].name -ne 'photo.png') {
      Ui-Fail 'the saved design does not keep the capture on C1 and the photo on its reply'
    }
    Write-Host 'ok   saved: the capture on C1 and the photo on its reply'
  }
}
Write-Host 'PASS: images on comments and replies, kept with the design'
