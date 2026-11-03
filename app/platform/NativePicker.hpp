// SPDX-License-Identifier: MIT
#pragma once

#include <QString>
#include <QtGlobal>

class QLineEdit;
class QListWidget;
class QWidget;

namespace mitcad {

// A picker of a search field over a list of entries under headings, shown
// by the AppKit frontend as a popover (mitcad#1066, mitcad#1068): the
// Material and Appearance tools' LibraryPicker and the Materials window's
// AppearancePopover. The presenter stays the Qt window that fills, filters,
// logs and chooses; it is shown off the screen (Qt::WA_DontShowOnScreen)
// and the popover binds its `search` field and `list`:
// - the list's shown entries with their icons, a heading (an entry that
//   cannot be selected) in bold, its current entry selected; LibraryPicker
//   entries retain their source tags and separator lines;
// - typing sets the search field's text (the presenter filters);
// - Up and Down move the list's current entry over the shown ones;
// - Return, or a click on an entry, sends Return to the list (the
//   presenter chooses its current entry);
// - Escape, or a click outside, sends Escape to the search field (the
//   presenter closes);
// - the presenter hidden or destroyed closes the popover.
// It hangs from the AppKit control whose accessibility identifier is
// `anchor` (a Qt widget's objectName in the generic bridge, input_<key> in
// a command form), else from the pointer. Its window has the presenter's
// window title and objectName (for focus-dialog), its search field and
// list their objectNames, an entry "<list>.item.<name>" (the entry's
// Qt::AccessibleTextRole, else its text).
// False unless AppKit is active and the popover actually opens. On an
// opening failure the presenter retains its normal Qt display attributes
// so the caller can show its Qt fallback.
#if defined(Q_OS_MACOS) && defined(MITCAD_MACOS_NATIVE_UI)
bool showNativePicker(QWidget* presenter, QLineEdit* search, QListWidget* list, const QString& anchor);
#else
inline bool showNativePicker(QWidget*, QLineEdit*, QListWidget*, const QString&) { return false; }
#endif

} // namespace mitcad
