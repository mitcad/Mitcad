// SPDX-License-Identifier: MIT
#pragma once

#include <QString>
#include <QtGlobal>

namespace mitcad::mac {

// Offscreen Qt component tests are bundled executables too. Prevent their
// process from activating AppKit or presenting a system window before Qt
// starts, and verify that isolation again after startup and at completion.
#ifdef Q_OS_MACOS
bool prepareHeadlessTestApplication(QString& error);
bool headlessTestApplicationIsolated(QString& error);
#else
inline bool prepareHeadlessTestApplication(QString&) { return true; }
inline bool headlessTestApplicationIsolated(QString&) { return true; }
#endif

} // namespace mitcad::mac
