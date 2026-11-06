// SPDX-License-Identifier: MIT
#pragma once

#include <QApplication>
#include <QString>

class QEvent;
class QObject;

namespace mitcad {

// The application object (main.cpp; test_app's too). Its notify() is the
// safety net of mitcad#1360: an event handler (an event filter, a widget's
// own handler, a timer, a queued call) that reads the document while a job
// computes it gets ModelBusy (MainWindow::idleDocument), and an exception
// out of an event handler ended the application. Such a ModelBusy is
// caught where the event is delivered, logged as droppedInputText() says
// and the event dropped: the input is lost, the application goes on. A
// close dropped so keeps its window open. Debug builds assert as well
// (assertOnDroppedInput), so that a path that reads the document during a
// job is found and made to wait (whenIdle) or to use what the window read
// before (app/COMMANDS.md, "Background computation"). Not on macOS, where
// Qt is built without exception support and a throw inside its event
// delivery terminates before it reaches notify() (mitcad#1382): there the
// read itself must catch ModelBusy.
class Application : public QApplication {
public:
  Application(int& argc, char** argv);

  bool notify(QObject* receiver, QEvent* event) override;

  // "Input dropped while the model computes: KeyPress Up on QLineEdit
  // input_tx": the event's type (and a key event's key), the receiver's
  // class and object name.
  static QString droppedInputText(const QObject* receiver, const QEvent* event);

  // Whether a dropped event asserts in a Debug build (by default it does);
  // the unit test that drops one on purpose turns it off.
  static void assertOnDroppedInput(bool on);
};

} // namespace mitcad
