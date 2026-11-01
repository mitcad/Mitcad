// SPDX-License-Identifier: MIT
#pragma once

#include <QString>
#include <QtGlobal>

// The UI tests' sync key on Windows (framework/TestSync.hpp): the Windows UI
// test library (tools/ui-windows-lib.ps1, Ui-Sync) posts the Pause key to the
// application's window. Portable code calls this without #ifdefs: elsewhere
// it is an inline stub. The Windows implementation is
// platform/windows/WindowsTestSync.cpp.
namespace mitcad::win {

#ifdef Q_OS_WIN

// Whether a native message of Qt's event dispatcher ("windows_generic_MSG",
// a MSG) is a key down or up of the Pause key; `released` says which.
bool isSyncKey(const void* message, bool* released);

// The shortcut diagnosis (MITCAD_LOG_SHORTCUTS, mitcad#1016): the native
// windows that the GUI thread's input queue has active and focused, and the
// system's foreground window (GetActiveWindow, GetFocus,
// GetForegroundWindow; 0 for none).
bool nativeActivation(quintptr* active, quintptr* focus, quintptr* foreground);

// Whether a native message is one of activation, focus, enabling or a
// top-level window's size state (WM_ACTIVATE, WM_ACTIVATEAPP,
// WM_NCACTIVATE, WM_SETFOCUS, WM_KILLFOCUS, WM_ENABLE, WM_SIZE); `what`
// names it ("WM_ACTIVATE inactive"), `window` is the window it is for and
// `other` the window it names (the one activated or focused instead, or
// the one that was), 0 if none.
bool activationMessage(const void* message, QString* what, quintptr* window, quintptr* other);

#else

inline bool isSyncKey(const void*, bool*) { return false; }
inline bool nativeActivation(quintptr*, quintptr*, quintptr*) { return false; }
inline bool activationMessage(const void*, QString*, quintptr*, quintptr*) { return false; }

#endif

} // namespace mitcad::win
