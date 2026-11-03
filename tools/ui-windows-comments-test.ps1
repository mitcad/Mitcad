# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowComments.cpp app/view/Comments.cpp app/view/CommentPinOverlay.cpp
# check-all sources: app/view/CommentsWindow.cpp app/view/NavigationBar.cpp app/MainWindowMcp.cpp app/mcp
# check-all sources: core/model-api/src core/model/src/document core/mcp tools/cli
# Input-driven Windows UI test of the comments for AI agents (mitcad#1119)
# in the build VM (tools/ui-windows-lib.ps1 says how the input is posted),
# as tools/ui-comments-test.sh on Linux, on the same design: Body1 (F2.b0),
# a 30 x 20 x 10 block on XY.
#   1. Add Comment (F8 here) on Body1's top face, its text typed in the
#      panel: C1 open, by the user, its numbered pin drawn in the view.
#   2. A click on the pin opens the Comments panel with C1's row selected.
#   3. An agent addresses C1 through the live MCP bridge (mitcad-cli.exe
#      mcp --connect, model_command address_comment): C1 addressed, its
#      resolution in its row, its pin dimmed.
#   4. Resolve in the panel (its keys: the input reaches the main window's
#      places only): C1 resolved, its pin gone, listed again under
#      Resolved; undo makes it addressed again.
#   5. Comment Pins in the navigation bar hides and shows the pins.
#   6. Saved and opened again in the Floating layout: C1 addressed, its pin
#      drawn, its row in the panel after a click on the pin.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-comments-test.ps1 [-App build\dev\mitcad.exe] [-Cli mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-comments'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'comment.add' = 'F8' } -StepPause 0
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$modelJson = Join-Path $work 'model.json'
$design = Join-Path $work 'design.mitcad'
Set-Content -Encoding ascii $modelJson @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
'@
$output = & $Cli run $modelJson --save $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }

# --- The bridge: JSON-RPC lines on the CLI's stdin and stdout (as
# tools/ui-windows-mcp-test.ps1).
$script:Bridge = $null
$script:BridgeError = $null
$script:BridgeId = 0

function Mcp-Fail([string]$Message) {
  if ($script:Bridge -and -not $script:Bridge.HasExited) { $script:Bridge.Kill() }
  if ($script:BridgeError) { Write-Host "     bridge stderr: $($script:BridgeError.Result)" }
  Ui-Fail $Message
}

function Mcp-Start([string]$Path) {
  $info = New-Object System.Diagnostics.ProcessStartInfo
  $info.FileName = $Path
  $info.Arguments = 'mcp --connect'
  $info.UseShellExecute = $false
  $info.CreateNoWindow = $true
  $info.RedirectStandardInput = $true
  $info.RedirectStandardOutput = $true
  $info.RedirectStandardError = $true
  $info.StandardOutputEncoding = New-Object System.Text.UTF8Encoding($false)
  # UTF-8 without a byte order mark for the writer (see ui-windows-mcp-test.ps1).
  $utf8 = New-Object System.Text.UTF8Encoding($false)
  $global:OutputEncoding = $utf8
  $inputEncoding = $null
  try { $inputEncoding = [Console]::InputEncoding; [Console]::InputEncoding = $utf8 } catch {
    Write-Host "note: the console's input encoding cannot be set here: $_"
  }
  try {
    $script:Bridge = [System.Diagnostics.Process]::Start($info)
  } finally {
    if ($null -ne $inputEncoding) { try { [Console]::InputEncoding = $inputEncoding } catch { } }
  }
  $script:BridgeError = $script:Bridge.StandardError.ReadToEndAsync()
}

function Mcp-Send($Message) {
  $json = ConvertTo-Json -InputObject $Message -Compress -Depth 30
  $bytes = (New-Object System.Text.UTF8Encoding($false)).GetBytes($json + "`n")
  $script:Bridge.StandardInput.BaseStream.Write($bytes, 0, $bytes.Length)
  $script:Bridge.StandardInput.BaseStream.Flush()
}

# Mcp-Receive id: the response to the request with that id; notifications
# are skipped.
function Mcp-Receive([int]$Id) {
  for ($i = 0; $i -lt 1000; $i++) {
    $read = $script:Bridge.StandardOutput.ReadLineAsync()
    if (-not $read.Wait(60000)) { Mcp-Fail "the bridge did not answer request $Id in 60 s" }
    $line = $read.Result
    if ($null -eq $line) { Mcp-Fail "the bridge closed its stdout before answering request $Id" }
    if ($line.Trim().Length -eq 0) { continue }
    $message = $null
    try { $message = $line | ConvertFrom-Json } catch { Mcp-Fail "the bridge sent a line that is not JSON: $line" }
    $hasId = $null -ne $message.PSObject.Properties['id']
    if (-not $hasId -and $null -ne $message.PSObject.Properties['method']) { continue }
    if (-not $hasId -or $message.id -ne $Id) { Mcp-Fail "the answer to request $Id is another message: $line" }
    return $message
  }
  Mcp-Fail "no answer to request $Id in 1000 lines"
}

function Mcp-Request([string]$Method, $Params = $null) {
  $script:BridgeId++
  $message = @{ jsonrpc = '2.0'; id = $script:BridgeId; method = $Method }
  if ($null -ne $Params) { $message.params = $Params }
  Mcp-Send $message
  return (Mcp-Receive $script:BridgeId)
}

# Mcp-Tool name arguments: the tool's structured result, which must not be
# an error.
function Mcp-Tool([string]$Name, [hashtable]$Arguments) {
  $response = Mcp-Request 'tools/call' @{ name = $Name; arguments = $Arguments }
  if ($response.error) { Mcp-Fail "${Name}: protocol error $($response.error.message)" }
  if ([bool]$response.result.isError) {
    $text = $response.result.content[0].text
    if ($text.Length -gt 2000) { $text = $text.Substring(0, 2000) }
    Mcp-Fail "${Name}: refused: $text"
  }
  return $response.result.structuredContent
}

# Keys to the Comments panel.
function Panel-Keys([string[]]$Keys) {
  Ui-FocusDialog '^Comments$'
  foreach ($key in $Keys) { Ui-Key $key }
}

# The listener's record in a folder of the test's own, which the app makes
# private (it must not exist yet); the bridge reads the same.
$env:MITCAD_MCP_DISCOVERY_DIR = Join-Path $script:UiOut 'discovery'
Remove-Item -Recurse -Force $env:MITCAD_MCP_DISCOVERY_DIR -ErrorAction SilentlyContinue

Ui-StartApp 'comments' @('--open', $design)
Ui-ExpectLog 'MCP live instance:' "the application's MCP listener is up"
Ui-Step 'fit (F6)'                         { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F2.b0' 'Body1 shown'
Ui-ExpectLog 'Comments: none' 'no comments yet'

Write-Host '--- 1. Add Comment on Body1''s top face'
Ui-Mark
Ui-Step 'Add Comment (F8)'                 { Ui-Key 'F8' }
Ui-ExpectNew 'Command Add Comment started' 'Add Comment''s panel'
Ui-ExpectNew 'Panel Add Comment input text at' 'its Comment field'
Ui-Step 'pick Body1''s top face'           { Ui-ClickPick 'face F2.b0/F2:end(' }
Ui-ExpectNew 'Add Comment On: 1 face' 'the top face picked'
Ui-Step 'type the comment'                 { Ui-TypeIn 'Panel Add Comment input text' 'make this face 2 mm thicker' }
Ui-Mark
Ui-Step 'OK (Enter)'                       { Ui-Key 'Return' }
Ui-ExpectNew 'Done Add Comment' 'added'
Ui-ExpectNew 'Comment added on face F2:end(' 'on the top face'
Ui-ExpectNew 'Comment C1: open, user, ' 'C1 open, by the user'
Ui-ExpectNew ': make this face 2 mm thicker' 'with its text'
Ui-ExpectNew 'Comment C1 pin: open at ' 'its place worked out'
Ui-ExpectNew 'Comments open: 1' 'one open comment'
Ui-ExpectNew 'Comment pins drawn: C1 open' 'its pin drawn'
Ui-ExpectNew 'Comment pin C1 at ' 'the pin''s place in the view'

Write-Host '--- 2. A click on the pin opens the Comments panel'
Ui-Mark
Ui-Step 'click C1''s pin'                  { Ui-ClickLogged 'Comment pin C1' }
Ui-ExpectNew 'Comment pin clicked: C1' 'the pin took the click'
Ui-ExpectNew 'Comments window opened: 1 comment(s)' 'the Comments panel'
Ui-ExpectNew 'Comments row C1: open, user, ' 'C1''s row'
Ui-ExpectNew 'Comments selected: C1' 'C1''s row selected'
Ui-ExpectNew 'Comments shown rows: C1' 'listed by the default filter'

Write-Host '--- 3. An agent addresses C1 through the live MCP bridge'
Mcp-Start $Cli
$init = Mcp-Request 'initialize' @{ protocolVersion = '2025-11-25'; capabilities = @{}
  clientInfo = @{ name = 'mitcad-windows-comments'; version = '1' } }
if (-not $init.result.protocolVersion) { Mcp-Fail "initialize failed: $($init | ConvertTo-Json -Compress -Depth 10)" }
Mcp-Send @{ jsonrpc = '2.0'; method = 'notifications/initialized' }
$documents = Mcp-Tool 'document_list' @{}
$doc = @($documents.documents)[0].document
if (-not $doc) { Mcp-Fail 'no document from document_list' }
Ui-Mark
$null = Mcp-Tool 'model_command' @{ document = $doc; command = @{ cmd = 'address_comment'; id = 'C1'
  resolution = 'Made the face 2 mm thicker' } }
Write-Host 'ok   model_command address_comment'
Ui-ExpectNew 'Comment C1: addressed, ' 'C1 addressed'
Ui-ExpectNew 'Comment C1 resolution: Made the face 2 mm thicker' 'with the agent''s resolution'
Ui-ExpectNew 'Comment C1 pin: dimmed at ' 'its pin dimmed'
Ui-ExpectNew 'Comment pins drawn: C1 dimmed' 'drawn dimmed'
Ui-ExpectNew 'Comments row C1: addressed' 'its row addressed'
Ui-ExpectNew 'resolution: Made the face 2 mm thicker, replies: 0' 'the resolution in its row'
$script:Bridge.StandardInput.Close()
if (-not $script:Bridge.WaitForExit(30000)) { Mcp-Fail 'the bridge did not end after its stdin closed' }

Write-Host '--- 4. Resolve in the panel'
Ui-Mark
Ui-Step 'Resolve (Alt+R)'                  { Panel-Keys @('alt+r') }
Ui-ExpectNew 'Comments Resolve clicked: C1' 'Resolve clicked'
Ui-ExpectNew 'Comment command: {"cmd":"resolve_comment","id":"C1"}' 'resolve_comment sent'
Ui-ExpectNew 'Comment C1: resolved' 'C1 resolved'
Ui-ExpectNew 'Comment pins drawn: none' 'its pin gone'
Ui-ExpectNew 'Comments open: 0' 'nothing open'
Ui-ExpectNew 'Comments shown rows: none' 'left out of the default filter'
Ui-Mark
Ui-Step 'filter Resolved (Alt+S)'          { Panel-Keys @('alt+s') }
Ui-ExpectNew 'Comments filter: Open, Addressed, Resolved' 'resolved ones listed too'
Ui-ExpectNew 'Comments shown rows: C1' 'C1 listed again'
Ui-FocusMain
Ui-Activate
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                    { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Comment C1: addressed' 'undo: C1 addressed again'
Ui-ExpectNew 'Comment pins drawn: C1 dimmed' 'its pin back, dimmed'

Write-Host '--- 5. Comment Pins in the navigation bar'
Ui-Mark
Ui-Step 'Comment Pins (bar)'               { Ui-ClickLogged 'Navigation bar Comment Pins' }
Ui-ExpectNew 'Comment pins hidden' 'the pins turned off'
Ui-ExpectNew 'Comment pins drawn: hidden (1)' 'C1''s pin not drawn'
Ui-ExpectNew 'Comment Pins off (View menu off)' 'the bar''s button and the View menu unchecked'
Ui-Mark
Ui-Step 'Comment Pins (bar)'               { Ui-ClickLogged 'Navigation bar Comment Pins' }
Ui-ExpectNew 'Comment pins shown' 'the pins on again'
Ui-ExpectNew 'Comment pins drawn: C1 dimmed' 'C1''s pin drawn again'

Write-Host '--- 6. Saved and opened again in the Floating layout'
Ui-Mark
Ui-Step 'save (Ctrl+S)'                    { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'
Ui-StartApp 'comments-floating' @('--chrome=floating', '--no-mcp', '--open', $design)
Ui-Step 'fit (F6)'                         { Ui-Key 'F6' }
Ui-ExpectLog 'Comment C1: addressed, ' 'C1 addressed in the saved design'
Ui-ExpectLog 'Comment C1 resolution: Made the face 2 mm thicker' 'with its resolution'
Ui-ExpectLog 'Comment pins drawn: C1 dimmed' 'its pin drawn'
Ui-ExpectLog 'Comment pin C1 at ' 'the pin''s place'
Ui-Mark
Ui-Step 'click C1''s pin'                  { Ui-ClickLogged 'Comment pin C1' }
Ui-ExpectNew 'Comments window opened: 1 comment(s)' 'the Comments panel in the Floating layout'
Ui-ExpectNew 'Comments row C1: addressed' 'C1''s row'
Ui-ExpectNew 'Comments selected: C1' 'selected'

if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }
Ui-Finish 'UI Windows Comments test'
