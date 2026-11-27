// SPDX-License-Identifier: MIT
#pragma once

// Insert Parameter in a drawing's text editors (mitcad#2483, app/COMMANDS.md
// "Drawings"): the Text and Note tools' Text field, the Note dialog (a
// Text's or a Note's Edit Text…) and Edit Title Block's field Text. Their
// standard context menu (a right-click, or the Menu key / Shift+F10) ends
// with Insert Parameter…, which lists the design's parameters (the user
// parameters first) with their values; the one chosen goes in at the
// cursor as {param:Name} (ParameterReferences.hpp). A field that refers to
// a parameter the design lacks says so under it.
//
// Logs, for the UI tests: "Insert Parameter menu: <field>" when the menu
// opens, "Insert Parameter dialog opened: <n> parameters", "Insert
// Parameter dialog: <name>" on OK and "Parameter inserted in <field>:
// <reference>".

#include <functional>
#include <optional>

#include <QJsonArray>
#include <QString>
#include <QStringList>

class QLabel;
class QLineEdit;
class QPlainTextEdit;
class QWidget;

namespace mitcad::drawings {

// The design's parameters, as the model's `parameters` query gives them.
using ParameterList = std::function<QJsonArray()>;

// The Insert Parameter dialog: the parameters (a filter over their names),
// the one chosen; none when cancelled or when there are none.
std::optional<QString> askParameter(QWidget* parent, const QJsonArray& parameters);

// Gives the field's context menu Insert Parameter…; `inserted` (when set)
// runs after a reference went in.
void addInsertParameter(QLineEdit* edit, ParameterList parameters, std::function<void()> inserted = {});
void addInsertParameter(QPlainTextEdit* edit, ParameterList parameters);

// The warning under a field whose text refers to parameters the design
// lacks (`missing`); empty when it lacks none.
QString missingParametersText(const QStringList& missing);

// A warning label for under a field (its object name `name`), hidden; and
// shows on it the warning of `text`'s parameters the design lacks.
QLabel* missingParametersLabel(const QString& name);
void showMissingParameters(QLabel* label, const QString& text, const QJsonArray& parameters);

} // namespace mitcad::drawings
