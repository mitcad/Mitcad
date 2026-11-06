// SPDX-License-Identifier: MIT
// The application's safety net for a read of the model while a job
// computes it (mitcad#1360): a ModelBusy out of an event filter, on the
// receiver or application-wide, is logged and the event dropped; the
// receiver does not get it, nothing escapes the delivery, and a close
// dropped so keeps its window open. test_app runs as mitcad::Application.
#include "framework/Application.hpp"
#include "framework/ModelBusy.hpp"

#include <cstdio>

#include <QCloseEvent>
#include <QCoreApplication>
#include <QEvent>
#include <QKeyEvent>
#include <QLineEdit>
#include <QObject>
#include <QString>
#include <QStringList>
#include <QWidget>
#include <QtLogging>

namespace {

QStringList& logged() {
  static QStringList messages;
  return messages;
}

QtMessageHandler previousHandler = nullptr;

void keepLog(QtMsgType, const QMessageLogContext&, const QString& message) {
  logged() << message;
}

// A filter that reads the model on a key press while a job computes it, as
// a panel's length field did on Up during its preview (mitcad#1360).
class ReadingFilter : public QObject {
public:
  int presses = 0;

protected:
  bool eventFilter(QObject* watched, QEvent* event) override {
    if (event->type() == QEvent::KeyPress) {
      ++presses;
      throw mitcad::ModelBusy();
    }
    return QObject::eventFilter(watched, event);
  }
};

// A field that counts the key presses it gets.
class CountingField : public QLineEdit {
public:
  int presses = 0;

protected:
  void keyPressEvent(QKeyEvent* event) override {
    ++presses;
    QLineEdit::keyPressEvent(event);
  }
};

// A window whose close reads the model.
class ReadingWindow : public QWidget {
protected:
  void closeEvent(QCloseEvent*) override { throw mitcad::ModelBusy(); }
};

// Sends the event as the window system's delivery does; whether anything
// escaped it.
bool escapes(QObject* receiver, QEvent* event, bool* handled) {
  try {
    *handled = QCoreApplication::sendEvent(receiver, event);
    return false;
  } catch (...) {
    return true;
  }
}

} // namespace

int busyInputTests() {
  int failures = 0;
  const auto check = [&failures](bool ok, const char* message) {
    if (!ok) {
      std::fprintf(stderr, "busy input: %s\n", message);
      ++failures;
    }
  };
  if (dynamic_cast<mitcad::Application*>(QCoreApplication::instance()) == nullptr) {
    check(false, "test_app does not run as mitcad::Application");
    return failures;
  }
#if defined(Q_OS_MACOS)
  // macOS (mitcad#1382): Qt's frameworks are built without exception
  // support, and a ModelBusy thrown inside Qt's delivery (an event filter, a
  // close event) is never unwound to a handler outside it: the process
  // terminates, so the net in Application::notify cannot hold there. On
  // macOS the reads catch ModelBusy themselves (ParameterField::stepText,
  // checked by the parameter suggestion tests); here only the log line the
  // net would write is checked.
  {
    const QString dropped = QStringLiteral("Input dropped while the model computes: ");
    CountingField field;
    field.setObjectName(QStringLiteral("input_tx"));
    QKeyEvent up(QEvent::KeyPress, Qt::Key_Up, Qt::NoModifier);
    QKeyEvent shift(QEvent::KeyPress, Qt::Key_Shift, Qt::NoModifier);
    ReadingWindow window;
    window.setObjectName(QStringLiteral("readingWindow"));
    QCloseEvent close;
    check(mitcad::Application::droppedInputText(&field, &up) == dropped + QStringLiteral("KeyPress Up on QLineEdit input_tx"),
          "the dropped key's line does not name its key and receiver");
    check(mitcad::Application::droppedInputText(&field, &shift) ==
              dropped + QStringLiteral("KeyPress Shift on QLineEdit input_tx"),
          "a modifier's own key is not named alone");
    check(mitcad::Application::droppedInputText(&window, &close) == dropped + QStringLiteral("Close on QWidget readingWindow"),
          "the dropped close's line does not name its window");
  }
  if (failures == 0) {
    std::printf("busy input tests passed (macOS: the dropped input's log line only, mitcad#1382)\n");
  }
  return failures;
#endif
  // Dropped on purpose here: no Debug assertion.
  mitcad::Application::assertOnDroppedInput(false);
  logged().clear();
  previousHandler = qInstallMessageHandler(keepLog);
  {
    CountingField field;
    field.setObjectName(QStringLiteral("input_tx"));
    ReadingFilter filter;

    // The field's own filter (ParameterField's).
    field.installEventFilter(&filter);
    QKeyEvent up(QEvent::KeyPress, Qt::Key_Up, Qt::NoModifier);
    bool handled = false;
    check(!escapes(&field, &up, &handled), "a ModelBusy out of the field's filter escaped the delivery");
    check(handled, "the dropped key counts as handled");
    check(filter.presses == 1 && field.presses == 0, "the field got the dropped key");
    field.removeEventFilter(&filter);

    // An application-wide filter (the view's Shift, mitcad#217).
    QCoreApplication::instance()->installEventFilter(&filter);
    QKeyEvent shift(QEvent::KeyPress, Qt::Key_Shift, Qt::NoModifier);
    handled = false;
    check(!escapes(&field, &shift, &handled), "a ModelBusy out of an application-wide filter escaped the delivery");
    check(handled && filter.presses == 2 && field.presses == 0,
          "the field got the key the application's filter dropped");
    QCoreApplication::instance()->removeEventFilter(&filter);

    // Without the filter the keys reach the field again.
    QKeyEvent letter(QEvent::KeyPress, Qt::Key_A, Qt::NoModifier, QStringLiteral("a"));
    QCoreApplication::sendEvent(&field, &letter);
    check(field.presses == 1 && field.text() == QStringLiteral("a"), "the field does not take keys after a drop");
  }
  {
    // A close that read the model: the window stays.
    ReadingWindow window;
    window.setObjectName(QStringLiteral("readingWindow"));
    window.resize(120, 80);
    window.show();
    bool closed = true;
    try {
      closed = window.close();
    } catch (...) {
      check(false, "a ModelBusy out of a close escaped");
    }
    check(!closed && window.isVisible(), "the window closed although its close was dropped");
    window.hide();
  }
  qInstallMessageHandler(previousHandler);
  mitcad::Application::assertOnDroppedInput(true);
  const QString dropped = QStringLiteral("Input dropped while the model computes: ");
  check(logged().contains(dropped + QStringLiteral("KeyPress Up on QLineEdit input_tx")),
        "the dropped key on the field is not logged with its key and receiver");
  check(logged().contains(dropped + QStringLiteral("KeyPress Shift on QLineEdit input_tx")),
        "the key dropped by the application's filter is not logged");
  check(logged().contains(dropped + QStringLiteral("Close on QWidget readingWindow")),
        "the dropped close is not logged");
  if (failures == 0) {
    std::printf("busy input tests passed\n");
  }
  return failures;
}
