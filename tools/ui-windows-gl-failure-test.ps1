# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/framework/TestDriver.cpp app/DocumentFrame.cpp tools/cli
# check-all sources: app/webbrowser/cef/CefView.cpp app/webbrowser/cef/CefView.hpp
# The 3D view without graphics (mitcad#2125) in the build VM, as
# tools/ui-gl-failure-test.sh on Linux: MITCAD_TEST_GL_FAILURE makes the
# view's attach to the graphics card fail in four runs of the shared
# scripts on the design that mitcad-cli makes
# (tools/ui-scripts/gl-failure.json):
#   init       the first attach (tools/ui-scripts/gl-failure-init.mitcad-ui),
#              with left, double and right clicks into the view without
#              graphics that do nothing (mitcad#2295);
#   reconnect  the attach after the view moved to a new window
#              (tools/ui-scripts/gl-failure-reconnect.mitcad-ui);
#   throw      that attach throws Standard_NullObject, as OCCT's
#              OpenGl_Context::Init did in the reported crash;
#   screen     MITCAD_TEST_GL_FAILURE=reconnect, the attach after a screen
#              change (the test driver's screen-change) with Component from
#              Web's window open, its browser told of the change too and
#              then made to throw there
#              (tools/ui-scripts/gl-failure-screen-web.mitcad-ui), where the
#              build made the web browser component (-WebBrowser); without
#              it the same screen changes without the browser
#              (tools/ui-scripts/gl-failure-screen.mitcad-ui).
# Each run: the message once, the failure logged once, the view drawn
# again once the script clears the switch (the test driver's unset-env),
# the design saved, the app quit with exit code 0 and no crash report.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-gl-failure-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir] [-WebBrowser build\dev\browser-component]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-gl-failure'),
  [string]$QtBin = '',
  [string]$WebBrowser = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$design = Join-Path $script:UiOut 'design.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/gl-failure.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_STEP_MS = '100'
$noAnswer = 'the driver answers no GL version or vendor \(GL_VERSION null, GL_VENDOR null'
$withBrowser = $WebBrowser -and (Test-Path (Join-Path $WebBrowser 'component.json')) -and
               (Test-Path (Join-Path $WebBrowser 'mitcad-webbrowser.dll'))
$screenScript = if ($withBrowser) { 'gl-failure-screen-web' } else { 'gl-failure-screen' }
$runs = @(
  @{ Name = 'init'; Mode = 'init'; Script = 'gl-failure-init'; Reason = $noAnswer },
  @{ Name = 'reconnect'; Mode = 'reconnect'; Script = 'gl-failure-reconnect'; Reason = $noAnswer },
  @{ Name = 'throw'; Mode = 'throw'; Script = 'gl-failure-reconnect'
     Reason = 'OCCT failed: Standard_NullObject: TCollection_ExtendedString : null parameter' },
  @{ Name = 'screen'; Mode = 'reconnect'; Script = $screenScript; Reason = $noAnswer })
$crashes = Join-Path $script:UiOut 'crashes'
foreach ($run in $runs) {
  $name = $run.Name
  $mode = $run.Mode
  $work = Join-Path $script:UiOut $name
  New-Item -ItemType Directory -Force $work | Out-Null
  Copy-Item $design (Join-Path $work 'first.mitcad') -Force
  Copy-Item $design (Join-Path $work 'second.mitcad') -Force
  $env:MITCAD_TEST_WORK = $work
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$($run.Script).mitcad-ui"
  $env:UI_GL_REASON = $run.Reason
  $env:MITCAD_TEST_GL_FAILURE = $mode
  if ($run.Script -eq 'gl-failure-screen-web') {
    # Component from Web's window on the component the build made: no
    # download, nothing of the user's.
    $env:MITCAD_BROWSER_COMPONENT_DIR = (Resolve-Path $WebBrowser).Path
    $env:MITCAD_BROWSER_COMPONENTS_DIR = Join-Path $work 'components'
    $env:MITCAD_WEB_PROFILE_DIR = Join-Path $work 'profile'
    $env:MITCAD_WEB_DOWNLOADS_DIR = Join-Path $work 'downloads'
    Remove-Item Env:MITCAD_WEB_GPU -ErrorAction SilentlyContinue
    Remove-Item Env:MITCAD_TEST_WEB_GPU_FAILURE -ErrorAction SilentlyContinue
  } elseif ($name -eq 'screen') {
    Write-Host "note screen: no web browser component ('$WebBrowser'); the screen changes without Component from Web's window"
  }
  Ui-StartApp "gl-failure-$name" @('--ui=qt', '--no-mcp', '--chrome=floating', '--open', (Join-Path $work 'first.mitcad'))
  Remove-Item Env:MITCAD_TEST_GL_FAILURE
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail "${name}: the driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ((Ui-CountLog '(?m)^TestDriver: FAIL') -gt 0) { Ui-Fail "${name}: a step failed" }
  if ((Ui-CountLog '(?m)^TestDriver: quit') -eq 0) { Ui-Fail "${name}: the script did not reach its quit" }
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${name}: the app did not quit cleanly (exit code $exitCode)"
  }
  if ((Test-Path $crashes) -and @(Get-ChildItem -Path $crashes -Filter '*.crash' -File).Count -gt 0) {
    Ui-Fail "${name}: a crash report was written"
  }
  if ((Ui-CountLog '(?m)^Status error: The 3D view lost its connection') -ne 1) {
    Ui-Fail "${name}: the message was not shown exactly once"
  }
  if ((Ui-CountLog '(?m)^3D view: no graphics: ') -ne 1) {
    Ui-Fail "${name}: the failure was not logged exactly once"
  }
  if ($run.Script -eq 'gl-failure-screen-web') {
    if ((Ui-CountLog '(?m)^Component browser: a screen change failed: ') -ne 1) {
      Ui-Fail "${name}: the browser's failure was not logged exactly once"
    }
    Write-Host "ok   ${name}: Component from Web's window took the screen changes"
  }
  Write-Host "ok   ${name}: told once, drawn again, saved, quit cleanly, no crash report"
}
Ui-StopApp
Write-Host 'PASS: the 3D view without graphics does not end MitCad'
