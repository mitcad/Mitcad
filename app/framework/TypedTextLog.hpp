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

} // namespace mitcad
