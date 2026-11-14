# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowPrint.cpp app/MainWindowMake.cpp app/print/MakeBodyCard.cpp
# check-all sources: app/framework/PickerComboBox.cpp app/browser/LibraryPicker.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/document/print_settings.rs core/model/src/exchange core/model/src/print_files tools/cli
# Send to Slicer's Process row shows the process set on the body
# (mitcad#1799) on Windows, the shared input script
# tools/ui-scripts/body-process-send.mitcad-ui as
# tools/ui-body-process-send-test.sh runs it on Linux: a process set on
# Body1 on the Make card is the row's choice, Body2 with another one gives
# Per body, and Fixture Fine chosen in the panel is set on both bodies in
# one undo step; Send (a fixture slicer that does nothing, 3MF) writes the
# file, read back here: Fixture Fine for both bodies, no part with layers
# of its own.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-body-process-send'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'body-process.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/body-process-send.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'Send to Slicer process fixture creation failed' }

# The app's temporary folder of the test's own, and a slicer that does
# nothing in the settings, 3MF: the file stays where it was written.
$printTemp = Join-Path $script:UiOut 'tmp'
Remove-Item -Recurse -Force $printTemp -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $printTemp | Out-Null
$env:TMP = $printTemp
$env:TEMP = $printTemp
$cmd = (Join-Path $env:SystemRoot 'System32\cmd.exe') -replace '\\', '/'
Add-Content -Encoding ascii (Join-Path $script:UiSettings 'Mitcad\Mitcad.ini') @(
  '[print]', 'slicerName=Fixture slicer', "slicerProgram=$cmd", 'format=3mf', 'reuseWindow=false',
  'slicerArguments=/c, rem')

$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/body-process-send.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'body-process-send' @('--ui=qt', '--no-mcp', '--chrome=floating', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(280000)) { Ui-Fail 'Send to Slicer process driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Send to Slicer process driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' "the body's process is the Process row's choice, Per body for two, a choice sets it on both bodies"

# The 3MF written: Fixture Fine for Body1 and Body2, 0.12 mm layers, no
# part with layers of its own.
$file = Join-Path $printTemp 'mitcad-print\body-process\body-process.3mf'
if (-not (Test-Path $file)) { Ui-Fail "no 3MF at $file" }
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead($file)
try {
  $read = {
    param($name)
    $entry = $zip.GetEntry($name)
    if ($null -eq $entry) { Ui-Fail "the 3MF has no $name" }
    $reader = New-Object IO.StreamReader($entry.Open())
    try { $reader.ReadToEnd() } finally { $reader.Dispose() }
  }
  $model = & $read 'Metadata/model_settings.config'
  $projectText = & $read 'Metadata/project_settings.config'
} finally {
  $zip.Dispose()
}
$invariant = [Globalization.CultureInfo]::InvariantCulture
$layers = {
  param($value)
  $text = "$(@($value)[0])"
  $parsed = 0.0
  if (-not [double]::TryParse($text, [Globalization.NumberStyles]::Float, $invariant, [ref]$parsed)) {
    Ui-Fail "layer_height '$text' is no number"
  }
  $parsed
}
foreach ($body in @('Body1', 'Body2')) {
  if (-not $model.Contains("<metadata key=`"name`" value=`"$body`"/>")) { Ui-Fail "no object $body in model_settings.config" }
}
$project = $projectText | ConvertFrom-Json
if ("$($project.print_settings_id)" -ne 'Fixture Fine') {
  Ui-Fail "the project's process is '$($project.print_settings_id)', not Fixture Fine"
}
if ([math]::Abs((& $layers $project.layer_height) - 0.12) -gt 1e-9) {
  Ui-Fail "the project's layer height is '$($project.layer_height)', not 0.12"
}
foreach ($match in [regex]::Matches($model, 'key="layer_height" value="([^"]*)"')) {
  if ([math]::Abs((& $layers $match.Groups[1].Value) - 0.12) -gt 1e-9) {
    Ui-Fail "a part has layers of its own: $($match.Groups[1].Value)"
  }
}
Write-Host 'ok   Fixture Fine for Body1 and Body2, 0.12 mm layers'
Ui-StopApp
Write-Host "PASS: Send to Slicer's Process row and the bodies' own process"
