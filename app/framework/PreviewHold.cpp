// SPDX-License-Identifier: MIT
#include "PreviewHold.hpp"

#include <utility>

#include <QAbstractSpinBox>
#include <QComboBox>
#include <QCoreApplication>
#include <QEvent>
#include <QGuiApplication>
#include <QKeyEvent>
#include <QLineEdit>
#include <QMouseEvent>
#include <QPlainTextEdit>
#include <QTextEdit>
#include <QWidget>
#include <QtLogging>

namespace mitcad {
namespace {

// Keys that only change what a click or another key means: held with Ctrl
// while the preview is hidden, they leave it hidden (Ctrl+Shift+click adds
// and widens, docs/user-guide.md "Selection filter").
bool isModifierKey(int key) {
  return key == Qt::Key_Shift || key == Qt::Key_Alt || key == Qt::Key_Meta || key == Qt::Key_AltGr ||
         key == Qt::Key_CapsLock || key == Qt::Key_Super_L || key == Qt::Key_Super_R;
}

} // namespace

PreviewHold::PreviewHold(Actions actions, QObject* parent) : QObject(parent), m_actions(std::move(actions)) {}

void PreviewHold::install() {
  if (auto* application = QCoreApplication::instance()) {
    application->installEventFilter(this);
  }
}

bool PreviewHold::eventFilter(QObject* watched, QEvent* event) {
  handle(event);
  return QObject::eventFilter(watched, event); // nothing is taken
}

void PreviewHold::handle(QEvent* event) {
  switch (event->type()) {
  case QEvent::ShortcutOverride:
  case QEvent::KeyPress: {
    // A key goes to the focused widget and up through its parents: the
    // filter sees it once per receiver, so each step is the same again.
    const auto* key = static_cast<const QKeyEvent*>(event);
    if (key->key() == Qt::Key_Control) {
      if (!key->isAutoRepeat()) {
        ctrlPressed(key->modifiers());
      }
      return;
    }
    if (m_state != State::Idle && !key->modifiers().testFlag(Qt::ControlModifier)) {
      released(QStringLiteral("Ctrl released")); // its release went elsewhere
      return;
    }
    if (!isModifierKey(key->key())) {
      otherKey();
    }
    return;
  }
  case QEvent::Shortcut:
    otherKey();
    return;
  case QEvent::KeyRelease: {
    const auto* key = static_cast<const QKeyEvent*>(event);
    if (key->key() == Qt::Key_Control && !key->isAutoRepeat()) {
      released(QStringLiteral("Ctrl released"));
    }
    return;
  }
  case QEvent::MouseButtonRelease:
    m_buttons = static_cast<const QMouseEvent*>(event)->buttons();
    return;
  case QEvent::Wheel:
  case QEvent::MouseButtonPress:
  case QEvent::MouseButtonDblClick: {
    if (event->type() != QEvent::Wheel) {
      // The buttons held, as the application's own mouse events say: a
      // Ctrl pressed during a drag is not alone.
      m_buttons = static_cast<const QMouseEvent*>(event)->buttons();
    }
    // With Ctrl down a click (a Ctrl+click) and the wheel leave the
    // preview as it is; without it, Ctrl's release went elsewhere.
    const auto* input = static_cast<const QInputEvent*>(event);
    if (m_state != State::Idle && !input->modifiers().testFlag(Qt::ControlModifier)) {
      released(QStringLiteral("Ctrl released"));
    }
    return;
  }
  case QEvent::ApplicationDeactivate:
    m_buttons = Qt::NoButton; // their releases go elsewhere
    released(QStringLiteral("the application deactivated")); // as Shift's scope does
    return;
  case QEvent::ApplicationStateChange:
    if (QGuiApplication::applicationState() != Qt::ApplicationActive) {
      m_buttons = Qt::NoButton;
      released(QStringLiteral("the application deactivated"));
    }
    return;
  default:
    return;
  }
}

void PreviewHold::ctrlPressed(Qt::KeyboardModifiers modifiers) {
  if (m_state != State::Idle) {
    // The same press again: its offer, the press, each parent it passes on
    // to, a floating panel's copy sent on to the main window, a platform's
    // repeated press while it is held (auto-repeat or not), and a press
    // whose modifiers say Ctrl or not (QKeyEvent gives a modifier key's own
    // modifier toggled). None of them changes anything.
    return;
  }
  // Not alone, or not here: nothing until its release. Ctrl's own
  // modifier is left out, whichever way the platform reports it.
  const Qt::KeyboardModifiers others = Qt::ShiftModifier | Qt::AltModifier | Qt::MetaModifier;
  if ((modifiers & others) != Qt::KeyboardModifiers()) {
    refuse(QStringLiteral("Ctrl is not alone"));
    return;
  }
  if (m_buttons != Qt::NoButton) {
    refuse(QStringLiteral("a mouse button is held"));
    return;
  }
  if (m_actions.mayHide && !m_actions.mayHide()) {
    m_state = State::Shown; // mayHide logs why
    return;
  }
  m_state = State::Hidden;
  if (m_actions.hide) {
    m_actions.hide();
  }
}

void PreviewHold::refuse(const QString& why) {
  m_state = State::Shown;
  qDebug().noquote() << QStringLiteral("Preview hold refused: %1").arg(why);
}

void PreviewHold::otherKey() {
  if (m_state != State::Hidden) {
    return;
  }
  // A shortcut (Ctrl+Z, Ctrl+S): the preview shows again at once, and the
  // shortcut runs as before; it stays shown until Ctrl is pressed again.
  m_state = State::Shown;
  if (m_actions.show) {
    m_actions.show(QStringLiteral("a key pressed"));
  }
}

void PreviewHold::released(const QString& why) {
  const State was = std::exchange(m_state, State::Idle);
  if (was == State::Hidden && m_actions.show) {
    m_actions.show(why);
  }
}

bool PreviewHold::editsText(const QWidget* widget) {
  if (widget == nullptr) {
    return false;
  }
  if (const auto* line = qobject_cast<const QLineEdit*>(widget)) {
    return !line->isReadOnly();
  }
  if (const auto* text = qobject_cast<const QTextEdit*>(widget)) {
    return !text->isReadOnly();
  }
  if (const auto* plain = qobject_cast<const QPlainTextEdit*>(widget)) {
    return !plain->isReadOnly();
  }
  if (const auto* spin = qobject_cast<const QAbstractSpinBox*>(widget)) {
    return !spin->isReadOnly();
  }
  if (const auto* combo = qobject_cast<const QComboBox*>(widget)) {
    return combo->isEditable();
  }
  return false;
}

} // namespace mitcad
