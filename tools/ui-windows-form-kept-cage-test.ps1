# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowForm.cpp app/MainWindowContextMenu.cpp app/browser/BrowserController.cpp app/commands/FormCommands.cpp app/commands/FormCage.cpp app/view/FormCageOverlay.cpp core/model/src/document/kept_cages.rs core/model/src/features/cage.rs core/model/src/features/base.rs core/model-api/src/form.rs core/subdiv tools/cli tools/ui-scripts/form-kept-cage.mitcad-ui
# Open Cage as Form (mitcad#1757, mitcad#1810) in the build VM, as
# tools/ui-form-kept-cage-test.sh does on Linux: the shared steps
# tools/ui-scripts/form-kept-cage.mitcad-ui on a design made here:
# mitcad-cli writes a 40 mm cube's B-rep data and imports it as a base
# feature whose body is CageBody (as an import's body is), and the saved
# design's body is given a kept control cage, a uniform cube net of 8
# vertices and 6 faces, as an imported Form body carries one. The body's
# Browser row opens the cage as Form1 in Edit Form; a face of it is moved,
# one undo step whose body is a solid; Close keeps it and the imported
# body stays.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-form-kept-cage-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-form-kept-cage'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
# The cube's B-rep data imported as a base feature (F2: the box F1 that
# made the data is deleted), its body named CageBody.
$at = $work -replace '\\', '/'
$model = Join-Path $work 'model.json'
[IO.File]::WriteAllText($model, @"
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-20, -20],
    "length": 40, "width": 40, "height": 40, "operation": "new_body"}},
  {"cmd": "export", "path": "$at/cube.brep", "format": "brep"},
  {"cmd": "delete_feature", "uid": "F1"},
  {"cmd": "import_file", "path": "$at/cube.brep", "name": "Imported Form", "wrap": false},
  {"cmd": "rename_body", "uid": "F2.b0", "name": "CageBody"}
]
"@)
$imported = Join-Path $work 'imported.mitcad'
& $Cli run $model --save $imported
if (-not (Test-Path $imported)) { Ui-Fail 'mitcad-cli did not make the imported design' }
# Its kept cage: the cube's net, uniform (degree 3, weight 1, no secondary
# points), its faces in no particular turn (the model turns them).
$cage = @'
{"source": "synthetic cube", "degree": 3,
  "vertices": [{"position": [-20, -20, 0]}, {"position": [20, -20, 0]}, {"position": [20, 20, 0]},
               {"position": [-20, 20, 0]}, {"position": [-20, -20, 40]}, {"position": [20, -20, 40]},
               {"position": [20, 20, 40]}, {"position": [-20, 20, 40]}],
  "faces": [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]]}
'@ | ConvertFrom-Json
$design = [IO.File]::ReadAllText($imported) | ConvertFrom-Json
$base = @($design.features | Where-Object { $_.type -eq 'base' })
if ($base.Count -lt 1) { Ui-Fail 'the design has no base feature to keep the cage' }
$base[0].bodies[0] | Add-Member -NotePropertyName cage -NotePropertyValue $cage -Force
$keptCage = Join-Path $work 'kept-cage.mitcad'
[IO.File]::WriteAllText($keptCage, ($design | ConvertTo-Json -Depth 100))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/form-kept-cage.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'form-kept-cage' @('--ui=qt', '--no-mcp', '--open', $keptCage)
# The handle read while the app runs: without it ExitCode stays empty
# after the exit (Start-Process -PassThru).
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'Open Cage as Form driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Open Cage as Form driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Opened the cage of F2.b0 as ' 'the kept cage opened as a Form'
Ui-ExpectLog 'Form edit Form1: cage of 8 vertices, 12 edges, 6 faces, closed' 'Edit Form on the kept cage'
Ui-ExpectLog 'Form edit Move in Form1: done' 'a face of the opened cage moved, one undo step'
Ui-ExpectLog 'Closed Edit Form after 1' 'Close kept the edit'
Ui-ExpectLog 'TestDriver: done' 'an imported body''s kept cage opened and edited as a Form'
Ui-StopApp
Write-Host 'PASS: Open Cage as Form'
