// SPDX-License-Identifier: MIT
#pragma once

// The shutdown's steps (mitcad#2275, GitHub mitcad/Mitcad#573): from the
// moment MitCad starts to quit to the session log's clean end
// (SessionLog::finish, which removes the session's lock), each step writes
// one info line, "Shutdown: <step>", and a step with an end
// "Shutdown: <step> done (<n> ms)", so that a close that never finished
// shows in the session log where it stopped.
//
// - The lines begin with begin(): the last document window closed and
//   MitCad quits (WindowManager), or the event loop ended (the first
//   aboutToQuit handler, which install() connects before any other, and
//   main() after QApplication::exec()). Before that a step writes nothing:
//   a window closed while MitCad runs on, an early return of the start-up.
// - Once the event loop has ended (eventLoopEnded()) the step timer
//   (setStepTimer: the hang watchdog's HangWatchdog::timeStep) is told of
//   every step: it times the newest step until the next one and warns,
//   naming it, when one takes longer than its limit. A step with an end
//   that finishes gives the time back to the step it ran in.
// - The web browser module, which links nothing of the application, reaches
//   the same steps through ModuleStep (ShutdownModuleStep.hpp), which
//   calls the object install() makes.
// - For tests, MITCAD_TEST_SHUTDOWN_DELAY=<ms>:<step> holds the UI thread
//   for that long when the step of that name begins (after its line), a
//   slow step for the watchdog's warning (tools/ui-session-log-test.sh).
//
// All on the GUI thread.

#include <functional>

#include <QObject>
#include <QString>

namespace mitcad {
namespace shutdown {

// Once, after the application object: the first aboutToQuit handler
// (eventLoopEnded) and the object the module's steps reach.
void install();
// Who times the steps once the event loop has ended: the step's name and
// its limit in ms (0: the timer's own). An empty function: nobody.
void setStepTimer(std::function<void(const QString& step, int limitMs)> timer);
// The teardown's lines from now on; `why` goes into its first line,
// "Shutdown begins: <why>". Again: nothing.
void begin(const QString& why);
// The event loop no longer turns: begins (when not yet) and from now on
// tells the step timer of each step. Again: nothing.
void eventLoopEnded();
bool running();
// A step without an end: "Shutdown: <name>"; it is timed until the next.
void step(const QString& name, int limitMs = 0);
// A step with an end (Step below): its line, then on endStep with the same
// name "Shutdown: <name> done (<n> ms)" and the step it ran in timed again.
void beginStep(const QString& name, int limitMs = 0);
void endStep(const QString& name);
// The last step is over: the step timer is let go (before the session log
// ends and the watchdog goes).
void end();

// The teardown's state back to the start (unit tests).
void resetForTests();

// A step with an end for the scope's life.
class Step {
public:
  explicit Step(const QString& name, int limitMs = 0) : m_name(name) { beginStep(m_name, limitMs); }
  ~Step() { endStep(m_name); }
  Step(const Step&) = delete;
  Step& operator=(const Step&) = delete;

private:
  QString m_name;
};

// What the module's steps call by name (QMetaObject: the module links
// nothing of the application).
class StepsObject : public QObject {
  Q_OBJECT

public:
  explicit StepsObject(QObject* parent);
  Q_INVOKABLE void beginStep(const QString& name);
  Q_INVOKABLE void endStep(const QString& name);
};

} // namespace shutdown
} // namespace mitcad
