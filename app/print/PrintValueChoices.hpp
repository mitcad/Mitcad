// SPDX-License-Identifier: MIT
#pragma once

// The print settings Add Print Value… offers for a body on the Make tab
// (mitcad#1178, app/COMMANDS.md "The selected body's card"): the keys of
// the `print_keys` query, grouped and named as the Properties window shows
// them (PrintKeyTable: category › sub-group headings, plain names), with
// the value the body prints with now. Plain Qt Core.

#include <QJsonObject>
#include <QJsonValue>
#include <QString>
#include <QVector>

#include "PrintKeys.hpp"

namespace mitcad {

struct PrintValueChoice {
  QString set;      // "process" or "printer"
  QString key;      // the key map's ("perimeters")
  QString name;     // its plain name ("Walls")
  QString heading;  // its category › sub-group, as the Properties window heads it
  QString text;     // the value the body prints with now, as shown; empty without one
  // That value as the model gives it; undefined without one.
  QJsonValue value = QJsonValue(QJsonValue::Undefined);
};

// The settings a body can take as its own, from the key table and the
// body's `print_settings` answer: the process keys, then the printer
// keys, each set in the table's display order (its layout's categories and
// sub-groups). Left out, as in the Properties window, a value kept per
// slicer (a preset's name) and the filament's density; and every key the
// body sets itself (a `property` value of the level `body`). The filament
// values are the material's (`set_property` refuses `Print Filament` on a
// body): none.
QVector<PrintValueChoice> printValueChoices(const PrintKeyTable& keys, const QJsonObject& settings);

// A category › sub-group heading as the Properties window writes it: the
// category's title alone when the group has the same title.
QString printGroupHeading(const PrintCategory& category, const PrintGroup& group);

// Whether a choice matches the search text: every word of it in the plain
// name or the key, ignoring case. Empty text matches every choice.
bool printValueMatches(const PrintValueChoice& choice, const QString& search);

// The choice the search puts first: the visible one whose plain name is the
// text (ignoring case), else the first that matches; -1 for none.
int printValueBestMatch(const QVector<PrintValueChoice>& choices, const QString& search);

} // namespace mitcad
