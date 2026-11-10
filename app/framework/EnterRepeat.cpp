// SPDX-License-Identifier: MIT
#include "EnterRepeat.hpp"

#include <QAbstractButton>
#include <QAbstractItemView>
#include <QAbstractSpinBox>
#include <QComboBox>
#include <QLineEdit>
#include <QPlainTextEdit>
#include <QTextEdit>
#include <QWidget>

namespace mitcad {

bool focusTakesEnter(const QWidget* focus) {
  if (focus == nullptr) {
    return false;
  }
  return qobject_cast<const QLineEdit*>(focus) != nullptr || qobject_cast<const QAbstractSpinBox*>(focus) != nullptr ||
         qobject_cast<const QTextEdit*>(focus) != nullptr || qobject_cast<const QPlainTextEdit*>(focus) != nullptr ||
         qobject_cast<const QComboBox*>(focus) != nullptr || qobject_cast<const QAbstractItemView*>(focus) != nullptr ||
         qobject_cast<const QAbstractButton*>(focus) != nullptr;
}

} // namespace mitcad
