// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QPair>
#include <QString>
#include <QVector>

namespace mitcad {

// Remembered values of command panels (mitcad#1028, app/COMMANDS.md
// "Remembered values"): a value input marked so (InputDef::withRemembered)
// starts a new feature's panel with what was given in it on that command's
// last OK, from any design, kept in the local settings
// ("commands/<command id>/<input id>", commands/solid.fillet/radius=3 mm).
// Editing a feature and Cancel keep nothing. Plain Qt Core: CommandSession
// asks and tells it, and tests use it without a panel.

// Whether panels remember values: on unless MITCAD_REMEMBER_PANEL_VALUES
// is 0, which the UI test libraries set, so that a value given in one step
// never changes where a later step's panel starts; a test of the
// remembered values sets it to 1.
bool panelValuesRemembered();

// The settings key of an input's remembered value.
QString rememberedValueKey(const QString& command, const QString& input);

// What is kept of a field: its expression ("3 mm", also for a bare "3"
// typed where the expression got the document's unit), or its text when it
// has no expression yet, without the name of a parameter it names
// ("r = 3 mm" keeps "3 mm": the next feature would name another parameter
// the same); empty when there is nothing to keep.
QString rememberedText(const QString& text, const QString& expression);

// The remembered value of an input; empty when none is kept.
QString rememberedValue(const QString& command, const QString& input);

// Where a new feature's panel starts an input marked to remember.
struct PanelStart {
  QString text;           // what the field starts with
  QString remembered;     // the remembered value, empty when none is kept
  bool fromMemory = false; // text is the remembered value
};

// The remembered value when one is kept and `evaluates` takes it (an
// expression of the design at hand: a parameter it names is there),
// otherwise the default, without an error.
PanelStart panelStartText(const QString& command, const QString& input, const QString& defaultText,
                          const std::function<bool(const QString&)>& evaluates);

// A panel ended: on a new feature's OK (committed, not editing) the fields
// given, (input id, rememberedText) pairs, are kept, an empty text left as
// it was; Cancel or an edit keeps nothing.
void rememberPanelValues(const QString& command, const QVector<QPair<QString, QString>>& fields,
                         bool committed, bool editing);

} // namespace mitcad
