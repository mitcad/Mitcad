// SPDX-License-Identifier: MIT
#include "EnterRepeat.hpp"

#include <QAbstractButton>
#include <QAbstractItemView>
#include <QAbstractSpinBox>
#include <QAction>
#include <QComboBox>
#include <QCoreApplication>
#include <QEvent>
#include <QKeyEvent>
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

RepeatKeyFilter::RepeatKeyFilter(QAction* action, QWidget* window)
    : QObject(window), m_action(action), m_window(window) {
  if (QCoreApplication::instance() != nullptr) {
    QCoreApplication::instance()->installEventFilter(this);
  }
}

RepeatKeyFilter::~RepeatKeyFilter() {
  if (QCoreApplication::instance() != nullptr) {
    QCoreApplication::instance()->removeEventFilter(this);
  }
}

QKeySequence RepeatKeyFilter::takeKey() {
  QKeySequence key = m_key;
  m_key = QKeySequence();
  return key;
}

bool RepeatKeyFilter::eventFilter(QObject* watched, QEvent* event) {
  if (event->type() != QEvent::ShortcutOverride || !watched->isWidgetType()) {
    return false;
  }
  // The offer goes to the focus widget first and then up through its
  // parents while nobody takes it: only the first (the focus) decides.
  auto* widget = static_cast<QWidget*>(watched);
  const QWidget* focus = widget->window()->focusWidget();
  if (focus != nullptr && focus != widget) {
    return false;
  }
  m_key = QKeySequence();
  if (m_action.isNull() || m_window.isNull() || !m_action->isEnabled()) {
    return false;
  }
  auto* key = static_cast<QKeyEvent*>(event);
  const QKeySequence pressed(
      QKeyCombination(key->modifiers() & ~Qt::KeypadModifier, static_cast<Qt::Key>(key->key())));
  if (!m_action->shortcuts().contains(pressed)) {
    return false;
  }
  if (focusTakesEnter(widget)) {
    // As the widget had asked for the key: no shortcut, the press is its.
    event->accept();
    return true;
  }
  if (m_window->isVisible() && (widget == m_window || m_window->isAncestorOf(widget))) {
    m_key = pressed;
  }
  return false;
}

} // namespace mitcad
