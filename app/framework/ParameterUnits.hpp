// SPDX-License-Identifier: MIT
#pragma once

#include <QString>
#include <QStringList>
#include <QStyledItemDelegate>

class QAbstractItemModel;
class QComboBox;
class QModelIndex;
class QStyleOptionViewItem;
class QWidget;

namespace mitcad {

// A parameter's Unit list (mitcad#1920, GitHub mitcad/Mitcad#378): the
// units, then Number for a number without a unit. Number is only what a
// person sees: the model's unit of such a parameter is empty, in the
// commands sent and in the answers read.

// The units Add User Parameter and the parameters table's Unit column
// offer before Number: mm, cm, m, in, ft, deg, rad and Text.
QStringList parameterUnits();

// "Number", translated.
QString numberUnitText();

// What a Unit list and the Unit column show for the model's `unit`:
// Number for the empty unit, else the unit.
QString unitDisplayText(const QString& unit);

// The model's unit of a text a Unit list shows or a person typed: empty
// for Number (translated or in English, in any case) and for nothing, else
// the text without its surrounding spaces.
QString unitOfText(const QString& text);

// Fills an editable `box` with `units` and Number as its last row; text
// typed in is not added to the rows.
void fillUnitBox(QComboBox* box, const QStringList& units);

// Selects the model's `unit` in `box`: Number for the empty unit, its row
// when the box lists it, else typed in.
void selectUnitInBox(QComboBox* box, const QString& unit);

// The model's unit `box` holds: empty when it shows Number.
QString unitOfBox(const QComboBox* box);

// The parameters table's Unit column (mitcad#1920): its cell shows the
// unit as unitDisplayText does and is edited in a Unit list of
// parameterUnits() that starts at the cell's unit (Number for none); what
// it writes back into the cell is the model's unit (empty for Number),
// and nothing when the unit is unchanged.
class ParameterUnitDelegate : public QStyledItemDelegate {
public:
  using QStyledItemDelegate::QStyledItemDelegate;

  QWidget* createEditor(QWidget* parent, const QStyleOptionViewItem& option,
                        const QModelIndex& index) const override;
  void setEditorData(QWidget* editor, const QModelIndex& index) const override;
  void setModelData(QWidget* editor, QAbstractItemModel* model, const QModelIndex& index) const override;
};

} // namespace mitcad
