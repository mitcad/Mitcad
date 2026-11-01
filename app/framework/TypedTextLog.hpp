// SPDX-License-Identifier: MIT
#pragma once

namespace mitcad {

// Typed text in the log, for the UI tests (MITCAD_LOG_TYPED_TEXT=1; the UI
// test libraries set it), so that a test that types a path or a number
// sees what the field holds before it presses Enter, instead of waiting a
// fixed time and hoping (tools/ui-test-lib.sh: ui_type_path, ui_type_field,
// ui_type_in; mitcad#102):
//
// - "File dialog <title>: <text>": the file name field of Qt's own file
//   dialog (--no-native-dialogs), at each change (typing, a completion, a
//   file clicked); "File dialog <title>: accepted" (or "rejected") when it
//   closes.
// - "Dialog field <title>: <text>": a text field of another dialog (also a
//   spin box's or an editable combo box's, and a table cell's editor), at
//   each edit by the user.
// - "Panel field <input>: <text>": a command panel's text input (the
//   input's id, as in "Panel <command> input <id> at x,y"), at each edit by
//   the user.
//
// A field is watched from the first time it has the keyboard; password
// fields are never logged. Without the variable nothing is installed.
void installTypedTextLogIfRequested();

// Key presses in the log, for the UI tests (MITCAD_LOG_KEYS=1; the Windows
// UI test library sets it): "Key press: F9 with shift (native 120)", once
// for each key press the application takes from the window system (a
// shortcut it triggers included), with the modifiers it saw then (ctrl,
// alt, shift, meta, in this order; "Key press: Return (native 13)" without
// any) and the platform's key code. "Key log on" says it is installed.
//
// The Windows tests hold a key's modifiers in the keyboard state until
// the key is logged: the application reads them when it handles the key,
// which a model job's wait puts off (its event loop keeps the input for
// later), so a fixed wait could release them too early (mitcad#271).
void installKeyLogIfRequested();

// The shortcut diagnosis in the log (MITCAD_LOG_SHORTCUTS=1, mitcad#1016:
// on Windows a key was logged but started nothing after a UI test had
// maximized and restored the window in session 0, which left the app with
// no active window; tools/ui-windows-lib.ps1, Ui-Activate). For each key
// press: "Shortcut check
// <key> to <receiver>: active window ..., focus window ..., focus widget
// ..., popup ..., modal ..., application <state>" (on Windows also the
// native active, focus and foreground windows), one "Shortcut <key>:
// action|shortcut ..." per action or shortcut that has the key (enabled,
// its context, whether its window is the active one), then "Shortcut event
// <key> to ..." when a shortcut took it or "Shortcut none for <key>: a key
// press to ..." when it went on as a key. Also "Window activated|
// deactivated: ...", "Window state <window>: maximized (was normal)",
// "Focus window: ...", "Application state: ..." and on Windows the native
// activation, focus, enabling and size messages ("Native WM_ACTIVATE
// inactive: <window> (other <window>)"). "Shortcut log on" says it is
// installed.
void installShortcutLogIfRequested();

} // namespace mitcad
