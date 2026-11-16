// SPDX-License-Identifier: MIT
// The unit Number (mitcad#1920, GitHub mitcad/Mitcad#378): the Unit list of
// Add User Parameter ends with Number, which is the model's empty unit;
// the empty unit selects it; the parameters table's Unit column shows
// Number for a parameter without a unit, is edited in the same list, and
// writes back the model's unit (empty for Number), nothing when unchanged.
// On Qt's offscreen platform.
#include "UnitTests.hpp"
#include "framework/ParameterUnits.hpp"

#include <QAbstractItemModel>
#include <QComboBox>
#include <QModelIndex>
#include <QString>
#include <QStringList>
#include <QStyleOptionViewItem>
#include <QTreeWidget>
#include <QTreeWidgetItem>
#include <QWidget>

#include <cstdio>

int parameterUnitsTests() {
  using namespace mitcad;
  int failures = 0;
  const auto check = [&failures](bool ok, const char* message) {
    if (!ok) {
      std::fprintf(stderr, "parameter units: %s\n", message);
      ++failures;
    }
  };

  // The units Add User Parameter offers, then Number.
  const QStringList units = parameterUnits();
  check(units == QStringList({QStringLiteral("mm"), QStringLiteral("cm"), QStringLiteral("m"),
                              QStringLiteral("in"), QStringLiteral("ft"), QStringLiteral("deg"),
                              QStringLiteral("rad"), QStringLiteral("Text")}),
        "the units are mm, cm, m, in, ft, deg, rad and Text");
  check(numberUnitText() == QStringLiteral("Number"), "the unit without a unit reads Number");
  check(unitDisplayText(QString()) == QStringLiteral("Number"), "the empty unit is shown as Number");
  check(unitDisplayText(QStringLiteral("mm")) == QStringLiteral("mm"), "a unit is shown as it is");
  check(unitOfText(QStringLiteral("Number")).isEmpty(), "Number is the empty unit");
  check(unitOfText(QStringLiteral(" number ")).isEmpty(), "number typed in any case is the empty unit");
  check(unitOfText(QString()).isEmpty(), "nothing is the empty unit");
  check(unitOfText(QStringLiteral(" in ")) == QStringLiteral("in"), "a unit typed with spaces is the unit");

  // The Unit list: its last row is Number, and it adds the empty unit.
  QComboBox box;
  fillUnitBox(&box, units);
  check(box.isEditable(), "the Unit list takes a typed unit");
  check(box.count() == units.size() + 1, "the Unit list has the units and Number");
  check(box.itemText(box.count() - 1) == QStringLiteral("Number"), "the Unit list's last row is Number");
  check(box.findText(QString()) < 0, "the Unit list has no empty row");
  box.setCurrentIndex(box.count() - 1);
  check(unitOfBox(&box).isEmpty(), "Number chosen: the parameter is added with the empty unit");
  box.setCurrentIndex(0);
  check(unitOfBox(&box) == QStringLiteral("mm"), "mm chosen: the unit mm");
  box.setCurrentText(QStringLiteral("Number"));
  check(unitOfBox(&box).isEmpty(), "Number typed: the empty unit");

  // A parameter without a unit selects Number; a unit its row; another
  // unit is typed in.
  selectUnitInBox(&box, QStringLiteral("deg"));
  check(box.currentIndex() == units.indexOf(QStringLiteral("deg")), "deg selects its row");
  selectUnitInBox(&box, QString());
  check(box.currentIndex() == box.count() - 1 && box.currentText() == QStringLiteral("Number"),
        "the empty unit selects Number");
  selectUnitInBox(&box, QStringLiteral("kg"));
  check(box.currentText() == QStringLiteral("kg") && unitOfBox(&box) == QStringLiteral("kg"),
        "a unit the list does not have is typed in");

  // The Unit column: Number for a parameter without a unit; its editor is
  // the Unit list at Number; unchanged, nothing is written back.
  QTreeWidget tree;
  tree.setColumnCount(2);
  auto* delegate = new ParameterUnitDelegate(&tree);
  tree.setItemDelegateForColumn(1, delegate);
  auto* row = new QTreeWidgetItem(&tree, {QStringLiteral("count"), unitDisplayText(QString())});
  row->setFlags(row->flags() | Qt::ItemIsEditable);
  int changes = 0;
  QObject::connect(&tree, &QTreeWidget::itemChanged, &tree, [&changes](QTreeWidgetItem*, int) { ++changes; });
  check(row->text(1) == QStringLiteral("Number"), "the Unit column shows Number for no unit");
  const QModelIndex cell = tree.model()->index(0, 1);
  const QStyleOptionViewItem option;
  QWidget* editor = delegate->createEditor(tree.viewport(), option, cell);
  auto* unitEditor = qobject_cast<QComboBox*>(editor);
  check(unitEditor != nullptr, "the Unit cell is edited in a Unit list");
  if (unitEditor != nullptr) {
    check(unitEditor->count() == units.size() + 1 &&
              unitEditor->itemText(unitEditor->count() - 1) == QStringLiteral("Number"),
          "the cell's Unit list ends with Number");
    delegate->setEditorData(unitEditor, cell);
    check(unitEditor->currentIndex() == unitEditor->count() - 1, "a parameter without a unit selects Number");
    delegate->setModelData(unitEditor, tree.model(), cell);
    check(changes == 0 && row->text(1) == QStringLiteral("Number"), "Number kept: nothing is sent");

    // mm chosen: the cell gets the unit mm.
    unitEditor->setCurrentIndex(0);
    delegate->setModelData(unitEditor, tree.model(), cell);
    check(changes == 1 && row->text(1) == QStringLiteral("mm"), "mm chosen: the unit mm is sent");

    // Number chosen for a millimetre parameter: the model's empty unit.
    delegate->setEditorData(unitEditor, cell);
    check(unitEditor->currentText() == QStringLiteral("mm"), "a parameter in mm selects mm");
    unitEditor->setCurrentIndex(unitEditor->count() - 1);
    delegate->setModelData(unitEditor, tree.model(), cell);
    check(changes == 2 && row->text(1).isEmpty(), "Number chosen: the empty unit is sent");
  }
  delete editor;

  // A refused entry left in the cell starts the editor at it.
  row->setText(1, QStringLiteral("kg"));
  QWidget* again = delegate->createEditor(tree.viewport(), option, cell);
  if (auto* refusedEditor = qobject_cast<QComboBox*>(again)) {
    delegate->setEditorData(refusedEditor, cell);
    check(refusedEditor->currentText() == QStringLiteral("kg"), "a refused unit stays in the editor");
  }
  delete again;
  return failures;
}
MITCAD_UNIT_TEST(parameterUnitsTests);
