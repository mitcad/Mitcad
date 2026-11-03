# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ConvertMeshCommands.cpp app/commands/ModifyCommands.cpp
# check-all sources: core/model/src/features/mesh_convert.rs core/model/src/features/mesh_convert
# check-all sources: core/model/src/document/mesh_convert.rs core/ffi/src/kernel/boundary.rs tools/cli
# Input-driven Windows UI test of Convert Mesh's panel (mitcad#1082) in the
# build VM (tools/ui-windows-lib.ps1 says how the input is posted), on a
# design mitcad-cli makes: a closed cube of 20 mm from an STL file of 12
# triangles inserted as the mesh body cube (Base1, F1.b0). Convert Mesh
# (F7), the cube picked in the browser, OK with the panel's defaults
# (Faceted Solid, 0.01 mm, 0.5 deg): the solid Body1 (F2.b0) of 8000 mm3,
# listed in the browser, the mesh body hidden, ConvertMesh1 in the
# timeline. The Linux counterpart is tools/ui-convert-mesh-test.sh.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-convert-mesh-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-convert-mesh'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'solid.convert_mesh' = 'F7' } -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

# The cube from 0 to 20 on every axis, counter-clockwise seen from outside.
$triangles = @(
  '0 0 0|0 20 0|20 20 0', '0 0 0|20 20 0|20 0 0',
  '0 0 20|20 0 20|20 20 20', '0 0 20|20 20 20|0 20 20',
  '0 0 0|20 0 0|20 0 20', '0 0 0|20 0 20|0 0 20',
  '20 0 0|20 20 0|20 20 20', '20 0 0|20 20 20|20 0 20',
  '20 20 0|0 20 0|0 20 20', '20 20 0|0 20 20|20 20 20',
  '0 20 0|0 0 0|0 0 20', '0 20 0|0 0 20|0 20 20')
$stl = @('solid cube')
foreach ($t in $triangles) {
  $v = $t.Split('|')
  $stl += 'facet normal 0 0 0', 'outer loop', "vertex $($v[0])", "vertex $($v[1])", "vertex $($v[2])", 'endloop',
    'endfacet'
}
$stl += 'endsolid cube'
$cube = Join-Path $work 'cube.stl'
Set-Content -Encoding ascii $cube $stl

$empty = Join-Path $work 'empty.json'
$design = Join-Path $work 'convert-mesh.mitcad'
Set-Content -Encoding ascii $empty '[{"comment": "An empty design for the inserted cube."}]'
$made = & $cli run $empty --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli run: $($made -join ' / ')" }
$made = & $cli import $cube --open $design --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli import: $($made -join ' / ')" }

Ui-StartApp 'convert-mesh' @('--open', $design)
Ui-ExpectLog 'convert-mesh.mitcad' 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F1.b0' 'the mesh body shown'

Write-Host '--- Convert Mesh on the inserted cube'
Ui-Mark
Ui-Step 'convert mesh (F7)'              { Ui-Key 'F7' }
Ui-ExpectNew 'Command Convert Mesh started' 'the panel opened' 30
Ui-Step 'pick cube'                      { Ui-ClickLogged 'Browser Root/Bodies/cube' }
Ui-ExpectNew 'Convert Mesh Mesh Body: 1 body [body F1.b0]' 'the mesh body picked' 30
Ui-ExpectNew 'Preview Convert Mesh: ok' 'the faceted solid previewed' 30
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added convert mesh (Faceted Solid) of F1.b0, tolerance 0.01' 'the conversion added' 30
Ui-ExpectVolume 'New body Body1 \(F2\.b0\): volume ([0-9.]+) mm3' 8000 'a solid of the cube''s volume'
Ui-ExpectLog 'Timeline ConvertMesh1 at' 'ConvertMesh1 in the timeline'
Ui-ExpectLog 'Browser Root/Bodies/Body1 at' 'the browser lists the solid Body1'
Ui-ExpectLog 'Bodies shown: F2.b0' 'the solid shown, the mesh body hidden'
if ((Ui-LogLines) | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Ui-Finish 'UI Windows convert mesh test'
