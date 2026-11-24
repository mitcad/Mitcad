# SPDX-License-Identifier: MIT
# check-all sources: app/framework/AccessibleTree.cpp app/framework/AccessibilityClients.cpp app/framework/TestDriverChrome.cpp app/tests/uia_walk.cpp tools/cli
# The window's accessibility tree in the build VM (mitcad#2227): a client
# that reads the whole window through UI Automation froze it for minutes.
# On a design of six components, each with a sketch and an extruded body,
# the browser open, in the Floating layout (the default) and in the Docked
# one (the shared script tools/ui-scripts/accessible-tree.mitcad-ui):
#   1. the tree walked as a client reads it stays within -Nodes nodes and
#      -Ms ms (0: no limit, only logged; about 130 nodes and 1 ms on
#      22.11.2026);
#   2. the UI Automation client mitcad-uia-walk (app/tests/uia_walk.cpp)
#      reads the whole window in one bulk read within -UiaMs ms (about
#      50 ms), and the log names it among the processes that can read.
# -DesignFile and -InputScript look at another design or script (by hand).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-accessible-tree-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe -UiaWalk build\dev\mitcad-uia-walk.exe [-Out dir] [-QtBin dir] [-Nodes n] [-Ms n] [-UiaMs n] [-Parts n] [-DesignFile file] [-InputScript file]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-accessible-tree'),
  [string]$QtBin = '',
  [string]$UiaWalk,
  [int]$Nodes = 400,
  [int]$Ms = 250,
  [int]$UiaMs = 1000,
  [int]$Parts = 6,
  [string]$DesignFile = '',
  [string]$InputScript = (Join-Path $PSScriptRoot 'ui-scripts/accessible-tree.mitcad-ui'))
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

# Part1..Part<Parts>, each a component with a sketch (a circle) extruded into a body.
$designScript = Join-Path $work 'design.json'
$design = Join-Path $work 'design.mitcad'
$lines = @('[')
for ($i = 1; $i -le $Parts; $i++) {
  $sketch = 'F' + (2 * $i - 1)
  $lines += '  {"cmd": "create_component", "name": "Part' + $i + '", "transform": {"translation": [' + ($i * 30) + ', 0, 0]}},'
  $lines += '  {"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy", "entities": [{"id": "p1", "type": "point", "at": [0, 0]}, {"id": "c2", "type": "circle", "center": "p1", "radius": 8}]}},'
  $lines += '  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "' + $sketch + '", "region": "r{c2}"}], "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},'
  $lines += '  {"cmd": "activate_component", "component": "Root"},'
}
$lines += '  {"expect": {"bodies": ' + $Parts + '}}'
$lines += ']'
if ($DesignFile) {
  # A design of one's own instead (a look at another tree).
  $design = (Resolve-Path $DesignFile).Path
} else {
  Set-Content -Encoding ascii $designScript $lines
  $output = & $Cli run $designScript --save $design 2>&1
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }
}

$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_TREE_NODES = "$Nodes"
$env:MITCAD_TEST_TREE_MS = "$Ms"
$env:MITCAD_TEST_UIA_WALK = (Resolve-Path $UiaWalk).Path -replace '\\', '/'
$env:MITCAD_TEST_UIA_MS = "$UiaMs"
# Every UI Automation burst's line, with the candidates of unknown callers.
$env:MITCAD_LOG_ACCESSIBILITY_CLIENTS = '1'
$env:MITCAD_TEST_INPUT = $InputScript
foreach ($layout in @('floating', 'docked')) {
  Write-Host "--- the $layout layout"
  Ui-StartApp "accessible-tree-$layout" @('--ui=qt', '--no-mcp', "--chrome=$layout", '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'the accessible tree driver timed out' }
  $exitCode = $script:UiProcess.ExitCode
  Ui-LogLines | Where-Object { $_ -match '^(Accessible |UIA client|Accessibility client)' } | ForEach-Object { Write-Host "     $_" }
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "the accessible tree driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "the $layout window's tree within $Nodes nodes and $Ms ms"
  Ui-StopApp
}
Write-Host 'PASS: accessible tree'
