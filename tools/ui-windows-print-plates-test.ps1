# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowPlates.cpp app/files/MainWindowPrint.cpp app/files/Print3d.cpp
# check-all sources: app/print/OwnSettingsDialog.cpp app/OcctViewer.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/exchange core/model/src/print_files core/model-api/src/print_plates.rs tools/cli
# Plates... (mitcad#1050) on Windows, the shared input script
# tools/ui-scripts/print-plates.mitcad-ui as tools/ui-print-plates-test.sh
# runs it on Linux: Plates... adds Plate 1 and Plate 2, Body2 picked goes on
# Plate 2, Spiral Vase is set for it and the plate is drawn beside the
# design (a picture); Send to Slicer (a fixture slicer that does nothing)
# sends the design's plates, and the 3MF written has two plates with the
# spiral mode on Plate 2 only.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-print-plates'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'plates.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/print-plates.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'Plates fixture creation failed' }

# The app's temporary folder of the test's own, and a slicer that does
# nothing in the settings: the 3MF stays where it was written.
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
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/print-plates.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_COLOR_SCHEME = 'light'
Ui-StartApp 'print-plates' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'Plates driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Plates driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'Plates... adds plates, puts Body2 on Plate 2 in vase mode, Send writes them'

# The 3MF written: two plates, the spiral mode on Plate 2 only.
$file = Join-Path $printTemp 'mitcad-print\plates\plates.3mf'
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
  $project = & $read 'Metadata/project_settings.config'
} finally {
  $zip.Dispose()
}
$plates = [regex]::Matches($model, '(?s)<plate>(.*?)</plate>')
if ($plates.Count -ne 2) { Ui-Fail "$($plates.Count) plates in model_settings.config, not 2" }
$first = $plates[0].Groups[1].Value
$second = $plates[1].Groups[1].Value
if ($first.Contains('spiral_mode')) { Ui-Fail 'Plate 1 has a spiral mode' }
if (-not $second.Contains('<metadata key="spiral_mode" value="true"/>')) { Ui-Fail 'Plate 2 has no spiral mode' }
if (-not $second.Contains('key="plater_name" value="Plate 2"')) { Ui-Fail 'the second plate is not Plate 2' }
if ($model.Split(@('<plate>'), [StringSplitOptions]::None)[0].Contains('spiral_mode')) {
  Ui-Fail 'an object overrides the spiral mode'
}
if ($project.Contains('"spiral_mode"')) { Ui-Fail 'the project sets the spiral mode' }
Write-Host 'ok   two plates, the spiral mode on Plate 2 only'
if (-not (Test-Path (Join-Path $work 'print-plates-light.png'))) { Ui-Fail 'no picture of the plate' }
Ui-StopApp
Write-Host 'PASS: Plates... and the plates sent to the slicer'
