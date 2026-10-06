// SPDX-License-Identifier: MIT
#pragma once

// The in-process UI test driver. Created in main() and inert unless the
// environment variable MITCAD_TEST_INPUT names a script: then it runs the
// script's steps in the application itself, one per timer tick, by
// synthesising Qt events (QKeyEvent, QMouseEvent, QWheelEvent) for the
// widgets and by triggering actions. Nothing needs the operating system's
// input: no xdotool, no Accessibility or Screen Recording permission on
// macOS, no window messages on Windows, so the same scripts run over SSH on
// every platform. (Qt::Test is not linked into the application.)
//
// Environment (besides MITCAD_TEST_INPUT=<script>):
//   MITCAD_TEST_STEP_MS    the tick, in ms (default 50): one step, or one
//                          event of a gesture, per tick
//   MITCAD_TEST_SETTLE_MS  the pause after an input before the next step
//                          (default 250)
// The driver also reads variables for the script: ${NAME} in an argument is
// the environment variable NAME (see below).
//
// What it does about the application's state: it records every log line
// (qDebug, qWarning, ...; the messages still go to the handler that was
// installed before, so stderr is as without it) and takes "Computing X
// started" and "... done in / cancelled after" to know when the model is
// busy: steps that give input wait while a job runs (as ui_wait_idle does in
// tools/ui-test-lib.sh), and steps for a window wait while a modal dialog
// has the input. A step that opens a modal dialog does not return before the
// dialog closes (its nested event loop), but the driver goes on with the next
// steps from inside that loop, which is how the script closes the dialog.
// The driver's own log lines start with "TestDriver: " and are not part of
// what `expect-log` and the others match.
//
// Exit: "TestDriver: done (<n> steps)" and exit code 0 at the end of the
// script (not with `stay`); a failed step logs "TestDriver: FAIL line <n>:
// <reason>" and exits with 1. A script that cannot be read or has a syntax
// error fails the same way before it runs anything.
//
// Test script format
// ------------------
// One step per line: a name and arguments, separated by spaces. `#` starts a
// comment (at the start of a word); blank lines are ignored. Quoting is as
// in a shell: 'single quotes' keep everything as it is (best for regular
// expressions), "double quotes" interpret only \" \\ \$, a backslash outside
// quotes escapes the next character. `${NAME}` in an argument is replaced
// when the step runs, by a variable of the script (`set`, `capture`) or else
// the environment variable NAME (an undefined one fails the step);
// `${NAME:re}` is the value escaped for a regular expression (paths);
// `\${` is a literal `${`. Regular expressions are PCRE (QRegularExpression)
// and match anywhere in a log line. Percentages are of the 3D view widget;
// coordinates that the application logs ("at x,y") are in the main window's
// logical pixels. Buttons: left (default), right, middle. Modifiers: ctrl
// (the Command key on macOS), shift, alt, meta, joined with +.
//
// Input (waits until the app computes nothing; steps for the main window
// wait while a modal dialog is open):
//   key <key>...            a key press and release for the widget with the
//                           focus of the target window (see focus-dialog),
//                           or for the open popup menu. <key> is a portable
//                           key text: r, F6, Return, Escape, Tab, Down,
//                           ctrl+z, ctrl+shift+s, alt+r, ... A key with a
//                           shortcut (a QAction, a QShortcut, a button's
//                           mnemonic) triggers it unless the focused widget
//                           takes the key as text.
//   type <text>             the characters as key events
//   click <x%> <y%> [button] [mods]
//   dblclick <x%> <y%> [button] [mods]
//   drag <x1%> <y1%> <x2%> <y2%> [button] [mods]     with moves between
//   wheel <x%> <y%> <notches> [mods]                 positive: away from you
//   click-logged <regex> [button]       the last log line matching regex that
//                           has "at <x>,<y>" ("Datum xy at ...", "Ribbon ...
//                           at ...", "Panel <command> input <id> at ...")
//   dblclick-logged <regex>
//   click-pick <regex>      like click-logged, among the "Pick <kind> <name>
//                           at x,y" lines after the last "Pick places:"
//   drag-logged <from regex> <to regex> [dx]    left drag between two logged
//                           places; dx pixels right of the end
//   drag-from <regex> <dx> <dy>         left drag from a logged place by
//                           dx, dy pixels (a manipulator's handle)
//   sketch-click <x> <y> [button] [mods]   a sketch point in mm, from the
//                           logged "Sketch view" (the origin, (100, 0) and
//                           (0, 100) in window coordinates)
//   sketch-drag <x1> <y1> <x2> <y2>
//   type-in <regex> <text>  click the logged panel field, select all, type
//   command <id>            trigger the command's action (objectName
//                           "command_<id>": "sketch.create", "file.export")
//   menu <Top>/<Item>[/...] trigger an entry of the main window's menu bar
//                           ('&' and "..." in the names are ignored)
//   popup <entry>           choose an entry of the open popup menu (a
//                           context menu), by its text
//   button <regex>          click the visible button of the target window
//                           whose text (without '&') matches
//
// Windows:
//   focus-dialog <title regex> [seconds]  wait for a visible window of the
//                           application with that title (a dialog); key,
//                           type and button then go to it. A message box has
//                           no title on macOS: the regex is matched with its
//                           text and informative text there (a script
//                           gives both: '^Restore Version$|^Restore v1 ')
//   focus-main              give them to the main window again (a dialog
//                           that closed is replaced by the main window too)
//
// The log (lines are numbered; `mark` remembers where the log is):
//   mark
//   wait-log <regex> [seconds]     a line since the mark matches (30 s)
//   expect-log <regex> [seconds]   the same, 6 s by default
//   expect-no-log <regex>          no line since the mark matches (now)
//   expect-ever <regex> [seconds]  a line anywhere in the log matches (also
//                           what was logged before the script started)
//   expect-never <regex>           no line in the whole log matches
//   capture <var> <regex> [seconds]   the last line since the mark that
//                           matches (10 s); group 1 (or all) into ${var}
//   set <var> <value>
//   expect-near <actual> <expected> [rel] [abs]   numbers: |a - e| <=
//                           max(rel * |e|, abs); rel 1e-6, abs 0.01 by default
//
// The system:
//   wait-idle [seconds]            no job computes (60 s)
//   sleep <ms>
//   expect-file <path> [seconds]   the file or folder exists
//   expect-files <dir> <glob> <count> [seconds]   <count> files in <dir>
//                           match the glob (hidden ones included)
//   screenshot <png>        the main window (the 3D view composed in)
//   screenshot-view <png>   the 3D view's framebuffer, as --screenshot
//   screenshot-screen <png> macOS: what the window server shows of the
//                           application's windows, the floating panels' child
//                           windows and their glass included (the others
//                           grab widgets; the 3D view of a session without a
//                           GPU may be blank in it)
//   screenshot-dialog <png> the window focus-dialog chose (the main window
//                           when none), as its widgets draw it
//   quit [code]             end at once, with exit code <code> (0)
//   kill                    end the process at once, as a killed app would
//                           (no cleanup, autosave files stay), exit code 0
//   stay                    at the end of the script: do not exit
//
// Example (see tools/ui-scripts/ for the workflow tests):
//   command sketch.create
//   expect-log 'Create Sketch: select a plane'
//   click-logged 'Datum xy'
//   key r
//   sketch-click 0 0
//   type 40
//   key Tab
//   type 25
//   key Return
//   expect-log 'Added rectangle 40 x 25 mm'

#include <atomic>
#include <deque>
#include <functional>
#include <vector>

#include <QElapsedTimer>
#include <QEvent>
#include <QHash>
#include <QMutex>
#include <QObject>
#include <QPoint>
#include <QPointer>
#include <QString>
#include <QStringList>

class QRegularExpression;
class QRegularExpressionMatch;
class QTimer;
class QWidget;

namespace mitcad {

class GlassCard;

class TestDriver : public QObject {
public:
  // Reads MITCAD_TEST_INPUT; without it nothing happens (no handler, no
  // timer). Create it once the QApplication exists and before the main
  // window, so that the log of the start-up is recorded; the driver finds the
  // window itself.
  TestDriver();
  ~TestDriver() override;
  TestDriver(const TestDriver&) = delete;
  TestDriver& operator=(const TestDriver&) = delete;

  bool active() const { return m_active; }

  // A log line (from the message handler, any thread).
  void record(const QString& message);

private:
  struct Step {
    int line = 0;
    QString text; // the script line, for the log
    QString name;
    QStringList args; // as written; ${...} is expanded when it runs
  };
  struct KeyPress {
    int key = 0;
    Qt::KeyboardModifiers modifiers;
    QString text;
  };

  void load(const QString& path);
  void tick();
  void run(const Step& step);
  void fail(int line, const QString& reason);
  void finish();
  void leave(int code);

  bool expand(const QString& raw, QString& out, QString& error) const;
  QWidget* inputWindow(const Step& step) const;
  QWidget* targetWindow() const;
  QWidget* viewer() const;
  bool findMain();
  static bool dialogMatches(const QWidget* window, const QRegularExpression& re);
  static bool isBlocked(const QWidget* window);
  bool computing() const;

  // Waiting: `ready` is polled every tick (without side effects), `then`
  // runs once it holds; `gate` (a window) must also not be blocked by a
  // modal dialog.
  void whenReady(std::function<bool()> ready, double seconds, const QString& timeoutMessage,
                 std::function<void()> then = {}, QWidget* gate = nullptr);
  void pollWait();

  // The log.
  int lineCount() const;
  int findLine(const QRegularExpression& re, int from, QString* line = nullptr) const;
  int findLastLine(const QRegularExpression& re, int from, QString* line = nullptr,
                   QRegularExpressionMatch* match = nullptr) const;
  bool loggedPoint(const QRegularExpression& re, int from, QPoint& out) const;
  bool sketchPoint(double x, double y, QPoint& out) const;
  bool viewPoint(double xPercent, double yPercent, QPoint& out) const;

  // Events. Gestures and keys are queued, one event per tick.
  void queue(std::function<void()> action);
  void sendKey(const KeyPress& press);
  bool triggerShortcut(QWidget* window, const KeyPress& press);
  GlassCard* cardAt(const QPoint& windowPos) const;
  void sendText(const QString& text);
  void sendMouse(QEvent::Type type, const QPoint& windowPos, Qt::MouseButton button, Qt::MouseButtons buttons,
                 Qt::KeyboardModifiers modifiers);
  void queueClick(const QPoint& at, Qt::MouseButton button, Qt::KeyboardModifiers modifiers, int clicks);
  void queueDrag(const QPoint& from, const QPoint& to, Qt::MouseButton button, Qt::KeyboardModifiers modifiers);
  void sendWheel(const QPoint& windowPos, int notches, Qt::KeyboardModifiers modifiers);
  bool saveScreenshot(const QString& path, bool viewOnly);

  bool m_active = false;
  QTimer* m_timer = nullptr;
  QElapsedTimer m_clock;
  qint64 m_notBefore = 0;
  int m_stepMs = 50;
  int m_settleMs = 250;

  std::vector<Step> m_steps;
  std::size_t m_next = 0;
  int m_line = 0; // the script line being run
  QString m_error;
  int m_errorLine = 0;
  bool m_stay = false;
  bool m_finished = false;

  QPointer<QWidget> m_main;
  QPointer<QWidget> m_dialog; // the target of key, type and button; else the main window
  QPointer<QWidget> m_grab;   // the widget a pressed mouse button belongs to
  QPointer<QObject> m_found;  // what a wait found
  mutable QPointer<QWidget> m_viewer;

  std::deque<std::function<void()>> m_micro;
  bool m_waiting = false;
  std::function<bool()> m_ready;
  std::function<void()> m_then;
  QElapsedTimer m_waitClock;
  qint64 m_waitLimitMs = 0;
  QString m_waitMessage;
  QString m_waitDetail;
  int m_waitLine = 0;
  QPointer<QWidget> m_gate;
  qint64 m_blockedSince = -1;
  qint64 m_busySince = -1;

  QHash<QString, QString> m_vars;
  int m_mark = 0;

  mutable QMutex m_mutex;
  QStringList m_lines;
  std::atomic<int> m_started{0};
  std::atomic<int> m_ended{0};
  int m_ignoreStarted = -1;
};

} // namespace mitcad
