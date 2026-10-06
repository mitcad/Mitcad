# SPDX-License-Identifier: MIT
# Shared helpers for the input-driven Windows UI tests in the build VM.
# Dot-source this file; it is not a script. The helpers mirror
# tools/ui-test-lib.sh (the Linux tests' xdotool helpers) and read the same
# log lines the app writes for tests ("View area ...", "Panel ... at x,y").
#
# The tests run over SSH, in session 0, whose window station has no input
# desktop: SendInput is refused (access denied), GetCursorPos fails, and UI
# Automation lists no windows there (they all report invisible). So the
# helpers post the mouse and keyboard messages a real device would cause
# (WM_MOUSEMOVE, WM_LBUTTONDOWN, WM_KEYDOWN, WM_CHAR) to the app's window.
# They go through Qt's Windows platform plugin like real input: Qt
# translates them into mouse and key events, and modifiers come from the
# keyboard state the helpers set while attached to the app's input queue
# (AttachThreadInput, SetKeyboardState).
#
# Limits in session 0: Qt cannot grab the mouse for a popup in a window
# station without a visible desktop, so popups close as soon as they open:
# the command search (S), menus (Alt+F) and the lists of drop-downs. The
# tests therefore start commands with shortcuts, which they assign in
# settings of their own (Ui-Init -Shortcuts, as Tools > Keyboard Shortcuts
# would), and type into dialogs, which are windows of their own. Qt never
# exposes the window there either, so the app draws the 3D view's frames
# directly (OcctViewer::requestFrame), and the view's picks work. Not
# covered: the system's input queue and the real cursor. The Linux tests
# (tools/ui-*.sh, real X input on Xvfb) cover the popups.

$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;

public static class UiInput {
    public delegate bool EnumProc(IntPtr hwnd, IntPtr lparam);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc proc, IntPtr lparam);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int max);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr hwnd, StringBuilder text, int max);
    [DllImport("user32.dll")] static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] static extern uint MapVirtualKey(uint code, uint type);
    [DllImport("user32.dll")] static extern short VkKeyScan(char ch);
    [DllImport("user32.dll")] static extern bool AttachThreadInput(uint attach, uint to, bool on);
    [DllImport("user32.dll")] static extern bool GetKeyboardState(byte[] state);
    [DllImport("user32.dll")] static extern bool SetKeyboardState(byte[] state);
    [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();

    const uint WM_KEYDOWN = 0x100, WM_KEYUP = 0x101, WM_SYSKEYDOWN = 0x104, WM_SYSKEYUP = 0x105;
    const uint WM_MOUSEMOVE = 0x200;
    const int VK_SHIFT = 0x10, VK_CONTROL = 0x11, VK_MENU = 0x12;
    const int VK_LSHIFT = 0xA0, VK_LCONTROL = 0xA2, VK_LMENU = 0xA4;

    // Top-level windows of a process whose title matches the pattern, in
    // Z order (the newest dialog first). Windows of session 0 report
    // invisible, so visibility is not asked.
    public static IntPtr[] Windows(int pid, string titlePattern) {
        var found = new List<IntPtr>();
        var regex = new Regex(titlePattern);
        EnumWindows((h, l) => {
            uint owner;
            GetWindowThreadProcessId(h, out owner);
            if (owner == (uint)pid) {
                var title = new StringBuilder(512);
                GetWindowText(h, title, 512);
                var cls = new StringBuilder(256);
                GetClassName(h, cls, 256);
                if (cls.ToString().StartsWith("Qt") && regex.IsMatch(title.ToString())) found.Add(h);
            }
            return true;
        }, IntPtr.Zero);
        return found.ToArray();
    }

    public static string Title(IntPtr hwnd) {
        var title = new StringBuilder(512);
        GetWindowText(hwnd, title, 512);
        return title.ToString();
    }

    static uint ThreadOf(IntPtr hwnd) {
        uint pid;
        return GetWindowThreadProcessId(hwnd, out pid);
    }

    // The modifiers Qt reads with GetKeyState while it handles the next
    // messages: the keyboard state of the input queue this thread shares
    // with the app's GUI thread.
    static void SetModifiers(IntPtr hwnd, bool ctrl, bool shift, bool alt) {
        AttachThreadInput(GetCurrentThreadId(), ThreadOf(hwnd), true);
        var state = new byte[256];
        GetKeyboardState(state);
        state[VK_CONTROL] = state[VK_LCONTROL] = (byte)(ctrl ? 0x80 : 0);
        state[VK_SHIFT] = state[VK_LSHIFT] = (byte)(shift ? 0x80 : 0);
        state[VK_MENU] = state[VK_LMENU] = (byte)(alt ? 0x80 : 0);
        SetKeyboardState(state);
    }

    static void ReleaseModifiers(IntPtr hwnd) {
        SetModifiers(hwnd, false, false, false);
        AttachThreadInput(GetCurrentThreadId(), ThreadOf(hwnd), false);
    }

    static bool Extended(int vk) {
        // Arrows, Insert, Delete, Home, End, Page Up and Down.
        return (vk >= 0x21 && vk <= 0x28) || vk == 0x2D || vk == 0x2E;
    }

    static void PostKey(IntPtr hwnd, int vk, bool alt) {
        uint scan = MapVirtualKey((uint)vk, 0);
        uint lparam = 1 | (scan << 16) | (Extended(vk) ? 1u << 24 : 0) | (alt ? 1u << 29 : 0);
        PostMessage(hwnd, alt ? WM_SYSKEYDOWN : WM_KEYDOWN, (IntPtr)vk, (IntPtr)(int)lparam);
        PostMessage(hwnd, alt ? WM_SYSKEYUP : WM_KEYUP, (IntPtr)vk,
                    (IntPtr)unchecked((int)(lparam | 0xC0000000u)));
    }

    // A key press and release with modifiers held. TranslateMessage in Qt's
    // event loop makes the character of the key down.
    public static void Key(IntPtr hwnd, int vk, bool ctrl, bool shift, bool alt) {
        bool modified = ctrl || shift || alt;
        if (modified) SetModifiers(hwnd, ctrl, shift, alt);
        PostKey(hwnd, vk, alt);
        if (modified) {
            // The state must hold until the app has taken the messages.
            Thread.Sleep(120);
            ReleaseModifiers(hwnd);
        }
    }

    const uint WM_CHAR = 0x102;

    // Text as characters (WM_CHAR, which Qt turns into key events with the
    // text), so that no modifiers are needed.
    public static void Type(IntPtr hwnd, string text, int delay) {
        foreach (char ch in text) {
            short scan = VkKeyScan(ch);
            uint code = scan == -1 ? 0 : MapVirtualKey((uint)(scan & 0xFF), 0);
            PostMessage(hwnd, WM_CHAR, (IntPtr)ch, (IntPtr)(int)(1 | (code << 16)));
            Thread.Sleep(delay);
        }
    }

    public static string Describe(int pid) {
        var text = new StringBuilder();
        EnumWindows((h, l) => {
            uint owner;
            GetWindowThreadProcessId(h, out owner);
            if (owner == (uint)pid) {
                var title = new StringBuilder(512);
                GetWindowText(h, title, 512);
                var cls = new StringBuilder(256);
                GetClassName(h, cls, 256);
                text.AppendLine(h.ToInt64() + " " + cls + " '" + title + "'");
            }
            return true;
        }, IntPtr.Zero);
        return text.ToString();
    }

    static IntPtr Point(int x, int y) { return (IntPtr)((y << 16) | (x & 0xFFFF)); }

    public static void Move(IntPtr hwnd, int x, int y, int keys) {
        PostMessage(hwnd, WM_MOUSEMOVE, (IntPtr)keys, Point(x, y));
    }

    // A button's messages: down (WM_xBUTTONDOWN), up, double-click.
    public static void Button(IntPtr hwnd, uint message, int x, int y, int keys) {
        PostMessage(hwnd, message, (IntPtr)keys, Point(x, y));
    }
}
'@

$script:UiApp = $null
$script:UiOut = $null
$script:UiLog = $null
$script:UiProcess = $null
$script:UiWindow = [IntPtr]::Zero
$script:UiTarget = [IntPtr]::Zero
$script:UiMark = 0
$script:UiSettings = $null
$script:UiAutosave = $null
$script:UiResults = $null
$script:UiThumbnails = $null

# Ui-Init app out [qtBin] [shortcuts]: where the app, its Qt and the test's
# files are; shortcuts (@{'sketch.create' = 'F7'}) go into the test's own
# settings, which the app reads from MITCAD_SETTINGS_DIR.
function Ui-Init([string]$App, [string]$Out, [string]$QtBin = '', [hashtable]$Shortcuts = @{}) {
  $script:UiApp = (Resolve-Path $App).Path
  New-Item -ItemType Directory -Force $Out | Out-Null
  $script:UiOut = (Resolve-Path $Out).Path
  if (-not $QtBin) {
    $cache = Join-Path (Split-Path $script:UiApp) 'CMakeCache.txt'
    $qtDir = if (Test-Path $cache) { Select-String -Path $cache -Pattern '^Qt6_DIR:PATH=(.*)' } else { $null }
    if ($qtDir) { $QtBin = Join-Path $qtDir.Matches[0].Groups[1].Value '..\..\..\bin' }
  }
  if (-not $QtBin -or -not (Test-Path (Join-Path $QtBin 'Qt6Core.dll'))) {
    Write-Host "FAIL: Qt's bin directory not found; pass -QtBin"
    exit 1
  }
  $env:Path = (Resolve-Path $QtBin).Path + ';' + $env:Path
  if (-not (Test-Path (Join-Path (Split-Path $script:UiApp) 'opengl32.dll'))) {
    Write-Host "FAIL: no opengl32.dll next to $($script:UiApp); configure with -DMITCAD_MESA_DIR=C:/dev/mesa/bin"
    exit 1
  }
  # Settings of the test's own (shortcuts, recent files, preferences).
  $script:UiSettings = Join-Path $script:UiOut 'settings'
  Remove-Item -Recurse -Force $script:UiSettings -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force (Join-Path $script:UiSettings 'Mitcad') | Out-Null
  $ini = @('[shortcuts]') + ($Shortcuts.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" })
  Set-Content -Encoding ascii (Join-Path $script:UiSettings 'Mitcad\Mitcad.ini') $ini
  # Autosave's recovery folder of the test's own: a killed app leaves its
  # files there, which neither the user's sessions nor other tests may see.
  $script:UiAutosave = Join-Path $script:UiOut 'autosave'
  Remove-Item -Recurse -Force $script:UiAutosave -ErrorAction SilentlyContinue
  # The store of computed results of the test's own (P7d), empty at the start.
  $script:UiResults = Join-Path $script:UiOut 'results'
  Remove-Item -Recurse -Force $script:UiResults -ErrorAction SilentlyContinue
  # Versions' previews of the test's own (P12e), not the user's local data.
  $script:UiThumbnails = Join-Path $script:UiOut 'thumbnails'
  Remove-Item -Recurse -Force $script:UiThumbnails -ErrorAction SilentlyContinue
}

# Ui-StartApp name arguments... [-Recovery] [-Updates]: starts Mitcad with
# its log in <out>\<name>.log and waits for the main window ("<document> -
# Mitcad"). The app offers no recovery of autosaved work at start-up
# (--no-recovery: a test that killed it and starts it again would get the
# question) unless -Recovery is given, and checks for no updates unless
# -Updates is given (a test with a server of its own: MITCAD_UPDATE_URL).
function Ui-StartApp([string]$Name, [string[]]$Arguments = @(), [switch]$Recovery, [switch]$Updates) {
  Ui-StopApp
  $script:UiLog = Join-Path $script:UiOut "$Name.log"
  Remove-Item -Force $script:UiLog -ErrorAction SilentlyContinue
  $env:QT_FORCE_STDERR_LOGGING = '1'
  # Where faces, edges and vertices can be clicked ("Pick face ... at x,y").
  $env:MITCAD_LOG_PICKS = '1'
  # The bodies' volumes before and after each change ("Body ... -> b mm3").
  $env:MITCAD_LOG_VOLUMES = '1'
  # Settings in an INI file of the test's own, not the user's registry.
  $env:MITCAD_SETTINGS_DIR = $script:UiSettings
  # No update checks (mitcad#9): the tests make no network requests.
  if ($Updates) { Remove-Item Env:MITCAD_NO_UPDATE_CHECK -ErrorAction SilentlyContinue }
  else { $env:MITCAD_NO_UPDATE_CHECK = '1' }
  $env:MITCAD_AUTOSAVE_DIR = $script:UiAutosave
  $env:MITCAD_THUMBNAIL_DIR = $script:UiThumbnails
  # The store of computed results (P7d) is off unless a test turns it on
  # (Ui-ResultStore), as in the Linux tests.
  if (-not $env:MITCAD_RESULT_STORE) { $env:MITCAD_RESULT_STORE = 'off' }
  # The app writes its log into the file as it goes (stderr is unbuffered),
  # so the helpers read it meanwhile.
  $options = @('--no-native-dialogs')
  if (-not $Recovery) { $options += '--no-recovery' }
  $arguments = ($options + $Arguments | ForEach-Object { '"' + $_ + '"' }) -join ' '
  $script:UiProcess = Start-Process -FilePath $script:UiApp -ArgumentList $arguments -PassThru `
    -RedirectStandardError $script:UiLog -RedirectStandardOutput (Join-Path $script:UiOut "$Name.out")
  $script:UiMark = 0
  for ($i = 0; $i -lt 100; $i++) {
    $windows = [UiInput]::Windows($script:UiProcess.Id, ' - Mitcad\*?$')
    if ($windows.Count -gt 0) { break }
    Start-Sleep -Milliseconds 200
  }
  if ($windows.Count -eq 0) { Ui-Fail 'the Mitcad window did not appear' }
  $script:UiWindow = $windows[0]
  $script:UiTarget = $script:UiWindow
  # The view logs its area once it is laid out and drawn.
  Ui-ExpectLog 'View area' 'the 3D view is up' | Out-Null
  Start-Sleep -Milliseconds 1500
}

function Ui-StopApp {
  if ($script:UiProcess) {
    if (-not $script:UiProcess.HasExited) {
      $script:UiProcess.Kill()
      $script:UiProcess.WaitForExit(10000) | Out-Null
    }
    Start-Sleep -Milliseconds 300
    $script:UiProcess = $null
  }
}

function Ui-LogLines {
  if (-not $script:UiLog -or -not (Test-Path $script:UiLog)) { return @() }
  $stream = [System.IO.File]::Open($script:UiLog, 'Open', 'Read', 'ReadWrite')
  try { $reader = New-Object System.IO.StreamReader $stream; $text = $reader.ReadToEnd() } finally { $stream.Dispose() }
  return $text -split "`r?`n"
}

function Ui-Fail([string]$Message) {
  Write-Host "FAIL: $Message"
  if ($script:UiProcess -and -not $script:UiProcess.HasExited) {
    Write-Host '     windows:'
    [UiInput]::Describe($script:UiProcess.Id) -split "`r?`n" | Where-Object { $_ } | ForEach-Object { Write-Host "       $_" }
  }
  Ui-LogLines | Select-Object -Last 40 | ForEach-Object { Write-Host "     $_" }
  Ui-StopApp
  exit 1
}

function Ui-Crashed { return $script:UiProcess.HasExited }

# Ui-WaitIdle: waits (up to 60 s) until the app computes nothing: every job
# that logged "Computing <label> started" (one that took over 50 ms; P7)
# has logged its end. While a job's progress dialog shows, the window takes
# no input, so UI steps wait first, as tools/ui-test-lib.sh does.
function Ui-WaitIdle {
  for ($i = 0; $i -lt 300; $i++) {
    $lines = Ui-LogLines
    $started = @($lines | Where-Object { $_ -match 'Computing .* started$' }).Count
    $ended = @($lines | Where-Object { $_ -match 'Computing .* (done in|cancelled after) [0-9]+ ms' }).Count
    if ($started -le $ended) { return }
    Start-Sleep -Milliseconds 200
  }
  Write-Host 'note: the app was still computing after 60 s'
}

# Ui-Step "description" { action }: runs a UI action (once the app computes
# nothing) and checks for crashes.
function Ui-Step([string]$Name, [scriptblock]$Action) {
  Ui-WaitIdle
  & $Action
  Start-Sleep -Milliseconds 700
  if (Ui-Crashed) { Ui-Fail "crashed during '$Name' (exit code $($script:UiProcess.ExitCode))" }
  Write-Host "ok   $Name"
}

# Ui-ExpectLog "text" "description": waits for a line in the log.
function Ui-ExpectLog([string]$Text, [string]$Description, [int]$Seconds = 6) {
  for ($i = 0; $i -lt $Seconds * 5; $i++) {
    if ((Ui-LogLines) | Where-Object { $_.Contains($Text) }) { Write-Host "ok   $Description"; return }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "${Description}: '$Text' not in the log"
}

# Ui-Mark; ...; Ui-ExpectNew "text" "description" [seconds]: only lines
# logged since the mark count.
function Ui-Mark { $script:UiMark = (Ui-LogLines).Count - 1 }
function Ui-ExpectNew([string]$Text, [string]$Description, [int]$Seconds = 6) {
  for ($i = 0; $i -lt $Seconds * 5; $i++) {
    $lines = Ui-LogLines | Select-Object -Skip $script:UiMark
    if ($lines | Where-Object { $_.Contains($Text) }) { Write-Host "ok   $Description"; return }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "${Description}: '$Text' not in the log since the mark"
}

# Ui-LastMatch "regex": the first group of the last log line matching it.
function Ui-LastMatch([string]$Pattern) {
  $match = Ui-LogLines | Select-String -Pattern $Pattern | Select-Object -Last 1
  if ($match) { return $match.Matches[0].Groups[1].Value }
  return $null
}

# Keys by name, as xdotool names them in the Linux tests.
$script:UiKeys = @{
  'Return' = 0x0D; 'Escape' = 0x1B; 'Tab' = 0x09; 'BackSpace' = 0x08; 'Delete' = 0x2E; 'space' = 0x20
  'Up' = 0x26; 'Down' = 0x28; 'Left' = 0x25; 'Right' = 0x27; 'Home' = 0x24; 'End' = 0x23
  'F2' = 0x71; 'F4' = 0x73; 'F6' = 0x75; 'F7' = 0x76; 'F8' = 0x77; 'F9' = 0x78; 'F11' = 0x7A; 'F12' = 0x7B
}

# Ui-Key "ctrl+z" ["Return" ...]: key presses with modifiers, to the window
# that has the keyboard (Ui-FocusDialog, Ui-FocusMain).
function Ui-Key([string[]]$Keys) {
  foreach ($spec in $Keys) {
    $parts = $spec -split '\+'
    $name = $parts[-1]
    $mods = $parts[0..($parts.Count - 2)] | Where-Object { $_ }
    if ($script:UiKeys.ContainsKey($name)) { $vk = $script:UiKeys[$name] }
    elseif ($name.Length -eq 1) { $vk = [int][char]$name.ToUpperInvariant() }
    else { throw "unknown key '$name'" }
    [UiInput]::Key($script:UiTarget, $vk, ($mods -contains 'ctrl'), ($mods -contains 'shift'), ($mods -contains 'alt'))
    Start-Sleep -Milliseconds 60
  }
}

function Ui-Type([string]$Text) { [UiInput]::Type($script:UiTarget, $Text, 40) }

# Mouse buttons: messages for down, up and double-click, and the key state.
$script:UiButtons = @{ 1 = @(0x201, 0x202, 0x203, 0x01); 2 = @(0x207, 0x208, 0x209, 0x10); 3 = @(0x204, 0x205, 0x206, 0x02) }

# Ui-Click x y [button]: a click at a point of the main window (the window
# coordinates the app logs).
function Ui-Click([int]$X, [int]$Y, [int]$Button = 1) {
  $b = $script:UiButtons[$Button]
  [UiInput]::Move($script:UiWindow, $X, $Y, 0)
  Start-Sleep -Milliseconds 80
  [UiInput]::Button($script:UiWindow, $b[0], $X, $Y, $b[3])
  Start-Sleep -Milliseconds 60
  [UiInput]::Button($script:UiWindow, $b[1], $X, $Y, 0)
}

function Ui-DoubleClick([int]$X, [int]$Y) {
  Ui-Click $X $Y
  Start-Sleep -Milliseconds 80
  [UiInput]::Button($script:UiWindow, 0x203, $X, $Y, 1)
  Start-Sleep -Milliseconds 60
  [UiInput]::Button($script:UiWindow, 0x202, $X, $Y, 0)
}

# Ui-Drag x1 y1 x2 y2 [button]: a drag in steps.
function Ui-Drag([int]$X1, [int]$Y1, [int]$X2, [int]$Y2, [int]$Button = 1) {
  $b = $script:UiButtons[$Button]
  [UiInput]::Move($script:UiWindow, $X1, $Y1, 0)
  Start-Sleep -Milliseconds 80
  [UiInput]::Button($script:UiWindow, $b[0], $X1, $Y1, $b[3])
  for ($i = 1; $i -le 8; $i++) {
    Start-Sleep -Milliseconds 80
    [UiInput]::Move($script:UiWindow, [int]($X1 + ($X2 - $X1) * $i / 8), [int]($Y1 + ($Y2 - $Y1) * $i / 8), $b[3])
  }
  Start-Sleep -Milliseconds 300
  [UiInput]::Button($script:UiWindow, $b[1], $X2, $Y2, 0)
}

# Ui-ViewAt x% y%: a point of the 3D view in percent of its size.
function Ui-ViewAt([double]$Px, [double]$Py) {
  $area = Ui-LastMatch 'View area (\d+ \d+ \d+ \d+)'
  if (-not $area) { Ui-Fail 'the app did not log its view area' }
  $v = $area -split ' ' | ForEach-Object { [int]$_ }
  return @([int]($v[0] + $v[2] * $Px / 100), [int]($v[1] + $v[3] * $Py / 100))
}

function Ui-ViewClick([double]$Px, [double]$Py, [int]$Button = 1) {
  $at = Ui-ViewAt $Px $Py
  Ui-Click $at[0] $at[1] $Button
}

# Ui-LoggedAt "text": the last "<text> at x,y" place.
function Ui-LoggedAt([string]$Text) {
  $line = Ui-LogLines | Where-Object { $_.Contains("$Text at ") } | Select-Object -Last 1
  if ($line -notmatch ' at (\d+),(\d+)') { Ui-Fail "no place logged for '$Text'" }
  return @([int]$Matches[1], [int]$Matches[2])
}

function Ui-ClickLogged([string]$Text, [int]$Button = 1) {
  $at = Ui-LoggedAt $Text
  Ui-Click $at[0] $at[1] $Button
}

function Ui-DoubleClickLogged([string]$Text) {
  $at = Ui-LoggedAt $Text
  Ui-DoubleClick $at[0] $at[1]
}

# Ui-PickAt "text": where a click picks the face, edge, vertex or profile
# whose "Pick ..." line (after the last "Pick places:") contains the text.
function Ui-PickAt([string]$Text) {
  $lines = Ui-LogLines
  $start = 0
  for ($i = $lines.Count - 1; $i -ge 0; $i--) { if ($lines[$i].Contains('Pick places:')) { $start = $i; break } }
  $line = $lines[$start..($lines.Count - 1)] | Where-Object { $_.Contains('Pick ') -and $_.Contains($Text) } |
    Select-Object -Last 1
  if ($line -notmatch ' at (\d+),(\d+)$') { Ui-Fail "no pick place logged for '$Text'" }
  return @([int]$Matches[1], [int]$Matches[2])
}

function Ui-ClickPick([string]$Text) {
  $at = Ui-PickAt $Text
  Ui-Click $at[0] $at[1]
}

# Ui-CreateSketch key [plane]: Create Sketch by its shortcut (assigned with
# Ui-Init -Shortcuts), then a click on the origin plane it asks for.
function Ui-CreateSketch([string]$Key, [string]$Plane = 'xy') {
  Ui-Mark
  Ui-Key $Key
  Ui-ExpectNew 'Create Sketch: select a plane' 'Create Sketch asks for a plane' | Out-Null
  Start-Sleep -Milliseconds 500
  Ui-ClickLogged "Datum $Plane"
  Ui-ExpectNew "Sketch started on $Plane" "sketch started on $Plane" | Out-Null
}

# Ui-SketchAt x y: the window point of a sketch point (mm), from the logged
# "Sketch view x0,y0 x1,y1 x2,y2" (sketch points (0, 0), (100, 0), (0, 100)).
function Ui-SketchAt([double]$X, [double]$Y) {
  $view = Ui-LastMatch 'Sketch view ([-0-9.,]+ [-0-9.,]+ [-0-9.,]+)'
  if (-not $view) { Ui-Fail 'the app did not log the sketch view' }
  $p = $view -split ' ' | ForEach-Object { , ($_ -split ',' | ForEach-Object { [double]::Parse($_, [Globalization.CultureInfo]::InvariantCulture) }) }
  $px = $p[0][0] + ($p[1][0] - $p[0][0]) * $X / 100 + ($p[2][0] - $p[0][0]) * $Y / 100
  $py = $p[0][1] + ($p[1][1] - $p[0][1]) * $X / 100 + ($p[2][1] - $p[0][1]) * $Y / 100
  return @([int][Math]::Round($px), [int][Math]::Round($py))
}

function Ui-SketchClick([double]$X, [double]$Y) {
  $at = Ui-SketchAt $X $Y
  Ui-Click $at[0] $at[1]
}

# Ui-TypeIn "Panel <command> input <id>" text: replaces a panel field's text.
function Ui-TypeIn([string]$Field, [string]$Text) {
  Ui-ClickLogged $Field
  Start-Sleep -Milliseconds 200
  Ui-Key 'ctrl+a'
  Ui-Type $Text
}

# Ui-FocusDialog "title regex": waits for a window of the app with that title
# (a dialog) and sends the keys there; Ui-FocusMain sends them to the main
# window again. Qt drops key events for a window a modal dialog blocks.
function Ui-FocusDialog([string]$TitlePattern) {
  for ($i = 0; $i -lt 50; $i++) {
    $windows = [UiInput]::Windows($script:UiProcess.Id, $TitlePattern)
    if ($windows.Count -gt 0) {
      $script:UiTarget = $windows[0]
      Start-Sleep -Milliseconds 500
      return
    }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "no window matching '$TitlePattern'"
}

function Ui-FocusMain {
  Start-Sleep -Milliseconds 300
  $script:UiTarget = $script:UiWindow
}

# Ui-ExpectVolume "regex with one group" expected description [tolerance]:
# the last logged number the pattern captures equals the expected volume.
function Ui-ExpectVolume([string]$Pattern, [double]$Expected, [string]$Description, [double]$Tolerance = 1e-6) {
  $volume = $null
  for ($i = 0; $i -lt 150 -and $null -eq $volume; $i++) {
    $volume = Ui-LastMatch $Pattern
    if ($null -eq $volume) { Start-Sleep -Milliseconds 200 }
  }
  if ($null -eq $volume) { Ui-Fail "${Description}: no volume logged" }
  $v = [double]::Parse($volume, [Globalization.CultureInfo]::InvariantCulture)
  if ([Math]::Pow($v - $Expected, 2) -ge [Math]::Pow($Tolerance * $Expected, 2) + 1e-4) {
    Ui-Fail "${Description}: volume $v mm3, expected $Expected"
  }
  Write-Host "ok   ${Description}: $v mm3"
}

function Ui-Finish([string]$Name) {
  if ((Ui-LogLines) | Where-Object { $_.Contains('OCCT view event failed') }) {
    Ui-Fail 'OCCT reported a failed view operation'
  }
  Ui-StopApp
  Write-Host "PASS: $Name"
}
