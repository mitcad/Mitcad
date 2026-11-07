# SPDX-License-Identifier: MIT
# check-all sources: app/render app/MainWindowSection.cpp tools/cli
# The render worker while a section analysis is shown and hidden in the
# rendered view (mitcad#1432) in the build VM, as
# tools/ui-render-section-toggle-test.sh on Linux: the shared steps
# tools/ui-scripts/render-section-toggle.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/render-section-toggle.json). Cycles
# renders on the CPU (Embree, as on the machine that crashed) with more
# samples than the test waits for, so every change reaches the worker while
# it traces; the worker must not stop, and it deletes the meshes it left
# only after its render thread has built the scene without them.
#
# -Design: another design with a section analysis Section1, hidden, instead
# of the one mitcad-cli makes (a reported design, say).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-render-section-toggle-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\mitcad-cli.exe [-Out dir] [-QtBin dir] [-Design file]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-render-section-toggle'),
  [string]$QtBin = '',
  [string]$Design = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
Add-Content -Encoding ascii (Join-Path $script:UiSettings 'Mitcad\Mitcad.ini') @('[render]', 'device=cpu')
$env:MITCAD_RENDER_SAMPLES = '100000'
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = $Design
if (-not $design) {
  $design = Join-Path $work 'render-section-toggle.mitcad'
  & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/render-section-toggle.json') --save $design
  if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
}
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/render-section-toggle.mitcad-ui'
# A large design computes for minutes before the view is up.
$viewSeconds = 30
if ($Design) { $viewSeconds = 900 }
Ui-StartApp 'render-section-toggle' @('--ui=qt', '--no-mcp', '--open', $design) -ViewSeconds $viewSeconds
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(1800000)) { Ui-Fail 'render section toggle driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "render section toggle driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the section analysis shown and hidden twelve times while the worker rendered'
if (Ui-LogLines | Where-Object { $_ -match '^Render worker stopped' }) { Ui-Fail 'the render worker stopped' }
Ui-StopApp
Write-Host 'PASS: the render worker renders on while a section analysis is shown and hidden'
