// SPDX-License-Identifier: MIT
#pragma once

// The helpers of the UI test driver that TestDriver.cpp shares with the step
// area files (mitcad#2173), defined in TestDriver.cpp. A helper only one area
// uses stays in that area's file.

#include <QPoint>
#include <QString>
#include <QStringList>
#include <QtGlobal>

class QWidget;

namespace mitcad::testdriver {

// How long a logged place waits for the watched timers (TestSync::settling),
// as long as the sync does.
constexpr qint64 kSettleLimitMs = 5000;

// A number of the script (surrounding blanks allowed).
bool toNumber(const QString& text, double& value);

// "ctrl+shift" -> modifier flags; false for an unknown name.
bool parseModifiers(const QString& text, Qt::KeyboardModifiers& modifiers);

// [button] [mods] from index `from` of the arguments, in either order.
bool parseMouseOptions(const QStringList& args, int from, Qt::MouseButton& button, Qt::KeyboardModifiers& modifiers,
                       QString& error);

// The key event a portable key text ("ctrl+shift+s", "Return", "F6") stands
// for.
bool parseKeySpec(const QString& spec, int& key, Qt::KeyboardModifiers& modifiers, QString& text, QString& error);

// A menu entry's text as the script writes it.
QString plainText(QString text);

// The widget of that object name in `scope`: the visible one of stacked
// workspaces that reuse names, else a disabled or a unique hidden one (see
// TestDriver.cpp).
QWidget* namedWidget(QWidget* scope, const QString& name, bool forInput = false);

// A panel's input, action or add button that a logged line places out of
// the panel's view, scrolled into view: its middle now, in `top`'s
// coordinates; false when the line names no such widget or it is in view,
// and the logged place stands (see TestDriver.cpp).
bool revealedPanelPlace(QWidget* main, QWidget* top, const QString& logged, QPoint& at);

} // namespace mitcad::testdriver
