// SPDX-License-Identifier: MIT
#pragma once

// Parameters in a drawing's texts (mitcad#2483, app/COMMANDS.md
// "Drawings"): `{param:Length}` in a Text, a Note or a title block field
// stands for the value of the design's parameter Length, which the model
// fills in whenever it draws the sheet (core/model/src/drawing/
// parameter_text.rs reads them the same way). What the editors need of
// them: the reference Insert Parameter writes, the parameters a text refers
// to, those the design lacks, and the parameters as Insert Parameter lists
// them from the model's `parameters` query. Plain Qt Core, so that the tests
// check it without a window.

#include <QJsonArray>
#include <QList>
#include <QString>
#include <QStringList>

namespace mitcad::drawings {

// A parameter as Insert Parameter lists it: its name and its value as the
// sheet shows it (a number with its unit, an angle in °, a text parameter's
// text).
struct ParameterEntry {
  QString name;
  QString value;
  // A user parameter (Edit Parameters' own), listed before the model's.
  bool user = false;
};

// The reference to a parameter: {param:Length}.
QString parameterReference(const QString& name);

// The parameters a text refers to, each once, in order: the names of its
// {param:Name} (the prefix in any case, spaces around the name left out);
// other braces are the text's own.
QStringList referencedParameters(const QString& text);

// The parameters `text` refers to that are not in `parameters` (the
// `parameters` query), each once.
QStringList missingParameters(const QString& text, const QJsonArray& parameters);

// The parameters of the `parameters` query as Insert Parameter lists them:
// the user parameters first, then the others, each in the query's order.
QList<ParameterEntry> parameterEntries(const QJsonArray& parameters);

} // namespace mitcad::drawings
