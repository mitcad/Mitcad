# SPDX-License-Identifier: MIT
# check-all sources: app/view/Turntable.cpp app/view/TurntableDialog.cpp app/view/TurntableExport.cpp app/view/Snapshot.cpp app/render/ShownScene.cpp app/framework/TestDriver.cpp tools/cli tools/ui-scripts/turntable.mitcad-ui tools/ui-scripts/turntable.json
# Export Turntable (mitcad#2523) in the build VM, as tools/ui-turntable-test.sh
# on Linux: the shared steps tools/ui-scripts/turntable.mitcad-ui on the
# block that mitcad-cli makes (tools/ui-scripts/turntable.json). 36 frames
# of a full turn about the design's Z, each camera direction 10 degrees on
# from the one before, written as PNG files; with an FFmpeg program on the
# PATH also an MP4 video, its container read back (36 frames of 320 x 240)
# and, with ffprobe, decoded here.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-turntable-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-turntable'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'turntable.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/turntable.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$ffmpeg = Get-Command ffmpeg -ErrorAction SilentlyContinue
if ($ffmpeg) {
  $env:MITCAD_FFMPEG = $ffmpeg.Source
  $env:TURNTABLE_OUTPUT = 'MP4 video and PNG frames'
  $env:TURNTABLE_PATH = Join-Path $work 'turn.mp4'
} else {
  Write-Host 'note: no ffmpeg on the PATH: the PNG frames only'
  $env:TURNTABLE_OUTPUT = 'PNG frames'
  $env:TURNTABLE_PATH = Join-Path $work 'turn-frames'
}
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/turntable.mitcad-ui'
Ui-StartApp 'turntable' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'turntable driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "turntable driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Turntable frame 36 of 36: camera direction' 'the 36th frame, 10 degrees on from the 35th'
Ui-ExpectLog 'turn-frames: 36 frames, 320 x 240' 'the 36 frames written'
if ($ffmpeg) {
  Ui-ExpectLog 'turn.mp4: 36 frames, 320 x 240' 'the video container: 36 frames of 320 x 240'
  $probe = Get-Command ffprobe -ErrorAction SilentlyContinue
  if ($probe) {
    $info = & $probe.Source -v error -select_streams v:0 -count_frames `
      -show_entries stream=nb_read_frames,width,height -of default=nw=1 (Join-Path $work 'turn.mp4')
    foreach ($want in @('width=320', 'height=240', 'nb_read_frames=36')) {
      if (-not (@($info) -contains $want)) { Ui-Fail "the video plays: $want not in ffprobe's answer: $info" }
    }
    Write-Host 'ok   the video plays: ffprobe decodes 36 frames of 320 x 240'
  }
}
Ui-ExpectLog 'TestDriver: done' 'Export Turntable (mitcad#2523): every step of the script'
Ui-StopApp
Write-Host 'PASS: Export Turntable (mitcad#2523)'
