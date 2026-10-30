# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowMcp.cpp app/MainWindowMcpUi.cpp app/mcp core/mcp core/ffi/src/mcp.rs tools/cli
# Live MCP for debugging (mitcad#265) in the build VM (session 0, Mesa;
# tools/ui-windows-lib.ps1): the actual application and the actual bridge,
# mitcad-cli.exe mcp --connect, over JSON-RPC on its stdin and stdout.
#   1. A design (mitcad-cli): a block with a fillet, and a sketch F4 whose
#      projection of the block's bottom face no longer matches the face.
#   2. No other socket can bind the listener's 127.0.0.1 port, not even
#      with SO_REUSEADDR (mitcad#945). The bridge connects: the catalog's
#      tools, "Agent connected", the agent indicator names the client
#      (mitcad#710).
#   3. ui_command edit F4 opens the sketch; queries with it open (sketch,
#      timeline); ui_state reports the sketch and its Sketch Palette's
#      warning with the projection's entities.
#   4. sketch.add_line into the open sketch through its session, labelled
#      "(agent)" in the undo text; another feature's command is refused,
#      and so is model_export (mitcad#750); undo takes the line back.
#   5. ui_select of a curve and a point, then ui_state's selection.
#   6. ui_screenshot of the view and of the window: PNGs under the limit.
#   7. ui_command Clean Up Stale Projections (its preview and warning), then
#      Finish Sketch.
#   8. document_save writes the design's file.
#   9. ui_log shows the commands' lines; since a sequence number.
#  10. The bridge's stdin closed: it ends, "Agent disconnected", the agent
#      indicator says no agent.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-mcp-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-mcp'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin

$projected = 'p1,p2,c3,p4,c5,p6,c7,c8'

# --- The bridge: JSON-RPC lines on the CLI's stdin and stdout.
$script:Bridge = $null
$script:BridgeError = $null
$script:BridgeId = 0

function Mcp-Fail([string]$Message) {
  if ($script:Bridge -and -not $script:Bridge.HasExited) { $script:Bridge.Kill() }
  if ($script:BridgeError) { Write-Host "     bridge stderr: $($script:BridgeError.Result)" }
  Ui-Fail $Message
}

function Mcp-Start([string]$Cli) {
  $info = New-Object System.Diagnostics.ProcessStartInfo
  $info.FileName = $Cli
  $info.Arguments = 'mcp --connect'
  $info.UseShellExecute = $false
  $info.CreateNoWindow = $true
  $info.RedirectStandardInput = $true
  $info.RedirectStandardOutput = $true
  $info.RedirectStandardError = $true
  $info.StandardOutputEncoding = New-Object System.Text.UTF8Encoding($false)
  # The process's input writer takes the console's input encoding (no
  # StandardInputEncoding in .NET Framework) and writes its preamble when it
  # is made: a UTF-8 one with a byte order mark would put a BOM before the
  # first message. UTF-8 without a BOM, for the writer and PowerShell's pipes.
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

# One line of UTF-8 without a byte order mark (the writer's encoding may add one).
function Mcp-Send($Message) {
  $json = ConvertTo-Json -InputObject $Message -Compress -Depth 30
  $bytes = (New-Object System.Text.UTF8Encoding($false)).GetBytes($json + "`n")
  $script:Bridge.StandardInput.BaseStream.Write($bytes, 0, $bytes.Length)
  $script:Bridge.StandardInput.BaseStream.Flush()
}

# Mcp-Receive id: the response to the request with that id. Empty lines and
# notifications (a method, no id) are skipped and shown; anything else
# without that id (an error without an id, another id, a line that is not a
# JSON-RPC message) fails with the whole line.
function Mcp-Receive([int]$Id) {
  for ($i = 0; $i -lt 1000; $i++) {
    $read = $script:Bridge.StandardOutput.ReadLineAsync()
    if (-not $read.Wait(60000)) { Mcp-Fail "the bridge did not answer request $Id in 60 s" }
    $line = $read.Result
    if ($null -eq $line) { Mcp-Fail "the bridge closed its stdout before answering request $Id" }
    if ($line.Trim().Length -eq 0) { Write-Host 'note: an empty line from the bridge'; continue }
    $message = $null
    try { $message = $line | ConvertFrom-Json } catch { Mcp-Fail "the bridge sent a line that is not JSON: $line" }
    if ($message.jsonrpc -ne '2.0') { Mcp-Fail "the bridge sent a line that is not JSON-RPC 2.0: $line" }
    $hasId = $null -ne $message.PSObject.Properties['id']
    if (-not $hasId -and $null -ne $message.PSObject.Properties['method']) {
      Write-Host "note: a notification from the bridge: $line"
      continue
    }
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

# Mcp-Tool name arguments [-IsError]: the tool's structured result.
function Mcp-Tool([string]$Name, [hashtable]$Arguments, [switch]$IsError) {
  $response = Mcp-Request 'tools/call' @{ name = $Name; arguments = $Arguments }
  if ($response.error) { Mcp-Fail "${Name}: protocol error $($response.error.message)" }
  $failed = [bool]$response.result.isError
  if ($failed -ne [bool]$IsError) {
    $text = $response.result.content[0].text
    if ($text.Length -gt 2000) { $text = $text.Substring(0, 2000) }
    Mcp-Fail "${Name}: unexpected result: $text"
  }
  return $response.result.structuredContent
}

function Mcp-Check([bool]$Condition, [string]$Description, [string]$Detail = '') {
  if (-not $Condition) { Mcp-Fail "${Description}: $Detail" }
  Write-Host "ok   $Description"
}

Write-Host '--- A design whose sketch F4 has a projection that no longer follows'
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$script = Join-Path $work 'stale.json'
$design = Join-Path $work 'stale.mitcad'
Set-Content -Encoding ascii $script (@'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
    "edges": ["E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"], "radius": 5}},
  {"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy",
    "entities": [
      {"id": "p1", "type": "point", "at": [-10, 0], "fixed": true, "reference": true},
      {"id": "p2", "type": "point", "at": [60, 0], "fixed": true, "reference": true},
      {"id": "c3", "type": "line", "start": "p1", "end": "p2", "fixed": true, "reference": true},
      {"id": "p4", "type": "point", "at": [60, 40], "fixed": true, "reference": true},
      {"id": "c5", "type": "line", "start": "p2", "end": "p4", "fixed": true, "reference": true},
      {"id": "p6", "type": "point", "at": [-10, 40], "fixed": true, "reference": true},
      {"id": "c7", "type": "line", "start": "p4", "end": "p6", "fixed": true, "reference": true},
      {"id": "c8", "type": "line", "start": "p6", "end": "p1", "fixed": true, "reference": true},
      {"id": "p9", "type": "point", "at": [20, 0]},
      {"id": "p10", "type": "point", "at": [20, 40]},
      {"id": "c11", "type": "line", "start": "p9", "end": "p10"}],
    "constraints": [
      {"id": "k12", "type": "coincident", "point": "p9", "entity": "c3"},
      {"id": "k13", "type": "coincident", "point": "p10", "entity": "c7"}],
    "projections": [{"source": "F2:start(RECT)",
                     "entities": ["p1", "p2", "c3", "p4", "c5", "p6", "c7", "c8"]}]}},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F4", "region": "r{c3[c11,c5],c5[c3,c7],c7[c5,c11],c11[c3,c7]}"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}}
]
'@ -creplace 'RECT', 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}')
$made = & $cli run $script --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# The listener's record in a folder of the test's own, which the app makes
# private (it must not exist yet); the bridge reads the same.
$env:MITCAD_MCP_DISCOVERY_DIR = Join-Path $script:UiOut 'discovery'
Remove-Item -Recurse -Force $env:MITCAD_MCP_DISCOVERY_DIR -ErrorAction SilentlyContinue

Ui-StartApp 'mcp' @('--open', $design)
Ui-ExpectLog 'MCP live instance:' "the application's MCP listener is up"
Ui-ExpectLog 'Agent indicator: no agent' 'the agent indicator says the window accepts agents'
Ui-ExpectLog 'stale.mitcad' 'the design opened'

# The listener answers an authenticated probe, the same discovery the
# bridge makes, before the bridge starts: under load it may take a while.
$listed = $null
for ($i = 0; $i -lt 150; $i++) {
  # Stderr lines become errors that 'Stop' throws in Windows PowerShell.
  try {
    $listed = & $cli mcp --list-instances 2>$null
    if ($LASTEXITCODE -eq 0 -and @(($listed | ConvertFrom-Json).instances).Count -eq 1) { break }
  } catch {
    Write-Host "note: --list-instances: $_"
  }
  $listed = $null
  Start-Sleep -Milliseconds 200
}
if ($null -eq $listed) { Ui-Fail 'mitcad-cli mcp --list-instances did not list the application in 30 s' }
Write-Host 'ok   the listener answers'

Write-Host '--- No other socket can bind the endpoint (mitcad#945)'
$port = [int]@(((@($listed) -join "`n") | ConvertFrom-Json).instances)[0].port
if ($port -le 0) { Ui-Fail "mitcad-cli mcp --list-instances gave no port: $($listed -join ' ')" }
# The listener's SO_EXCLUSIVEADDRUSE refuses a second bind to its address
# and port also with SO_REUSEADDR, which a plain listener would let through.
foreach ($reuse in @($false, $true)) {
  $other = New-Object System.Net.Sockets.Socket([System.Net.Sockets.AddressFamily]::InterNetwork,
    [System.Net.Sockets.SocketType]::Stream, [System.Net.Sockets.ProtocolType]::Tcp)
  $bound = $false
  try {
    if ($reuse) {
      $other.SetSocketOption([System.Net.Sockets.SocketOptionLevel]::Socket,
        [System.Net.Sockets.SocketOptionName]::ReuseAddress, $true)
    }
    $other.Bind((New-Object System.Net.IPEndPoint([System.Net.IPAddress]::Loopback, $port)))
    $bound = $true
  } catch {
    Write-Host "note: the second bind (SO_REUSEADDR $reuse) was refused: $($_.Exception.Message)"
  } finally {
    $other.Close()
  }
  if ($bound) { Ui-Fail "another socket bound 127.0.0.1:$port (SO_REUSEADDR $reuse) while MitCad listens" }
  Write-Host "ok   another socket cannot bind 127.0.0.1:$port (SO_REUSEADDR $reuse)"
}

Write-Host '--- The bridge connects'
Ui-Mark
Mcp-Start $cli
$init = Mcp-Request 'initialize' @{ protocolVersion = '2025-11-25'; capabilities = @{}
  clientInfo = @{ name = 'mitcad-windows-test'; version = '1' } }
Mcp-Check ($init.result.protocolVersion -eq '2025-11-25') 'initialized' ($init | ConvertTo-Json -Compress -Depth 10)
Mcp-Send @{ jsonrpc = '2.0'; method = 'notifications/initialized' }
$tools = @((Mcp-Request 'tools/list').result.tools | ForEach-Object { $_.name }) | Sort-Object
$expected = @('document_list', 'document_open', 'document_save', 'model_command', 'model_export', 'model_query', 'model_reference', 'ui_camera',
  'ui_command', 'ui_log', 'ui_screenshot', 'ui_select', 'ui_state') | Sort-Object
Mcp-Check (($tools -join ',') -eq ($expected -join ',')) 'the live catalog has the debugging tools' ($tools -join ',')
$document = @((Mcp-Tool 'document_list' @{}).documents)[0].document
Ui-ExpectNew 'Agent connected' 'the window says an agent is connected'
Ui-ExpectNew 'Agent indicator: connected: mitcad-windows-test' 'the agent indicator names the client'

Write-Host '--- The sketch opened, queries with it open, ui_state'
$edited = Mcp-Tool 'ui_command' @{ document = $document; edit = 'F4' }
Mcp-Check ($edited.mode -eq 'sketch' -and $edited.sketch -eq 'F4') 'ui_command edit opens the sketch F4' ($edited | ConvertTo-Json -Compress -Depth 10)
Ui-ExpectLog 'Editing sketch F4 (Sketch2)' 'the window edits the sketch'

function Sketch-Query {
  return (Mcp-Tool 'model_query' @{ document = $document; query = @{ query = 'sketch'; uid = 'F4' } }).result
}
$sketch = Sketch-Query
$about = @($sketch.warning_entities)
Mcp-Check ($about.Count -eq 1 -and (@($about[0]) -join ',') -eq $projected) 'the sketch query works with the sketch open' `
  ($sketch.warning_entities | ConvertTo-Json -Compress)
$timeline = (Mcp-Tool 'model_query' @{ document = $document; query = @{ query = 'timeline' } }).result
Mcp-Check (@($timeline.features).Count -eq 5) 'the timeline query works with the sketch open' "$(@($timeline.features).Count) features"
$state = Mcp-Tool 'ui_state' @{}
$warnings = @($state.sketch.palette.warnings)
Mcp-Check ($state.mode -eq 'sketch' -and $state.sketch.uid -eq 'F4') 'ui_state reports the sketch' ($state.sketch | ConvertTo-Json -Compress -Depth 10)
Mcp-Check ($warnings.Count -eq 1 -and (@($warnings[0].entities) -join ',') -eq $projected -and
  $warnings[0].text.Contains('does not follow')) "ui_state reports the palette's warning with its entities" ($warnings | ConvertTo-Json -Compress -Depth 10)
Mcp-Check ($state.agents -ge 1 -and $state.app.version) 'ui_state reports the agent and the version' ($state.app | ConvertTo-Json -Compress)

Write-Host '--- A line into the open sketch, through its session, then undo'
$before = @($sketch.entities).Count
Ui-Mark
$added = Mcp-Tool 'model_command' @{ document = $document; command = @{ cmd = 'sketch.add_line'; sketch = 'F4'
  start = @(100, 100); end = @(130, 100) } }
Mcp-Check ($added.applied -and $added.sketch -eq 'F4') 'sketch.add_line goes into the open sketch'
Mcp-Check (@((Sketch-Query).entities).Count -eq $before + 3) 'the line and its points are in the sketch'
$state = Mcp-Tool 'ui_state' @{}
Mcp-Check ($state.undo -and $state.undo.EndsWith(' (agent)')) "the agent's change says so in the undo text" $state.undo
Ui-ExpectNew 'MCP: an agent ran sketch.add_line in sketch F4' 'the log says what the agent ran'
$refused = Mcp-Tool 'model_command' @{ document = $document; command = @{ cmd = 'add_parameter'; name = 'agent_width'; value = 10 } } -IsError
Mcp-Check ($refused.error.Contains('being edited')) "another feature's command waits for the sketch" $refused.error
# F5 after the sketch is rolled back while it is edited (mitcad#750).
$refused = Mcp-Tool 'model_export' @{ document = $document; path = 'edited.stl'; format = 'stl' } -IsError
Mcp-Check ($refused.error.Contains('Finish the sketch')) 'model_export waits for the sketch' $refused.error
$null = Mcp-Tool 'model_command' @{ document = $document; command = @{ cmd = 'undo' } }
Mcp-Check (@((Sketch-Query).entities).Count -eq $before) 'undo takes the line back'

Write-Host '--- ui_select, then ui_state'
$selected = Mcp-Tool 'ui_select' @{ document = $document; items = @(
    @{ kind = 'sketch_curve'; owner = 'F4'; name = 'c3' },
    @{ kind = 'sketch_point'; owner = 'F4'; name = 'p1' }) }
Mcp-Check ($selected.selected -eq 2) 'ui_select selects a curve and a point'
$shown = @((Mcp-Tool 'ui_state' @{}).selection | ForEach-Object { "$($_.kind) $($_.owner) $($_.name)" }) | Sort-Object
Mcp-Check (($shown -join ';') -eq 'sketch_curve F4 c3;sketch_point F4 p1') 'ui_state shows the selection' ($shown -join ';')
Ui-ExpectLog 'Selected: 1 sketch curve, 1 sketch point' 'the window selected them'

Write-Host '--- ui_screenshot'
foreach ($whole in @($false, $true)) {
  # The image is an MCP image block after the text (mitcad#689).
  $response = Mcp-Request 'tools/call' @{ name = 'ui_screenshot'; arguments = @{ window = $whole; max_size = 400 } }
  if ($response.error -or $response.result.isError) { Mcp-Fail "ui_screenshot: $($response | ConvertTo-Json -Compress -Depth 5)" }
  $shot = $response.result.structuredContent
  $image = @($response.result.content | Where-Object { $_.type -eq 'image' })[0]
  $png = [Convert]::FromBase64String($image.data)
  $signature = ($png[0..7] | ForEach-Object { $_.ToString('X2') }) -join ''
  Mcp-Check ($signature -eq '89504E470D0A1A0A' -and $png.Length -eq $shot.bytes -and $png.Length -lt 8MB -and
    [Math]::Max($shot.width, $shot.height) -le 400) "ui_screenshot (window $whole) is a PNG under the limit" `
    "$signature, $($png.Length) bytes, $($shot.width) x $($shot.height)"
}

Write-Host '--- Clean Up Stale Projections and Finish Sketch through ui_command'
$cleaned = Mcp-Tool 'ui_command' @{ document = $document; id = 'sketch.clean_up_stale_projections' }
Mcp-Check ($cleaned.committed -eq $true) 'ui_command ran Clean Up Stale Projections' ($cleaned | ConvertTo-Json -Compress -Depth 10)
Mcp-Check (@($cleaned.previews | Where-Object { $_.StartsWith('Preview Clean Up Stale Projections: ok, warning: 1 stale curve removed: c8') }).Count -ge 1) `
  'the result has the preview and its warning' (@($cleaned.previews) -join ' / ')
Mcp-Check (@($cleaned.log | Where-Object { $_.Contains('Cleaned up stale projections') }).Count -ge 1) 'the result has the log' (@($cleaned.log) -join ' / ')
Mcp-Check ($cleaned.undo.EndsWith(' (agent)')) "the clean-up is the agent's in the undo text" $cleaned.undo
$finished = Mcp-Tool 'ui_command' @{ document = $document; id = 'sketch.finish' }
Mcp-Check ($finished.mode -eq 'idle') 'ui_command finished the sketch' $finished.mode
Ui-ExpectLog 'Sketch finished' 'the window finished the sketch'

Write-Host '--- document_save'
$written = (Get-Item $design).LastWriteTimeUtc
Start-Sleep -Milliseconds 100
Ui-Mark
$saved = Mcp-Tool 'document_save' @{ document = $document }
Mcp-Check ($saved.saved -eq $true -and (Get-Item $design).LastWriteTimeUtc -gt $written) 'document_save writes the file' ($saved | ConvertTo-Json -Compress -Depth 5)
Mcp-Check ((Mcp-Tool 'ui_state' @{}).modified -eq $false) 'the document is saved'
Ui-ExpectNew ('Saved ' + $design.Replace('\', '/')) 'the window saved it'

Write-Host '--- ui_log'
$log = Mcp-Tool 'ui_log' @{ limit = 2000 }
$texts = @($log.lines | ForEach-Object { $_.text })
foreach ($line in @('MCP: an agent ran sketch.add_line in sketch F4', 'Cleaned up stale projections', 'MCP: an agent saved')) {
  Mcp-Check (@($texts | Where-Object { $_.Contains($line) }).Count -ge 1) "ui_log shows '$line'"
}
$last = @($log.lines)[-1].seq
$later = Mcp-Tool 'ui_log' @{ since = $last }
Mcp-Check (@($later.lines | Where-Object { $_.seq -le $last }).Count -eq 0) "ui_log's since leaves out the older lines"

Write-Host '--- The bridge ends'
Ui-Mark
$script:Bridge.StandardInput.Close()
if (-not $script:Bridge.WaitForExit(30000)) { Mcp-Fail 'the bridge did not end after its stdin closed' }
Mcp-Check ($script:Bridge.ExitCode -eq 0) 'the bridge ended' "exit code $($script:Bridge.ExitCode): $($script:BridgeError.Result)"
Ui-ExpectNew 'Agent disconnected' 'the window says the agent disconnected'
Ui-ExpectNew 'Agent indicator: no agent' 'the agent indicator says no agent again'

Ui-Finish 'UI Windows live MCP test'
