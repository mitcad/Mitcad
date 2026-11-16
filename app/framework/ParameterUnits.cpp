// SPDX-License-Identifier: MIT
#include "ParameterUnits.hpp"

#include <QAbstractItemModel>
#include <QComboBox>
#include <QCoreApplication>
#include <QModelIndex>
#include <QVariant>

namespace mitcad {

QStringList parameterUnits() {
  return {QStringLiteral("mm"),  QStringLiteral("cm"),  QStringLiteral("m"),   QStringLiteral("in"),
          QStringLiteral("ft"),  QStringLiteral("deg"), QStringLiteral("rad"), QStringLiteral("Text")};
}

QString numberUnitText() {
  return QCoreApplication::translate("mitcad::ParameterUnits", "Number");
}

QString unitDisplayText(const QString& unit) {
  return unit.trimmed().isEmpty() ? numberUnitText() : unit;
}

QString unitOfText(const QString& text) {
  const QString unit = text.trimmed();
  if (unit.compare(numberUnitText(), Qt::CaseInsensitive) == 0 ||
      unit.compare(QStringLiteral("Number"), Qt::CaseInsensitive) == 0) {
    return QString();
  }
  return unit;
}

void fillUnitBox(QComboBox* box, const QStringList& units) {
  box->setEditable(true);
  box->setInsertPolicy(QComboBox::NoInsert);
  box->addItems(units);
  box->addItem(numberUnitText());
}

void selectUnitInBox(QComboBox* box, const QString& unit) {
  const QString shown = unitDisplayText(unit);
  const int item = box->findText(shown);
  if (item >= 0) {
    box->setCurrentIndex(item);
  } else {
    box->setCurrentText(shown);
  }
}

QString unitOfBox(const QComboBox* box) {
  return unitOfText(box->currentText());
}

QWidget* ParameterUnitDelegate::createEditor(QWidget* parent, const QStyleOptionViewItem& /*option*/,
                                             const QModelIndex& /*index*/) const {
  auto* box = new QComboBox(parent);
  box->setObjectName(QStringLiteral("parameterUnitEditor"));
  fillUnitBox(box, parameterUnits());
  return box;
}

void ParameterUnitDelegate::setEditorData(QWidget* editor, const QModelIndex& index) const {
  auto* box = qobject_cast<QComboBox*>(editor);
  if (box == nullptr) {
    QStyledItemDelegate::setEditorData(editor, index);
    return;
  }
  // The cell's text: the unit, Number, or a refused entry left in it.
  selectUnitInBox(box, unitOfText(index.data(Qt::EditRole).toString()));
}

void ParameterUnitDelegate::setModelData(QWidget* editor, QAbstractItemModel* model,
                                         const QModelIndex& index) const {
  auto* box = qobject_cast<QComboBox*>(editor);
  if (box == nullptr) {
    QStyledItemDelegate::setModelData(editor, model, index);
    return;
  }
  const QString unit = unitOfBox(box);
  if (unit == unitOfText(index.data(Qt::EditRole).toString())) {
    return; // unchanged: no command
  }
  model->setData(index, unit, Qt::EditRole);
}

} // namespace mitcad
