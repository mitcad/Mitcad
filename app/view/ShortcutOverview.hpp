// SPDX-License-Identifier: MIT
#pragma once

// The Keyboard and Mouse Overview's rows (Help › Keyboard and Mouse
// Overview) and its cheat sheet (mitcad#2095): the same groups and rows in
// the window, in its Save as PDF… and in its Print…. Plain Qt Gui.

#include <functional>

#include <QDate>
#include <QKeySequence>
#include <QList>
#include <QString>
#include <QVector>

#include "../build/ListTables.hpp"

namespace mitcad {

class CommandRegistry;

// A row: its keys or mouse action and what it does; `id` names the row for
// the UI tests (a command's id, "mouse.N", "key.N"), `icon` a command's
// icon.
struct OverviewRow {
  QString keys;
  QString does;
  QString id;
  QString icon;
};

// A group of rows under its caption: the mouse, the fixed keys, a toolbar
// group's commands ("Solid › Create": the tab and
// group as the toolbar shows them).
struct OverviewGroup {
  QString caption;
  QVector<OverviewRow> rows;
};

// A command's keys as the overview writes them: each in the native text
// ("Ctrl+Z", a sequence "G, L"); several joined by ", ", or by "; " when
// one of them is a sequence, whose own comma would make them run together.
QString overviewKeysText(const QList<QKeySequence>& keys);

// The commands that have keys (the user's current ones, the effective keys
// of the registry), one group per `groupOf(id)` (the command's home on the
// toolbar, "Solid › Create", or "General"), the groups in the order of
// their first command, the rows in the registry's order.
QVector<OverviewGroup> commandShortcutGroups(const CommandRegistry& registry,
                                             const std::function<QString(const QString& id)>& groupOf);

// Whether a row is one the overview's search leaves: its keys or what it
// does contains `filter`, in any case; every row for an empty one.
bool overviewRowMatches(const OverviewRow& row, const QString& filter);

// The cheat sheet's contents.
struct CheatSheet {
  QString title;    // "Keyboard and Mouse Overview"
  QString subtitle; // the mouse scheme, the search and the date
  QVector<build::ListTable> tables;
};

// The cheat sheet of the overview's rows: one table per group with a row
// that the search leaves (`filter`; all of them for an empty one), its
// caption the group's, its columns Keys or mouse and Does; titled
// "Keyboard and Mouse Overview", with the mouse scheme (`scheme`, its name),
// the search when there is one and the date under it, set apart by " · "
// ("Mouse: Middle pans, Shift + middle orbits · Search: Sketch ·
// 2026-11-19").
CheatSheet shortcutCheatSheet(const QVector<OverviewGroup>& groups, const QString& filter, const QString& scheme,
                              const QDate& date);

// The file name Save as PDF… offers.
QString cheatSheetFileName();

} // namespace mitcad
