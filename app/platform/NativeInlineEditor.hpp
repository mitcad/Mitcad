// SPDX-License-Identifier: MIT
#pragma once

#include <QtGlobal>

class QLineEdit;

namespace mitcad {

// Sketch precision/dimension editors retain a hidden Qt value presenter,
// while the user edits a real NSTextField in an AppKit child panel. Geometry,
// text, selection, validity and destruction follow the presenter. Set its
// nativeInlineInvalid property to expose the shared validation state.
#if defined(MITCAD_MAC_UI) && defined(MITCAD_MACOS_NATIVE_UI)
bool showNativeInlineEditor(QLineEdit* presenter);
bool focusNativeInlineEditor(QLineEdit* presenter);
bool nativeInlineEditorFocused(const QLineEdit* presenter);
void hideNativeInlineEditor(QLineEdit* presenter);
#else
inline bool showNativeInlineEditor(QLineEdit*) { return false; }
inline bool focusNativeInlineEditor(QLineEdit*) { return false; }
inline bool nativeInlineEditorFocused(const QLineEdit*) { return false; }
inline void hideNativeInlineEditor(QLineEdit*) {}
#endif

} // namespace mitcad
