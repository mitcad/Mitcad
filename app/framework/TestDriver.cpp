// SPDX-License-Identifier: MIT
#include "TestDriver.hpp"

#include <algorithm>
#include <cstdlib>
#include <memory>
#include <utility>

#include <QAbstractButton>
#include <QAction>
#include <QApplication>
#include <QDir>
#include <QEvent>
#include <QFile>
#include <QFileInfo>
#include <QImage>
#include <QKeyEvent>
#include <QKeySequence>
#include <QMenu>
#include <QMenuBar>
#include <QMessageBox>
#include <QMouseEvent>
#include <QOpenGLWidget>
#include <QPainter>
#include <QPixmap>
#include <QPointingDevice>
#include <QRegion>
#include <QRegularExpression>
#include <QShortcut>
#include <QTimer>
#include <QWheelEvent>
#include <QWidget>
#include <QtGlobal>
#include <QtLogging>

#include "../platform/MacChrome.hpp"
#include "GlassCard.hpp"

namespace mitcad {
namespace {

std::atomic<TestDriver*> g_driver{nullptr};
QtMessageHandler g_previousHandler = nullptr;

// Records the line and passes it on, so that stderr shows what it did without
// the driver.
void handleMessage(QtMsgType type, const QMessageLogContext& context, const QString& message) {
  if (TestDriver* driver = g_driver.load()) {
    driver->record(message);
  }
  if (g_previousHandler != nullptr) {
    g_previousHandler(type, context, message);
  }
}

// A '$' that was escaped in the script (so that no ${...} is made of it).
const QChar kLiteralDollar(0xE000);

constexpr qint64 kBusyLimitMs = 60000;
constexpr qint64 kBlockedLimitMs = 60000;
constexpr qint64 kNoMainLimitMs = 60000;
constexpr int kDragSteps = 8;

// How a step gives input: kIdle waits for the model, the others also for the
// window to be free of modal dialogs (of the target window, or of the main
// window).
enum StepKind { kPlain = 0, kIdle = 1, kInputOfTarget = 2, kInputOfMain = 3 };

struct StepSpec {
  const char* name;
  int minArgs;
  int maxArgs; // -1: any number
  int kind;
};

const StepSpec kSpecs[] = {
    {"key", 1, -1, kInputOfTarget},
    {"type", 1, -1, kInputOfTarget},
    {"click", 2, 4, kInputOfMain},
    {"dblclick", 2, 4, kInputOfMain},
    {"drag", 4, 6, kInputOfMain},
    {"wheel", 3, 4, kInputOfMain},
    {"click-logged", 1, 3, kInputOfMain},
    {"dblclick-logged", 1, 2, kInputOfMain},
    {"click-pick", 1, 2, kInputOfMain},
    {"drag-logged", 2, 3, kInputOfMain},
    {"drag-from", 3, 3, kInputOfMain},
    {"sketch-click", 2, 4, kInputOfMain},
    {"sketch-drag", 4, 4, kInputOfMain},
    {"type-in", 2, 2, kInputOfMain},
    {"command", 1, 1, kInputOfMain},
    {"menu", 1, 1, kInputOfMain},
    {"popup", 1, 1, kInputOfTarget},
    {"button", 1, 1, kInputOfTarget},
    {"focus-dialog", 1, 2, kPlain},
    {"focus-main", 0, 0, kPlain},
    {"mark", 0, 0, kPlain},
    {"wait-log", 1, 2, kPlain},
    {"expect-log", 1, 2, kPlain},
    {"expect-no-log", 1, 1, kIdle},
    {"expect-ever", 1, 2, kPlain},
    {"expect-never", 1, 1, kIdle},
    {"capture", 2, 3, kPlain},
    {"set", 2, 2, kPlain},
    {"expect-near", 2, 4, kPlain},
    {"wait-idle", 0, 1, kPlain},
    {"sleep", 1, 1, kPlain},
    {"expect-file", 1, 2, kPlain},
    {"expect-files", 3, 4, kPlain},
    {"screenshot", 1, 1, kIdle},
    {"screenshot-view", 1, 1, kIdle},
    {"screenshot-screen", 1, 1, kIdle},
    {"screenshot-dialog", 1, 1, kIdle},
    {"quit", 0, 1, kPlain},
    {"kill", 0, 0, kPlain},
    {"stay", 0, 0, kPlain},
};

const StepSpec* findSpec(const QString& name) {
  for (const StepSpec& spec : kSpecs) {
    if (name == QLatin1String(spec.name)) {
      return &spec;
    }
  }
  return nullptr;
}

// Splits a script line into words, as a shell does (see TestDriver.hpp).
bool tokenize(const QString& text, QStringList& words, QString& error) {
  QString current;
  bool inWord = false;
  const auto literal = [](QChar c) { return c == QLatin1Char('$') ? kLiteralDollar : c; };
  for (int i = 0; i < text.size(); ++i) {
    const QChar c = text.at(i);
    if (c == QLatin1Char('\'')) {
      const int end = text.indexOf(QLatin1Char('\''), i + 1);
      if (end < 0) {
        error = QStringLiteral("a ' is not closed");
        return false;
      }
      current += text.mid(i + 1, end - i - 1);
      inWord = true;
      i = end;
    } else if (c == QLatin1Char('"')) {
      inWord = true;
      bool closed = false;
      for (++i; i < text.size(); ++i) {
        const QChar d = text.at(i);
        if (d == QLatin1Char('"')) {
          closed = true;
          break;
        }
        if (d == QLatin1Char('\\') && i + 1 < text.size() &&
            QStringLiteral("\"\\$").contains(text.at(i + 1))) {
          ++i;
          current += literal(text.at(i));
        } else {
          current += d;
        }
      }
      if (!closed) {
        error = QStringLiteral("a \" is not closed");
        return false;
      }
    } else if (c == QLatin1Char('\\')) {
      if (i + 1 >= text.size()) {
        error = QStringLiteral("a \\ at the end of the line");
        return false;
      }
      ++i;
      current += literal(text.at(i));
      inWord = true;
    } else if (c.isSpace()) {
      if (inWord) {
        words << current;
        current.clear();
        inWord = false;
      }
    } else if (c == QLatin1Char('#') && !inWord) {
      break;
    } else {
      current += c;
      inWord = true;
    }
  }
  if (inWord) {
    words << current;
  }
  return true;
}

bool toNumber(const QString& text, double& value) {
  bool ok = false;
  value = text.trimmed().toDouble(&ok);
  return ok;
}

// "ctrl+shift" -> modifier flags; false for an unknown name.
bool parseModifiers(const QString& text, Qt::KeyboardModifiers& modifiers) {
  for (const QString& part : text.toLower().split(QLatin1Char('+'), Qt::SkipEmptyParts)) {
    if (part == QLatin1String("ctrl") || part == QLatin1String("control") || part == QLatin1String("cmd") ||
        part == QLatin1String("command")) {
      modifiers |= Qt::ControlModifier;
    } else if (part == QLatin1String("shift")) {
      modifiers |= Qt::ShiftModifier;
    } else if (part == QLatin1String("alt") || part == QLatin1String("option")) {
      modifiers |= Qt::AltModifier;
    } else if (part == QLatin1String("meta")) {
      modifiers |= Qt::MetaModifier;
    } else {
      return false;
    }
  }
  return true;
}

// [button] [mods] from index `from` of the arguments, in either order.
bool parseMouseOptions(const QStringList& args, int from, Qt::MouseButton& button, Qt::KeyboardModifiers& modifiers,
                       QString& error) {
  button = Qt::LeftButton;
  modifiers = Qt::NoModifier;
  for (int i = from; i < args.size(); ++i) {
    const QString word = args.at(i).toLower();
    if (word == QLatin1String("left")) {
      button = Qt::LeftButton;
    } else if (word == QLatin1String("right")) {
      button = Qt::RightButton;
    } else if (word == QLatin1String("middle")) {
      button = Qt::MiddleButton;
    } else if (!parseModifiers(word, modifiers)) {
      error = QStringLiteral("'%1' is neither a mouse button nor modifiers").arg(args.at(i));
      return false;
    }
  }
  return true;
}

int keyFromName(const QString& name) {
  static const QHash<QString, int> names = {
      {QStringLiteral("return"), Qt::Key_Return},      {QStringLiteral("enter"), Qt::Key_Enter},
      {QStringLiteral("escape"), Qt::Key_Escape},      {QStringLiteral("esc"), Qt::Key_Escape},
      {QStringLiteral("tab"), Qt::Key_Tab},            {QStringLiteral("backtab"), Qt::Key_Backtab},
      {QStringLiteral("backspace"), Qt::Key_Backspace}, {QStringLiteral("delete"), Qt::Key_Delete},
      {QStringLiteral("del"), Qt::Key_Delete},         {QStringLiteral("space"), Qt::Key_Space},
      {QStringLiteral("up"), Qt::Key_Up},              {QStringLiteral("down"), Qt::Key_Down},
      {QStringLiteral("left"), Qt::Key_Left},          {QStringLiteral("right"), Qt::Key_Right},
      {QStringLiteral("home"), Qt::Key_Home},          {QStringLiteral("end"), Qt::Key_End},
      {QStringLiteral("pageup"), Qt::Key_PageUp},      {QStringLiteral("pagedown"), Qt::Key_PageDown},
      {QStringLiteral("insert"), Qt::Key_Insert},
  };
  const QString lower = name.toLower();
  const auto named = names.constFind(lower);
  if (named != names.constEnd()) {
    return named.value();
  }
  static const QRegularExpression functionKey(QStringLiteral("^f([0-9]{1,2})$"));
  const QRegularExpressionMatch function = functionKey.match(lower);
  if (function.hasMatch()) {
    const int number = function.captured(1).toInt();
    if (number >= 1 && number <= 35) {
      return static_cast<int>(Qt::Key_F1) + number - 1;
    }
  }
  if (name.size() == 1) {
    const QChar c = name.at(0);
    if (c.unicode() > 0x20 && c.unicode() < 0x7f) {
      return c.isLetter() ? static_cast<int>(Qt::Key_A) + (c.toUpper().unicode() - 'A') : c.unicode();
    }
  }
  return 0;
}

// The key event a portable key text ("ctrl+shift+s", "Return", "F6") stands
// for.
bool parseKeySpec(const QString& spec, int& key, Qt::KeyboardModifiers& modifiers, QString& text, QString& error) {
  key = 0;
  modifiers = Qt::NoModifier;
  text.clear();
  QStringList parts = spec.split(QLatin1Char('+'));
  const QString name = parts.takeLast();
  if (name.isEmpty()) {
    // "ctrl++": the plus key.
    if (parts.isEmpty() || !parts.last().isEmpty()) {
      error = QStringLiteral("'%1' is not a key").arg(spec);
      return false;
    }
    parts.removeLast();
    key = Qt::Key_Plus;
  } else {
    key = keyFromName(name);
  }
  if (key == 0) {
    error = QStringLiteral("unknown key '%1'").arg(name);
    return false;
  }
  if (!parts.isEmpty() && !parseModifiers(parts.join(QLatin1Char('+')), modifiers)) {
    error = QStringLiteral("unknown modifier in '%1'").arg(spec);
    return false;
  }
  if (!(modifiers & (Qt::ControlModifier | Qt::AltModifier | Qt::MetaModifier))) {
    if (key >= Qt::Key_A && key <= Qt::Key_Z) {
      const QChar letter = QLatin1Char(static_cast<char>('a' + (key - Qt::Key_A)));
      text = modifiers.testFlag(Qt::ShiftModifier) ? QString(letter.toUpper()) : QString(letter);
    } else if (key == Qt::Key_Return || key == Qt::Key_Enter) {
      text = QStringLiteral("\r");
    } else if (key == Qt::Key_Tab) {
      text = QStringLiteral("\t");
    } else if (key == Qt::Key_Backspace) {
      text = QStringLiteral("\b");
    } else if (key == Qt::Key_Escape) {
      text = QStringLiteral("\x1b");
    } else if (key >= Qt::Key_Space && key <= Qt::Key_AsciiTilde) {
      text = QString(QChar(key));
    }
  }
  return true;
}

// A menu entry's text as the script writes it.
QString plainText(QString text) {
  text.remove(QLatin1Char('&'));
  text = text.trimmed();
  while (text.endsWith(QLatin1Char('.')) || text.endsWith(QChar(0x2026))) {
    text.chop(1);
  }
  return text.trimmed();
}

} // namespace

// ---------------------------------------------------------------------------
// Start and script

TestDriver::TestDriver() {
  const QString script = qEnvironmentVariable("MITCAD_TEST_INPUT");
  if (script.isEmpty()) {
    return;
  }
  m_active = true;
  bool ok = false;
  const int stepMs = qEnvironmentVariableIntValue("MITCAD_TEST_STEP_MS", &ok);
  if (ok && stepMs > 0) {
    m_stepMs = stepMs;
  }
  const int settleMs = qEnvironmentVariableIntValue("MITCAD_TEST_SETTLE_MS", &ok);
  if (ok && settleMs >= 0) {
    m_settleMs = settleMs;
  }
  m_clock.start();
  load(script);
  g_previousHandler = qInstallMessageHandler(handleMessage);
  g_driver.store(this);
  m_timer = new QTimer(this);
  m_timer->setInterval(m_stepMs);
  connect(m_timer, &QTimer::timeout, this, [this] { tick(); });
  m_timer->start();
  qDebug().noquote() << QStringLiteral("TestDriver: %1, %2 steps").arg(script).arg(m_steps.size());
}

TestDriver::~TestDriver() {
  if (m_active) {
    g_driver.store(nullptr);
    qInstallMessageHandler(g_previousHandler);
  }
}

void TestDriver::load(const QString& path) {
  QFile file(path);
  if (!file.open(QIODevice::ReadOnly | QIODevice::Text)) {
    m_error = QStringLiteral("cannot read the script %1: %2").arg(path, file.errorString());
    return;
  }
  const QStringList lines = QString::fromUtf8(file.readAll()).split(QLatin1Char('\n'));
  for (int i = 0; i < lines.size(); ++i) {
    const QString text = lines.at(i).trimmed();
    QStringList words;
    QString error;
    if (!tokenize(text, words, error)) {
      m_error = error;
      m_errorLine = i + 1;
      return;
    }
    if (words.isEmpty()) {
      continue;
    }
    Step step;
    step.line = i + 1;
    step.text = text;
    step.name = words.takeFirst();
    step.args = words;
    const StepSpec* spec = findSpec(step.name);
    if (spec == nullptr) {
      m_error = QStringLiteral("unknown step '%1'").arg(step.name);
      m_errorLine = step.line;
      return;
    }
    if (step.args.size() < spec->minArgs || (spec->maxArgs >= 0 && step.args.size() > spec->maxArgs)) {
      m_error = QStringLiteral("'%1' takes %2 argument(s), not %3")
                    .arg(step.name)
                    .arg(spec->maxArgs == spec->minArgs
                             ? QString::number(spec->minArgs)
                             : (spec->maxArgs < 0 ? QStringLiteral("%1 or more").arg(spec->minArgs)
                                                  : QStringLiteral("%1 to %2").arg(spec->minArgs).arg(spec->maxArgs)))
                    .arg(step.args.size());
      m_errorLine = step.line;
      return;
    }
    m_steps.push_back(step);
  }
}

bool TestDriver::expand(const QString& raw, QString& out, QString& error) const {
  out.clear();
  int i = 0;
  while (i < raw.size()) {
    if (raw.at(i) == QLatin1Char('$') && i + 1 < raw.size() && raw.at(i + 1) == QLatin1Char('{')) {
      const int end = raw.indexOf(QLatin1Char('}'), i + 2);
      if (end < 0) {
        error = QStringLiteral("a ${ is not closed in '%1'").arg(raw);
        return false;
      }
      QString name = raw.mid(i + 2, end - i - 2);
      bool escape = false;
      if (name.endsWith(QLatin1String(":re"))) {
        escape = true;
        name.chop(3);
      }
      QString value;
      if (m_vars.contains(name)) {
        value = m_vars.value(name);
      } else {
        const QByteArray key = name.toUtf8();
        if (!qEnvironmentVariableIsSet(key.constData())) {
          error = QStringLiteral("the variable ${%1} is not defined").arg(name);
          return false;
        }
        value = qEnvironmentVariable(key.constData());
      }
      out += escape ? QRegularExpression::escape(value) : value;
      i = end + 1;
    } else {
      out += raw.at(i);
      ++i;
    }
  }
  out.replace(kLiteralDollar, QLatin1Char('$'));
  return true;
}

// ---------------------------------------------------------------------------
// The log

void TestDriver::record(const QString& message) {
  if (message.startsWith(QLatin1String("TestDriver: "))) {
    return; // the driver's own lines are not what the script looks for
  }
  if (message.startsWith(QLatin1String("Computing "))) {
    // As ui_wait_idle in tools/ui-test-lib.sh does: a job that logged its
    // start is busy until it logs its end.
    static const QRegularExpression ended(QStringLiteral("^Computing .* (done in|cancelled after) [0-9]+ ms"));
    if (message.endsWith(QLatin1String(" started"))) {
      m_started.fetch_add(1);
    } else if (ended.match(message).hasMatch()) {
      m_ended.fetch_add(1);
    }
  }
  const QMutexLocker lock(&m_mutex);
  m_lines.append(message);
}

int TestDriver::lineCount() const {
  const QMutexLocker lock(&m_mutex);
  return static_cast<int>(m_lines.size());
}

int TestDriver::findLine(const QRegularExpression& re, int from, QString* line) const {
  const QMutexLocker lock(&m_mutex);
  for (int i = std::max(from, 0); i < m_lines.size(); ++i) {
    if (re.match(m_lines.at(i)).hasMatch()) {
      if (line != nullptr) {
        *line = m_lines.at(i);
      }
      return i;
    }
  }
  return -1;
}

int TestDriver::findLastLine(const QRegularExpression& re, int from, QString* line,
                             QRegularExpressionMatch* match) const {
  const QMutexLocker lock(&m_mutex);
  for (int i = static_cast<int>(m_lines.size()) - 1; i >= std::max(from, 0); --i) {
    const QRegularExpressionMatch found = re.match(m_lines.at(i));
    if (found.hasMatch()) {
      if (line != nullptr) {
        *line = m_lines.at(i);
      }
      if (match != nullptr) {
        *match = found;
      }
      return i;
    }
  }
  return -1;
}

// The place of the last line matching `re` (from line `from` on) that says
// "at <x>,<y>": the window's logical pixels.
bool TestDriver::loggedPoint(const QRegularExpression& re, int from, QPoint& out) const {
  static const QRegularExpression at(QStringLiteral(" at (-?[0-9]+(?:\\.[0-9]+)?),(-?[0-9]+(?:\\.[0-9]+)?)"));
  QStringList candidates;
  {
    const QMutexLocker lock(&m_mutex);
    for (int i = static_cast<int>(m_lines.size()) - 1; i >= std::max(from, 0); --i) {
      if (re.match(m_lines.at(i)).hasMatch()) {
        candidates << m_lines.at(i);
      }
    }
  }
  for (const QString& line : std::as_const(candidates)) {
    QRegularExpressionMatchIterator places = at.globalMatch(line);
    bool found = false;
    QRegularExpressionMatch last;
    while (places.hasNext()) {
      last = places.next();
      found = true;
    }
    if (found) {
      out = QPoint(qRound(last.captured(1).toDouble()), qRound(last.captured(2).toDouble()));
      return true;
    }
  }
  return false;
}

// A sketch point (mm) in window pixels, from "Sketch view x0,y0 x1,y1 x2,y2"
// (the points (0, 0), (100, 0) and (0, 100)).
bool TestDriver::sketchPoint(double x, double y, QPoint& out) const {
  static const QRegularExpression view(QStringLiteral(
      "Sketch view ([-0-9.]+),([-0-9.]+) ([-0-9.]+),([-0-9.]+) ([-0-9.]+),([-0-9.]+)"));
  QRegularExpressionMatch match;
  if (findLastLine(view, 0, nullptr, &match) < 0) {
    return false;
  }
  const double ox = match.captured(1).toDouble();
  const double oy = match.captured(2).toDouble();
  const double ax = match.captured(3).toDouble();
  const double ay = match.captured(4).toDouble();
  const double bx = match.captured(5).toDouble();
  const double by = match.captured(6).toDouble();
  out = QPoint(qRound(ox + (ax - ox) * x / 100.0 + (bx - ox) * y / 100.0),
               qRound(oy + (ay - oy) * x / 100.0 + (by - oy) * y / 100.0));
  return true;
}

// ---------------------------------------------------------------------------
// Windows

// Whether a window is the dialog a script asks for: its title matches, or,
// for a message box with no title (macOS drops it: QMessageBox::setWindowTitle
// does nothing there), its text or its informative text does.
bool TestDriver::dialogMatches(const QWidget* window, const QRegularExpression& re) {
  const QString title = window->windowTitle();
  if (re.match(title).hasMatch()) {
    return true;
  }
  const auto* box = qobject_cast<const QMessageBox*>(window);
  return title.isEmpty() && box != nullptr &&
         (re.match(box->text()).hasMatch() || re.match(box->informativeText()).hasMatch());
}

bool TestDriver::findMain() {
  if (!m_main.isNull()) {
    return true;
  }
  const QWidgetList widgets = QApplication::topLevelWidgets();
  for (QWidget* widget : widgets) {
    if (widget->inherits("mitcad::MainWindow")) {
      m_main = widget;
      // Not needed where the window is active already; where it is not (a
      // session without a user at the console), it may help.
      widget->raise();
      widget->activateWindow();
      return true;
    }
  }
  return false;
}

QWidget* TestDriver::targetWindow() const {
  if (!m_dialog.isNull() && m_dialog->isVisible()) {
    return m_dialog.data();
  }
  return m_main.data();
}

QWidget* TestDriver::viewer() const {
  if (m_viewer.isNull() && !m_main.isNull()) {
    const QList<QWidget*> children = m_main->findChildren<QWidget*>();
    for (QWidget* child : children) {
      if (child->inherits("mitcad::OcctViewer")) {
        m_viewer = child;
        break;
      }
    }
  }
  return m_viewer.data();
}

bool TestDriver::viewPoint(double xPercent, double yPercent, QPoint& out) const {
  QWidget* view = viewer();
  if (view == nullptr || m_main.isNull()) {
    return false;
  }
  out = view->mapTo(m_main.data(), QPoint(qRound(view->width() * xPercent / 100.0),
                                          qRound(view->height() * yPercent / 100.0)));
  return true;
}

// Whether a modal dialog other than `window` (and its own children) has the
// input: Qt drops the events for `window` then.
bool TestDriver::isBlocked(const QWidget* window) {
  const QWidget* modal = QApplication::activeModalWidget();
  return modal != nullptr && window != nullptr && modal->isVisible() && modal != window &&
         !modal->isAncestorOf(window);
}

QWidget* TestDriver::inputWindow(const Step& step) const {
  const StepSpec* spec = findSpec(step.name);
  if (spec == nullptr) {
    return nullptr;
  }
  if (spec->kind == kInputOfMain) {
    return m_main.data();
  }
  if (spec->kind == kInputOfTarget) {
    return targetWindow();
  }
  return nullptr;
}

bool TestDriver::computing() const {
  const int started = m_started.load();
  return started > m_ended.load() && started != m_ignoreStarted;
}

// ---------------------------------------------------------------------------
// The ticks

void TestDriver::fail(int line, const QString& reason) {
  if (m_finished) {
    return;
  }
  qCritical().noquote() << QStringLiteral("TestDriver: FAIL line %1: %2").arg(line).arg(reason);
  leave(1);
}

void TestDriver::leave(int code) {
  m_finished = true;
  if (m_timer != nullptr) {
    m_timer->stop();
  }
  // Closes no window (no closeEvent, no question about unsaved work); every
  // event loop that runs, a dialog's too, ends.
  QCoreApplication::exit(code);
}

void TestDriver::finish() {
  if (m_stay) {
    qDebug().noquote() << QStringLiteral("TestDriver: done (%1 steps), staying").arg(m_steps.size());
    m_finished = true;
    if (m_timer != nullptr) {
      m_timer->stop();
    }
    return;
  }
  qDebug().noquote() << QStringLiteral("TestDriver: done (%1 steps)").arg(m_steps.size());
  leave(0);
}

void TestDriver::queue(std::function<void()> action) { m_micro.push_back(std::move(action)); }

void TestDriver::whenReady(std::function<bool()> ready, double seconds, const QString& timeoutMessage,
                           std::function<void()> then, QWidget* gate) {
  m_ready = std::move(ready);
  m_then = std::move(then);
  m_waitLimitMs = static_cast<qint64>(seconds * 1000.0);
  m_waitClock.start();
  m_waitMessage = timeoutMessage;
  m_waitDetail.clear();
  m_waitLine = m_line;
  m_gate = gate;
  m_waiting = true;
  pollWait();
}

void TestDriver::pollWait() {
  if (!m_waiting) {
    return;
  }
  const bool blocked = !m_gate.isNull() && isBlocked(m_gate.data());
  if (!blocked && m_ready()) {
    // Cleared first: what `then` does may run an event loop (a dialog),
    // in which the next steps run.
    const std::function<void()> then = std::move(m_then);
    m_waiting = false;
    m_ready = nullptr;
    m_then = nullptr;
    if (then) {
      then();
    }
    return;
  }
  if (m_waitClock.elapsed() >= m_waitLimitMs) {
    QString why = m_waitMessage;
    if (!m_waitDetail.isEmpty()) {
      why += QStringLiteral(" (%1)").arg(m_waitDetail);
    }
    if (blocked) {
      const QWidget* modal = QApplication::activeModalWidget();
      why += QStringLiteral(" (the window is blocked by the dialog '%1')")
                 .arg(modal != nullptr ? modal->windowTitle() : QString());
    }
    const int line = m_waitLine;
    m_waiting = false;
    fail(line, why);
  }
}

void TestDriver::tick() {
  if (m_finished) {
    return;
  }
  // A step can open a modal dialog, whose event loop runs inside this call,
  // and the steps that close it must run in that loop. Qt does not fire a
  // timer again while its own slot runs (macOS: the dialog's loop then ran
  // without the driver); a restarted timer is a new one that does.
  m_timer->start();
  if (!m_error.isEmpty()) {
    fail(m_errorLine, m_error);
    return;
  }
  if (!findMain()) {
    if (m_clock.elapsed() > kNoMainLimitMs) {
      fail(0, QStringLiteral("no main window"));
    }
    return;
  }
  const qint64 now = m_clock.elapsed();
  if (!m_micro.empty()) {
    // One event of a gesture or key; taken out first, as it may run an
    // event loop.
    const std::function<void()> action = std::move(m_micro.front());
    m_micro.pop_front();
    m_notBefore = now + m_settleMs;
    action();
    return;
  }
  if (m_waiting) {
    pollWait();
    return;
  }
  if (m_next >= m_steps.size()) {
    finish();
    return;
  }
  const Step& step = m_steps[m_next];
  const StepSpec* spec = findSpec(step.name);
  const int kind = spec != nullptr ? spec->kind : kPlain;
  if (kind != kPlain) {
    if (now < m_notBefore) {
      return;
    }
    if (computing()) {
      if (m_busySince < 0) {
        m_busySince = now;
      }
      if (now - m_busySince < kBusyLimitMs) {
        return;
      }
      qWarning().noquote() << QStringLiteral("TestDriver: note: the app was still computing after %1 s")
                                  .arg(kBusyLimitMs / 1000);
      m_ignoreStarted = m_started.load();
    }
    m_busySince = -1;
    const QWidget* window = inputWindow(step);
    if (window != nullptr && isBlocked(window)) {
      if (m_blockedSince < 0) {
        m_blockedSince = now;
      }
      if (now - m_blockedSince >= kBlockedLimitMs) {
        const QWidget* modal = QApplication::activeModalWidget();
        fail(step.line, QStringLiteral("the window is blocked by the dialog '%1'")
                            .arg(modal != nullptr ? modal->windowTitle() : QString()));
      }
      return;
    }
    m_blockedSince = -1;
  }
  // The step is taken before it runs: a dialog it opens runs an event loop
  // in which this function goes on with the following steps.
  ++m_next;
  m_notBefore = now + (kind >= kInputOfTarget ? m_settleMs : 0);
  const Step current = step;
  run(current);
}

// ---------------------------------------------------------------------------
// Events

void TestDriver::sendKey(const KeyPress& press) {
  QWidget* window = targetWindow();
  if (window == nullptr) {
    return;
  }
  QWidget* popup = QApplication::activePopupWidget();
  QWidget* active = QApplication::activeWindow();
  if (popup == nullptr && active != window && (active == nullptr || GlassCard::hostWindow(active) != window)) {
    // A window the system did not activate (a session without a user at the
    // console; macOS does not let a process started from a shell take the
    // focus) has no focused widget for Qt: QWidget::hasFocus() is false, and
    // the application's code that checks it (the sketch's value field that
    // selects its live text, so that typing replaces it) acts differently
    // from a window the user sees. Qt's own activation, as QTest does it.
    qWarning().noquote() << QStringLiteral("TestDriver: note: '%1' is not the active window; activated by Qt")
                                .arg(window->windowTitle());
    // QApplication::setActiveWindow is deprecated in favour of
    // QWidget::activateWindow, which asks the system and is what findMain
    // tried; only the former sets Qt's own state without it.
    QT_WARNING_PUSH
    QT_WARNING_DISABLE_DEPRECATED
    QApplication::setActiveWindow(window);
    QT_WARNING_POP
  }
  QWidget* receiver = popup != nullptr ? popup : window;
  QWidget* focused = QApplication::focusWidget();
  if (popup == nullptr && focused != nullptr && GlassCard::hostWindow(focused) == window) {
    receiver = focused; // in the main window or in one of its panels' windows
  } else if (receiver->focusWidget() != nullptr) {
    receiver = receiver->focusWidget();
  }
  const Qt::KeyboardModifiers modifiers = press.modifiers;
  // A key is a shortcut unless the focused widget takes it (a line edit
  // asks to keep the letters): what the platform does before it delivers
  // a key press.
  QKeyEvent offer(QEvent::ShortcutOverride, press.key, modifiers, press.text);
  offer.ignore();
  QCoreApplication::sendEvent(receiver, &offer);
  if (popup == nullptr && !offer.isAccepted() && triggerShortcut(window, press)) {
    return;
  }
  QKeyEvent down(QEvent::KeyPress, press.key, modifiers, press.text);
  QCoreApplication::sendEvent(receiver, &down);
  QKeyEvent up(QEvent::KeyRelease, press.key, modifiers, press.text);
  QCoreApplication::sendEvent(receiver, &up);
}

// With native glass the floating panels are windows of their own: the one
// that has the position (in the main window's coordinates), if any. A card
// that lets the mouse through (the status pill) has none.
GlassCard* TestDriver::cardAt(const QPoint& windowPos) const {
  QWidget* top = m_main.data();
  if (top == nullptr) {
    return nullptr;
  }
  const QPoint global = top->mapToGlobal(windowPos);
  const QList<GlassCard*> cards = top->findChildren<GlassCard*>();
  for (GlassCard* card : cards) {
    if (card->isWindow() && card->isVisible() && !card->testAttribute(Qt::WA_TransparentForMouseEvents) &&
        card->geometry().contains(global)) {
      return card;
    }
  }
  return nullptr;
}

// What a platform's key handling does for a shortcut: the enabled action,
// QShortcut or button mnemonic of the window that has it.
bool TestDriver::triggerShortcut(QWidget* window, const KeyPress& press) {
  const Qt::KeyboardModifiers modifiers = press.modifiers & ~Qt::KeypadModifier;
  const QKeySequence wanted(QKeyCombination(modifiers, static_cast<Qt::Key>(press.key)));
  QList<QWidget*> widgets = window->findChildren<QWidget*>();
  widgets.prepend(window);
  for (QWidget* widget : std::as_const(widgets)) {
    const QList<QAction*> actions = widget->actions();
    for (QAction* action : actions) {
      if (!action->isEnabled() || !action->isVisible() || action->shortcutContext() == Qt::WidgetShortcut ||
          action->shortcutContext() == Qt::WidgetWithChildrenShortcut || !action->shortcuts().contains(wanted)) {
        continue;
      }
      action->trigger();
      return true;
    }
  }
  const QList<QShortcut*> shortcuts = window->findChildren<QShortcut*>();
  for (QShortcut* shortcut : shortcuts) {
    if (shortcut->isEnabled() && shortcut->context() != Qt::WidgetShortcut &&
        shortcut->context() != Qt::WidgetWithChildrenShortcut && shortcut->key() == wanted) {
      emit shortcut->activated();
      return true;
    }
  }
  const QList<QAbstractButton*> buttons = window->findChildren<QAbstractButton*>();
  for (QAbstractButton* button : buttons) {
    if (button->isEnabled() && button->isVisibleTo(window) && !button->shortcut().isEmpty() &&
        button->shortcut() == wanted) {
      button->click();
      return true;
    }
  }
  return false;
}

void TestDriver::sendText(const QString& text) {
  for (const QChar c : text) {
    KeyPress press;
    press.text = QString(c);
    const ushort code = c.unicode();
    if (code < 0x80 && c.isLetter()) {
      press.key = static_cast<int>(Qt::Key_A) + (c.toUpper().unicode() - 'A');
      if (c.isUpper()) {
        press.modifiers = Qt::ShiftModifier;
      }
    } else if (code >= 0x20 && code < 0x7f) {
      press.key = code;
    } else {
      press.key = Qt::Key_unknown;
    }
    sendKey(press);
  }
}

void TestDriver::sendMouse(QEvent::Type kind, const QPoint& windowPos, Qt::MouseButton button, Qt::MouseButtons buttons,
                           Qt::KeyboardModifiers modifiers) {
  QWidget* top = m_main.data();
  if (top == nullptr) {
    return;
  }
  const bool pressing = kind == QEvent::MouseButtonPress || kind == QEvent::MouseButtonDblClick;
  // A position over a panel's window is for that window (a press gives it
  // the keys, as a click does).
  QWidget* window = top;
  QWidget* target = pressing ? nullptr : m_grab.data();
  if (target == nullptr) {
    if (GlassCard* card = cardAt(windowPos)) {
      window = card;
    }
    const QPoint inWindow = window->mapFromGlobal(top->mapToGlobal(windowPos));
    target = window->childAt(inWindow);
    if (target == nullptr) {
      target = window;
    }
  } else {
    window = target->window();
  }
  if (pressing) {
    m_grab = target;
    QWidget* active = QApplication::activeWindow();
    if (active != window && (qobject_cast<GlassCard*>(window) != nullptr || qobject_cast<GlassCard*>(active) != nullptr)) {
      QT_WARNING_PUSH
      QT_WARNING_DISABLE_DEPRECATED
      QApplication::setActiveWindow(window); // as the system does for a click
      QT_WARNING_POP
    }
  }
  const QPoint global = top->mapToGlobal(windowPos);
  const QPoint local = target->mapFromGlobal(global);
  QMouseEvent event(kind, QPointF(local), QPointF(window->mapFromGlobal(global)), QPointF(global), button, buttons,
                    modifiers, QPointingDevice::primaryPointingDevice());
  QCoreApplication::sendEvent(target, &event);
  if (kind == QEvent::MouseButtonRelease && buttons == Qt::NoButton) {
    m_grab = nullptr;
  }
}

void TestDriver::queueClick(const QPoint& at, Qt::MouseButton button, Qt::KeyboardModifiers modifiers, int clicks) {
  queue([this, at, modifiers] { sendMouse(QEvent::MouseMove, at, Qt::NoButton, Qt::NoButton, modifiers); });
  for (int i = 0; i < clicks; ++i) {
    const QEvent::Type press = i == 0 ? QEvent::MouseButtonPress : QEvent::MouseButtonDblClick;
    queue([this, press, at, button, modifiers] { sendMouse(press, at, button, button, modifiers); });
    queue([this, at, button, modifiers] { sendMouse(QEvent::MouseButtonRelease, at, button, Qt::NoButton, modifiers); });
  }
}

void TestDriver::queueDrag(const QPoint& from, const QPoint& to, Qt::MouseButton button, Qt::KeyboardModifiers modifiers) {
  queue([this, from, modifiers] { sendMouse(QEvent::MouseMove, from, Qt::NoButton, Qt::NoButton, modifiers); });
  queue([this, from, button, modifiers] { sendMouse(QEvent::MouseButtonPress, from, button, button, modifiers); });
  for (int i = 1; i <= kDragSteps; ++i) {
    const QPoint at(from.x() + (to.x() - from.x()) * i / kDragSteps, from.y() + (to.y() - from.y()) * i / kDragSteps);
    queue([this, at, button, modifiers] { sendMouse(QEvent::MouseMove, at, Qt::NoButton, button, modifiers); });
  }
  queue([] {}); // a pause before the button goes up
  queue([] {});
  queue([this, to, button, modifiers] { sendMouse(QEvent::MouseButtonRelease, to, button, Qt::NoButton, modifiers); });
}

void TestDriver::sendWheel(const QPoint& windowPos, int notches, Qt::KeyboardModifiers modifiers) {
  QWidget* top = m_main.data();
  if (top == nullptr) {
    return;
  }
  const QPoint global = top->mapToGlobal(windowPos);
  QWidget* window = cardAt(windowPos);
  if (window == nullptr) {
    window = top;
  }
  QWidget* target = window->childAt(window->mapFromGlobal(global));
  if (target == nullptr) {
    target = window;
  }
  const QPoint local = target->mapFromGlobal(global);
  QWheelEvent event(QPointF(local), QPointF(global), QPoint(), QPoint(0, 120 * notches), Qt::NoButton,
                    modifiers, Qt::NoScrollPhase, false);
  QCoreApplication::sendEvent(target, &event);
}

bool TestDriver::saveScreenshot(const QString& path, bool viewOnly) {
  QDir().mkpath(QFileInfo(path).absolutePath());
  QOpenGLWidget* gl = qobject_cast<QOpenGLWidget*>(viewer());
  if (viewOnly) {
    // What --screenshot saves.
    return gl != nullptr && gl->grabFramebuffer().save(path);
  }
  QWidget* top = m_main.data();
  if (top == nullptr) {
    return false;
  }
  QPixmap shot = top->grab();
  if (gl != nullptr) {
    // The window's grab may lack what the view draws with OpenGL: put the
    // view's framebuffer in, and the widgets over it (the orientation
    // cube's buttons, the sketch overlay) again.
    const QImage frame = gl->grabFramebuffer();
    if (!frame.isNull()) {
      QPainter painter(&shot);
      painter.drawImage(QRect(gl->mapTo(top, QPoint(0, 0)), gl->size()), frame);
      const QList<QWidget*> overlays = gl->findChildren<QWidget*>(Qt::FindDirectChildrenOnly);
      for (QWidget* overlay : overlays) {
        if (overlay->isVisibleTo(gl)) {
          overlay->render(&painter, overlay->mapTo(top, QPoint(0, 0)), QRegion(), QWidget::DrawChildren);
        }
      }
      // Floating chrome: the cards (and their shadows) are siblings of the
      // view, stacked over it.
      if (QWidget* parent = gl->parentWidget()) {
        bool above = false;
        for (QObject* child : parent->children()) {
          auto* widget = qobject_cast<QWidget*>(child);
          if (widget == gl) {
            above = true;
          } else if (above && widget != nullptr && widget->isVisibleTo(parent) && !widget->isWindow()) {
            widget->render(&painter, widget->mapTo(top, QPoint(0, 0)), QRegion(), QWidget::DrawChildren);
          }
        }
      }
      painter.end();
    }
  }
  if (!shot.isNull()) {
    // Native glass: the panels are windows over this one (without the
    // glass, which only the window server draws: screenshot-screen).
    QPainter painter(&shot);
    const QList<GlassCard*> cards = top->findChildren<GlassCard*>();
    for (GlassCard* card : cards) {
      if (card->isWindow() && card->isVisible()) {
        painter.drawPixmap(top->mapFromGlobal(card->geometry().topLeft()), card->grab());
      }
    }
  }
  return shot.save(path);
}

// ---------------------------------------------------------------------------
// The steps

void TestDriver::run(const Step& step) {
  m_line = step.line;
  const int line = step.line;
  qDebug().noquote() << QStringLiteral("TestDriver: line %1: %2").arg(line).arg(step.text);

  QStringList a;
  for (const QString& raw : step.args) {
    QString value;
    QString error;
    if (!expand(raw, value, error)) {
      fail(line, error);
      return;
    }
    a << value;
  }
  const QString& name = step.name;

  // A regular expression argument.
  const auto regex = [&](const QString& pattern, QRegularExpression& re) {
    re = QRegularExpression(pattern);
    if (!re.isValid()) {
      fail(line, QStringLiteral("bad regular expression '%1': %2").arg(pattern, re.errorString()));
      return false;
    }
    return true;
  };
  // Seconds from an optional argument.
  const auto seconds = [&](int index, double fallback, double& value) {
    value = fallback;
    if (index < a.size() && !toNumber(a.at(index), value)) {
      fail(line, QStringLiteral("'%1' is not a number of seconds").arg(a.at(index)));
      return false;
    }
    return true;
  };
  QWidget* mainWindow = m_main.data();

  // --- Keys and text
  if (name == QLatin1String("key")) {
    std::vector<KeyPress> presses;
    for (const QString& spec : a) {
      KeyPress press;
      QString error;
      if (!parseKeySpec(spec, press.key, press.modifiers, press.text, error)) {
        fail(line, error);
        return;
      }
      presses.push_back(press);
    }
    for (const KeyPress& press : presses) {
      queue([this, press] { sendKey(press); });
    }
  } else if (name == QLatin1String("type")) {
    const QString text = a.join(QLatin1Char(' '));
    queue([this, text] { sendText(text); });

    // --- The mouse in the 3D view
  } else if (name == QLatin1String("click") || name == QLatin1String("dblclick")) {
    double x = 0;
    double y = 0;
    Qt::MouseButton button = Qt::LeftButton;
    Qt::KeyboardModifiers modifiers;
    QString error;
    if (!toNumber(a.at(0), x) || !toNumber(a.at(1), y)) {
      fail(line, QStringLiteral("the position is not a pair of percentages"));
      return;
    }
    if (!parseMouseOptions(a, 2, button, modifiers, error)) {
      fail(line, error);
      return;
    }
    QPoint at;
    if (!viewPoint(x, y, at)) {
      fail(line, QStringLiteral("no 3D view"));
      return;
    }
    queueClick(at, button, modifiers, name == QLatin1String("click") ? 1 : 2);
  } else if (name == QLatin1String("drag")) {
    double c[4] = {0, 0, 0, 0};
    Qt::MouseButton button = Qt::LeftButton;
    Qt::KeyboardModifiers modifiers;
    QString error;
    for (int i = 0; i < 4; ++i) {
      if (!toNumber(a.at(i), c[i])) {
        fail(line, QStringLiteral("'%1' is not a percentage").arg(a.at(i)));
        return;
      }
    }
    if (!parseMouseOptions(a, 4, button, modifiers, error)) {
      fail(line, error);
      return;
    }
    QPoint from;
    QPoint to;
    if (!viewPoint(c[0], c[1], from) || !viewPoint(c[2], c[3], to)) {
      fail(line, QStringLiteral("no 3D view"));
      return;
    }
    queueDrag(from, to, button, modifiers);
  } else if (name == QLatin1String("wheel")) {
    double x = 0;
    double y = 0;
    double notches = 0;
    Qt::KeyboardModifiers modifiers;
    if (!toNumber(a.at(0), x) || !toNumber(a.at(1), y) || !toNumber(a.at(2), notches) ||
        (a.size() > 3 && !parseModifiers(a.at(3), modifiers))) {
      fail(line, QStringLiteral("wheel <x%> <y%> <notches> [mods]"));
      return;
    }
    QPoint at;
    if (!viewPoint(x, y, at)) {
      fail(line, QStringLiteral("no 3D view"));
      return;
    }
    queue([this, at] { sendMouse(QEvent::MouseMove, at, Qt::NoButton, Qt::NoButton, Qt::NoModifier); });
    queue([this, at, notches, modifiers] { sendWheel(at, qRound(notches), modifiers); });

    // --- The mouse where the application logged a place
  } else if (name == QLatin1String("click-logged") || name == QLatin1String("dblclick-logged") ||
             name == QLatin1String("click-pick")) {
    const bool pick = name == QLatin1String("click-pick");
    QRegularExpression re;
    if (!regex(pick ? QStringLiteral("^Pick (?:.*)(?:%1)").arg(a.at(0)) : a.at(0), re)) {
      return;
    }
    Qt::MouseButton button = Qt::LeftButton;
    Qt::KeyboardModifiers modifiers;
    QString error;
    if (!parseMouseOptions(a, 1, button, modifiers, error)) {
      fail(line, error);
      return;
    }
    const int clicks = name == QLatin1String("dblclick-logged") ? 2 : 1;
    const auto at = std::make_shared<QPoint>();
    whenReady(
        [this, re, pick, at] {
          int from = 0;
          if (pick) {
            // The places of the newest "Pick places:" only; older ones may
            // be stale.
            static const QRegularExpression places(QStringLiteral("^Pick places:"));
            from = std::max(findLastLine(places, 0), 0);
          }
          return loggedPoint(re, from, *at);
        },
        6, QStringLiteral("no place logged for '%1'").arg(a.at(0)),
        [this, at, button, modifiers, clicks] { queueClick(*at, button, modifiers, clicks); }, mainWindow);
  } else if (name == QLatin1String("drag-logged") || name == QLatin1String("drag-from")) {
    const bool relative = name == QLatin1String("drag-from");
    QRegularExpression first;
    QRegularExpression second;
    double dx = 0;
    double dy = 0;
    if (!regex(a.at(0), first)) {
      return;
    }
    if (relative) {
      if (!toNumber(a.at(1), dx) || !toNumber(a.at(2), dy)) {
        fail(line, QStringLiteral("drag-from <regex> <dx> <dy>"));
        return;
      }
    } else {
      if (!regex(a.at(1), second)) {
        return;
      }
      if (a.size() > 2 && !toNumber(a.at(2), dx)) {
        fail(line, QStringLiteral("'%1' is not a number of pixels").arg(a.at(2)));
        return;
      }
    }
    const auto from = std::make_shared<QPoint>();
    const auto to = std::make_shared<QPoint>();
    whenReady(
        [this, first, second, relative, from, to, dx, dy] {
          if (!loggedPoint(first, 0, *from)) {
            return false;
          }
          if (relative) {
            *to = *from + QPoint(qRound(dx), qRound(dy));
            return true;
          }
          if (!loggedPoint(second, 0, *to)) {
            return false;
          }
          *to += QPoint(qRound(dx), 0);
          return true;
        },
        6, QStringLiteral("no place logged for '%1'").arg(a.at(0)),
        [this, from, to] { queueDrag(*from, *to, Qt::LeftButton, Qt::NoModifier); }, mainWindow);
  } else if (name == QLatin1String("type-in")) {
    QRegularExpression re;
    if (!regex(a.at(0), re)) {
      return;
    }
    const QString text = a.at(1);
    const auto at = std::make_shared<QPoint>();
    whenReady(
        [this, re, at] { return loggedPoint(re, 0, *at); }, 6,
        QStringLiteral("no place logged for '%1'").arg(a.at(0)),
        [this, at, text] {
          queueClick(*at, Qt::LeftButton, Qt::NoModifier, 1);
          KeyPress selectAll;
          selectAll.key = Qt::Key_A;
          selectAll.modifiers = Qt::ControlModifier;
          queue([this, selectAll] { sendKey(selectAll); });
          queue([this, text] { sendText(text); });
        },
        mainWindow);

    // --- The mouse at sketch points
  } else if (name == QLatin1String("sketch-click") || name == QLatin1String("sketch-drag")) {
    const bool drag = name == QLatin1String("sketch-drag");
    double c[4] = {0, 0, 0, 0};
    const int count = drag ? 4 : 2;
    for (int i = 0; i < count; ++i) {
      if (!toNumber(a.at(i), c[i])) {
        fail(line, QStringLiteral("'%1' is not a sketch coordinate").arg(a.at(i)));
        return;
      }
    }
    Qt::MouseButton button = Qt::LeftButton;
    Qt::KeyboardModifiers modifiers;
    QString error;
    if (!drag && !parseMouseOptions(a, 2, button, modifiers, error)) {
      fail(line, error);
      return;
    }
    const auto from = std::make_shared<QPoint>();
    const auto to = std::make_shared<QPoint>();
    const double x1 = c[0];
    const double y1 = c[1];
    const double x2 = c[2];
    const double y2 = c[3];
    whenReady(
        [this, drag, x1, y1, x2, y2, from, to] {
          return sketchPoint(x1, y1, *from) && (!drag || sketchPoint(x2, y2, *to));
        },
        6, QStringLiteral("the app logged no sketch view"),
        [this, drag, from, to, button, modifiers] {
          if (drag) {
            queueDrag(*from, *to, Qt::LeftButton, Qt::NoModifier);
          } else {
            queueClick(*from, button, modifiers, 1);
          }
        },
        mainWindow);

    // --- Commands, menus and buttons
  } else if (name == QLatin1String("command")) {
    QAction* action = mainWindow->findChild<QAction*>(QStringLiteral("command_") + a.at(0));
    if (action == nullptr) {
      fail(line, QStringLiteral("no command '%1'").arg(a.at(0)));
      return;
    }
    m_found = action;
    whenReady([this] { return !m_found.isNull() && qobject_cast<QAction*>(m_found.data())->isEnabled(); }, 6,
              QStringLiteral("the command '%1' is not enabled").arg(a.at(0)), [this] {
                if (QAction* target = qobject_cast<QAction*>(m_found.data())) {
                  target->trigger();
                }
              },
              mainWindow);
  } else if (name == QLatin1String("menu")) {
    QMenuBar* bar = mainWindow->findChild<QMenuBar*>();
    if (bar == nullptr) {
      fail(line, QStringLiteral("the main window has no menu bar"));
      return;
    }
    QList<QAction*> level = bar->actions();
    QAction* found = nullptr;
    for (const QString& part : a.at(0).split(QLatin1Char('/'))) {
      found = nullptr;
      for (QAction* candidate : std::as_const(level)) {
        if (plainText(candidate->text()).compare(plainText(part), Qt::CaseInsensitive) == 0) {
          found = candidate;
          break;
        }
      }
      if (found == nullptr) {
        fail(line, QStringLiteral("no menu entry '%1' in '%2'").arg(part, a.at(0)));
        return;
      }
      level.clear();
      if (QMenu* menu = found->menu()) {
        emit menu->aboutToShow(); // entries that are made when it opens
        level = menu->actions();
      }
    }
    m_found = found;
    whenReady([this] { return !m_found.isNull() && qobject_cast<QAction*>(m_found.data())->isEnabled(); }, 6,
              QStringLiteral("the menu entry '%1' is not enabled").arg(a.at(0)), [this] {
                if (QAction* target = qobject_cast<QAction*>(m_found.data())) {
                  target->trigger();
                }
              },
              mainWindow);
  } else if (name == QLatin1String("popup")) {
    const QString entry = a.at(0);
    whenReady(
        [this, entry] {
          const QMenu* menu = qobject_cast<QMenu*>(QApplication::activePopupWidget());
          if (menu == nullptr) {
            return false;
          }
          for (QAction* action : menu->actions()) {
            if (plainText(action->text()).compare(plainText(entry), Qt::CaseInsensitive) == 0 &&
                action->isEnabled()) {
              m_found = action;
              return true;
            }
          }
          return false;
        },
        6, QStringLiteral("no entry '%1' in an open menu").arg(entry),
        [this] {
          QMenu* menu = qobject_cast<QMenu*>(QApplication::activePopupWidget());
          QAction* action = qobject_cast<QAction*>(m_found.data());
          if (menu == nullptr || action == nullptr) {
            return;
          }
          // As the keyboard does: the entry current, then Return.
          menu->setActiveAction(action);
          KeyPress enter;
          enter.key = Qt::Key_Return;
          enter.text = QStringLiteral("\r");
          sendKey(enter);
        });
  } else if (name == QLatin1String("button")) {
    QRegularExpression re;
    if (!regex(a.at(0), re)) {
      return;
    }
    const QPointer<QWidget> window = targetWindow();
    whenReady(
        [this, re, window] {
          if (window.isNull()) {
            return false;
          }
          const QList<QAbstractButton*> buttons = window->findChildren<QAbstractButton*>();
          for (QAbstractButton* button : buttons) {
            if (button->isVisibleTo(window.data()) && button->isEnabled() &&
                re.match(plainText(button->text())).hasMatch()) {
              m_found = button;
              return true;
            }
          }
          return false;
        },
        6, QStringLiteral("no button matching '%1'").arg(a.at(0)),
        [this] {
          if (auto* button = qobject_cast<QAbstractButton*>(m_found.data())) {
            button->click();
          }
        },
        window.data());

    // --- Windows
  } else if (name == QLatin1String("focus-dialog")) {
    QRegularExpression re;
    double limit = 10;
    if (!regex(a.at(0), re) || !seconds(1, 10, limit)) {
      return;
    }
    whenReady(
        [this, re] {
          m_found = nullptr;
          const QWidgetList widgets = QApplication::topLevelWidgets();
          for (QWidget* widget : widgets) {
            if (widget == m_main.data() || qobject_cast<GlassCard*>(widget) != nullptr || !widget->isVisible() ||
                !dialogMatches(widget, re)) {
              continue;
            }
            m_found = widget;
            if (widget == QApplication::activeModalWidget()) {
              break;
            }
          }
          return !m_found.isNull();
        },
        limit, QStringLiteral("no window with a title matching '%1'").arg(a.at(0)),
        [this] {
          m_dialog = qobject_cast<QWidget*>(m_found.data());
          if (!m_dialog.isNull()) {
            m_dialog->raise();
            m_dialog->activateWindow();
            qDebug().noquote() << QStringLiteral("TestDriver: dialog '%1'").arg(m_dialog->windowTitle());
          }
          m_notBefore = std::max(m_notBefore, m_clock.elapsed() + 300);
        });
  } else if (name == QLatin1String("focus-main")) {
    m_dialog = nullptr;
    m_notBefore = std::max(m_notBefore, m_clock.elapsed() + 300);

    // --- The log
  } else if (name == QLatin1String("mark")) {
    m_mark = lineCount();
  } else if (name == QLatin1String("wait-log") || name == QLatin1String("expect-log") ||
             name == QLatin1String("expect-ever")) {
    QRegularExpression re;
    double limit = 0;
    if (!regex(a.at(0), re) || !seconds(1, name == QLatin1String("wait-log") ? 30 : 6, limit)) {
      return;
    }
    const bool ever = name == QLatin1String("expect-ever");
    const int from = ever ? 0 : m_mark;
    whenReady([this, re, from] { return findLine(re, from) >= 0; }, limit,
              QStringLiteral("'%1' is not in the log%2").arg(a.at(0), ever ? QString() : QStringLiteral(" since the mark")));
  } else if (name == QLatin1String("expect-no-log") || name == QLatin1String("expect-never")) {
    QRegularExpression re;
    if (!regex(a.at(0), re)) {
      return;
    }
    QString found;
    if (findLine(re, name == QLatin1String("expect-never") ? 0 : m_mark, &found) >= 0) {
      fail(line, QStringLiteral("'%1' is in the log: %2").arg(a.at(0), found));
      return;
    }
  } else if (name == QLatin1String("capture")) {
    QRegularExpression re;
    double limit = 10;
    if (!regex(a.at(1), re) || !seconds(2, 10, limit)) {
      return;
    }
    const QString variable = a.at(0);
    const auto value = std::make_shared<QString>();
    whenReady(
        [this, re, value] {
          QRegularExpressionMatch match;
          if (findLastLine(re, m_mark, nullptr, &match) < 0) {
            return false;
          }
          *value = match.lastCapturedIndex() >= 1 ? match.captured(1) : match.captured(0);
          return true;
        },
        limit, QStringLiteral("'%1' is not in the log since the mark").arg(a.at(1)),
        [this, variable, value] { m_vars.insert(variable, *value); });
  } else if (name == QLatin1String("set")) {
    m_vars.insert(a.at(0), a.at(1));
  } else if (name == QLatin1String("expect-near")) {
    double actual = 0;
    double expected = 0;
    double relative = 1e-6;
    double absolute = 0.01;
    if (!toNumber(a.at(0), actual)) {
      fail(line, QStringLiteral("'%1' is not a number").arg(a.at(0)));
      return;
    }
    if (!toNumber(a.at(1), expected) || (a.size() > 2 && !toNumber(a.at(2), relative)) ||
        (a.size() > 3 && !toNumber(a.at(3), absolute))) {
      fail(line, QStringLiteral("expect-near <actual> <expected> [rel] [abs]: not a number"));
      return;
    }
    if (qAbs(actual - expected) > std::max(relative * qAbs(expected), absolute)) {
      fail(line, QStringLiteral("%1, expected %2 (rel %3, abs %4)").arg(actual, 0, 'g', 12).arg(expected, 0, 'g', 12)
                     .arg(relative).arg(absolute));
      return;
    }

    // --- The system
  } else if (name == QLatin1String("wait-idle")) {
    double limit = 60;
    if (!seconds(0, 60, limit)) {
      return;
    }
    whenReady([this] { return !computing(); }, limit, QStringLiteral("the app is still computing"));
  } else if (name == QLatin1String("sleep")) {
    double milliseconds = 0;
    if (!toNumber(a.at(0), milliseconds)) {
      fail(line, QStringLiteral("'%1' is not a number of milliseconds").arg(a.at(0)));
      return;
    }
    const auto clock = std::make_shared<QElapsedTimer>();
    clock->start();
    whenReady([clock, milliseconds] { return clock->elapsed() >= milliseconds; }, milliseconds / 1000.0 + 5,
              QStringLiteral("sleep"));
  } else if (name == QLatin1String("expect-file")) {
    double limit = 6;
    if (!seconds(1, 6, limit)) {
      return;
    }
    const QString path = a.at(0);
    whenReady([path] { return QFileInfo::exists(path); }, limit, QStringLiteral("'%1' does not exist").arg(path));
  } else if (name == QLatin1String("expect-files")) {
    double limit = 6;
    bool ok = false;
    const int count = a.at(2).toInt(&ok);
    if (!ok || !seconds(3, 6, limit)) {
      fail(line, QStringLiteral("expect-files <dir> <glob> <count> [seconds]"));
      return;
    }
    const QString dir = a.at(0);
    const QString glob = a.at(1);
    whenReady(
        [this, dir, glob, count] {
          const QStringList names =
              QDir(dir).entryList(QStringList{glob}, QDir::Files | QDir::Hidden | QDir::NoDotAndDotDot);
          m_waitDetail = QStringLiteral("found %1: %2").arg(names.size()).arg(names.join(QStringLiteral(", ")));
          return names.size() == count;
        },
        limit, QStringLiteral("not %1 files '%2' in %3").arg(count).arg(glob, dir));
  } else if (name == QLatin1String("screenshot") || name == QLatin1String("screenshot-view")) {
    if (!saveScreenshot(a.at(0), name == QLatin1String("screenshot-view"))) {
      fail(line, QStringLiteral("could not save the screenshot %1").arg(a.at(0)));
      return;
    }
  } else if (name == QLatin1String("screenshot-screen")) {
#ifdef Q_OS_MACOS
    QDir().mkpath(QFileInfo(a.at(0)).absolutePath());
    if (!mac::captureOwnWindows(a.at(0))) {
      fail(line, QStringLiteral("could not save the screenshot %1 (no window on the screen?)").arg(a.at(0)));
      return;
    }
#else
    fail(line, QStringLiteral("screenshot-screen is available on macOS only"));
    return;
#endif
  } else if (name == QLatin1String("screenshot-dialog")) {
    QWidget* window = targetWindow();
    QDir().mkpath(QFileInfo(a.at(0)).absolutePath());
    if (window == nullptr || !window->grab().save(a.at(0))) {
      fail(line, QStringLiteral("could not save the screenshot %1").arg(a.at(0)));
      return;
    }
  } else if (name == QLatin1String("quit")) {
    double code = 0;
    if (a.size() > 0 && !toNumber(a.at(0), code)) {
      fail(line, QStringLiteral("'%1' is not an exit code").arg(a.at(0)));
      return;
    }
    qDebug().noquote() << QStringLiteral("TestDriver: quit (%1 of %2 steps)").arg(m_next).arg(m_steps.size());
    leave(static_cast<int>(code));
  } else if (name == QLatin1String("kill")) {
    qDebug().noquote() << QStringLiteral("TestDriver: kill (%1 of %2 steps)").arg(m_next).arg(m_steps.size());
    std::_Exit(0);
  } else if (name == QLatin1String("stay")) {
    m_stay = true;
  }
}

} // namespace mitcad
