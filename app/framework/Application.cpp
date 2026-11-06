// SPDX-License-Identifier: MIT
#include "Application.hpp"

#include <QEvent>
#include <QKeyEvent>
#include <QKeySequence>
#include <QMetaEnum>
#include <QObject>
#include <QtGlobal>
#include <QtLogging>

#include "ModelBusy.hpp"

namespace mitcad {

namespace {

bool& assertDrops() {
  static bool on = true;
  return on;
}

} // namespace

Application::Application(int& argc, char** argv) : QApplication(argc, argv) {}

bool Application::notify(QObject* receiver, QEvent* event) {
  try {
    return QApplication::notify(receiver, event);
  } catch (const ModelBusy&) {
    // Read while a job computes the model (mitcad#1360): the event is
    // dropped instead of ending the application.
    qWarning().noquote() << droppedInputText(receiver, event);
    if (event != nullptr && event->type() == QEvent::Close) {
      event->ignore(); // the window stays
    }
#ifndef QT_NO_DEBUG
    if (assertDrops()) {
      Q_ASSERT_X(false, "Application::notify", "a ModelBusy escaped an event handler");
    }
#endif
    return true;
  }
}

QString Application::droppedInputText(const QObject* receiver, const QEvent* event) {
  QString what = QStringLiteral("no event");
  if (event != nullptr) {
    const char* name = QMetaEnum::fromType<QEvent::Type>().valueToKey(static_cast<int>(event->type()));
    what = name != nullptr ? QString::fromLatin1(name)
                           : QStringLiteral("event %1").arg(static_cast<int>(event->type()));
    const QEvent::Type type = event->type();
    if (type == QEvent::KeyPress || type == QEvent::KeyRelease || type == QEvent::ShortcutOverride) {
      const auto* key = static_cast<const QKeyEvent*>(event);
      // A modifier's own key by its name alone ("Shift", not "Shift+Shift").
      const int code = key->key();
      const bool modifier =
          code == Qt::Key_Shift || code == Qt::Key_Control || code == Qt::Key_Alt || code == Qt::Key_Meta;
      const QKeySequence sequence = modifier ? QKeySequence(code) : QKeySequence(key->keyCombination());
      const QString pressed = sequence.toString(QKeySequence::PortableText);
      if (!pressed.isEmpty()) {
        what += QLatin1Char(' ') + pressed;
      }
    }
  }
  QString on = QStringLiteral("nothing");
  if (receiver != nullptr) {
    on = QString::fromLatin1(receiver->metaObject()->className());
    if (!receiver->objectName().isEmpty()) {
      on += QLatin1Char(' ') + receiver->objectName();
    }
  }
  return QStringLiteral("Input dropped while the model computes: %1 on %2").arg(what, on);
}

void Application::assertOnDroppedInput(bool on) {
  assertDrops() = on;
}

} // namespace mitcad
