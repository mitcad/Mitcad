# SPDX-License-Identifier: MIT
# check-all sources: app/files/FileDialogs.cpp app/files/MainWindowFiles.cpp app/files/MainWindowPlates.cpp
# check-all sources: app/print/PrintKeys.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/exchange core/model/src/print_files core/model-api/src/print_plates.rs tools/cli
# File > Export of a 3MF file with the design's print plates (mitcad#1679)
# on Windows, the shared input script tools/ui-scripts/export-plates.mitcad-ui
# as tools/ui-export-plates-test.sh runs it on Linux: all plates in one file
# write a project of two plates with the spiral mode on Plate 2 only, a file
# per plate writes one file each, and Only Plate 2 that plate alone.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-export-plates'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'plates.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/export-plates.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'Export plates fixture creation failed' }

# The script types the paths with forward slashes, as the log shows them.
$env:MITCAD_TEST_WORK = $work -replace '\\', '/'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/export-plates.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'export-plates' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'Export driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Export driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'File > Export writes the plates: in one file, a file per plate, one plate'

# The 3MF of both plates: two plates, the spiral mode on Plate 2 only.
$file = Join-Path $work 'kit.3mf'
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
if ($project.Contains('"spiral_mode"')) { Ui-Fail 'the project sets the spiral mode' }
Write-Host 'ok   two plates in one file, the spiral mode on Plate 2 only'
foreach ($name in @('each - Plate 1.3mf', 'each - Plate 2.3mf', 'two.3mf')) {
  if (-not (Test-Path (Join-Path $work $name))) { Ui-Fail "no $name" }
}
Write-Host 'ok   a file per plate and Plate 2 alone written'
Ui-StopApp
Write-Host 'PASS: File > Export of the print plates'
