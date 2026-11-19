// SPDX-License-Identifier: MIT
#pragma once

#include <QString>
#include <QtGlobal>
#ifdef Q_OS_MACOS
#include <QStringList>

class QMenuBar;
#endif

namespace mitcad::mac {

// Offscreen Qt component tests are bundled executables too. Prevent their
// process from activating AppKit or presenting a system window before Qt
// starts, and verify that isolation again after startup and at completion.
#ifdef Q_OS_MACOS
bool prepareHeadlessTestApplication(QString& error);
bool headlessTestApplicationIsolated(QString& error);

// Observe the actual, usable AppKit menus of the active document frame.
bool systemMenuBarMatches(QMenuBar& bar, const QStringList& expected, QString& error);
#else
inline bool prepareHeadlessTestApplication(QString&) { return true; }
inline bool headlessTestApplicationIsolated(QString&) { return true; }
#endif

} // namespace mitcad::mac
