// SPDX-License-Identifier: MIT
#pragma once

#include <QtGlobal>

#include "MacDialogs.hpp"
#include "NativeDialogs.hpp"

class QDialog;
class QFormLayout;
class QLineEdit;
class QModelIndex;
class QPushButton;
class QStyleOptionViewItem;
class QTreeWidget;
class QTreeWidgetItem;

#ifdef MITCAD_MAC_UI
#include <algorithm>

#include <QApplication>
#include <QDialog>
#include <QEvent>
#include <QFontDatabase>
#include <QFormLayout>
#include <QHeaderView>
#include <QLineEdit>
#include <QPushButton>
#include <QStyle>
#include <QStyledItemDelegate>
#include <QTreeWidget>

#include "MacChrome.hpp"
#include "../framework/Theme.hpp"
#endif

// Qt stays the macOS frontend. Keep its native controls, focus rings and
// selection colours while giving the parameter sheet the system's spacing.
namespace mitcad::mac {

// The empty row at the end of the Parameters window's User Parameters
// (mitcad#2202), in its Name cell: a row to type a new parameter in, not
// a section heading (whose rows have no parameter's name either).
inline constexpr int NewParameterRowRole = Qt::UserRole + 73; // after the groups' roles (mitcad#2180)

#ifdef MITCAD_MAC_UI
namespace parameters {

// The shared role, so that the AppKit table shows the same state.
constexpr int kRefusedRole = NativeRefusedRole;

} // namespace parameters

// Column delegates override the table's generic delegate. Both must apply
// the same semantic cell state before either native or numeric painting.
inline void styleParameterCell(QStyleOptionViewItem* option, const QModelIndex& index) {
  if (index.siblingAtColumn(0).data(Qt::UserRole).toString().isEmpty() &&
      !index.siblingAtColumn(0).data(NewParameterRowRole).toBool()) {
    option->font.setWeight(QFont::DemiBold);
  }
  if (index.data(parameters::kRefusedRole).toBool() && !option->state.testFlag(QStyle::State_Selected)) {
    const QColor error = errorColor(option->palette);
    const QColor base = option->palette.color(QPalette::Base);
    option->palette.setColor(QPalette::Text, error);
    option->backgroundBrush = QColor((base.red() * 9 + error.red()) / 10,
                                     (base.green() * 9 + error.green()) / 10,
                                     (base.blue() * 9 + error.blue()) / 10);
  }
}

namespace parameters {

class RowDelegate : public dialogs::ListRowDelegate {
public:
  using dialogs::ListRowDelegate::ListRowDelegate;

protected:
  void initStyleOption(QStyleOptionViewItem* option, const QModelIndex& index) const override {
    QStyledItemDelegate::initStyleOption(option, index);
    styleParameterCell(option, index);
  }
};

} // namespace parameters

inline void styleParameterForm(QDialog* dialog, QFormLayout* form) {
  styleDialog(dialog);
  form->setContentsMargins(20, 20, 20, 20);
  form->setHorizontalSpacing(12);
  form->setVerticalSpacing(12);
  form->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);
  dialog->setMinimumWidth(420);
}

// Parameters are added in the table's empty row and deleted by their rows'
// buttons (mitcad#2202): the window has no Add and Delete buttons.
inline void styleParametersDialog(QDialog* dialog, QTreeWidget* table, QLineEdit* filter, QPushButton* ok) {
  styleDialog(dialog);
  dialog->resize(1240, 590); // with the limits' columns (mitcad#665) and the tabs (mitcad#667)
  dialog->setMinimumSize(640, 360);
  dialog->layout()->setContentsMargins(20, 20, 20, 20);
  dialog->layout()->setSpacing(12);
  filter->setMinimumWidth(180);
  filter->setMaximumWidth(260);
  styleSearchField(filter);
  ok->setMinimumHeight(28);
  table->setItemDelegate(new parameters::RowDelegate(table));
  table->setSelectionBehavior(QAbstractItemView::SelectRows);
  table->setAllColumnsShowFocus(true);
  table->header()->setMinimumHeight(28);
  table->header()->setDefaultAlignment(Qt::AlignLeft | Qt::AlignVCenter);
  table->header()->setHighlightSections(false);
  table->setAttribute(Qt::WA_MacShowFocusRect, true);
}

inline bool styleRefusedParameter(QTreeWidgetItem* row, int column) {
  // Store the state, not a colour: the delegate follows a live theme change
  // and leaves selected text to QMacStyle's HighlightedText role.
  row->setData(column, parameters::kRefusedRole, true);
  return true;
}

#else

inline void styleParameterCell(QStyleOptionViewItem*, const QModelIndex&) {}
inline void styleParameterForm(QDialog*, QFormLayout*) {}
inline void styleParametersDialog(QDialog*, QTreeWidget*, QLineEdit*, QPushButton*) {}
inline bool styleRefusedParameter(QTreeWidgetItem*, int) { return false; }

#endif

} // namespace mitcad::mac
