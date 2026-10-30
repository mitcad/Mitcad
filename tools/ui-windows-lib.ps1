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
# (AttachThreadInput, SetKeyboardState), held until the app logs that it
# took the key (MITCAD_LOG_KEYS).
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
using System.Linq;
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
    [DllImport("user32.dll", SetLastError = true)] static extern bool AttachThreadInput(uint attach, uint to, bool on);
    [DllImport("user32.dll")] static extern bool GetKeyboardState(byte[] state);
    [DllImport("user32.dll")] static extern bool SetKeyboardState(byte[] state);
    [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint msg, IntPtr wparam, IntPtr lparam,
                                                               uint flags, uint timeout, out IntPtr result);

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

    [DllImport("user32.dll", EntryPoint = "IsWindow")] static extern bool IsWindowHandle(IntPtr hwnd);
    [DllImport("user32.dll")] static extern IntPtr GetWindow(IntPtr hwnd, uint command);
    const uint GW_OWNER = 4;

    // Whether the window still exists (a dialog's is destroyed once it
    // has closed and Qt deleted it).
    public static bool Exists(IntPtr hwnd) { return hwnd != IntPtr.Zero && IsWindowHandle(hwnd); }

    // The window that owns a top-level window: Qt makes a dialog's
    // transient parent (the window it was opened over) its owner.
    public static IntPtr Owner(IntPtr hwnd) { return GetWindow(hwnd, GW_OWNER); }

    static uint ThreadOf(IntPtr hwnd) {
        uint pid;
        return GetWindowThreadProcessId(hwnd, out pid);
    }

    // What the last key's modifiers needed besides setting them once (an
    // attach that failed, the state set again), for the test's output.
    public static string Note = "";

    static uint attachedFrom, attachedTo;

    // This thread shares the input queue, and so the keyboard state, of the
    // app's GUI thread while a key's modifiers are held: only then, as a
    // thread that reads no messages of its own, attached for long, can stall
    // the app's input (a whole key lost, mitcad#271). An attach that fails
    // is tried once more and noted.
    static void Attach(IntPtr hwnd) {
        uint self = GetCurrentThreadId(), target = ThreadOf(hwnd);
        if (attachedFrom == self && attachedTo == target) return;
        Detach();
        for (int attempt = 0; attempt < 2; attempt++) {
            if (AttachThreadInput(self, target, true)) {
                attachedFrom = self;
                attachedTo = target;
                return;
            }
            Note += "attaching thread " + self + " to the app's thread " + target + " failed (error " +
                    Marshal.GetLastWin32Error() + "); ";
            Sync(hwnd);
        }
    }

    public static void Detach() {
        if (attachedTo != 0) AttachThreadInput(attachedFrom, attachedTo, false);
        attachedFrom = attachedTo = 0;
    }

    static byte[] Modified(bool ctrl, bool shift, bool alt) {
        var state = new byte[256];
        GetKeyboardState(state);
        state[VK_CONTROL] = state[VK_LCONTROL] = (byte)(ctrl ? 0x80 : 0);
        state[VK_SHIFT] = state[VK_LSHIFT] = (byte)(shift ? 0x80 : 0);
        state[VK_MENU] = state[VK_LMENU] = (byte)(alt ? 0x80 : 0);
        return state;
    }

    // The modifiers held in the shared keyboard state now, as the app's key
    // log names them ("ctrl+shift").
    public static string Held() {
        var state = new byte[256];
        GetKeyboardState(state);
        var held = new List<string>();
        if ((state[VK_CONTROL] & 0x80) != 0) held.Add("ctrl");
        if ((state[VK_MENU] & 0x80) != 0) held.Add("alt");
        if ((state[VK_SHIFT] & 0x80) != 0) held.Add("shift");
        return string.Join("+", held);
    }

    // The modifiers Qt reads with GetKeyState while it handles the next
    // messages: the keyboard state of the input queue this thread shares
    // with the app's GUI thread. Set, then the app's thread waited for
    // (whatever it applies when it reads its messages is applied), and set
    // again if it did not hold.
    static void SetModifiers(IntPtr hwnd, bool ctrl, bool shift, bool alt) {
        Attach(hwnd);
        string wanted = string.Join("+", new[] { ctrl ? "ctrl" : null, alt ? "alt" : null, shift ? "shift" : null }
                                         .Where(name => name != null));
        for (int attempt = 0; attempt < 5; attempt++) {
            SetKeyboardState(Modified(ctrl, shift, alt));
            if (wanted.Length == 0) return;
            Sync(hwnd);
            string held = Held();
            if (held == wanted) return;
            Note += "the keyboard state lost " + wanted + " (held '" + held + "') before the key; set again; ";
        }
    }

    static void ReleaseModifiers(IntPtr hwnd) {
        SetModifiers(hwnd, false, false, false);
        Detach();
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
        Press(hwnd, vk, ctrl, shift, alt);
        if (modified) {
            // The state must hold until the app has taken the messages: a
            // busy app (a build beside the test) may take longer than the
            // pause, so its thread is waited for too. Not enough while the
            // app waits for a model job (see Ui-Key): an app that logs its
            // keys is waited for by the log.
            Thread.Sleep(120);
        }
        Release(hwnd, modified);
    }

    // The first half of Key: the modifiers set in the keyboard state the app
    // reads, and the key posted. Release, once the app has taken the key.
    public static void Press(IntPtr hwnd, int vk, bool ctrl, bool shift, bool alt) {
        Note = "";
        if (ctrl || shift || alt) SetModifiers(hwnd, ctrl, shift, alt);
        PostKey(hwnd, vk, alt);
    }

    public static void Release(IntPtr hwnd, bool modified) {
        // Also without modifiers, the app takes the key before the test
        // goes on: Qt takes the WM_CHAR that follows a key down as that
        // key's text, so characters typed after a key the app had not yet
        // handled (Tab, then 25, under load) would lose their first one
        // (mitcad#255).
        Sync(hwnd);
        if (modified) ReleaseModifiers(hwnd);
    }

    // Modifiers held for mouse input (Ui-ClickWith, a Ctrl+click): Qt reads
    // them with GetKeyState while it handles the buttons' messages, as for
    // a key. Unhold once the messages are posted: the app takes them first.
    public static void Hold(IntPtr hwnd, bool ctrl, bool shift, bool alt) {
        Note = "";
        SetModifiers(hwnd, ctrl, shift, alt);
    }

    public static void Unhold(IntPtr hwnd) {
        Sync(hwnd);
        ReleaseModifiers(hwnd);
    }

    const uint WM_NULL = 0, SMTO_NORMAL = 0;

    // Returns once the window's thread has handled the messages posted to it
    // before: a sent message is handled when the thread next looks for
    // messages, so the second one only after the posted ones the first call
    // found are done (up to 10 s each).
    static void Sync(IntPtr hwnd) {
        IntPtr result;
        SendMessageTimeout(hwnd, WM_NULL, IntPtr.Zero, IntPtr.Zero, SMTO_NORMAL, 10000, out result);
        Thread.Sleep(30);
        SendMessageTimeout(hwnd, WM_NULL, IntPtr.Zero, IntPtr.Zero, SMTO_NORMAL, 10000, out result);
    }

    const int VK_PAUSE = 0x13;

    // The sync key (Ui-Sync): Pause down and up, which the app takes before
    // its widgets do (framework/TestSync.hpp).
    public static void SyncKey(IntPtr hwnd) {
        PostKey(hwnd, VK_PAUSE, false);
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

    [StructLayout(LayoutKind.Sequential)] struct POINT { public int X, Y; }
    [DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr hwnd, ref POINT point);
    const uint WM_MOUSEWHEEL = 0x20A;

    // Wheel steps at a point of the window (client coordinates; the message
    // carries screen coordinates): a negative count turns towards the user.
    public static void Wheel(IntPtr hwnd, int x, int y, int steps) {
        var point = new POINT { X = x, Y = y };
        ClientToScreen(hwnd, ref point);
        PostMessage(hwnd, WM_MOUSEWHEEL, (IntPtr)((steps * 120) << 16), Point(point.X, point.Y));
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
# The pause after each step (Ui-Step) once the app answered the sync key:
# none by default (mitcad#839), as the Linux tests' ui_step: the sync key
# (Ui-Sync) waits for the input handled and no watched timer running, and
# each check waits for the line its step causes. Once the tests' first
# safety margin (700 ms, from before the sync key); a test can still ask
# for one with Ui-Init -StepPause.
$script:UiStepPause = 0
# Where and when the last button press was posted (Ui-Click): Qt makes a
# press within the double-click interval and a few pixels of the last one
# a double-click.
$script:UiLastPress = $null
# With MITCAD_UI_STEP_TIMES set, each step and app start prints where its
# time went ("time <step>: idle N ms, action N ms, sync N ms, pause N ms"):
# the app's share (idle, sync: its jobs, its answer to the sync key) apart
# from the test's own (mitcad#834).
$script:UiTimes = [bool]$env:MITCAD_UI_STEP_TIMES

# Ui-Init app out [qtBin] [shortcuts] [-StepPause ms]: where the app, its Qt
# and the test's files are; shortcuts (@{'sketch.create' = 'F7'}) go into
# the test's own settings, which the app reads from MITCAD_SETTINGS_DIR. A
# key of one context names its group ('shortcuts.sketch/tools.search' =
# 'F3', mitcad#720). StepPause: the pause after each step ($UiStepPause).
function Ui-Init([string]$App, [string]$Out, [string]$QtBin = '', [hashtable]$Shortcuts = @{},
                 [int]$StepPause = 0) {
  $script:UiStepPause = $StepPause
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
  $groups = [ordered]@{ 'shortcuts' = @() }
  foreach ($entry in $Shortcuts.GetEnumerator()) {
    $group = 'shortcuts'
    $key = [string]$entry.Key
    if ($key.Contains('/')) { $group, $key = $key -split '/', 2 }
    if (-not $groups.Contains($group)) { $groups[$group] = @() }
    $groups[$group] = @($groups[$group]) + "$key=$($entry.Value)"
  }
  $ini = @(foreach ($group in $groups.Keys) { "[$group]"; $groups[$group] })
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

# Ui-StartApp name arguments... [-Recovery] [-Updates] [-DefaultChrome]:
# starts Mitcad with its log in <out>\<name>.log and waits for the main
# window ("<document> - MitCad"). The app offers no recovery of autosaved
# work at start-up (--no-recovery: a test that killed it and starts it
# again would get the question) unless -Recovery is given, and checks for
# no updates unless -Updates is given (a test with a server of its own:
# MITCAD_UPDATE_URL). The window is Docked (--chrome=docked), which the
# tests were written for, unless the arguments choose a layout (--chrome)
# or -DefaultChrome leaves it to the preference and the default (Floating
# since mitcad#429).
function Ui-StartApp([string]$Name, [string[]]$Arguments = @(), [switch]$Recovery, [switch]$Updates,
                     [switch]$DefaultChrome) {
  Ui-StopApp
  $script:UiLog = Join-Path $script:UiOut "$Name.log"
  Remove-Item -Force $script:UiLog -ErrorAction SilentlyContinue
  $env:QT_FORCE_STDERR_LOGGING = '1'
  # Where faces, edges and vertices can be clicked ("Pick face ... at x,y").
  $env:MITCAD_LOG_PICKS = '1'
  # The bodies' volumes before and after each change ("Body ... -> b mm3").
  $env:MITCAD_LOG_VOLUMES = '1'
  # What typed-in text fields hold ("File dialog Save As: <path>"), so a
  # failed run shows what a typed path or value became (mitcad#102).
  $env:MITCAD_LOG_TYPED_TEXT = '1'
  # The keys the app takes, with the modifiers it saw ("Key press: F9 with
  # shift (native 120)"): Ui-Key holds the modifiers until then (mitcad#271).
  $env:MITCAD_LOG_KEYS = '1'
  $script:UiKeyLog = $null
  # The sync key's answers ("Sync <n>", Ui-Sync).
  $env:MITCAD_TEST_SYNC = '1'
  $script:UiSyncOn = $null
  # Settings in an INI file of the test's own, not the user's registry.
  $env:MITCAD_SETTINGS_DIR = $script:UiSettings
  # No update checks (mitcad#9): the tests make no network requests.
  if ($Updates) { Remove-Item Env:MITCAD_NO_UPDATE_CHECK -ErrorAction SilentlyContinue }
  else { $env:MITCAD_NO_UPDATE_CHECK = '1' }
  # The first dimension of a drawn sketch scales nothing (mitcad#860), as in
  # the Linux tests.
  $env:MITCAD_SKETCH_FIRST_DIMENSION_SCALE = '0'
  # No question whether to activate a picked face's component (mitcad#960),
  # as in the Linux tests; a test of the question sets it to 1 first.
  if (-not $env:MITCAD_ASK_ACTIVATE_COMPONENT) { $env:MITCAD_ASK_ACTIVATE_COMPONENT = '0' }
  $env:MITCAD_AUTOSAVE_DIR = $script:UiAutosave
  # Crash reports of the test's own (mitcad#62): none offered from the user's.
  $env:MITCAD_CRASH_DIR = Join-Path $script:UiOut 'crashes'
  $env:MITCAD_THUMBNAIL_DIR = $script:UiThumbnails
  # The store of computed results (P7d) is off unless a test turns it on
  # (Ui-ResultStore), as in the Linux tests.
  if (-not $env:MITCAD_RESULT_STORE) { $env:MITCAD_RESULT_STORE = 'off' }
  # The app writes its log into the file as it goes (stderr is unbuffered),
  # so the helpers read it meanwhile.
  $options = @('--no-native-dialogs')
  if (-not $Recovery) { $options += '--no-recovery' }
  if (-not $DefaultChrome -and -not ($Arguments | Where-Object { $_ -like '--chrome*' })) { $options += '--chrome=docked' }
  $arguments = ($options + $Arguments | ForEach-Object { '"' + $_ + '"' }) -join ' '
  $script:UiProcess = Start-Process -FilePath $script:UiApp -ArgumentList $arguments -PassThru `
    -RedirectStandardError $script:UiLog -RedirectStandardOutput (Join-Path $script:UiOut "$Name.out")
  $script:UiMark = 0
  $script:UiLastPress = $null
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    $windows = [UiInput]::Windows($script:UiProcess.Id, ' - MitCad\*?$')
    if ($windows.Count -gt 0) { break }
    Start-Sleep -Milliseconds 50
  } while ($clock.ElapsedMilliseconds -lt 20000)
  if ($windows.Count -eq 0) { Ui-Fail 'the Mitcad window did not appear' }
  $window = $clock.ElapsedMilliseconds
  $script:UiWindow = $windows[0]
  $script:UiTarget = $script:UiWindow
  # The view logs its area once it is laid out and drawn.
  Ui-ExpectLog 'View area' 'the 3D view is up' | Out-Null
  $view = $clock.ElapsedMilliseconds
  # Then the app has handled its start (what it set going, such as the
  # window's first layouts, logged) and computes nothing, as the Linux
  # tests' ui_start_app waits (mitcad#870: a fixed 1.5 s before); an app
  # without the sync (an earlier release in an update test) gets the pause.
  Ui-WaitIdle
  if (-not $script:UiSyncOn) { Start-Sleep -Milliseconds 1500 }
  if ($script:UiTimes) {
    Write-Host "time start ${Name}: window $window ms, the 3D view $view ms, idle $($clock.ElapsedMilliseconds) ms"
  }
}

function Ui-StopApp {
  [UiInput]::Detach()
  if ($script:UiProcess) {
    if (-not $script:UiProcess.HasExited) {
      $script:UiProcess.Kill()
      $script:UiProcess.WaitForExit(10000) | Out-Null
    }
    # A process that has exited has closed its files (its log, locks and
    # autosaves): no pause then (mitcad#870).
    if (-not $script:UiProcess.HasExited) { Start-Sleep -Milliseconds 300 }
    $script:UiProcess = $null
  }
}

function Ui-LogLines {
  $text = Ui-LogText
  if ($null -eq $text) { return @() }
  return $text -split "`r?`n"
}

# Ui-LogText: the whole log as one string ($null before there is one). The
# helpers that poll the log search it with .NET calls: a PowerShell
# pipeline over every line (Where-Object, Select-Object -Skip) takes tens
# of milliseconds once a session's log has thousands of lines, on each
# poll (mitcad#834).
function Ui-LogText {
  if (-not $script:UiLog -or -not (Test-Path $script:UiLog)) { return $null }
  $stream = [System.IO.File]::Open($script:UiLog, 'Open', 'Read', 'ReadWrite')
  try { $reader = New-Object System.IO.StreamReader $stream; return $reader.ReadToEnd() } finally { $stream.Dispose() }
}

# Ui-CountLog "regex": how many times the (multi-line) pattern matches the log.
function Ui-CountLog([string]$Pattern) {
  $text = Ui-LogText
  if ($null -eq $text) { return 0 }
  return [regex]::Matches($text, $Pattern).Count
}

# Ui-NewLines: the log's lines since the mark (Ui-Mark), as
# "Ui-LogLines | Select-Object -Skip <mark>" gives them.
function Ui-NewLines {
  $lines = @(Ui-LogLines)
  $from = [Math]::Max(0, $script:UiMark)
  if ($from -ge $lines.Count) { return @() }
  return $lines[$from..($lines.Count - 1)]
}

# Ui-LastLineWith "text" [-Start]: the last line of the log that contains
# the text (with -Start: begins with it), $null for none.
function Ui-LastLineWith([string]$Text, [switch]$Start) {
  $log = Ui-LogText
  if (-not $log) { return $null }
  $at = $log.Length
  while ($at -gt 0) {
    $at = $log.LastIndexOf($Text, $at - 1, [StringComparison]::Ordinal)
    if ($at -lt 0) { return $null }
    $begin = $log.LastIndexOf("`n", [Math]::Max(0, $at - 1), [StringComparison]::Ordinal) + 1
    if ($at -eq 0) { $begin = 0 }
    if ($Start -and $begin -ne $at) { continue }
    $end = $log.IndexOf("`n", $at, [StringComparison]::Ordinal)
    if ($end -lt 0) { $end = $log.Length }
    return $log.Substring($begin, $end - $begin).TrimEnd("`r")
  }
  return $null
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
# no input, so UI steps wait first, as tools/ui-test-lib.sh does. Idle is
# confirmed by a sync (Ui-Sync): a job in its first 50 ms, or one a watched
# timer is about to start (a preview after a change), has not logged its
# start yet; input posted then waited in Qt's queue and went to the window
# the job's progress dialog blocked, which dropped it (a click lost under
# load, mitcad#271).
function Ui-WaitIdle {
  # Polled at 50 ms (a job's end is mostly a moment away), up to 60 s.
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    if (Ui-Computing) { Start-Sleep -Milliseconds 50; continue }
    Ui-Sync
    if (-not (Ui-Computing)) { return }
  } while ($clock.ElapsedMilliseconds -lt 60000)
  Write-Host 'note: the app was still computing after 60 s'
}

function Ui-Computing {
  $text = Ui-LogText
  if ($null -eq $text) { return $false }
  # Per line, as 'Computing .* started$' and 'Computing .* (done in|...)'.
  $started = [regex]::Matches($text, '(?m)Computing [^\r\n]* started\r?$').Count
  $ended = [regex]::Matches($text, 'Computing [^\r\n]* (done in|cancelled after) [0-9]+ ms').Count
  return $started -gt $ended
}

# Ui-Sync: posts the sync key (Pause) to the app and waits (up to 20 s) for
# its answer "Sync <n>", which the app logs once the input before it is
# handled and no watched timer runs (framework/TestSync.hpp, as ui_sync in
# tools/ui-test-lib.sh). Nothing with an app that has no sync ("Test sync
# on" not logged, as an earlier release in an update test).
function Ui-Sync {
  if (-not $script:UiProcess -or $script:UiProcess.HasExited) { return }
  if ($null -eq $script:UiSyncOn) {
    $script:UiSyncOn = "$(Ui-LogText)".Contains('Test sync on')
  }
  if (-not $script:UiSyncOn) { return }
  # Lines '^Sync [0-9]+$'.
  $answer = '(?m)^Sync [0-9]+\r?$'
  $before = Ui-CountLog $answer
  [UiInput]::SyncKey($script:UiWindow)
  $clock = [Diagnostics.Stopwatch]::StartNew()
  while ($clock.ElapsedMilliseconds -lt 20000) {
    if ((Ui-CountLog $answer) -gt $before) { return }
    if ($script:UiProcess.HasExited) { return }
    Start-Sleep -Milliseconds 20
  }
  Write-Host 'note: no answer to the sync key in 20 s'
}

# Ui-Step "description" { action }: runs a UI action (once the app computes
# nothing), waits until the app has handled it (Ui-Sync) and checks for
# crashes.
function Ui-Step([string]$Name, [scriptblock]$Action) {
  # The action runs in a scope below this one and sees its variables: the
  # timings' names are the step's own, no test's.
  $uiStepClock = [Diagnostics.Stopwatch]::StartNew()
  Ui-WaitIdle
  $uiStepIdle = $uiStepClock.ElapsedMilliseconds
  & $Action
  $uiStepActed = $uiStepClock.ElapsedMilliseconds
  Ui-Sync
  $uiStepSynced = $uiStepClock.ElapsedMilliseconds
  if ($script:UiStepPause -gt 0) { Start-Sleep -Milliseconds $script:UiStepPause }
  if (Ui-Crashed) { Ui-Fail "crashed during '$Name' (exit code $($script:UiProcess.ExitCode))" }
  Write-Host "ok   $Name"
  if ($script:UiTimes) {
    Write-Host ("time ${Name}: idle $uiStepIdle ms, action $($uiStepActed - $uiStepIdle) ms, " +
                "sync $($uiStepSynced - $uiStepActed) ms, pause $($uiStepClock.ElapsedMilliseconds - $uiStepSynced) ms")
  }
}

# Ui-ExpectLog "text" "description": waits for a line in the log.
function Ui-ExpectLog([string]$Text, [string]$Description, [int]$Seconds = 6) {
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    if ("$(Ui-LogText)".Contains($Text)) { Write-Host "ok   $Description"; return }
    Start-Sleep -Milliseconds 50
  } while ($clock.ElapsedMilliseconds -lt $Seconds * 1000)
  Ui-Fail "${Description}: '$Text' not in the log"
}

# Ui-Mark; ...; Ui-ExpectNew "text" "description" [seconds]: only lines
# logged since the mark count.
function Ui-Mark { $script:UiMark = (Ui-LogLines).Count - 1 }
function Ui-ExpectNew([string]$Text, [string]$Description, [int]$Seconds = 6) {
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    foreach ($line in @(Ui-NewLines)) {
      if ($line.Contains($Text)) { Write-Host "ok   $Description"; return }
    }
    Start-Sleep -Milliseconds 50
  } while ($clock.ElapsedMilliseconds -lt $Seconds * 1000)
  Ui-Fail "${Description}: '$Text' not in the log since the mark"
}

# Ui-LastMatch "regex": the first group of the last log line matching it
# (as Select-String matches: case-insensitive).
function Ui-LastMatch([string]$Pattern) {
  $regex = New-Object System.Text.RegularExpressions.Regex $Pattern, 'IgnoreCase'
  $lines = @(Ui-LogLines)
  for ($i = $lines.Count - 1; $i -ge 0; $i--) {
    $match = $regex.Match($lines[$i])
    if ($match.Success) { return $match.Groups[1].Value }
  }
  return $null
}

# Keys by name, as xdotool names them in the Linux tests.
$script:UiKeys = @{
  'Return' = 0x0D; 'Escape' = 0x1B; 'Tab' = 0x09; 'BackSpace' = 0x08; 'Delete' = 0x2E; 'space' = 0x20
  'Up' = 0x26; 'Down' = 0x28; 'Left' = 0x25; 'Right' = 0x27; 'Home' = 0x24; 'End' = 0x23
  'F2' = 0x71; 'F3' = 0x72; 'F4' = 0x73; 'F5' = 0x74; 'F6' = 0x75; 'F7' = 0x76; 'F8' = 0x77; 'F9' = 0x78; 'F11' = 0x7A; 'F12' = 0x7B
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
    $ctrl = $mods -contains 'ctrl'
    $shift = $mods -contains 'shift'
    $alt = $mods -contains 'alt'
    if (($ctrl -or $shift -or $alt) -and (Ui-KeyLogOn)) {
      # The modifiers stay in the keyboard state until the app has taken the
      # key: Qt reads them when it handles the key down, and while the app
      # waits for a model job (MainWindow::waitForJob), its event loop keeps
      # the input for later but answers Sync, so the modifiers went up
      # before the key was handled, and Shift+F9 became F9 under load
      # (mitcad#271).
      $from = "$(Ui-LogText)".Length
      [UiInput]::Press($script:UiTarget, $vk, $ctrl, $shift, $alt)
      if ([UiInput]::Note) { Write-Host "note: '$spec': $([UiInput]::Note)" }
      Ui-WaitKeyLogged $vk (@(@('ctrl', 'alt', 'shift') | Where-Object { $mods -contains $_ }) -join '+') $spec $from
      [UiInput]::Release($script:UiTarget, $true)
    } else {
      [UiInput]::Key($script:UiTarget, $vk, $ctrl, $shift, $alt)
      if ([UiInput]::Note) { Write-Host "note: '$spec': $([UiInput]::Note)" }
    }
    Start-Sleep -Milliseconds 60
  }
}

# Ui-KeyLogOn: whether the app logs the keys it takes (MITCAD_LOG_KEYS; an
# app before mitcad#271 does not, as an earlier release in an update test).
function Ui-KeyLogOn {
  if ($null -eq $script:UiKeyLog) {
    $script:UiKeyLog = "$(Ui-LogText)".Contains('Key log on')
  }
  return $script:UiKeyLog
}

# Ui-WaitKeyLogged vk modifiers spec from: waits (up to 60 s) for the app's
# "Key press: <key> with <modifiers> (native <vk>)" in the log after its
# first <from> characters (the log's length before the key). The same key
# with other modifiers fails the test unless the right line follows within
# 5 s (the line of a key without modifiers sent just before, which the app
# took late). A key the app never takes (one for a window a modal dialog
# blocks, which Qt drops) ends the wait with a note. Polled at 20 ms: the
# app logs the key a moment after it is posted (mitcad#870: 100 ms, and
# every line of the log split on each look, before).
function Ui-WaitKeyLogged([int]$Vk, [string]$Modifiers, [string]$Spec, [int]$From) {
  $pattern = New-Object System.Text.RegularExpressions.Regex ("(?m)Key press: \S+(?: with ([a-z+]+))? \(native $Vk\)\r?$")
  $other = $null
  $clock = [Diagnostics.Stopwatch]::StartNew()
  while ($clock.ElapsedMilliseconds -lt 60000) {
    $text = "$(Ui-LogText)"
    # From the start of the line the log ended in before the key.
    $start = [Math]::Min([Math]::Max(0, $From), $text.Length)
    if ($start -gt 0) { $start = $text.LastIndexOf("`n", $start - 1, [StringComparison]::Ordinal) + 1 }
    for ($match = $pattern.Match($text, $start); $match.Success; $match = $match.NextMatch()) {
      if ($match.Groups[1].Value -eq $Modifiers) { return }
      if ($null -eq $other) {
        $begin = $text.LastIndexOf("`n", [Math]::Max(0, $match.Index - 1), [StringComparison]::Ordinal) + 1
        if ($match.Index -eq 0) { $begin = 0 }
        $other = $text.Substring($begin, $match.Index + $match.Length - $begin).TrimEnd("`r")
        $seen = $clock.ElapsedMilliseconds
      }
    }
    if ($null -ne $other -and $clock.ElapsedMilliseconds - $seen -gt 5000) {
      Ui-Fail ("'$Spec': the app took the key without the modifiers $Modifiers held: $other " +
               "(the keyboard state holds '$([UiInput]::Held())' now)")
    }
    # A job keeps the key for later; without one, 10 s are enough.
    if ($clock.ElapsedMilliseconds -gt 10000 -and -not (Ui-Computing)) { break }
    Start-Sleep -Milliseconds 20
  }
  Write-Host "note: '$Spec': the app did not log the key within $([int]($clock.ElapsedMilliseconds / 1000)) s"
}

function Ui-Type([string]$Text) { [UiInput]::Type($script:UiTarget, $Text, 40) }

# Mouse buttons: messages for down, up and double-click, and the key state.
$script:UiButtons = @{ 1 = @(0x201, 0x202, 0x203, 0x01); 2 = @(0x207, 0x208, 0x209, 0x10); 3 = @(0x204, 0x205, 0x206, 0x02) }

# Ui-Click x y [button]: a click at a point of the main window (the window
# coordinates the app logs).
function Ui-Click([int]$X, [int]$Y, [int]$Button = 1) {
  $b = $script:UiButtons[$Button]
  Ui-Apart $script:UiWindow $X $Y
  [UiInput]::Move($script:UiWindow, $X, $Y, 0)
  Start-Sleep -Milliseconds 80
  [UiInput]::Button($script:UiWindow, $b[0], $X, $Y, $b[3])
  Start-Sleep -Milliseconds 60
  [UiInput]::Button($script:UiWindow, $b[1], $X, $Y, 0)
}

# Ui-Apart window x y: before a press at (x, y) of the window. Qt makes a
# press within the double-click interval (Windows' 500 ms) and a few pixels
# of the last one a double-click, as Linux's ui_apart says: a press that
# near waits until the interval is over. With the steps' pause (Ui-Step)
# that never happened; a test without it clicks faster (mitcad#834).
function Ui-Apart([IntPtr]$Window, [int]$X, [int]$Y) {
  $last = $script:UiLastPress
  if ($last -and $last.Window -eq $Window -and [Math]::Abs($last.X - $X) -le 10 -and [Math]::Abs($last.Y - $Y) -le 10) {
    $wait = 550 - $last.Clock.ElapsedMilliseconds
    if ($wait -gt 0) { Start-Sleep -Milliseconds $wait }
  }
  $script:UiLastPress = @{ Window = $Window; X = $X; Y = $Y; Clock = [Diagnostics.Stopwatch]::StartNew() }
}

function Ui-DoubleClick([int]$X, [int]$Y) {
  Ui-Click $X $Y
  Start-Sleep -Milliseconds 80
  [UiInput]::Button($script:UiWindow, 0x203, $X, $Y, 1)
  # The next press there waits from this second one (Ui-Apart).
  $script:UiLastPress.Clock.Restart()
  Start-Sleep -Milliseconds 60
  [UiInput]::Button($script:UiWindow, 0x202, $X, $Y, 0)
}

# Ui-Drag x1 y1 x2 y2 [button]: a drag in steps, its press apart from a
# click there just before (Ui-Apart).
function Ui-Drag([int]$X1, [int]$Y1, [int]$X2, [int]$Y2, [int]$Button = 1) {
  $b = $script:UiButtons[$Button]
  Ui-Apart $script:UiWindow $X1 $Y1
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

# Ui-LoggedAt "text": the last "<text> at x,y" place, waited for (up to
# 6 s): a busy app (other tests beside it) may log it a moment after the
# line the test waited for (mitcad#255).
function Ui-LoggedAt([string]$Text) {
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    $line = Ui-LastLineWith "$Text at "
    # "$line", not [string]$line: the .NET method gets null for the latter
    # while nothing is logged yet, and the exception ended the script (the
    # app, left running with ctest's output pipe, kept ctest waiting until
    # its time-out).
    $match = [regex]::Match("$line", ' at (\d+),(\d+)')
    if ($match.Success) { return @([int]$match.Groups[1].Value, [int]$match.Groups[2].Value) }
    Start-Sleep -Milliseconds 50
  } while ($clock.ElapsedMilliseconds -lt 6000)
  Ui-Fail "no place logged for '$Text'"
}

# Ui-PanelReveal "Panel <command> input <id>": a long panel scrolled, by
# wheel steps over its scroll bar, until the input's logged place is in its
# view. In session 0's 1024x768 desktop a panel can be higher than its dock
# (Timber's frame with its fasteners), and a click on a place scrolled away
# hits whatever is there instead. The panel logs its view with its places
# ("Panel <command> view y A to B, scrolled S of M, bar at x,y"; the bar
# only while it scrolls) and logs them again when it scrolls.
function Ui-PanelReveal([string]$Text) {
  if ($Text -notmatch '^Panel (.+) input \S+$') { return }
  $view = "Panel $($Matches[1]) view y "
  $synced = $false
  for ($i = 0; $i -lt 30; $i++) {
    $line = Ui-LastLineWith $view -Start
    if ("$line" -notmatch ' view y (-?\d+) to (-?\d+), scrolled \d+ of \d+, bar at (\d+),(\d+)$') { return }
    if (-not $synced) {
      # The places and the view of one layout, logged whole.
      Ui-Sync
      $synced = $true
      continue
    }
    $top = [int]$Matches[1]
    $bottom = [int]$Matches[2]
    $bar = @([int]$Matches[3], [int]$Matches[4])
    $place = Ui-LastLineWith "$Text at "
    if ("$place" -notmatch ' at (\d+),(\d+)' -or "$place".EndsWith('(hidden)')) { return }
    $y = [int]$Matches[2]
    $margin = 12
    if ($y -ge $top + $margin -and $y -le $bottom - $margin) { return }
    $step = if ($y -gt $bottom - $margin) { -1 } else { 1 }
    # From the last line, as Ui-Mark: the log ends with a line break, and
    # the empty piece after it is where the next line comes (skipping it
    # missed the panel's new view and waited 3 s, mitcad#834).
    $count = @(Ui-LogLines).Count - 1
    [UiInput]::Move($script:UiWindow, $bar[0], $bar[1], 0)
    Start-Sleep -Milliseconds 80
    [UiInput]::Wheel($script:UiWindow, $bar[0], $bar[1], $step)
    for ($j = 0; $j -lt 60; $j++) {
      $new = @(Ui-LogLines)
      $found = $false
      for ($n = [Math]::Max(0, $count); $n -lt $new.Count; $n++) { if ($new[$n].StartsWith($view)) { $found = $true; break } }
      if ($found) { break }
      Start-Sleep -Milliseconds 50
    }
    Ui-Sync
  }
  Ui-Fail "the panel did not scroll '$Text' into its view"
}

function Ui-ClickLogged([string]$Text, [int]$Button = 1) {
  Ui-PanelReveal $Text
  $at = Ui-LoggedAt $Text
  Ui-Click $at[0] $at[1] $Button
}

# Ui-ClickWith "ctrl" x y: a left click with modifiers held ("ctrl",
# "shift", "ctrl+shift"), such as a Ctrl+click that adds a browser row to
# the selection; the button messages carry them too (MK_CONTROL, MK_SHIFT).
function Ui-ClickWith([string]$Modifiers, [int]$X, [int]$Y) {
  $mods = @($Modifiers -split '\+')
  $ctrl = $mods -contains 'ctrl'
  $shift = $mods -contains 'shift'
  $alt = $mods -contains 'alt'
  $held = 0
  if ($ctrl) { $held = $held -bor 0x08 }
  if ($shift) { $held = $held -bor 0x04 }
  Ui-Apart $script:UiWindow $X $Y
  [UiInput]::Hold($script:UiWindow, $ctrl, $shift, $alt)
  try {
    [UiInput]::Move($script:UiWindow, $X, $Y, $held)
    Start-Sleep -Milliseconds 80
    [UiInput]::Button($script:UiWindow, 0x201, $X, $Y, ($held -bor 0x01))
    Start-Sleep -Milliseconds 60
    [UiInput]::Button($script:UiWindow, 0x202, $X, $Y, $held)
  } finally {
    [UiInput]::Unhold($script:UiWindow)
  }
}

# Ui-WheelLogged "text" steps: wheel steps over a logged place, such as a
# panel's drop-down ("Panel 3D Print input format"), whose list cannot open
# in session 0: -1 (a step towards the user) chooses the next entry.
function Ui-WheelLogged([string]$Text, [int]$Steps) {
  Ui-PanelReveal $Text
  $at = Ui-LoggedAt $Text
  [UiInput]::Move($script:UiWindow, $at[0], $at[1], 0)
  Start-Sleep -Milliseconds 80
  [UiInput]::Wheel($script:UiWindow, $at[0], $at[1], $Steps)
}

function Ui-DoubleClickLogged([string]$Text) {
  Ui-PanelReveal $Text
  $at = Ui-LoggedAt $Text
  Ui-DoubleClick $at[0] $at[1]
}

# Ui-PickAt "text": where a click picks the face, edge, vertex or profile
# whose "Pick ..." line (after the last "Pick places:") contains the text.
function Ui-PickAt([string]$Text) {
  # The lines from the last "Pick places:" one on, searched with .NET calls
  # (a pipeline over a long log's lines takes long, mitcad#870).
  $log = "$(Ui-LogText)"
  $at = $log.LastIndexOf('Pick places:', [StringComparison]::Ordinal)
  $start = if ($at -gt 0) { $log.LastIndexOf("`n", $at - 1, [StringComparison]::Ordinal) + 1 } else { 0 }
  $lines = $log.Substring($start) -split "`r?`n"
  $line = $null
  for ($i = $lines.Count - 1; $i -ge 0; $i--) {
    if ($lines[$i].Contains('Pick ') -and $lines[$i].Contains($Text)) { $line = $lines[$i]; break }
  }
  # "$line": with no line, $line is an empty result, and -notmatch on it
  # gave an empty (false) result, not true; $Matches was then null and the
  # error ended the script with the app left running (mitcad#525).
  if ("$line" -notmatch ' at (\d+),(\d+)$') { Ui-Fail "no pick place logged for '$Text'" }
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
  # The planes shown and their places logged, as ui_create_sketch syncs
  # (mitcad#870: a fixed 500 ms before).
  Ui-SyncOr 500
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
  # The click handled (the field has the keyboard), as ui_type_in syncs
  # (mitcad#870: a fixed 200 ms before).
  Ui-SyncOr 200
  Ui-Key 'ctrl+a'
  Ui-Type $Text
}

# Ui-Windows "title regex": the app's windows with that title, newest first.
function Ui-Windows([string]$TitlePattern) { return , [UiInput]::Windows($script:UiProcess.Id, $TitlePattern) }

# Ui-FocusDialog "title regex" [-Except windows]: waits for a window of the
# app with that title (a dialog) and sends the keys there; Ui-FocusMain
# sends them to the main window again. Qt drops key events for a window a
# modal dialog blocks. Windows in -Except (Ui-Windows before the dialog
# opens) do not count: Qt titles a hidden window that has no title of its
# own after the application, such as the 3D view's offscreen surfaces
# ("Mitcad", the application name), so a message box (titled "MitCad") is
# best found as a new window.
#
# Neither waits a fixed time once the window is there (mitcad#870: 500 ms
# and 300 ms before), as the Linux tests' ui_focus_dialog and ui_focus_main
# sync: the dialog the keys went to before, if it is closing (a button's
# click comes a moment after its key, Qt's animateClick, and nothing is
# logged for it), is waited for until its window is gone, as long as the
# pause was at the most; then the app has handled the input before
# (Ui-Sync).
function Ui-FocusDialog([string]$TitlePattern, [IntPtr[]]$Except = @()) {
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    $windows = @([UiInput]::Windows($script:UiProcess.Id, $TitlePattern) | Where-Object { $Except -notcontains $_ })
    if ($windows.Count -gt 0) {
      $previous = $script:UiTarget
      $script:UiTarget = $windows[0]
      # Not a dialog that owns this one: it stays open under it.
      if ($previous -ne $script:UiWindow -and $previous -ne $windows[0] -and
          [UiInput]::Owner($windows[0]) -ne $previous) {
        Ui-WaitGone $previous 500
      }
      Ui-SyncOr 500
      return
    }
    Start-Sleep -Milliseconds 50
  } while ($clock.ElapsedMilliseconds -lt 10000)
  Ui-Fail "no window matching '$TitlePattern'"
}

function Ui-FocusMain {
  $previous = $script:UiTarget
  $script:UiTarget = $script:UiWindow
  if ($previous -ne $script:UiWindow) { Ui-WaitGone $previous 300 }
  Ui-SyncOr 300
}

# Ui-WaitGone window ms: waits until the window is gone, up to ms.
function Ui-WaitGone([IntPtr]$Window, [int]$Milliseconds) {
  $clock = [Diagnostics.Stopwatch]::StartNew()
  while ([UiInput]::Exists($Window) -and $clock.ElapsedMilliseconds -lt $Milliseconds) { Start-Sleep -Milliseconds 20 }
}

# Ui-SyncOr ms: the app has handled the input before (Ui-Sync); an app
# without the sync (an earlier release in an update test) gets the fixed
# pause of ms instead.
function Ui-SyncOr([int]$Milliseconds) {
  Ui-Sync
  if (-not $script:UiSyncOn) { Start-Sleep -Milliseconds $Milliseconds }
}

# Ui-ExpectVolume "regex with one group" expected description [tolerance]:
# the last logged number the pattern captures equals the expected volume.
function Ui-ExpectVolume([string]$Pattern, [double]$Expected, [string]$Description, [double]$Tolerance = 1e-6) {
  $volume = $null
  $clock = [Diagnostics.Stopwatch]::StartNew()
  while ($null -eq $volume -and $clock.ElapsedMilliseconds -lt 30000) {
    $volume = Ui-LastMatch $Pattern
    if ($null -eq $volume) { Start-Sleep -Milliseconds 50 }
  }
  if ($null -eq $volume) { Ui-Fail "${Description}: no volume logged" }
  $v = [double]::Parse($volume, [Globalization.CultureInfo]::InvariantCulture)
  if ([Math]::Pow($v - $Expected, 2) -ge [Math]::Pow($Tolerance * $Expected, 2) + 1e-4) {
    Ui-Fail "${Description}: volume $v mm3, expected $Expected"
  }
  Write-Host "ok   ${Description}: $v mm3"
}

# Places a part that waits for a pick (mitcad#656: Pulley, Gear, Bearing)
# on the XY plane at the origin: a click on the plane while Place On takes
# picks, then both distances 0 in the panel.
function Ui-PlaceOnXy([string]$Command) {
  Ui-Mark
  Ui-Step 'Place On: the XY plane' { Ui-ClickLogged 'Datum xy' }
  Ui-ExpectNew "Place ${Command}: face plane xy, " 'placed on the XY plane' 30
  $first = "Panel $Command input at_offset1"
  $second = "Panel $Command input at_offset2"
  Ui-Step 'Distance 1: 0' { Ui-TypeIn $first '0' }
  Ui-Step 'Distance 2: 0' { Ui-TypeIn $second '0' }
  Ui-ExpectNew "$Command Distance 1: 0 = 0 mm" 'Distance 1 typed' 30
  Ui-ExpectNew "$Command Distance 2: 0 = 0 mm" 'Distance 2 typed' 30
  Ui-ExpectNew "Preview ${Command}: ok" 'previewed at the origin' 30
}

function Ui-Finish([string]$Name) {
  if ((Ui-LogLines) | Where-Object { $_.Contains('OCCT view event failed') }) {
    Ui-Fail 'OCCT reported a failed view operation'
  }
  Ui-StopApp
  Write-Host "PASS: $Name"
}
