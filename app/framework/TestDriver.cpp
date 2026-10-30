// SPDX-License-Identifier: MIT
#include "TestDriver.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdlib>
#include <memory>
#include <utility>

#include <QAbstractButton>
#include <QAbstractItemView>
#include <QAction>
#include <QApplication>
#include <QComboBox>
#include <QContextMenuEvent>
#include <QDir>
#include <QDockWidget>
#include <QEvent>
#include <QFile>
#include <QFileInfo>
#include <QFormLayout>
#include <QHeaderView>
#include <QImage>
#include <QJsonDocument>
#include <QKeyEvent>
#include <QKeySequence>
#include <QLabel>
#include <QLayout>
#include <QLineEdit>
#include <QMenu>
#include <QMenuBar>
#include <QMessageBox>
#include <QMouseEvent>
#include <QNativeGestureEvent>
#include <QOpenGLWidget>
#include <QPainter>
#include <QPixmap>
#include <QPointingDevice>
#include <QRegion>
#include <QRegularExpression>
#include <QScreen>
#include <QScrollArea>
#include <QSettings>
#include <QShortcut>
#include <QTimer>
#include <QToolBar>
#include <QToolTip>
#include <QTreeWidget>
#include <QTreeWidgetItemIterator>
#include <QTextDocument>
#include <QWheelEvent>
#include <QWidget>
#include <QtGlobal>
#include <QtLogging>

#include "../platform/MacChrome.hpp"
#include "../platform/NativeTestInput.hpp"
#include "../OcctViewer.hpp"
#include "../MainWindow.hpp"
#include "../view/SpaceMouseController.hpp"
#include "../sketch/SketchController.hpp"
#include "../sketch/SketchOverlay.hpp"
#include "../sketch/SketchTool.hpp"
#include "../browser/TimelineWidget.hpp"
#include "GlassCard.hpp"
#include "TestSync.hpp"
#include "Dialogs.hpp"
#include "ComputeProgress.hpp"
#include "Theme.hpp"
#include "TreeItemDelegate.hpp"
#include "mitcad/geometry/geometry.hpp"

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
// How long a logged place waits for the watched timers (TestSync::settling),
// as long as the sync does.
constexpr qint64 kSettleLimitMs = 5000;
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
    {"key-now", 1, -1, kPlain},
    {"qt-type-now", 2, 2, kPlain},
    {"qt-choice", 2, 2, kInputOfTarget},
    {"qt-cancel-progress", 1, 1, kPlain},
    {"cancel-model-job", 1, 1, kPlain},
    {"type", 1, -1, kInputOfTarget},
    {"click", 2, 4, kInputOfMain},
    {"dblclick", 2, 4, kInputOfMain},
    {"drag", 4, 6, kInputOfMain},
    {"wheel", 3, 4, kInputOfMain},
    {"wheel-burst", 4, 5, kInputOfMain},
    {"trackpad-scroll", 5, 6, kInputOfMain},
    {"hover-logged", 1, 1, kInputOfMain},
    {"timeline-leave", 0, 0, kInputOfMain},
    {"expect-timeline-hover", 1, 1, kIdle},
    {"expect-timeline-tooltip", 1, 1, kIdle},
    {"capture-view-image", 1, 1, kIdle},
    {"expect-view-image", 2, 2, kIdle},
    {"click-logged", 1, 3, kInputOfMain},
    {"dblclick-logged", 1, 2, kInputOfMain},
    {"dialog-click-logged", 1, 1, kInputOfTarget},
    {"dialog-dblclick-logged", 1, 1, kInputOfTarget},
    {"context-menu-logged", 2, 2, kInputOfMain},
    {"click-pick", 1, 2, kInputOfMain},
    {"expect-pick-offset", 4, 4, kInputOfMain},
    {"click-pick-offset", 4, 4, kInputOfMain},
    {"click-visible-face", 0, 0, kInputOfMain},
    {"drag-logged", 2, 3, kInputOfMain},
    {"drag-from", 3, 3, kInputOfMain},
    {"sketch-click", 2, 4, kInputOfMain},
    {"sketch-dblclick", 2, 4, kInputOfMain},
    {"sketch-move", 2, 2, kInputOfMain},
    {"expect-sketch-field-dimension", 3, 3, kIdle},
    {"expect-sketch-dimension-clear", 1, 1, kIdle},
    {"sketch-drag", 4, 4, kInputOfMain},
    {"sketch-pointer", 3, 3, kInputOfMain},
    {"expect-profiles", 1, 1, kIdle},
    {"type-in", 2, 2, kInputOfMain},
    {"command", 1, 1, kInputOfMain},
    {"test-status-message", 2, 2, kInputOfMain},
    {"open-file", 1, 1, kInputOfMain},
    {"open-file-now", 1, 1, kPlain},
    {"file-dialog", 2, 4, kInputOfMain},
    {"menu", 1, 1, kInputOfMain},
    {"menu-now", 1, 1, kPlain},
    {"popup", 1, 1, kInputOfTarget},
    {"button", 1, 2, kInputOfTarget},
    {"native-click", 1, 1, kInputOfTarget},
    {"native-hover", 2, 2, kInputOfTarget},
    {"native-type", 2, 2, kInputOfTarget},
    {"native-choice", 2, 2, kInputOfTarget},
    {"expect-native-duplicate-choices", 0, 0, kIdle},
    {"expect-native-widget-refresh", 0, 0, kIdle},
    {"expect-native-application-delegate", 0, 0, kIdle},
    {"native-toolbar-item", 2, 2, kInputOfMain},
    {"expect-native-toolbar-item", 2, 2, kIdle},
    {"expect-native-form-controls", 0, 0, kIdle},
    {"expect-native-disclosure-layout", 0, 0, kIdle},
    {"native-action", 1, 1, kInputOfMain},
    {"native-activate", 1, 1, kInputOfTarget},
    {"native-select", 1, 2, kInputOfTarget},
    {"native-expand", 1, 1, kInputOfTarget},
    {"native-click-now", 1, 1, kPlain},
    {"native-click-on-job", 2, 2, kIdle},
    {"native-history-drag-on-job", 2, 2, kIdle},
    {"fail-next-recovery-command", 1, 1, kIdle},
    {"expect-picking-on-job", 1, 1, kIdle},
    {"native-type-now", 2, 2, kPlain},
    {"native-key-now", 1, -1, kPlain},
    {"native-key-event", 1, -1, kInputOfTarget},
    {"native-file-name", 1, 1, kInputOfTarget},
    {"native-file-directory", 1, 1, kInputOfTarget},
    {"native-file-cancel", 0, 0, kInputOfTarget},
    {"native-file-state", 0, 1, kPlain},
    {"native-system-key", 1, 1, kInputOfTarget},
    {"native-system-type", 1, 1, kInputOfTarget},
    {"native-system-pointer", 1, 1, kInputOfTarget},
    {"native-resize", 2, 2, kInputOfMain},
    {"window-resize", 2, 2, kInputOfMain},
    {"native-move", 2, 2, kInputOfMain},
    {"native-appearance", 1, 1, kInputOfMain},
    {"native-pinch", 3, 3, kInputOfMain},
    {"native-context-menu", 2, 2, kInputOfMain},
    {"native-popup", 2, 2, kInputOfMain},
    {"view-context-menu", 3, 3, kInputOfMain},
    {"view-context-menu-logged", 2, 2, kInputOfMain},
    {"view-pointer", 4, 5, kInputOfMain},
    {"qt-pinch", 3, 3, kInputOfMain},
    {"capture-view-height", 1, 1, kIdle},
    {"expect-view-zoom", 2, 4, kIdle},
    {"capture-view-target", 1, 1, kIdle},
    {"expect-view-pan", 2, 2, kIdle},
    {"expect-viewport", 0, 0, kIdle},
    {"window-state", 1, 1, kIdle},
    {"expect-window-state", 1, 1, kIdle},
    {"window-move", 2, 2, kIdle},
    {"capture-view-orientation", 1, 1, kIdle},
    {"expect-view-orientation", 2, 2, kIdle},
    {"expect-view-buttons", 1, 1, kIdle},
    {"expect-selection", 2, 2, kIdle},
    {"expect-navigation-runtime", 0, 0, kIdle},
    {"expect-assistant-setup", 1, 1, kIdle},
    {"expect-formula-builder", 0, 0, kIdle},
    {"open-formula-field", 1, 1, kInputOfTarget},
    {"expect-navigation-controls", 1, 1, kIdle},
    {"expect-selection-info", 1, 3, kIdle},
    {"expect-native-text", 2, 2, kIdle},
    {"expect-native-enabled", 2, 2, kIdle},
    {"expect-native-visible", 2, 2, kIdle},
    {"expect-native-present", 2, 2, kIdle},
    {"expect-native-style", 2, 2, kIdle},
    {"expect-native-columns", 1, 2, kIdle},
    {"native-dialog-size", 2, 2, kInputOfTarget},
    // Native tile grids, group lists and utility panels (mitcad#813, #819).
    {"native-tiles", 2, 4, kInputOfTarget},
    {"expect-native-tiles", 1, 4, kIdle},
    {"expect-native-layout", 0, 0, kIdle},
    {"expect-native-history", 0, -1, kIdle},
    {"expect-native-history-wheel", 0, 0, kIdle},
    {"native-history-drag", 1, 2, kInputOfMain},
    {"native-history-drag-now", 1, 2, kPlain},
    {"expect-model-busy", 1, 1, kPlain},
    {"hold-model-preview", 1, 1, kPlain},
    {"test-manipulator-feedback", 0, 0, kInputOfMain},
    {"test-body-highlights", 1, 1, kInputOfMain},
    {"capture-undo-depth", 1, 1, kIdle},
    {"capture-sketch", 2, 2, kIdle},
    {"expect-undo-depth", 2, 2, kIdle},
    {"expect-model-bodies", 2, 2, kIdle},
    {"expect-model-parameter", 2, 2, kIdle},
    {"expect-input-text", 2, 2, kIdle},
    {"expect-shown-body", 3, 9, kIdle},
    {"capture-shown-body", 3, 3, kIdle},
    {"expect-native-dialog-gone", 1, 2, kPlain},
    {"focus-dialog", 1, 2, kPlain},
    {"focus-main", 0, 0, kPlain},
    {"focus-widget", 1, 1, kInputOfTarget},
    {"expect-widget-property", 3, 3, kIdle},
    {"expect-no-qt-popup", 0, 0, kIdle},
    {"expect-projects-layout", 1, 1, kIdle},
    {"expect-sketch-finish-layout", 0, 0, kIdle},
    {"expect-ribbon-labels", 1, -1, kIdle},
    {"expect-tree-rename", 1, 1, kIdle},
    {"native-drag", 2, 2, kInputOfTarget},
    {"expect-native-focus", 1, 2, kIdle},
    {"expect-native-selection", 2, 2, kIdle},
    {"collapse-card", 2, 2, kInputOfMain},
    {"expect-setting", 2, 2, kIdle},
    {"expect-dialog-gone", 1, 1, kIdle},
    {"dialog-resize", 2, 2, kInputOfTarget},
    {"expect-dialog-appearance", 1, 1, kIdle},
    {"show-dialog-style-fixture", 1, 1, kIdle},
    {"test-dialog-controls", 1, 1, kInputOfTarget},
    {"expect-macos-tool-buttons", 0, 0, kIdle},
    {"expect-command-search", 2, 3, kIdle},
    {"click-parameter", 2, 2, kInputOfTarget},
    {"expect-parameter-cell", 4, 4, kIdle},
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
    {"screenshot-widget", 2, 2, kIdle},
    {"quit", 0, 1, kPlain},
    {"kill", 0, 0, kPlain},
    {"stay", 0, 0, kPlain},
    // Card widths and windows within the screen (mitcad#818, TestDriverWindowFit.cpp).
    {"expect-dialog-fits", 0, 2, kIdle},
    {"fit-main-frame", 2, 2, kInputOfMain},
    {"expect-main-frame", 2, 2, kIdle},
    {"native-card-width", 2, 2, kInputOfMain},
    {"expect-native-card-width", 2, 3, kIdle},
    {"expect-native-name-fits", 2, 2, kIdle},
    {"native-screen-limit", 2, 2, kIdle},
    {"expect-native-window-fits", 1, 1, kIdle},
    // Compact AppKit selection and command cards (mitcad#815).
    {"expect-native-selection-card", 1, 1, kIdle},
    {"expect-native-command-layout", 0, -1, kIdle},
    // Shared layout bounds, real pointer input and button order (mitcad#812, #817).
    {"expect-native-bounds", 2, 3, kIdle},
    {"expect-native-form-layout", 1, -1, kIdle},
    {"expect-native-configuration-layout", 0, 0, kIdle},
    {"expect-native-spanned-heading", 2, 2, kIdle},
    {"expect-native-first-row", 2, 2, kIdle},
    {"expect-native-color-field", 4, 5, kIdle},
    {"expect-native-button-order", 1, -1, kIdle},
    {"expect-native-control-focus", 1, 1, kIdle},
    {"expect-native-layout-contract", 0, 0, kIdle},
    {"expect-native-settings-layout", 1, 1, kIdle},
    {"native-pointer-click", 1, 3, kInputOfTarget},
    {"native-pointer-popup", 2, 3, kInputOfTarget},
    {"native-pointer-dblclick", 1, 1, kInputOfTarget},
    {"native-pointer-choice", 4, 34, kInputOfTarget},
    {"native-pointer-button", 1, 1, kInputOfTarget},
    {"native-pointer-drag", 3, 3, kInputOfTarget},
    {"native-header-resize", 3, 3, kInputOfTarget},
    {"expect-native-header-resize-retained", 2, 2, kIdle},
    {"native-dialog-resize", 2, 2, kInputOfTarget},
    // A 3D mouse's axes and buttons (mitcad#951).
    {"space-mouse", 6, 7, kInputOfMain},
    {"space-mouse-button", 1, 1, kInputOfMain},
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

// Stacked workspaces can reuse object names. Resolve the visible instance
// inside the existing lookup scope, never a hidden page's first match.
// Property assertions also inspect disabled controls and, when there is no
// visible match, a unique hidden control (for example visible=false).
QWidget* namedWidget(QWidget* scope, const QString& name, bool forInput = false) {
  if (scope == nullptr) return nullptr;
  const QList<QWidget*> matches = scope->findChildren<QWidget*>(name);
  QWidget* visible = nullptr;
  for (QWidget* candidate : matches) {
    if (!candidate->isVisible() || (forInput && !candidate->isEnabled())) continue;
    if (visible != nullptr) return nullptr; // ambiguous visible identifier
    visible = candidate;
  }
  if (visible != nullptr || forInput) return visible;
  return matches.size() == 1 ? matches.first() : nullptr;
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
      if (mac::test::active()) {
        mac::test::focusMain();
        return true;
      }
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
  QObject::disconnect(m_jobPickConnection);
  m_jobPickLabel.clear();
  QObject::disconnect(m_nativeJobClickConnection);
  m_nativeJobClickLabel.clear();
  QObject::disconnect(m_nativeJobHistoryDragConnection);
  m_nativeJobHistoryDragLabel.clear();
  m_finished = true;
  if (m_timer != nullptr) {
    m_timer->stop();
  }
  // Closes no window (no closeEvent, no question about unsaved work); every
  // event loop that runs, a dialog's too, ends.
  QCoreApplication::exit(code);
}

void TestDriver::finish() {
  if (!m_jobPickLabel.isEmpty()) {
    fail(m_jobPickLine, QStringLiteral("the armed pick's job '%1' never started").arg(m_jobPickLabel));
    return;
  }
  if (!m_nativeJobClickLabel.isEmpty()) {
      fail(m_nativeJobClickLine, QStringLiteral("the armed button's job '%1' never started").arg(m_nativeJobClickLabel));
    return;
  }
  if (!m_nativeJobHistoryDragLabel.isEmpty()) {
    fail(m_nativeJobHistoryDragLine,
         QStringLiteral("the armed native history drag's job '%1' never started").arg(m_nativeJobHistoryDragLabel));
    return;
  }
  if (auto* window = qobject_cast<MainWindow*>(m_main.data())) {
    if (window->documentSession().m_testHoldPreview->load()) {
      window->documentSession().m_testHoldPreview->store(false);
      fail(m_line, QStringLiteral("the preview worker gate was not released"));
      return;
    }
    if (!window->documentSession().m_testFailCommand.isEmpty()) {
      fail(m_line, QStringLiteral("the armed recovery command '%1' never started")
                       .arg(window->documentSession().m_testFailCommand));
      return;
    }
  }
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
  if (mac::test::sendKey(press.key, press.modifiers, press.text)) return;
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
#ifdef Q_OS_MACOS
  // AppKit dispatches the global menu's Close even when a Qt tool window is key.
  if (window != m_main.data() && wanted == QKeySequence(QKeySequence::Close) && m_main) {
    if (auto* close = m_main->findChild<QAction*>(QStringLiteral("command_file.close")); close && close->isEnabled()) {
      close->trigger();
      return true;
    }
  }
#endif
  return false;
}

void TestDriver::sendText(const QString& text) {
  if (mac::test::sendText(text)) return;
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

void TestDriver::queueDialogClick(const QPoint& at, int clicks) {
  const QPointer<QWidget> window(targetWindow());
  if (!window || window == m_main) {
    fail(m_line, QStringLiteral("dialog pointer input requires focus-dialog"));
    return;
  }
  const QPoint global = m_main->mapToGlobal(at);
  const QPoint inWindow = window->mapFromGlobal(global);
  const QPointer<QWidget> receiver(window->childAt(inWindow));
  if (!receiver || !receiver->isVisible()) {
    fail(m_line, QStringLiteral("logged dialog point is outside its visible controls"));
    return;
  }
  const auto send = [window, receiver, global](QEvent::Type type) {
    if (!window || !receiver) return;
    const bool released = type == QEvent::MouseButtonRelease;
    const bool moved = type == QEvent::MouseMove;
    QMouseEvent event(type, receiver->mapFromGlobal(global), window->mapFromGlobal(global), global,
                      moved ? Qt::NoButton : Qt::LeftButton,
                      moved || released ? Qt::NoButton : Qt::LeftButton, Qt::NoModifier);
    QCoreApplication::sendEvent(receiver, &event);
  };
  queue([send] { send(QEvent::MouseMove); });
  for (int i = 0; i < clicks; ++i) {
    queue([send, i] { send(i == 0 ? QEvent::MouseButtonPress : QEvent::MouseButtonDblClick); });
    queue([send] { send(QEvent::MouseButtonRelease); });
  }
}

void TestDriver::queueDrag(const QPoint& from, const QPoint& to, Qt::MouseButton button, Qt::KeyboardModifiers modifiers) {
  const int requested = qEnvironmentVariableIntValue("MITCAD_TEST_DRAG_STEPS");
  const int steps = requested > 0 ? std::clamp(requested, 4, 1000) : kDragSteps;
  const int pause = std::clamp(qEnvironmentVariableIntValue("MITCAD_TEST_DRAG_PAUSE_TICKS"), 0, 1000);
  const bool waitPreview = qEnvironmentVariableIntValue("MITCAD_TEST_DRAG_WAIT_PREVIEW") == 1;
  queue([this, from, modifiers] { sendMouse(QEvent::MouseMove, from, Qt::NoButton, Qt::NoButton, modifiers); });
  auto clock = std::make_shared<QElapsedTimer>();
  auto gestureStart = std::make_shared<int>(0);
  queue([this, from, button, modifiers, clock, requested, gestureStart] {
    if (requested > 0) { qInfo("Held drag: begin"); clock->start(); }
    *gestureStart = lineCount();
    sendMouse(QEvent::MouseButtonPress, from, button, button, modifiers);
  });
  for (int i = 1; i <= steps; ++i) {
    if (requested > 0 && i == steps / 2) {
      queue([] { qInfo("Held drag: pause"); });
      for (int tick = 0; tick < pause; ++tick) { queue([] {}); }
      queue([] { qInfo("Held drag: resumed"); });
    }
    const QPoint at(from.x() + (to.x() - from.x()) * i / steps, from.y() + (to.y() - from.y()) * i / steps);
    queue([this, at, button, modifiers] { sendMouse(QEvent::MouseMove, at, Qt::NoButton, button, modifiers); });
  }
  queue([] {}); // a pause before the button goes up
  queue([] {});
  const auto release = [this, to, button, modifiers, clock, requested, steps, waitPreview] {
    if (waitPreview) { qInfo("Held drag: exact preview available before release"); }
    sendMouse(QEvent::MouseButtonRelease, to, button, Qt::NoButton, modifiers);
    if (requested > 0) { qInfo() << "Held drag: end, delivered=" << steps << "elapsed_ms=" << clock->elapsed(); }
  };
  queue([this, waitPreview, gestureStart, release] {
    if (!waitPreview) { release(); return; }
    // The final micro-event owns release: wait ticks keep processing the
    // worker completion and paints without delivering another move. Only
    // a solid installed during this gesture counts, never an older preview.
    const QRegularExpression installed(QStringLiteral("^Preview installed: .+, dragging=1$"));
    whenReady([this, installed, gestureStart] { return findLine(installed, *gestureStart) >= 0; },
              6, QStringLiteral("no exact preview was installed while the mouse button was held"), release);
  });
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

bool TestDriver::saveScreenshot(const QString& path, bool viewOnly, QWidget* crop) {
  QDir().mkpath(QFileInfo(path).absolutePath());
  QOpenGLWidget* gl = qobject_cast<QOpenGLWidget*>(viewer());
  if (viewOnly) {
    // What --screenshot saves.
    return gl != nullptr && gl->grabFramebuffer().save(path);
  }
  if (mac::test::active()) return crop == nullptr && mac::test::screenshot(path, gl);
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
  if (crop != nullptr && !shot.isNull()) {
    // Cut to the widget, whatever window it is in (a card's or the main
    // window's), in the shot's device pixels.
    const QRect area =
        QRect(top->mapFromGlobal(crop->mapToGlobal(QPoint(0, 0))), crop->size()).intersected(top->rect());
    if (area.isEmpty()) {
      return false;
    }
    const qreal scale = shot.devicePixelRatio();
    shot = shot.copy(QRectF(QPointF(area.topLeft()) * scale, QSizeF(area.size()) * scale).toAlignedRect());
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

  // Card widths and windows within the screen (mitcad#818).
  if (runWindowFitStep(name, a, line)) {
    return;
  }

  // --- Keys and text
  bool stepMatched = true;
  if (name == QLatin1String("expect-picking-on-job")) {
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    auto* session = window ? window->findChild<DocumentSession*>() : nullptr;
    if (!session || window->modelBusy() || !m_jobPickLabel.isEmpty()) {
      fail(line, QStringLiteral("expect-picking-on-job requires an idle document and no armed pick")); return;
    }
    auto& viewer = window->viewer();
    const auto points = viewer.pickPoints(SelectKind::Face | SelectKind::Edge | SelectKind::Body);
    if (points.empty() || !viewer.itemAt(points.front().second)) {
      fail(line, QStringLiteral("expect-picking-on-job requires a pickable body")); return;
    }
    const QPoint point = points.front().second;
    const QString label = a.at(0);
    m_jobPickLabel = label;
    m_jobPickLine = line;
    const QPointer<MainWindow> guard(window);
    m_jobPickConnection = connect(session, &DocumentSession::jobStarted, this,
        [this, guard, point, label, line](const QString& started, const QString&) {
          if (started != label) return;
          QObject::disconnect(m_jobPickConnection);
          m_jobPickLabel.clear();
          if (!guard || !guard->modelBusy()) {
            fail(line, QStringLiteral("the armed pick did not observe a busy job")); return;
          }
          // Same OCCT MoveTo/filter path as a hover inside paintGL, with
          // deterministic timing before the job can finish.
          if (guard->viewer().itemAt(point) || !guard->viewer().itemsAt(point).empty() ||
              !guard->viewer().pickPoints(SelectKind::Face | SelectKind::Edge | SelectKind::Body).empty()) {
            fail(line, QStringLiteral("a model item remained pickable during a job")); return;
          }
          qInfo().noquote() << QStringLiteral("Picking suspended during %1 (busy)").arg(label);
        });
  } else if (name == QLatin1String("cancel-model-job")) {
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    auto* session = window ? window->findChild<DocumentSession*>() : nullptr;
    if (!session || !session->busy() || !session->activeJob() ||
        session->activeJob()->stage() != tr("Preview of %1").arg(a.at(0))) {
      fail(line, QStringLiteral("cancel-model-job requires the named command's running preview")); return;
    }
    session->cancelJob();
    qInfo().noquote() << QStringLiteral("Test preview cancellation requested: %1").arg(a.at(0));
  } else if (name == QLatin1String("qt-cancel-progress")) {
    if (mac::test::active() || a.at(0).isEmpty()) {
      fail(line, QStringLiteral("qt-cancel-progress requires the Qt frontend and a job label")); return;
    }
    const QString label = a.at(0);
    const auto ownsProgress = [](const MainWindow* window, const ComputeProgress* progress) {
      if (!window || !progress) return false;
      // A window-modal QDialog is a top-level widget. QWidget ancestry
      // stops at that window boundary, but QObject ownership retains the
      // document that constructed this particular progress dialog.
      for (const QObject* owner = progress->parent(); owner; owner = owner->parent()) {
        if (owner == window) return true;
      }
      return false;
    };
    whenReady([this, label, ownsProgress] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      QWidget* modal = QApplication::activeModalWidget();
      auto* progress = qobject_cast<ComputeProgress*>(modal);
      // The synchronous Qt wait logs its current job before showing this
      // modal dialog. Match the newest start, so a preceding preview cannot
      // satisfy an edit_feature cancellation request.
      static const QRegularExpression started(QStringLiteral("^Computing (\\S+) started$"));
      QRegularExpressionMatch match;
      const int newest = findLastLine(started, m_mark, nullptr, &match);
      QStringList candidates;
      if (window) {
        for (const auto* candidate : window->findChildren<ComputeProgress*>()) {
          candidates << QStringLiteral("%1:visible=%2,modal=%3,owned=%4")
              .arg(candidate->objectName()).arg(candidate->isVisible()).arg(int(candidate->windowModality()))
              .arg(ownsProgress(window, candidate));
        }
      }
      const QString detail = QStringLiteral("busy=%1, appState=%2, activeModal=%3/%4, visible=%5, progress=%6, "
          "cancelling=%7, owned=%8, newestJob=%9, candidates=[%10]")
          .arg(window && window->modelBusy()).arg(int(QGuiApplication::applicationState()))
          .arg(modal ? QString::fromLatin1(modal->metaObject()->className()) : QStringLiteral("none"))
          .arg(modal ? modal->objectName() : QStringLiteral("none")).arg(modal && modal->isVisible())
          .arg(progress != nullptr).arg(progress && progress->isCancelling())
          .arg(ownsProgress(window, progress))
          .arg(newest >= 0 ? match.captured(1) : QStringLiteral("none")).arg(candidates.join(QStringLiteral("; ")));
      if (detail != m_waitDetail && window && window->modelBusy()) {
        qInfo().noquote() << QStringLiteral("TestDriver: Qt progress readiness: %1").arg(detail);
      }
      m_waitDetail = detail;
      if (!window || !window->modelBusy() || !progress || !progress->isVisible() || progress->isCancelling() ||
          !ownsProgress(window, progress) || newest < 0 || match.captured(1) != label) return false;
      m_found = progress;
      return true;
    }, 10, QStringLiteral("no live Qt progress dialog for '%1'").arg(label), [this, label, line, ownsProgress] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      auto* progress = qobject_cast<ComputeProgress*>(m_found.data());
      if (!window || !window->modelBusy() || !progress || QApplication::activeModalWidget() != progress ||
          !progress->isVisible() || progress->isCancelling() || !ownsProgress(window, progress)) {
        fail(line, QStringLiteral("the Qt progress dialog lost its live job before cancellation")); return;
      }
      const auto buttons = progress->findChildren<QAbstractButton*>();
      for (QAbstractButton* button : buttons) {
        if (plainText(button->text()) == QStringLiteral("Cancel") && button->isVisible() && button->isEnabled()) {
          button->click();
          if (!progress->isCancelling()) {
            fail(line, QStringLiteral("the real Qt progress Cancel button did not request cancellation")); return;
          }
          qInfo().noquote() << QStringLiteral("Qt test: clicked progress Cancel during %1 (busy)").arg(label);
          return;
        }
      }
      fail(line, QStringLiteral("the live Qt progress dialog has no enabled Cancel button"));
    });
  } else if (name == QLatin1String("native-click-on-job")) {
    if (!mac::test::active()) { fail(line, QStringLiteral("native-click-on-job requires the native macOS UI")); return; }
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    auto* session = window ? window->findChild<DocumentSession*>() : nullptr;
    if (!session || window->modelBusy() || a.at(0).isEmpty() || a.at(1).isEmpty() || !m_nativeJobClickLabel.isEmpty()) {
      fail(line, QStringLiteral("native-click-on-job requires an idle document and no existing armed click")); return;
    }
    m_nativeJobClickLabel = a.at(0);
    m_nativeJobClickLine = line;
    const QString label = a.at(0);
    const QString identifier = a.at(1);
    const QPointer<MainWindow> guard(window);
    m_nativeJobClickConnection = QObject::connect(session, &DocumentSession::jobStarted, this,
        [this, guard, label, identifier, line](const QString& started, const QString&) {
          if (started != label) return;
          QObject::disconnect(m_nativeJobClickConnection);
          m_nativeJobClickLabel.clear();
          if (!guard || !guard->modelBusy()) {
            fail(line, QStringLiteral("the armed click did not observe an actual busy '%1' job").arg(label)); return;
          }
          QString error;
          if (!mac::test::clickImmediately(identifier, error)) { fail(line, error); return; }
          qInfo().noquote() << QStringLiteral("Native test: clicked %1 during %2 (busy)").arg(identifier, label);
        });
  } else if (name == QLatin1String("native-history-drag-on-job")) {
    if (!mac::test::active()) { fail(line, QStringLiteral("native-history-drag-on-job requires the native macOS UI")); return; }
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    auto* session = window ? window->findChild<DocumentSession*>() : nullptr;
    double index = -1;
    if (!session || window->modelBusy() || a.at(0).isEmpty() || !m_nativeJobHistoryDragLabel.isEmpty() ||
        !toNumber(a.at(1), index) || index < 0 || index != std::floor(index)) {
      fail(line, QStringLiteral("native-history-drag-on-job requires an idle document, a final index and no armed drag"));
      return;
    }
    m_nativeJobHistoryDragLabel = a.at(0);
    m_nativeJobHistoryDragLine = line;
    const QString label = a.at(0);
    const QPointer<MainWindow> guard(window);
    m_nativeJobHistoryDragConnection = QObject::connect(session, &DocumentSession::jobStarted, this,
        [this, guard, label, index, line](const QString& started, const QString&) {
          if (started != label) return;
          QObject::disconnect(m_nativeJobHistoryDragConnection);
          m_nativeJobHistoryDragLabel.clear();
          for (const QString& phase : {QStringLiteral("to"), QStringLiteral("end")}) {
            if (!guard || !guard->modelBusy()) {
              fail(line, QStringLiteral("the armed history drag did not observe busy '%1' before %2").arg(label, phase));
              return;
            }
            QString error;
            if (!mac::test::historyMarkerEvent(phase, int(index), error)) { fail(line, error); return; }
            if (!guard || !guard->modelBusy()) {
              fail(line, QStringLiteral("the armed history drag did not observe busy '%1' after %2").arg(label, phase));
              return;
            }
          }
          qInfo().noquote() << QStringLiteral("Native test: history marker moved to %1 and released during %2 (busy before/after both inputs)")
              .arg(int(index)).arg(label);
        });
  } else if (name == QLatin1String("fail-next-recovery-command")) {
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    auto* session = window ? window->findChild<DocumentSession*>() : nullptr;
    if (!mac::test::active() || !session || window->modelBusy() || !session->state().activeCommand ||
        !session->m_testFailCommand.isEmpty() ||
        (a.at(0) != QLatin1String("restore_command") && a.at(0) != QLatin1String("discard_command"))) {
      fail(line, QStringLiteral("fail-next-recovery-command requires an idle native command and restore_command|discard_command"));
      return;
    }
    session->m_testFailCommand = a.at(0);
  } else if (name == QLatin1String("capture-undo-depth")) {
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    if (!window || window->modelBusy()) { fail(line, QStringLiteral("capture-undo-depth requires an idle document")); return; }
    const QJsonValue depth = window->query({{QStringLiteral("query"), QStringLiteral("document")}})
                                 .toObject().value(QStringLiteral("undo_depth"));
    if (!depth.isDouble() || !std::isfinite(depth.toDouble()) || depth.toDouble() < 0 ||
        depth.toDouble() != std::floor(depth.toDouble())) {
      fail(line, QStringLiteral("the document query returned no valid numeric undo depth")); return;
    }
    m_vars[a.at(0)] = QString::number(depth.toInteger());
  } else if (name == QLatin1String("expect-undo-depth")) {
    double base = 0;
    double delta = 0;
    if (!toNumber(a.at(0), base) || !toNumber(a.at(1), delta) || base != std::floor(base) ||
        delta != std::floor(delta) || base + delta < 0) {
      fail(line, QStringLiteral("expect-undo-depth <base-depth> <integer-change>")); return;
    }
    whenReady([this, base, delta] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window || window->modelBusy()) return false;
      const int depth = window->query({{QStringLiteral("query"), QStringLiteral("document")}})
                            .toObject().value(QStringLiteral("undo_depth")).toInt(-1);
      m_waitDetail = QStringLiteral("actual undo depth %1").arg(depth);
      return depth == base + delta;
    }, 6, QStringLiteral("undo depth did not change by %1 from %2").arg(delta).arg(base));
  } else if (name == QLatin1String("expect-input-text")) {
    QRegularExpression re;
    if (!regex(a.at(1), re)) { return; }
    const QString id = QStringLiteral("input_") + a.at(0);
    whenReady([this, id, re] {
      if (mac::test::active()) { return mac::test::textMatches(id, re); }
      QWidget* window = m_main.data();
      const auto* input = window ? window->findChild<QLineEdit*>(id) : nullptr;
      return input && re.match(input->text()).hasMatch();
    }, 6, QStringLiteral("input %1 did not match %2").arg(a.at(0), a.at(1)));
  } else if (name == QLatin1String("capture-sketch")) {
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    if (!window || window->modelBusy()) {
      fail(line, QStringLiteral("capture-sketch requires an idle document")); return;
    }
    const QJsonValue sketch = window->query({{QStringLiteral("query"), QStringLiteral("sketch")},
                                             {QStringLiteral("uid"), a.at(0)}});
    if (!sketch.isObject() || !sketch.toObject().value(QStringLiteral("entities")).isArray()) {
      fail(line, QStringLiteral("the sketch query returned no entities")); return;
    }
    const QByteArray json = QJsonDocument(sketch.toObject()).toJson();
    QDir().mkpath(QFileInfo(a.at(1)).absolutePath());
    QFile output(a.at(1));
    if (!output.open(QIODevice::WriteOnly) || output.write(json) != json.size()) {
      fail(line, QStringLiteral("cannot write the sketch query snapshot"));
    }
  } else if (name == QLatin1String("expect-model-parameter")) {
    const QString parameter = a.at(0);
    const bool absent = a.at(1) == QStringLiteral("absent");
    double expected = 0;
    if (!absent && !toNumber(a.at(1), expected)) { return; }
    whenReady([this, parameter, absent, expected] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window || window->modelBusy()) { return false; }
      for (const QJsonValue& value : window->queryArray(QStringLiteral("parameters"))) {
        const QJsonObject current = value.toObject();
        if (current.value(QStringLiteral("name")).toString() != parameter) { continue; }
        return !absent && current.value(QStringLiteral("kind")).toString() == QStringLiteral("user") &&
            std::abs(current.value(QStringLiteral("value")).toDouble() - expected) < 1e-6;
      }
      return absent;
    }, 6, QStringLiteral("parameter %1 did not match %2").arg(parameter, a.at(1)));
  } else if (name == QLatin1String("expect-model-bodies")) {
    double count = 0;
    double volume = 0;
    if (!toNumber(a.at(0), count) || !toNumber(a.at(1), volume) || count < 0 || count != std::floor(count) || volume < 0) {
      fail(line, QStringLiteral("expect-model-bodies <count> <total-volume-mm3>")); return;
    }
    whenReady([this, count, volume] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window || window->modelBusy()) return false;
      const QJsonValue result = window->query({{QStringLiteral("query"), QStringLiteral("bodies")},
                                              {QStringLiteral("volumes"), true}});
      if (!result.isArray()) {
        m_waitDetail = QStringLiteral("the bodies query returned a non-array result");
        return false;
      }
      const QJsonArray bodies = result.toArray();
      double actual = 0;
      for (const QJsonValue& body : bodies) {
        const QJsonValue amount = body.toObject().value(QStringLiteral("volume"));
        if (!amount.isDouble() || !std::isfinite(amount.toDouble()) || amount.toDouble() < 0) {
          m_waitDetail = QStringLiteral("a body has no finite numeric volume");
          return false;
        }
        actual += amount.toDouble();
      }
      if (!std::isfinite(actual)) { m_waitDetail = QStringLiteral("the total body volume is not finite"); return false; }
      m_waitDetail = QStringLiteral("%1 bodies, total volume %2 mm3").arg(bodies.size()).arg(actual, 0, 'g', 12);
      return bodies.size() == count && std::abs(actual - volume) <= std::max(1e-5, volume * 1e-6);
    }, 6, QStringLiteral("expected %1 bodies with total volume %2 mm3").arg(count).arg(volume));
  } else if (name == QLatin1String("capture-shown-body")) {
    const QString prefix = a.at(0);
    const QString uid = a.at(1);
    const QString occurrence = a.at(2) == QLatin1String("-") ? QString() : a.at(2);
    whenReady([this, prefix, uid, occurrence] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window || window->modelBusy()) return false;
      for (const BodyDisplay& body : window->viewer().shownBodies()) {
        if (QString::fromStdString(body.uid) != uid || QString::fromStdString(body.occurrence) != occurrence || !body.shape)
          continue;
        const double volume = geometry::volume(*body.shape);
        const geometry::Shape world(body.shape->occt().Moved(body.placement));
        const auto box = geometry::bounding_box(world);
        if (!std::isfinite(volume) || volume < 0 || box.empty) break;
        const std::array<double, 7> values{volume, box.min.X(), box.min.Y(), box.min.Z(),
                                         box.max.X(), box.max.Y(), box.max.Z()};
        const char* keys[] = {"volume", "min_x", "min_y", "min_z", "max_x", "max_y", "max_z"};
        for (std::size_t i = 0; i < values.size(); ++i) {
          if (!std::isfinite(values[i])) return false;
        }
        for (std::size_t i = 0; i < values.size(); ++i)
          m_vars.insert(prefix + QLatin1Char('_') + QLatin1String(keys[i]), QString::number(values[i], 'g', 17));
        qInfo().noquote() << QStringLiteral("TestDriver: shown body %1: %2 at %3, volume %4, bounds %5 %6 %7 %8 %9 %10")
            .arg(prefix, uid, occurrence).arg(volume, 0, 'g', 17)
            .arg(values[1], 0, 'g', 17).arg(values[2], 0, 'g', 17).arg(values[3], 0, 'g', 17)
            .arg(values[4], 0, 'g', 17).arg(values[5], 0, 'g', 17).arg(values[6], 0, 'g', 17);
        return true;
      }
      m_waitDetail = QStringLiteral("no finite displayed shape and world bounds for %1 at %2").arg(uid, occurrence);
      return false;
    }, 6, QStringLiteral("cannot capture displayed geometry for %1 at %2").arg(uid, occurrence));
  } else if (name == QLatin1String("expect-shown-body")) {
    std::array<double, 7> expected{};
    bool valid = a.size() == 3 || a.size() == 9;
    for (int i = 2; valid && i < a.size(); ++i) valid = toNumber(a.at(i), expected[static_cast<std::size_t>(i - 2)]);
    if (!valid || expected[0] < 0) {
      fail(line, QStringLiteral("expect-shown-body <uid> <occurrence|-> <volume> [min-x min-y min-z max-x max-y max-z]"));
      return;
    }
    const QString uid = a.at(0);
    const QString occurrence = a.at(1) == QLatin1String("-") ? QString() : a.at(1);
    const bool bounds = a.size() == 9;
    whenReady([this, uid, occurrence, expected, bounds] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window || window->modelBusy()) return false;
      const auto close = [](double actual, double wanted) {
        return std::isfinite(actual) && std::abs(actual - wanted) <= std::max(1e-5, std::abs(wanted) * 1e-6);
      };
      for (const BodyDisplay& body : window->viewer().shownBodies()) {
        if (QString::fromStdString(body.uid) != uid || QString::fromStdString(body.occurrence) != occurrence) continue;
        if (!body.shape) break;
        const double volume = geometry::volume(*body.shape);
        m_waitDetail = QStringLiteral("shown %1 at %2: volume %3 mm3").arg(uid, occurrence).arg(volume, 0, 'g', 12);
        if (!close(volume, expected[0])) return false;
        if (!bounds) return true;
        const geometry::Shape world(body.shape->occt().Moved(body.placement));
        const auto box = geometry::bounding_box(world);
        if (box.empty) { m_waitDetail += QStringLiteral(", empty bounds"); return false; }
        const std::array<double, 6> actual{box.min.X(), box.min.Y(), box.min.Z(), box.max.X(), box.max.Y(), box.max.Z()};
        for (std::size_t i = 0; i < actual.size(); ++i) {
          if (!close(actual[i], expected[i + 1])) {
            m_waitDetail += QStringLiteral(", bound %1: %2 instead of %3").arg(i).arg(actual[i]).arg(expected[i + 1]);
            return false;
          }
        }
        return true;
      }
      m_waitDetail = QStringLiteral("no displayed shape for %1 at %2").arg(uid, occurrence);
      return false;
    }, 6, QStringLiteral("displayed geometry differs for %1 at %2").arg(uid, occurrence));
  } else if (name == QLatin1String("expect-native-history-wheel")) {
    QString error;
    if (!mac::test::historyWheel(error)) { fail(line, error); }
  } else if (name == QLatin1String("expect-native-history")) {
    if (!mac::test::active()) { fail(line, QStringLiteral("expect-native-history requires the native macOS UI")); return; }
    const bool checkSelection = !a.isEmpty();
    const QStringList selected = a == QStringList{QStringLiteral("-")} ? QStringList{} : a;
    whenReady([this, selected, checkSelection] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window || window->modelBusy()) return false;
      auto* timeline = window->findChild<TimelineWidget*>();
      if (checkSelection && (!timeline || timeline->selectedFeatures() != selected)) {
        m_waitDetail = QStringLiteral("shared timeline selection '%1' differs from '%2'")
            .arg(timeline ? timeline->selectedFeatures().join(QLatin1Char(',')) : QStringLiteral("missing"),
                 selected.join(QLatin1Char(',')));
        return false;
      }
      const QJsonObject state = window->query({{QStringLiteral("query"), QStringLiteral("timeline")}}).toObject();
      return mac::test::historyLayout(state, timeline ? timeline->filteredFeatures() : QSet<QString>(),
                                      selected, checkSelection, m_waitDetail);
    }, 6, QStringLiteral("native history does not match the real model timeline"));
  } else if (name == QLatin1String("test-body-highlights")) {
    const QString style = a.at(0);
    if (style != QLatin1String("none") && style != QLatin1String("default") &&
        style != QLatin1String("opaque") && style != QLatin1String("no-boundaries")) {
      fail(line, QStringLiteral("test-body-highlights none|default|opaque|no-boundaries"));
      return;
    }
    queue([this, style, line] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window) { fail(line, QStringLiteral("no document window")); return; }
      const auto bodies = window->viewer().shownBodies();
      if (bodies.size() != 15) {
        fail(line, QStringLiteral("whole-body diagnostic requires exactly 15 displayed bodies")); return;
      }
      // A fixed navigation contract in this test process only: identical
      // middle-button orbit and immediate camera resets in every variant.
      NavigationSettings navigation = window->viewer().navigation();
      navigation.scheme = NavigationScheme::MiddleOrbit;
      navigation.animate = false;
      window->viewer().setNavigation(navigation);
      std::vector<HighlightDisplay> highlights;
      if (style != QLatin1String("none")) {
        for (const BodyDisplay& body : bodies) {
          SelectionItem item;
          item.kind = SelectKind::Body;
          item.owner = QString::fromStdString(body.uid);
          item.occurrence = QString::fromStdString(body.occurrence);
          const TopoDS_Shape shape = window->itemShape(item);
          if (shape.IsNull()) { fail(line, QStringLiteral("a displayed body has no highlight shape")); return; }
          HighlightDisplay highlight{item, shape, QColor(0x1f, 0x7a, 0xff)};
          highlight.opaqueWholeBody = style == QLatin1String("opaque");
          highlight.wholeBodyBoundaries = style != QLatin1String("no-boundaries");
          highlights.push_back(std::move(highlight));
        }
      }
      window->viewer().setHighlights(highlights);
      qInfo().noquote() << QStringLiteral("Body highlight diagnostic: %1, 15 bodies, %2 overlays")
                              .arg(style).arg(highlights.size());
    });
  } else if (name == QLatin1String("test-manipulator-feedback")) {
    testManipulatorFeedback();
  } else if (name == QLatin1String("hold-model-preview")) {
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    if (!window || (a.at(0) != QStringLiteral("true") && a.at(0) != QStringLiteral("false"))) {
      fail(line, QStringLiteral("hold-model-preview true|false requires a document window")); return;
    }
    window->documentSession().m_testHoldPreview->store(a.at(0) == QStringLiteral("true"));
  } else if (name == QLatin1String("expect-model-busy")) {
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    if (!window || (a.at(0) != QStringLiteral("true") && a.at(0) != QStringLiteral("false"))) {
      fail(line, QStringLiteral("expect-model-busy true|false requires a document window")); return;
    }
    if (window->modelBusy() != (a.at(0) == QStringLiteral("true"))) {
      fail(line, QStringLiteral("the document was not busy=%1 at this input step").arg(a.at(0)));
    }
  } else if (name == QLatin1String("native-history-drag") || name == QLatin1String("native-history-drag-now")) {
    if (!mac::test::active()) { fail(line, QStringLiteral("%1 requires the native macOS UI").arg(name)); return; }
    const QString phase = a.at(0);
    double index = -1;
    if ((phase == QStringLiteral("to") && (a.size() != 2 || !toNumber(a.at(1), index) || index < 0 || index != std::floor(index))) ||
        (phase != QStringLiteral("to") && (a.size() != 1 || (phase != QStringLiteral("begin") && phase != QStringLiteral("end"))))) {
      fail(line, QStringLiteral("native-history-drag begin|to <index>|end")); return;
    }
    const auto perform = [this, phase, index, line] {
      QString error;
      if (!mac::test::historyMarkerEvent(phase, int(index), error)) fail(line, error);
    };
    if (name == QLatin1String("native-history-drag-now")) perform();
    else queue(perform);
  } else if (name == QLatin1String("native-appearance")) {
    if (!mac::test::active()) { fail(line, QStringLiteral("native-appearance requires the native macOS UI")); return; }
    queue([this, a, line] {
      QString error;
      if (!mac::test::setAppearance(a.at(0), error)) fail(line, error);
    });
  } else if (name == QLatin1String("hover-logged")) {
    QRegularExpression re;
    if (!regex(a.at(0), re)) return;
    const auto point = std::make_shared<QPoint>();
    whenReady([this, re, point] { return loggedPoint(re, 0, *point); }, 6,
              QStringLiteral("no hover position logged"), [this, point] {
      sendMouse(QEvent::MouseMove, *point, Qt::NoButton, Qt::NoButton, Qt::NoModifier);
    }, mainWindow);
  } else if (name == QLatin1String("timeline-leave")) {
    auto* strip = m_main->findChild<QWidget*>(QStringLiteral("timelineStrip"));
    if (!strip) { fail(line, QStringLiteral("timeline strip missing")); return; }
    QEvent leave(QEvent::Leave);
    QCoreApplication::sendEvent(strip, &leave);
  } else if (name == QLatin1String("expect-timeline-hover")) {
    auto* timeline = m_main->findChild<TimelineWidget*>();
    const QString actual = timeline ? timeline->hoveredFeatures().join(QLatin1Char(',')) : QStringLiteral("missing");
    const QString expected = a.at(0) == QStringLiteral("none") ? QString() : a.at(0);
    if (actual != expected) fail(line, QStringLiteral("timeline hover '%1', expected '%2'").arg(actual, expected));
  } else if (name == QLatin1String("expect-timeline-tooltip")) {
    QRegularExpression expected;
    if (!regex(a.at(0), expected)) return;
    whenReady([this, expected] {
      QTextDocument text;
      text.setHtml(QToolTip::text());
      m_waitDetail = text.toPlainText();
      return QToolTip::isVisible() && expected.match(m_waitDetail).hasMatch();
    }, 2, QStringLiteral("timeline tooltip did not show expected feature details"));
  } else if (name == QLatin1String("capture-view-image") || name == QLatin1String("expect-view-image")) {
    auto* view = qobject_cast<OcctViewer*>(viewer());
    if (!view) { fail(line, QStringLiteral("OCCT viewport missing")); return; }
    // Screenshot assertions compare a settled camera, not an intermediate
    // Fit All or animated view frame that may finish while the tooltip opens.
    view->settleCamera();
    if (name == QLatin1String("capture-view-image")) {
      QDir().mkpath(QFileInfo(a.at(0)).absolutePath());
      if (!view->grabFramebuffer().save(a.at(0))) fail(line, QStringLiteral("cannot save viewport image"));
      return;
    }
    const QImage original(a.at(0));
    const bool changed = a.at(1) == QStringLiteral("changed");
    if (original.isNull() || (!changed && a.at(1) != QStringLiteral("same"))) {
      fail(line, QStringLiteral("expect-view-image requires an image and same|changed")); return;
    }
    whenReady([this, view, original, changed] {
      const QImage current = view->grabFramebuffer();
      if (original.size() != current.size()) { m_waitDetail = QStringLiteral("viewport size changed"); return false; }
      int pixels = 0;
      for (int y = 0; y < current.height(); ++y) for (int x = 0; x < current.width(); ++x) {
        const QColor a = original.pixelColor(x, y), b = current.pixelColor(x, y);
        if (std::abs(a.red() - b.red()) + std::abs(a.green() - b.green()) + std::abs(a.blue() - b.blue()) > 30) ++pixels;
      }
      m_waitDetail = QStringLiteral("viewport changed pixels: %1").arg(pixels);
      return changed ? pixels >= 50 : pixels <= 10;
    }, 6, QStringLiteral("hidden feature framebuffer indication"));
  } else if (name == QLatin1String("expect-navigation-runtime")) {
    testNavigationRuntime();
  } else if (name == QLatin1String("expect-assistant-setup")) {
    testAssistantSetup(a.at(0));
  } else if (name == QLatin1String("expect-formula-builder")) {
    testFormulaBuilder();
  } else if (name == QLatin1String("open-formula-field")) {
    openFormulaField(a.at(0));
  } else if (name == QLatin1String("expect-navigation-controls")) {
    testNavigationControls(a.at(0));
  } else if (name == QLatin1String("expect-selection-info")) {
    QRegularExpression expected;
    if (!regex(a.at(0), expected)) return;
    double maxHeight = 0;
    if (a.size() > 1 && (!toNumber(a.at(1), maxHeight) || maxHeight <= 0)) {
      fail(line, QStringLiteral("expect-selection-info <text-regex> [maximum-card-height] [with-create-row]")); return;
    }
    const bool withCreateRow = a.size() > 2 && a.at(2) == QLatin1String("with-create-row");
    if (a.size() > 2 && !withCreateRow) {
      fail(line, QStringLiteral("expect-selection-info: expected with-create-row")); return;
    }
    whenReady([this, expected, maxHeight, withCreateRow] {
      const auto* label = m_main->findChild<QLabel*>(QStringLiteral("selectionInfo"));
      const auto* card = m_main->findChild<GlassCard*>(QStringLiteral("commandDock"));
      if (!label) return false;
      QTextDocument document;
      document.setHtml(label->text());
      const QString text = document.toPlainText();
      // The optional Create row (#619) has its own content-sized height.
      // Keep the original measurements budget, adding only that visible row's
      // size hint and its inter-widget gap, not an arbitrary larger card cap.
      int createHeight = 0;
      const auto* create = m_main->findChild<QWidget*>(QStringLiteral("selectionCreatePanel"));
      if (withCreateRow && create && create->isVisible()) {
        const auto* layout = create->parentWidget()->layout();
        createHeight = create->sizeHint().height() + (layout ? std::max(0, layout->spacing()) : 0);
      }
      m_waitDetail = QStringLiteral("selection info: %1; card height %2; Create row allowance %3")
                         .arg(text).arg(card ? card->height() : -1).arg(createHeight);
      const bool matches = (text.isEmpty() || label->isVisible()) && expected.match(text).hasMatch() &&
                           (maxHeight == 0 || (card && card->height() <= maxHeight + createHeight));
      if (matches) {
        qDebug().noquote() << QStringLiteral("TestDriver: selection info: %1").arg(text);
      }
      return matches;
    }, 6, QStringLiteral("selection measurements"));
  } else if (name == QLatin1String("expect-selection")) {
    double total = 0;
    double profiles = 0;
    if (!toNumber(a.at(0), total) || !toNumber(a.at(1), profiles) || total < 0 || profiles < 0 ||
        total != std::floor(total) || profiles != std::floor(profiles) || profiles > total) {
      fail(line, QStringLiteral("expect-selection <total-count> <profile-count>")); return;
    }
    whenReady([this, total, profiles] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window || window->modelBusy()) return false;
      const Selection& selected = window->selection();
      const auto count = std::count_if(selected.cbegin(), selected.cend(), [](const SelectionItem& item) {
        return item.kind == SelectKind::Profile;
      });
      m_waitDetail = QStringLiteral("actual selection: %1 items, %2 profiles").arg(selected.size()).arg(count);
      return selected.size() == total && count == profiles;
    }, 6, QStringLiteral("expected %1 selected items, including %2 profiles").arg(total).arg(profiles));
  } else if (name == QLatin1String("expect-view-buttons")) {
    const QHash<QString, Aspect_VKeyMouse> buttons = {
      {QStringLiteral("none"), Aspect_VKeyMouse_NONE},
      {QStringLiteral("left"), Aspect_VKeyMouse_LeftButton},
      {QStringLiteral("right"), Aspect_VKeyMouse_RightButton},
      {QStringLiteral("middle"), Aspect_VKeyMouse_MiddleButton}};
    auto* view = qobject_cast<OcctViewer*>(viewer());
    if (!view || !buttons.contains(a.at(0))) {
      fail(line, QStringLiteral("expect-view-buttons none|left|right|middle requires an OCCT view")); return;
    }
    if (view->PressedMouseButtons() != buttons.value(a.at(0))) {
      fail(line, QStringLiteral("OCCT retains mouse buttons %1 instead of %2")
          .arg(int(view->PressedMouseButtons())).arg(a.at(0)));
    }
  } else if (name == QLatin1String("capture-view-orientation") || name == QLatin1String("expect-view-orientation")) {
    const auto orientation = [](const CameraState& camera) {
      const gp_Dir direction(gp_Vec(camera.eye, camera.target));
      return std::array<double, 6>{direction.X(), direction.Y(), direction.Z(),
                                  camera.up.X(), camera.up.Y(), camera.up.Z()};
    };
    auto* view = qobject_cast<OcctViewer*>(viewer());
    if (!view) { fail(line, QStringLiteral("camera orientation requires an initialized OCCT view")); return; }
    if (name == QLatin1String("capture-view-orientation")) {
      QStringList values;
      for (double value : orientation(view->camera())) values.append(QString::number(value, 'g', 17));
      m_vars[a.at(0)] = values.join(QLatin1Char(','));
    } else {
      const QStringList values = a.at(0).split(QLatin1Char(','));
      const bool changed = a.at(1) == QStringLiteral("changed");
      std::array<double, 6> before{};
      if (values.size() != int(before.size()) || (!changed && a.at(1) != QStringLiteral("unchanged"))) {
        fail(line, QStringLiteral("expect-view-orientation <captured-orientation> changed|unchanged")); return;
      }
      for (int index = 0; index < int(before.size()); ++index) {
        if (!toNumber(values.at(index), before[size_t(index)])) {
          fail(line, QStringLiteral("a captured camera orientation must contain six finite numbers")); return;
        }
      }
      whenReady([this, before, changed, orientation] {
        auto* currentView = qobject_cast<OcctViewer*>(viewer());
        if (!currentView) return false;
        const auto current = orientation(currentView->camera());
        double difference = 0;
        for (size_t index = 0; index < before.size(); ++index)
          difference = std::max(difference, std::abs(current[index] - before[index]));
        m_waitDetail = QStringLiteral("actual camera direction/up difference %1").arg(difference, 0, 'g', 12);
        return changed ? difference > 0.002 : difference <= 1e-7;
      }, 6, QStringLiteral("the camera orientation did not remain %1").arg(a.at(1)));
    }
  } else if (name == QLatin1String("native-context-menu") || name == QLatin1String("native-popup") ||
             name == QLatin1String("native-pointer-popup")) {
    if (!mac::test::active()) { fail(line, QStringLiteral("%1 requires the native macOS UI").arg(name)); return; }
    queue([this, name, a, line] {
      QString error;
      m_timer->stop();
      if (name == QLatin1String("native-popup") || name == QLatin1String("native-pointer-popup")) {
        QString clickError;
        const bool tracked = mac::test::trackContextMenu(a.at(1), [&] {
          if (name == QLatin1String("native-pointer-popup")) mac::test::pointerClick(a.at(0), clickError);
          else mac::test::click(a.at(0), clickError);
        }, error, [this] { if (!m_finished) m_timer->start(); },
            name == QLatin1String("native-pointer-popup") && a.size() == 3 ? a.at(2) : QString());
        if (!clickError.isEmpty()) fail(line, clickError);
        else if (!tracked) fail(line, error);
      } else if (!mac::test::contextMenu(a.at(0), a.at(1), error, [this] {
                   if (!m_finished) m_timer->start();
                 })) fail(line, error);
      if (!m_finished) m_timer->start();
    });
  } else if (name == QLatin1String("context-menu-logged")) {
    QRegularExpression re;
    if (!regex(a.at(0), re)) return;
    const auto at = std::make_shared<QPoint>();
    const auto receiver = std::make_shared<QPointer<QWidget>>();
    const auto window = std::make_shared<QPointer<QWidget>>();
    whenReady([this, re, at, receiver, window] {
      if (TestSync::settling() || !m_main || !loggedPoint(re, 0, *at)) return false;
      *window = cardAt(*at);
      if (!*window) *window = m_main;
      const QPoint local = (*window)->mapFromGlobal(m_main->mapToGlobal(*at));
      *receiver = (*window)->childAt(local);
      return *receiver && (*receiver)->isVisible() && (*receiver)->isEnabled() &&
          (*receiver)->visibleRegion().contains((*receiver)->mapFrom(*window, local));
    }, 6, QStringLiteral("no visible widget at the logged context-menu point '%1'").arg(a.at(0)),
    [this, at, receiver, window, choice = a.at(1), line] {
      const auto open = [this, at, receiver, window, line] {
        if (!*receiver || !*window || !m_main) {
          fail(line, QStringLiteral("the context-menu target was destroyed")); return;
        }
        const QPoint global = m_main->mapToGlobal(*at);
        const QPoint local = (*receiver)->mapFromGlobal(global);
        if (!(*receiver)->isVisible() || !(*receiver)->isEnabled() ||
            !(*receiver)->visibleRegion().contains(local) ||
            (*window)->childAt((*window)->mapFromGlobal(global)) != receiver->data()) {
          fail(line, QStringLiteral("the context-menu target moved or became covered")); return;
        }
        QContextMenuEvent event(QContextMenuEvent::Mouse, local, global);
        QCoreApplication::sendEvent(receiver->data(), &event);
      };
      if (mac::test::active()) {
        queue([this, open, choice, line] {
          QString error;
          m_timer->stop();
          if (!mac::test::trackContextMenu(choice, open, error, [this] {
                if (!m_finished) m_timer->start();
              })) fail(line, error);
          if (!m_finished) m_timer->start();
        });
      } else {
        // execMenu owns a nested loop. Its following queued step chooses
        // the real visible QMenu in that loop, as ordinary popup input does.
        queue(open);
        queue([this, choice, line] {
          run(Step{line, {}, choice == QStringLiteral("cancel") ? QStringLiteral("key") : QStringLiteral("popup"),
                   {choice == QStringLiteral("cancel") ? QStringLiteral("Escape") : choice}});
        });
      }
    });
  } else if (name == QLatin1String("view-context-menu") || name == QLatin1String("view-context-menu-logged")) {
    if (!mac::test::active()) { fail(line, QStringLiteral("%1 requires the native macOS UI").arg(name)); return; }
    const QString choice = a.last();
    const auto perform = [this, choice, line](const QPoint& at) {
      queue([this, at, choice, line] {
        QString error;
        const auto open = [this, at] {
          sendMouse(QEvent::MouseMove, at, Qt::NoButton, Qt::NoButton, Qt::NoModifier);
          sendMouse(QEvent::MouseButtonPress, at, Qt::RightButton, Qt::RightButton, Qt::NoModifier);
          sendMouse(QEvent::MouseButtonRelease, at, Qt::RightButton, Qt::NoButton, Qt::NoModifier);
        };
        m_timer->stop();
        if (!mac::test::trackContextMenu(choice, open, error)) fail(line, error);
        if (!m_finished) m_timer->start();
      });
    };
    if (name == QLatin1String("view-context-menu-logged")) {
      QRegularExpression re;
      if (!regex(a.at(0), re)) return;
      const auto at = std::make_shared<QPoint>();
      whenReady([this, re, at] { return loggedPoint(re, 0, *at); }, 6,
          QStringLiteral("no logged context-menu position matches '%1'").arg(a.at(0)),
          [at, perform] { perform(*at); });
    } else {
      double x = 0, y = 0;
      QPoint at;
      if (!toNumber(a.at(0), x) || !toNumber(a.at(1), y) || !viewPoint(x, y, at)) {
        fail(line, QStringLiteral("view-context-menu <x%> <y%> cancel|item requires an OCCT view")); return;
      }
      perform(at);
    }
  } else if (name == QLatin1String("view-pointer")) {
    const QString phase = a.at(0);
    const QHash<QString, Qt::MouseButton> buttons = {
      {QStringLiteral("none"), Qt::NoButton}, {QStringLiteral("left"), Qt::LeftButton},
      {QStringLiteral("right"), Qt::RightButton}, {QStringLiteral("middle"), Qt::MiddleButton}};
    double x = 0, y = 0;
    QPoint at;
    Qt::KeyboardModifiers modifiers;
    if ((phase != QStringLiteral("press") && phase != QStringLiteral("move") && phase != QStringLiteral("release")) ||
        !buttons.contains(a.at(3)) || !toNumber(a.at(1), x) || !toNumber(a.at(2), y) || !viewPoint(x, y, at) ||
        (a.size() == 5 && !parseModifiers(a.at(4), modifiers)) ||
        (phase != QStringLiteral("move") && buttons.value(a.at(3)) == Qt::NoButton)) {
      fail(line, QStringLiteral("view-pointer press|move|release <x%> <y%> none|left|right|middle [mods]")); return;
    }
    const Qt::MouseButton button = buttons.value(a.at(3));
    queue([this, phase, at, button, modifiers] {
      const auto type = phase == QStringLiteral("press") ? QEvent::MouseButtonPress :
                        phase == QStringLiteral("release") ? QEvent::MouseButtonRelease : QEvent::MouseMove;
      sendMouse(type, at, phase == QStringLiteral("move") ? Qt::NoButton : button,
                phase == QStringLiteral("release") ? Qt::NoButton : button, modifiers);
    });
  } else if (name == QLatin1String("expect-viewport")) {
    whenReady([this] {
      auto* view = qobject_cast<OcctViewer*>(viewer());
      if (!view || !view->isValid() || view->glVersion().isEmpty()) return false;
      const QImage image = view->grabFramebuffer();
      const qreal scale = view->devicePixelRatioF();
      const bool retina = qEnvironmentVariableIntValue("MITCAD_UI_REQUIRE_RETINA") != 0;
      m_waitDetail = QStringLiteral("viewport logical %1x%2, framebuffer %3x%4, scale %5")
          .arg(view->width()).arg(view->height()).arg(image.width()).arg(image.height()).arg(scale);
      if (image.isNull() || image.width() < 300 || image.height() < 200 ||
          std::abs(image.width() - view->width() * scale) > 1 ||
          std::abs(image.height() - view->height() * scale) > 1 || (retina && scale < 1.9)) return false;
      const QScreen* screen = view->screen();
      qDebug().noquote() << QStringLiteral("TestDriver: %1; screen %2x%3 points, refresh %4 Hz; OpenGL %5")
          .arg(m_waitDetail).arg(screen ? screen->geometry().width() : 0)
          .arg(screen ? screen->geometry().height() : 0).arg(screen ? screen->refreshRate() : 0)
          .arg(view->glVersion());
      return true;
    }, 10, QStringLiteral("the viewport framebuffer does not match its actual backing scale"));
  } else if (name == QLatin1String("capture-view-target")) {
    auto* view = qobject_cast<OcctViewer*>(viewer());
    if (!view || !view->isValid()) { fail(line, QStringLiteral("capture-view-target requires an initialized viewport")); return; }
    const gp_Pnt target = view->camera().target;
    m_vars[a.at(0)] = QStringLiteral("%1,%2,%3").arg(target.X(), 0, 'g', 17)
        .arg(target.Y(), 0, 'g', 17).arg(target.Z(), 0, 'g', 17);
  } else if (name == QLatin1String("expect-view-pan")) {
    const QStringList coordinates = a.at(0).split(QLatin1Char(','));
    double x = 0, y = 0, z = 0;
    if (coordinates.size() != 3 || !toNumber(coordinates.at(0), x) || !toNumber(coordinates.at(1), y) ||
        !toNumber(coordinates.at(2), z) || (a.at(1) != QLatin1String("changed") && a.at(1) != QLatin1String("unchanged"))) {
      fail(line, QStringLiteral("expect-view-pan <captured-target> changed|unchanged")); return;
    }
    const bool changed = a.at(1) == QLatin1String("changed");
    whenReady([this, x, y, z, changed] {
      auto* view = qobject_cast<OcctViewer*>(viewer());
      if (!view) return false;
      const double distance = view->camera().target.Distance(gp_Pnt(x, y, z));
      m_waitDetail = QStringLiteral("camera target moved %1 mm").arg(distance, 0, 'g', 12);
      return std::isfinite(distance) && (changed ? distance > 1e-6 : distance <= 1e-6);
    }, 6, QStringLiteral("the camera pan did not remain %1").arg(a.at(1)));
  } else if (name == QLatin1String("window-state") || name == QLatin1String("expect-window-state")) {
    const QString state = a.at(0);
    if (state != QLatin1String("normal") && state != QLatin1String("minimized") && state != QLatin1String("fullscreen")) {
      fail(line, QStringLiteral("%1 normal|minimized|fullscreen").arg(name)); return;
    }
    if (name == QLatin1String("window-state")) {
      queue([this, state, line] {
        if (mac::test::active()) {
          QString error;
          if (!mac::test::setDocumentState(state, error)) fail(line, error);
        } else if (m_main) {
          if (state == QLatin1String("minimized")) m_main->showMinimized();
          else if (state == QLatin1String("fullscreen")) m_main->showFullScreen();
          else m_main->showNormal();
        } else fail(line, QStringLiteral("window-state requires a main window"));
      });
    } else {
      whenReady([this, state] {
        if (mac::test::active()) return mac::test::documentState() == state;
        return m_main && (m_main->isMinimized() ? QStringLiteral("minimized") :
                         m_main->isFullScreen() ? QStringLiteral("fullscreen") : QStringLiteral("normal")) == state;
      }, 10, QStringLiteral("the document did not become %1").arg(state));
    }
  } else if (name == QLatin1String("window-move")) {
    double x = 0, y = 0;
    if (!toNumber(a.at(0), x) || !toNumber(a.at(1), y) || std::abs(x) > 10000 || std::abs(y) > 10000) {
      fail(line, QStringLiteral("window-move expects screen-relative offsets -10000..10000")); return;
    }
    queue([this, x, y, line] {
      if (mac::test::active()) {
        QString error;
        if (!mac::test::moveDocument(qRound(x), qRound(y), error)) fail(line, error);
      } else if (m_main && m_main->screen()) {
        m_main->move(m_main->screen()->availableGeometry().topLeft() + QPoint(qRound(x), qRound(y)));
      } else fail(line, QStringLiteral("window-move requires a main window and screen"));
    });
  } else if (name == QLatin1String("capture-view-height")) {
    auto* view = qobject_cast<OcctViewer*>(viewer());
    if (!view || !std::isfinite(view->camera().height) || view->camera().height <= 0) {
      fail(line, QStringLiteral("capture-view-height requires an initialized OCCT camera"));
      return;
    }
    m_vars[a.at(0)] = QString::number(view->camera().height, 'g', 17);
  } else if (name == QLatin1String("expect-view-zoom")) {
    double before = 0;
    double referenceBefore = 0;
    double referenceAfter = 0;
    const QString direction = a.at(1);
    if (!toNumber(a.at(0), before) || before <= 0 ||
        (direction != QStringLiteral("in") && direction != QStringLiteral("out") && direction != QStringLiteral("unchanged")) ||
        (a.size() != 2 && (a.size() != 4 || !toNumber(a.at(2), referenceBefore) ||
                          !toNumber(a.at(3), referenceAfter) || referenceBefore <= 0 || referenceAfter <= 0))) {
      fail(line, QStringLiteral("expect-view-zoom <previous-height> in|out|unchanged [control-before control-after]"));
      return;
    }
    const double referenceRatio = referenceBefore > 0 ? referenceAfter / referenceBefore : 0;
    whenReady([this, before, direction, referenceRatio] {
      auto* view = qobject_cast<OcctViewer*>(viewer());
      if (!view) return false;
      const double ratio = view->camera().height / before;
      m_waitDetail = QStringLiteral("camera height ratio %1").arg(ratio, 0, 'g', 12);
      return std::isfinite(ratio) && (direction == QStringLiteral("in") ? ratio < 0.97 :
          direction == QStringLiteral("out") ? ratio > 1.03 : std::abs(ratio - 1) <= 0.001) &&
          (referenceRatio == 0 || std::abs(ratio - referenceRatio) <= 0.005 * referenceRatio);
    }, 6, QStringLiteral("the view did not zoom %1 from camera height %2").arg(direction).arg(before, 0, 'g', 12));
  } else if (name == QLatin1String("native-pinch") || name == QLatin1String("qt-pinch")) {
    double x = 0;
    double y = 0;
    double magnification = 0;
    if (!toNumber(a.at(0), x) || !toNumber(a.at(1), y) || !toNumber(a.at(2), magnification) ||
        x < 0 || x > 100 || y < 0 || y > 100 || std::abs(magnification) > 1) {
      fail(line, QStringLiteral("%1 <x%> <y%> <magnification -1..1>").arg(name));
      return;
    }
    if (name == QLatin1String("native-pinch") && !mac::test::active()) {
      fail(line, QStringLiteral("native-pinch requires the native macOS UI"));
      return;
    }
    auto* view = qobject_cast<OcctViewer*>(viewer());
    if (!view) { fail(line, QStringLiteral("pinch requires an OCCT view")); return; }
    const QPointer<OcctViewer> guard(view);
    const QPointF local(view->width() * x / 100, view->height() * y / 100);
    queue([this, guard, local, magnification, name, line] {
      if (!guard) { fail(line, QStringLiteral("the pinch viewport was destroyed")); return; }
      if (name == QLatin1String("native-pinch")) {
        QString error;
        if (!mac::test::pinch(guard, local, magnification, error)) fail(line, error);
      } else {
        const QPointF scene = guard->mapTo(guard->window(), local.toPoint());
        const QPointF global = guard->mapToGlobal(local.toPoint());
        for (Qt::NativeGestureType type : {Qt::BeginNativeGesture, Qt::ZoomNativeGesture, Qt::EndNativeGesture}) {
          QNativeGestureEvent event(type, QPointingDevice::primaryPointingDevice(), 2, local, scene, global,
                                    type == Qt::ZoomNativeGesture ? magnification : 0, QPointF());
          QCoreApplication::sendEvent(guard, &event);
        }
      }
    });
  } else if (name == QLatin1String("native-resize") || name == QLatin1String("window-resize")) {
    if (name == QLatin1String("native-resize") && !mac::test::active()) {
      fail(line, QStringLiteral("native-resize requires the native macOS UI")); return;
    }
    double width = 0;
    double height = 0;
    if (!toNumber(a.at(0), width) || !toNumber(a.at(1), height) || width < 820 || height < 520 ||
        width > 10000 || height > 10000) {
      fail(line, QStringLiteral("%1 expects width 820..10000 and height 520..10000").arg(name));
      return;
    }
    queue([this, width, height, line] {
      if (mac::test::active()) {
        QString error;
        if (!mac::test::resizeDocument(int(width), int(height), error)) fail(line, error);
      } else if (m_main) {
        m_main->resize(int(width), int(height));
      } else {
        fail(line, QStringLiteral("window-resize requires a main window"));
      }
    });
    m_notBefore = std::max(m_notBefore, m_clock.elapsed() + 300);
  } else if (name == QLatin1String("native-move")) {
    if (!mac::test::active()) { fail(line, QStringLiteral("native-move requires the native macOS UI")); return; }
    double x = 0;
    double y = 0;
    if (!toNumber(a.at(0), x) || !toNumber(a.at(1), y) || std::abs(x) > 10000 || std::abs(y) > 10000) {
      fail(line, QStringLiteral("native-move expects offsets -10000..10000 from the main screen's visible origin"));
      return;
    }
    queue([this, x, y, line] {
      QString error;
      if (!mac::test::moveDocument(qRound(x), qRound(y), error)) fail(line, error);
    });
  } else if (name == QLatin1String("expect-native-layout")) {
    if (!mac::test::active()) { fail(line, QStringLiteral("expect-native-layout requires the native macOS UI")); return; }
    QString error;
    if (!mac::test::floatingLayout(error)) {
      whenReady([] { QString detail; return mac::test::floatingLayout(detail); }, 6, error);
    }
  } else if (name == QLatin1String("expect-native-dialog-gone")) {
    if (!mac::test::active()) { fail(line, QStringLiteral("expect-native-dialog-gone requires the native macOS UI")); return; }
    QRegularExpression re;
    double limit = 6;
    if (!regex(a.at(0), re) || !seconds(1, 6, limit)) return;
    whenReady([re] { return !mac::test::dialogExists(re); }, limit,
              QStringLiteral("native window matching '%1' remains open").arg(a.at(0)));
  } else if (name.startsWith(QLatin1String("native-file-"))) {
    if (!mac::test::active()) { fail(line, QStringLiteral("'%1' requires the native macOS UI").arg(name)); return; }
    if (name == QLatin1String("native-file-state")) {
      qInfo().noquote() << QStringLiteral("Native file panel state %1: %2")
          .arg(a.value(0), mac::test::filePanelDiagnostic());
      return;
    }
    queue([this, name, a, line] {
      QString error;
      const bool ok = name == QLatin1String("native-file-name") ? mac::test::replaceFileName(a.at(0), error) :
                      name == QLatin1String("native-file-directory") ? mac::test::setFileDirectory(a.at(0), error) : mac::test::cancelFilePanel(error);
      if (!ok) fail(line, error);
    });
  } else if (name == QLatin1String("expect-native-disclosure-layout")) {
    if (!mac::test::active()) { fail(line, QStringLiteral("disclosure layout requires the native UI")); return; }
    whenReady([this] {
      QString error;
      const bool ready = mac::test::disclosureLayout(error);
      m_waitDetail = error;
      return ready;
    }, 6, QStringLiteral("native disclosure layout did not settle"));
  } else if (name == QLatin1String("expect-native-color-field")) {
    if (!mac::test::active() || (a.at(3) != QLatin1String("placeholder") && a.at(3) != QLatin1String("explicit"))) {
      fail(line, QStringLiteral("expect-native-color-field <id> <color|none> <words|-> placeholder|explicit [AX label] requires native UI"));
      return;
    }
    whenReady([this, a] {
      QString error;
      const bool ready = mac::test::colorField(a.at(0), a.at(1), a.at(2) == QLatin1String("-") ? QString() : a.at(2),
                                              a.at(3) == QLatin1String("placeholder"), a.value(4), error);
      m_waitDetail = error;
      return ready;
    }, 6, QStringLiteral("native colour field did not show its swatch and placeholder"));
  } else if (name == QLatin1String("expect-native-selection-card") ||
             name == QLatin1String("expect-native-command-layout")) {
    // Actual AppKit bounds and hit tests of the cards' actions (mitcad#815).
    if (!mac::test::active()) { fail(line, QStringLiteral("'%1' requires the native macOS UI").arg(name)); return; }
    bool counted = true;
    const int expected = name == QLatin1String("expect-native-selection-card") ? a.at(0).toInt(&counted) : 0;
    if (!counted) { fail(line, QStringLiteral("expect-native-selection-card <number of Create actions>")); return; }
    whenReady([this, name, a, expected] {
      QString error;
      const bool ready = name == QLatin1String("expect-native-selection-card")
                             ? mac::test::selectionCard(expected, error)
                             : mac::test::commandLayout(a, error);
      m_waitDetail = error;
      return ready;
    }, 6, QStringLiteral("'%1' did not hold").arg(name));
  } else if (name == QLatin1String("expect-native-form-controls")) {
    auto* window = qobject_cast<MainWindow*>(m_main.data());
    QString error;
    if (!window || !mac::test::active() || !mac::test::formControls(*window, error)) {
      fail(line, error.isEmpty() ? QStringLiteral("form controls require an idle native window") : error);
    }
  } else if (name == QLatin1String("expect-native-application-delegate")) {
    QString error;
    if (!mac::test::applicationDelegate(error)) {
      fail(line, error.isEmpty() ? QStringLiteral("application delegate check requires the native UI") : error);
    }
  } else if (name == QLatin1String("expect-native-widget-refresh")) {
    QString error;
    // The contract pumps presenter events. Keep later script steps from
    // entering while its synthetic controls are mounted.
    m_timer->stop();
    const bool passed = mac::test::active() && mac::test::widgetRefreshContract(error);
    if (!m_finished) m_timer->start();
    if (!passed) {
      fail(line, error.isEmpty() ? QStringLiteral("widget refresh requires the native UI") : error);
    }
  } else if (name == QLatin1String("native-header-resize") ||
             name == QLatin1String("expect-native-header-resize-retained")) {
    if (!mac::test::active()) { fail(line, QStringLiteral("native header steps require the native UI")); return; }
    bool validColumn = false;
    const int column = a.at(1).toInt(&validColumn);
    double delta = 0;
    const bool drag = name == QLatin1String("native-header-resize");
    if (!validColumn || column < 0 || (drag && (!toNumber(a.at(2), delta) || !std::isfinite(delta) ||
                                              std::abs(delta) < 4 || std::abs(delta) > 300))) {
      fail(line, QStringLiteral("native header steps need a logical column and resize delta of 4..300 points")); return;
    }
    const QString id = a.at(0);
    if (drag) {
      whenReady([this, id] { return mac::test::controlExists(id, &m_waitDetail); }, 6,
                QStringLiteral("no native table '%1'").arg(id), [this, line, id, column, delta] {
        queue([this, line, id, column, delta] {
          m_timer->stop();
          QString detail;
          const bool passed = mac::test::resizeHeader(id, column, delta, detail);
          if (!m_finished) m_timer->start();
          if (!passed) fail(line, detail);
        });
      });
    } else {
      whenReady([this, id, column] { return mac::test::headerResizeRetained(id, column, m_waitDetail); }, 6,
                QStringLiteral("native header '%1' column %2 lost its dragged width").arg(id).arg(column));
    }
  } else if (name == QLatin1String("expect-native-duplicate-choices")) {
    auto* window = qobject_cast<MainWindow*>(m_main.data());
    QString error;
    if (!window || !mac::test::active() || !mac::test::duplicateChoices(*window, error)) {
      fail(line, error.isEmpty() ? QStringLiteral("duplicate choices require an idle native window") : error);
    }
  } else if (name == QLatin1String("native-toolbar-item") || name == QLatin1String("expect-native-toolbar-item")) {
    if (!mac::test::active()) { fail(line, QStringLiteral("native toolbar steps require the native macOS UI")); return; }
    if (name == QLatin1String("native-toolbar-item")) {
      QString error;
      if (!mac::test::configureToolbarItem(a.at(0), a.at(1), error)) fail(line, error);
    } else {
      if (a.at(1) != QLatin1String("true") && a.at(1) != QLatin1String("false")) {
        fail(line, QStringLiteral("expect-native-toolbar-item expects true or false")); return;
      }
      const bool expected = a.at(1) == QLatin1String("true");
      whenReady([a, expected] { return mac::test::toolbarItemPresent(a.at(0), expected); }, 6,
                QStringLiteral("native toolbar item '%1' has unexpected membership").arg(a.at(0)));
    }
  } else if (name == QLatin1String("native-tiles") || name == QLatin1String("expect-native-tiles")) {
    // native-tiles <action> <id> [tile|value] [more]; expect-native-tiles
    // <check> <id> [value] [more] (NativeTestInput.hpp, mitcad#813, #819).
    if (!mac::test::active()) {
      fail(line, QStringLiteral("'%1' requires the native macOS UI").arg(name));
      return;
    }
    if (name == QLatin1String("native-tiles")) {
      queue([this, a, line] {
        QString error;
        if (!mac::test::tileAction(a, error)) fail(line, error);
      });
    } else {
      whenReady([this, a] {
        QString detail;
        const bool ready = mac::test::tileCheck(a, detail);
        m_waitDetail = detail;
        return ready;
      }, 6, QStringLiteral("native tile check '%1' did not pass").arg(a.join(QLatin1Char(' '))));
    }
  } else if (name == QLatin1String("expect-native-text") || name == QLatin1String("expect-native-enabled") ||
             name == QLatin1String("expect-native-visible") || name == QLatin1String("expect-native-present")) {
    if (!mac::test::active()) {
      fail(line, QStringLiteral("'%1' requires the native macOS UI").arg(name));
      return;
    }
    if (name == QLatin1String("expect-native-text")) {
      QRegularExpression re;
      if (!regex(a.at(1), re)) return;
      const QString id = a.at(0);
      whenReady([id, re] { return mac::test::textMatches(id, re); }, 6,
                QStringLiteral("native text '%1' did not match '%2'").arg(id, a.at(1)));
    } else {
      const QString value = a.at(1).toLower();
      if (value != QStringLiteral("true") && value != QStringLiteral("false")) {
        fail(line, QStringLiteral("%1 <identifier> true|false").arg(name)); return;
      }
      const QString id = a.at(0);
      const bool expected = value == QStringLiteral("true");
      const bool visibility = name == QLatin1String("expect-native-visible");
      const bool presence = name == QLatin1String("expect-native-present");
      whenReady([id, expected, visibility, presence] {
        if (presence) return mac::test::present(id, expected);
        return visibility ? mac::test::visible(id, expected) : mac::test::enabled(id, expected);
      }, 6, QStringLiteral("native control '%1' was not %2=%3")
                .arg(id, presence ? QStringLiteral("present") : visibility ? QStringLiteral("visible") : QStringLiteral("enabled"), value));
    }
  } else if (name == QLatin1String("expect-native-style") || name == QLatin1String("expect-native-columns") ||
             name == QLatin1String("native-dialog-size")) {
    // Native table semantics and sizing (mitcad#814).
    if (!mac::test::active()) {
      fail(line, QStringLiteral("'%1' requires the native macOS UI").arg(name));
      return;
    }
    if (name == QLatin1String("native-dialog-size")) {
      double width = 0;
      double height = 0;
      if (!toNumber(a.at(0), width) || !toNumber(a.at(1), height) || width < 360 || height < 160 ||
          width > 3000 || height > 2000) {
        fail(line, QStringLiteral("native-dialog-size expects width 360..3000 and height 160..2000"));
        return;
      }
      queue([this, width, height, line] {
        QString error;
        if (!mac::test::resizeNativeDialog(qRound(width), qRound(height), error)) fail(line, error);
      });
      m_notBefore = std::max(m_notBefore, m_clock.elapsed() + 300);
      return;
    }
    const QString id = a.at(0);
    if (name == QLatin1String("expect-native-style")) {
      const QStringList states = a.at(1).split(QLatin1Char(','), Qt::SkipEmptyParts);
      whenReady([this, id, states] {
        QString detail;
        const bool ready = mac::test::nativeStyle(id, states, detail);
        m_waitDetail = detail;
        return ready;
      }, 6, QStringLiteral("native '%1' was not %2").arg(id, a.at(1)));
    } else {
      const QString mode = a.size() > 1 ? a.at(1) : QStringLiteral("any");
      if (mode != QLatin1String("fit") && mode != QLatin1String("scroll") && mode != QLatin1String("any")) {
        fail(line, QStringLiteral("expect-native-columns <id> [fit|scroll|any]"));
        return;
      }
      whenReady([this, id, mode] {
        QString detail;
        const bool ready = mac::test::nativeColumns(id, mode, detail);
        m_waitDetail = detail;
        return ready;
      }, 6, QStringLiteral("native table '%1' columns were not %2").arg(id, mode));
    }
  } else if (name == QLatin1String("native-drag") || name == QLatin1String("expect-native-focus") ||
             name == QLatin1String("expect-native-selection")) {
    // Native outline rows: drags, keyboard focus and selection (mitcad#816).
    if (!mac::test::active()) {
      fail(line, QStringLiteral("'%1' requires the native macOS UI").arg(name));
      return;
    }
    if (name == QLatin1String("native-drag")) {
      const QString from = a.at(0);
      const QString to = a.at(1);
      whenReady([from, to] { return mac::test::rowsPresent({from, to}); }, 6,
                QStringLiteral("no native rows '%1' and '%2'").arg(from, to), [this, line, from, to] {
        queue([this, line, from, to] {
          QString error;
          if (!mac::test::dragRow(from, to, error)) fail(line, error);
        });
      });
    } else if (name == QLatin1String("expect-native-focus")) {
      QRegularExpression editor;
      if (a.size() == 2 && !regex(a.at(1), editor)) return;
      const QString id = a.at(0);
      const bool editing = a.size() == 2;
      whenReady([this, id, editor, editing] { return mac::test::focusedIn(id, editing ? &editor : nullptr, m_waitDetail); },
                6, QStringLiteral("the keyboard is not in native '%1'%2")
                       .arg(id, editing ? QStringLiteral(" in a text editor matching '%1'").arg(a.at(1)) : QString()));
    } else {
      const QString outline = a.at(0);
      const QString row = a.at(1);
      whenReady([this, outline, row] { return mac::test::selectedRow(outline, row, m_waitDetail); }, 6,
                QStringLiteral("native '%1' does not select '%2' as its presenter does").arg(outline, row));
    }
  } else if (name == QLatin1String("expect-native-bounds") || name == QLatin1String("expect-native-button-order") ||
             name == QLatin1String("expect-native-form-layout") || name == QLatin1String("expect-native-settings-layout") ||
             name == QLatin1String("expect-native-configuration-layout") ||
             name == QLatin1String("expect-native-spanned-heading") || name == QLatin1String("expect-native-first-row") ||
             name == QLatin1String("expect-native-control-focus") || name == QLatin1String("expect-native-layout-contract") ||
             name == QLatin1String("native-pointer-click") || name == QLatin1String("native-pointer-dblclick") ||
             name == QLatin1String("native-pointer-button") ||
             name == QLatin1String("native-pointer-drag") || name == QLatin1String("native-dialog-resize")) {
    // Shared layout bounds, real pointer input and button order (mitcad#812, #817).
    if (!mac::test::active()) {
      fail(line, QStringLiteral("'%1' requires the native macOS UI").arg(name));
      return;
    }
    if (name == QLatin1String("expect-native-bounds")) {
      double minimum = 0;
      // "placeholder": the minimum is the AppKit cell's size for its whole
      // placeholder, read from the actual field when the check runs (mitcad#953);
      // "text" the same for its whole current text (mitcad#975).
      const bool placeholder = a.at(1) == QLatin1String("placeholder");
      const bool wholeText = a.at(1) == QLatin1String("text");
      if (!placeholder && !wholeText && a.at(1) == QLatin1String("shared")) {
        // Read the same populated presenter that owns the native control.
        // Native modeless windows keep their Qt windows hidden, so lookup
        // cannot depend on QApplication's active Qt window or hasFocus().
        QWidget* field = nullptr;
        for (QWidget* candidate : QApplication::allWidgets()) {
          if (candidate->objectName() != a.at(0) || candidate->isHidden()) continue;
          if (field != nullptr) {
            fail(line, QStringLiteral("shared bounds needs one presenter '%1'").arg(a.at(0))); return;
          }
          field = candidate;
        }
        if (field == nullptr) {
          fail(line, QStringLiteral("no shared bounds presenter '%1'").arg(a.at(0))); return;
        }
        const auto formOf = [field](QLayout* layout, const auto& self) -> QFormLayout* {
          if (layout == nullptr) return nullptr;
          if (auto* form = qobject_cast<QFormLayout*>(layout)) {
            int row = -1;
            QFormLayout::ItemRole role = QFormLayout::SpanningRole;
            form->getWidgetPosition(field, &row, &role);
            if (row >= 0 && role == QFormLayout::FieldRole) return form;
          }
          for (int i = 0; i < layout->count(); ++i) {
            if (QFormLayout* form = self(layout->itemAt(i)->layout(), self)) return form;
          }
          return nullptr;
        };
        QFormLayout* form = field->parentWidget() ? formOf(field->parentWidget()->layout(), formOf) : nullptr;
        const QWidgetItem item(field);
        const int least = item.minimumSize().width();
        const int hint = item.sizeHint().width();
        const auto policy = field->sizePolicy().horizontalPolicy();
        const bool cappedByForm = form != nullptr &&
            (form->fieldGrowthPolicy() == QFormLayout::FieldsStayAtSizeHint ||
             (form->fieldGrowthPolicy() == QFormLayout::ExpandingFieldsGrow &&
              (int(policy) & int(QSizePolicy::ExpandFlag)) == 0));
        minimum = cappedByForm ? std::max(least, hint) : least;
        if (minimum <= 0) {
          fail(line, QStringLiteral("presenter '%1' has no positive shared width contract").arg(a.at(0))); return;
        }
        qInfo().noquote() << QStringLiteral("Native shared bounds: %1 minimum=%2 hint=%3 explicit=%4 policy=%5 formGrowth=%6 required=%7")
            .arg(a.at(0)).arg(least).arg(hint).arg(field->minimumWidth()).arg(int(policy))
            .arg(form ? int(form->fieldGrowthPolicy()) : -1).arg(minimum);
      } else if (!placeholder && !wholeText && (!toNumber(a.at(1), minimum) || minimum < 0)) {
        fail(line, QStringLiteral("expect-native-bounds <identifier> <minimum width|shared|placeholder|text> [label regex]"));
        return;
      }
      const QString id = a.at(0);
      const QString label = a.size() > 2 ? a.at(2) : QString();
      const auto check = [id, minimum, label, placeholder, wholeText](QString& error) {
        return placeholder ? mac::test::placeholderBounds(id, label, error)
               : wholeText ? mac::test::textBounds(id, label, error)
                           : mac::test::controlBounds(id, minimum, label, error);
      };
      QString detail;
      if (!check(detail)) {
        whenReady([check] {
          QString ignored;
          return check(ignored);
        }, 6, detail);
      }
    } else if (name == QLatin1String("expect-native-configuration-layout")) {
      whenReady([this] { return mac::test::configurationLayout(m_waitDetail); }, 6,
                QStringLiteral("native configuration controls or captions are clipped or unreachable"));
    } else if (name == QLatin1String("expect-native-form-layout")) {
      whenReady([this, entries = a] { return mac::test::formLayout(entries, m_waitDetail); }, 6,
                QStringLiteral("native form controls are clipped, overlap or have unreadable labels"));
    } else if (name == QLatin1String("expect-native-spanned-heading")) {
      QRegularExpression caption;
      if (!regex(a.at(1), caption)) return;
      whenReady([this, id = a.at(0), caption] { return mac::test::spannedHeading(id, caption, m_waitDetail); }, 6,
                QStringLiteral("native '%1' has no whole, visible spanning heading matching '%2'").arg(a.at(0), a.at(1)));
    } else if (name == QLatin1String("expect-native-first-row")) {
      // The table's own viewport shows this row first (mitcad#972).
      QRegularExpression text;
      if (!regex(a.at(1), text)) return;
      whenReady([this, id = a.at(0), text] { return mac::test::firstVisibleRow(id, text, m_waitDetail); }, 6,
                QStringLiteral("native '%1' does not show a row matching '%2' first").arg(a.at(0), a.at(1)));
    } else if (name == QLatin1String("expect-native-button-order")) {
      const QStringList patterns = a;
      QString detail;
      if (!mac::test::buttonOrder(patterns, detail)) {
        whenReady([patterns] {
          QString ignored;
          return mac::test::buttonOrder(patterns, ignored);
        }, 6, detail);
      }
    } else if (name == QLatin1String("expect-native-control-focus")) {
      const QString id = a.at(0);
      whenReady([id] { return mac::test::focused(id); }, 6,
                QStringLiteral("native control '%1' does not have keyboard focus").arg(id));
    } else if (name == QLatin1String("expect-native-settings-layout")) {
      const QString page = a.at(0);
      whenReady([this, page] { return mac::test::settingsLayout(page, m_waitDetail); }, 6,
                QStringLiteral("Settings layout does not satisfy the %1 page contract").arg(page));
    } else if (name == QLatin1String("expect-native-layout-contract")) {
      // Its synthetic dialogs pump events while they are mounted. Later
      // steps, including finish(), must wait until every fixture is gone.
      m_timer->stop();
      QString detail;
      const bool passed = mac::test::layoutContract(detail);
      if (!m_finished) m_timer->start();
      if (!passed) fail(line, detail);
    } else if (name == QLatin1String("native-dialog-resize")) {
      double width = 0;
      double height = 0;
      if (!toNumber(a.at(0), width) || !toNumber(a.at(1), height) || width < 200 || height < 120 ||
          width > 10000 || height > 10000) {
        fail(line, QStringLiteral("native-dialog-resize expects width 200..10000 and height 120..10000"));
        return;
      }
      queue([this, width, height, line] {
        QString detail;
        if (!mac::test::resizeDialog(int(width), int(height), detail)) fail(line, detail);
      });
      m_notBefore = std::max(m_notBefore, m_clock.elapsed() + 300);
    } else {
      double from = 0;
      double to = 0;
      const bool fromKnob = name == QLatin1String("native-pointer-drag") && a.at(1) == QLatin1String("knob");
      double x = 0.5;
      double y = 0.5;
      if (name == QLatin1String("native-pointer-click") &&
          (a.size() == 2 || (a.size() == 3 &&
              (!toNumber(a.at(1), x) || !toNumber(a.at(2), y) || !(x >= 0 && x <= 1 && y >= 0 && y <= 1))))) {
        fail(line, QStringLiteral("native-pointer-click <identifier> [<x fraction 0..1> <y fraction 0..1>]"));
        return;
      }
      if (name == QLatin1String("native-pointer-drag") &&
          ((!fromKnob && !toNumber(a.at(1), from)) || !toNumber(a.at(2), to))) {
        fail(line, QStringLiteral("native-pointer-drag <identifier> <from fraction|knob> <to fraction>"));
        return;
      }
      const QString target = a.at(0);
      const bool byTitle = name == QLatin1String("native-pointer-button");
      QRegularExpression re;
      if (byTitle && !regex(target, re)) return;
      // Without fractions a click aims at the control's default point.
      const bool atFractions = name == QLatin1String("native-pointer-click") && a.size() == 3;
      const auto perform = [this, line, name, target, re, from, to, fromKnob, x, y, atFractions] {
        // Keep later steps outside a held drag. Button actions can open
        // modal dialogs that the following script steps must still drive.
        const bool heldDrag = name == QLatin1String("native-pointer-drag");
        if (heldDrag) m_timer->stop();
        QString detail;
        const bool ok = name == QLatin1String("native-pointer-button") ? mac::test::pointerButton(re, detail)
                        : name == QLatin1String("native-pointer-drag") ? mac::test::pointerDrag(target, from, to, detail, fromKnob)
                        : name == QLatin1String("native-pointer-dblclick") ? mac::test::pointerDoubleClick(target, detail)
                        : atFractions ? mac::test::pointerClick(target, detail, x, y)
                                      : mac::test::pointerClick(target, detail);
        if (!ok) fail(line, detail.isEmpty() ? QStringLiteral("native pointer input failed for '%1'").arg(target) : detail);
        if (heldDrag && !m_finished) m_timer->start();
      };
      if (byTitle) {
        whenReady([re] { return mac::test::buttonExists(re); }, 6,
                  QStringLiteral("no native button matching '%1'").arg(target), [this, perform] { queue(perform); });
      } else {
        whenReady([this, target] { return mac::test::controlExists(target, &m_waitDetail); }, 6,
                  QStringLiteral("no native control '%1'").arg(target), [this, perform] { queue(perform); });
      }
    }
  } else if (name == QLatin1String("native-system-pointer")) {
    if (!mac::test::active()) {
      fail(line, QStringLiteral("native-system-pointer requires the native macOS UI")); return;
    }
    const QString identifier = a.at(0);
    whenReady([this, identifier] { return mac::test::controlExists(identifier, &m_waitDetail); }, 6,
        QStringLiteral("no visible native system pointer target '%1'").arg(identifier), [this, identifier, line] {
      queue([this, identifier, line] {
        QString error;
        if (!mac::test::sendSystemPointer(identifier, error)) fail(line, error);
      });
    });
  } else if (name == QLatin1String("native-pointer-choice")) {
    if (!mac::test::active()) {
      fail(line, QStringLiteral("native-pointer-choice requires the native macOS UI")); return;
    }
    const QString identifier = a.at(0);
    const QString expected = a.at(1);
    const QStringList keys = a.mid(2);
    mac::test::requestKeyEventFocus();
    whenReady([this, identifier] {
      if (!mac::test::keyEventFocusReady()) { m_waitDetail = mac::test::keyEventFocusDiagnostic(); return false; }
      return mac::test::controlExists(identifier, &m_waitDetail);
    }, 6, QStringLiteral("native pointer choice has no active visible target '%1'").arg(identifier),
        [this, line, identifier, expected, keys] {
      queue([this, line, identifier, expected, keys] {
        // The helper supplies real keys in the menu's tracking loop. Do not
        // let the next script step run inside that loop before it closes.
        m_timer->stop();
        QString error;
        if (!mac::test::pointerChoice(identifier, expected, keys, error)) { fail(line, error); return; }
        if (!m_finished) m_timer->start();
      });
    });
  } else if (name.startsWith(QLatin1String("native-")) && name != QLatin1String("native-key-now") &&
             name != QLatin1String("native-key-event") && name != QLatin1String("native-system-key") &&
             name != QLatin1String("native-system-type")) {
    if (!mac::test::active()) {
      fail(line, QStringLiteral("'%1' requires the native macOS UI").arg(name));
      return;
    }
    const QString identifier = a.at(0);
    if (name == QLatin1String("native-hover") && a.at(1) != QStringLiteral("enter") && a.at(1) != QStringLiteral("leave")) {
      fail(line, QStringLiteral("native-hover <id> enter|leave")); return;
    }
    Qt::KeyboardModifiers modifiers;
    if (name == QLatin1String("native-select") && a.size() == 2) {
      if (a.at(1) == QStringLiteral("shift")) modifiers = Qt::ShiftModifier;
      else if (a.at(1) == QStringLiteral("ctrl")) modifiers = Qt::ControlModifier;
      else { fail(line, QStringLiteral("native-select <id> [shift|ctrl]")); return; }
    }
    const auto perform = [this, line, name, identifier, a, modifiers] {
      QString error;
      const bool ok = (name == QLatin1String("native-click") || name == QLatin1String("native-click-now")) ? mac::test::click(identifier, error) :
                      name == QLatin1String("native-hover") ? mac::test::hover(identifier, a.at(1) == QStringLiteral("enter"), error) :
                      (name == QLatin1String("native-type") || name == QLatin1String("native-type-now")) ? mac::test::replaceText(identifier, a.at(1), error) :
                      name == QLatin1String("native-choice") ? mac::test::choose(identifier, a.at(1), error) :
                      name == QLatin1String("native-activate") ? mac::test::activate(identifier, error) :
                      name == QLatin1String("native-select") ? mac::test::select(identifier, error, modifiers) :
                      name == QLatin1String("native-expand") ? mac::test::expand(identifier, error) :
                      mac::test::triggerAction(identifier, error);
      if (!ok) fail(line, error.isEmpty() ? QStringLiteral("native input failed for '%1'").arg(identifier) : error);
    };
    if (name == QLatin1String("native-action")) queue(perform);
    else whenReady([identifier] { return mac::test::controlExists(identifier); }, 6,
                   QStringLiteral("no native control '%1'").arg(identifier), [this, perform] { queue(perform); });
  } else if (name == QLatin1String("qt-choice")) {
    QWidget* window = targetWindow();
    QWidget* field = window ? window->findChild<QWidget*>(a.at(0)) : nullptr;
    QComboBox* found = qobject_cast<QComboBox*>(field);
    if (!found && field) { found = field->findChild<QComboBox*>(); }
    const QPointer<QComboBox> combo(found);
    const int row = combo ? combo->findText(a.at(1), Qt::MatchExactly | Qt::MatchCaseSensitive) : -1;
    const QPersistentModelIndex index = row >= 0 ? combo->model()->index(row, combo->modelColumn(), combo->rootModelIndex())
                                               : QModelIndex();
    if (mac::test::active() || !combo || !combo->isVisible() || !combo->isEnabled() || !index.isValid() ||
        !index.flags().testFlag(Qt::ItemIsEnabled) || !index.flags().testFlag(Qt::ItemIsSelectable)) {
      fail(line, QStringLiteral("qt-choice needs an enabled Qt combo and an enabled item '%1' in '%2'")
                     .arg(a.at(1), a.at(0))); return;
    }
    const QPointer<QAbstractItemView> list(combo->view());
    const QString identifier = a.at(0);
    const QString label = a.at(1);
    const auto nativeSelected = std::make_shared<bool>(false);
    const auto rowInputSent = std::make_shared<bool>(false);
    const QPointer<QWidget> popupWindow(list->window());
    const auto waitClosed = [this, combo, list, popupWindow, identifier, label, nativeSelected, rowInputSent] {
      // The selected row can synchronously resize its command card. Do not
      // advance to another field while the real popup still owns input.
      // A chosen action may open a modal dialog; it must remain available
      // to subsequent script steps once this original popup has closed.
      whenReady([this, combo, list, popupWindow, nativeSelected, rowInputSent] {
        m_waitDetail = QStringLiteral("item view visible=%1, popup visible=%2 active=%3, current='%4', row release sent=%5, native selection=%6")
            .arg(list && list->isVisible()).arg(popupWindow && popupWindow->isVisible())
            .arg(popupWindow && QApplication::activePopupWidget() == popupWindow)
            .arg(combo ? combo->currentText() : QStringLiteral("<deleted>"))
            .arg(*rowInputSent).arg(*nativeSelected);
        // Actions can replace their temporary label or destroy the combo.
        // Completion proves the input and this popup's closure; subsequent
        // script assertions check the action's actual semantic result.
        return (*rowInputSent || *nativeSelected) && (!list || !list->isVisible()) &&
            (!popupWindow || (!popupWindow->isVisible() && QApplication::activePopupWidget() != popupWindow));
      }, 6, QStringLiteral("Qt choice '%1' in '%2' did not close its real popup").arg(label, identifier),
      [combo, identifier, label] {
        qInfo().noquote() << QStringLiteral("Qt choice: popup closed after '%1' in '%2', current='%3'")
            .arg(label, identifier, combo ? combo->currentText() : QStringLiteral("<deleted>"));
      });
    };
    // Install the wait before opening: a native style may track its popup
    // in a nested loop. A key sent to the main window must never confirm
    // the command when its NoFocus combo did not receive that key.
    whenReady([this, combo, list, index, nativeSelected] {
      if (*nativeSelected) return true;
      if (!combo || !list || !index.isValid() || !list->isVisible()) {
        m_waitDetail = QStringLiteral("the combo's item view is not a visible popup");
        return false;
      }
      list->scrollTo(index, QAbstractItemView::PositionAtCenter);
      list->doItemsLayout();
      const QRect rect = list->visualRect(index);
      QWidget* viewport = list->viewport();
      // A macOS menu-style row can include trailing layout space beyond
      // its narrow viewport while its label is fully shown. Hit the real
      // exposed row, not the center of that non-visible layout space.
      const QRect exposed = rect.intersected(viewport->rect());
      const QPoint point = exposed.center();
      QWidget* hit = QApplication::widgetAt(viewport->mapToGlobal(point));
      m_waitDetail = QStringLiteral("popup row %1 at %2,%3, viewport %4x%5, exposed %6x%7, index-hit=%8, widget-hit=%9")
          .arg(index.row()).arg(point.x()).arg(point.y()).arg(viewport->width()).arg(viewport->height())
          .arg(exposed.width()).arg(exposed.height()).arg(list->indexAt(point) == index)
          .arg(hit == viewport || (hit && viewport->isAncestorOf(hit)));
      return !exposed.isEmpty() && list->indexAt(point) == index &&
          QApplication::activePopupWidget() == list->window() &&
          (hit == viewport || (hit && viewport->isAncestorOf(hit)));
    }, 6, QStringLiteral("the real Qt choice popup did not display '%1'").arg(label),
    [this, combo, list, index, identifier, label, nativeSelected, rowInputSent, waitClosed, line] {
      waitClosed();
      if (*nativeSelected) {
        qInfo().noquote() << QStringLiteral("Qt choice: selected '%1' in '%2' native popup").arg(label, identifier);
        return;
      }
      const auto send = [this, combo, list, index, identifier, label, rowInputSent, line](QEvent::Type type) {
        if (!combo || !list || !index.isValid()) {
          fail(line, QStringLiteral("Qt choice '%1' in '%2' lost its live popup row").arg(label, identifier)); return;
        }
        const QPointer<QWidget> receiver(list->viewport());
        list->doItemsLayout();
        const QRect rect = list->visualRect(index);
        const QRect exposed = rect.intersected(receiver->rect());
        const QPoint point = exposed.center();
        QWidget* hit = QApplication::widgetAt(receiver->mapToGlobal(point));
        if (!list->isVisible() || exposed.isEmpty() || list->indexAt(point) != index ||
            QApplication::activePopupWidget() != list->window() ||
            (hit != receiver && (!hit || !receiver->isAncestorOf(hit)))) {
          fail(line, QStringLiteral("Qt choice '%1' in '%2' has no exposed popup row").arg(label, identifier)); return;
        }
        // Resolve every phase from the persistent row's current geometry.
        // A press can change the motion type, shrink the floating card and
        // move its popup before the release; the old screen point is stale.
        const QPoint global = receiver->mapToGlobal(point);
        QWidget* popup = receiver->window();
        const bool moved = type == QEvent::MouseMove;
        const QString phase = moved ? QStringLiteral("move") : type == QEvent::MouseButtonPress
            ? QStringLiteral("press") : QStringLiteral("release");
        qInfo().noquote() << QStringLiteral("Qt choice pointer: %1 phase=%2 row=%3 rect=%4,%5 %6x%7 global=%8,%9 popup-visible=%10 current='%11' exposed=%12,%13 %14x%15 index-hit=1 widget-hit=1")
            .arg(identifier, phase).arg(index.row()).arg(rect.x()).arg(rect.y()).arg(rect.width()).arg(rect.height())
            .arg(global.x()).arg(global.y()).arg(popup->isVisible()).arg(combo->currentText())
            .arg(exposed.x()).arg(exposed.y()).arg(exposed.width()).arg(exposed.height());
        QMouseEvent event(type, receiver->mapFromGlobal(global), popup->mapFromGlobal(global), global,
                          moved ? Qt::NoButton : Qt::LeftButton,
                          type == QEvent::MouseButtonPress ? Qt::LeftButton : Qt::NoButton, Qt::NoModifier);
        // Mark only after the release passed every live row/hit guard, just
        // before dispatch: its handler may open a dialog in a nested loop.
        if (type == QEvent::MouseButtonRelease) *rowInputSent = true;
        QCoreApplication::sendEvent(receiver, &event);
      };
      queue([send] { send(QEvent::MouseMove); });
      queue([send] { send(QEvent::MouseButtonPress); });
      queue([send, identifier, label] {
        send(QEvent::MouseButtonRelease);
        qInfo().noquote() << QStringLiteral("Qt choice: clicked '%1' in '%2' popup").arg(label, identifier);
      });
    }, window);
    if (!list->isVisible()) {
      queue([this, combo, list, label, nativeSelected, line] {
        QString error;
        const auto closed = [nativeSelected] { *nativeSelected = true; };
        const bool selected = mac::chooseTestPopup(label, [combo] { if (combo) combo->showPopup(); },
            [list] { return list && list->isVisible(); }, error, closed);
        if (selected) *nativeSelected = true;
        if (!error.isEmpty()) fail(line, error);
      });
    }
  } else if (name == QLatin1String("qt-type-now")) {
    const QPointer<QLineEdit> edit(m_main ? m_main->findChild<QLineEdit*>(a.at(0)) : nullptr);
    if (mac::test::active() || !edit || !edit->isVisible() || !edit->isEnabled()) {
      fail(line, QStringLiteral("qt-type-now requires a visible, enabled Qt text field")); return;
    }
    // Real editor events, deliberately without the driver's idle gate.
    // Delivering a whole text replacement as one event avoids intermediate
    // numeric prefixes starting extra previews unrelated to this race.
    edit->setFocus(Qt::OtherFocusReason);
    edit->selectAll();
    QKeyEvent down(QEvent::KeyPress, Qt::Key_unknown, Qt::NoModifier, a.at(1));
    QCoreApplication::sendEvent(edit.data(), &down);
    if (!edit) { fail(line, QStringLiteral("the Qt text field was destroyed during input")); return; }
    QKeyEvent up(QEvent::KeyRelease, Qt::Key_unknown, Qt::NoModifier, a.at(1));
    QCoreApplication::sendEvent(edit.data(), &up);
    if (edit->text() != a.at(1)) fail(line, QStringLiteral("the Qt field did not retain the requested text"));
  } else if (name == QLatin1String("native-system-key") || name == QLatin1String("native-system-type")) {
    if (!mac::test::active()) { fail(line, QStringLiteral("native system input requires the AppKit test UI")); return; }
    if (name == QLatin1String("native-system-type")) {
      queue([this, a, line] {
        // Restoring an XPC file panel can require an AppKit run-loop turn.
        // Keep unrelated work running, but do not reenter the next step.
        m_timer->stop();
        QString error;
        if (!mac::test::sendSystemFileText(a.at(0), error)) { fail(line, error); return; }
        if (!m_finished) m_timer->start();
      });
    } else {
      KeyPress press;
      QString error;
      if (!parseKeySpec(a.at(0), press.key, press.modifiers, press.text, error)) { fail(line, error); return; }
      queue([this, press, line] {
        m_timer->stop();
        QString error;
        if (!mac::test::sendSystemFileKey(press.key, press.modifiers, error)) { fail(line, error); return; }
        if (!m_finished) m_timer->start();
      });
    }
  } else if (name == QLatin1String("key") || name == QLatin1String("key-now") || name == QLatin1String("native-key-now") ||
             name == QLatin1String("native-key-event")) {
    if (name.startsWith(QLatin1String("native-")) && !mac::test::active()) {
      fail(line, QStringLiteral("%1 requires the native macOS UI").arg(name));
      return;
    }
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
    if (name == QLatin1String("native-key-event")) {
      mac::test::requestKeyEventFocus();
      // AppKit activation is asynchronous. Posting while inactive can use
      // menu fallback instead of the document's guarded shortcut dispatch.
      whenReady([this] {
        if (mac::test::keyEventFocusReady()) return true;
        m_waitDetail = mac::test::keyEventFocusDiagnostic();
        return false;
      }, 6,
                QStringLiteral("native application did not activate its key-event target"),
                [this, presses, line] {
        for (const KeyPress& press : presses) {
          queue([this, press, line] {
            if (!mac::test::sendKeyEvent(press.key, press.modifiers, press.text)) {
              fail(line, QStringLiteral("native application key event lost its active target (%1)")
                             .arg(mac::test::keyEventFocusDiagnostic()));
            }
          });
        }
      });
    } else {
      for (const KeyPress& press : presses) {
        queue([this, press] { sendKey(press); });
      }
    }
  } else if (name == QLatin1String("type")) {
    const QString text = a.join(QLatin1Char(' '));
    queue([this, text] { sendText(text); });
  } else {
    stepMatched = false;
  }
  // MSVC refuses an if-else chain of more than 128 links (C1061), so the
  // steps are split into two chains.
  if (stepMatched) {
    return;
  }

  // --- The mouse in the 3D view
  if (name == QLatin1String("click") || name == QLatin1String("dblclick")) {
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
  } else if (name == QLatin1String("trackpad-scroll")) {
    double x = 0, y = 0, dx = 0, dy = 0;
    Qt::KeyboardModifiers modifiers;
    const bool momentum = a.size() == 6 && a.at(5) == QLatin1String("momentum");
    if (!toNumber(a.at(0), x) || !toNumber(a.at(1), y) ||
        !toNumber(a.at(2), dx) || !toNumber(a.at(3), dy) ||
        !parseModifiers(a.at(4), modifiers) || (a.size() == 6 && !momentum)) {
      fail(line, QStringLiteral("trackpad-scroll <x%> <y%> <dx> <dy> <mods> [momentum]"));
      return;
    }
    QPoint at;
    if (!viewPoint(x, y, at)) {
      fail(line, QStringLiteral("no 3D view"));
      return;
    }
    queue([this, at, dx, dy, modifiers, momentum] {
      QWidget* view = viewer();
      if (!view || !m_main) return;
      const QPoint global = m_main->mapToGlobal(at);
      QWheelEvent event(QPointF(view->mapFromGlobal(global)), QPointF(global),
                        QPoint(qRound(dx), qRound(dy)), QPoint(), Qt::NoButton,
                        modifiers, momentum ? Qt::ScrollMomentum : Qt::ScrollUpdate, false);
      QCoreApplication::sendEvent(view, &event);
    });
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
  } else if (name == QLatin1String("wheel-burst")) {
    double x = 0;
    double y = 0;
    double notches = 0;
    double count = 0;
    Qt::KeyboardModifiers modifiers;
    if (!toNumber(a.at(0), x) || !toNumber(a.at(1), y) || !toNumber(a.at(2), notches) || !toNumber(a.at(3), count) ||
        count < 1 || (a.size() > 4 && !parseModifiers(a.at(4), modifiers))) {
      fail(line, QStringLiteral("wheel-burst <x%> <y%> <notches> <count> [mods]"));
      return;
    }
    QPoint at;
    if (!viewPoint(x, y, at)) {
      fail(line, QStringLiteral("no 3D view"));
      return;
    }
    queue([this, at] { sendMouse(QEvent::MouseMove, at, Qt::NoButton, Qt::NoButton, Qt::NoModifier); });
    for (int i = 0; i < qRound(count); ++i) {
      queue([this, at, notches, modifiers] { sendWheel(at, qRound(notches), modifiers); });
    }

    // --- The mouse where the application logged a place
  } else if (name == QLatin1String("click-visible-face")) {
    const auto point = std::make_shared<QPoint>();
    const auto expected = std::make_shared<SelectionItem>();
    whenReady([this, point, expected] {
      auto* view = qobject_cast<OcctViewer*>(viewer());
      if (!view || !m_main || !view->isValid()) return false;
      // Ask the current OCCT selector after this lifecycle transition;
      // idle views do not emit the command-only "Pick places" log.
      const auto candidates = view->pickPoints(SelectKind::Face);
      for (const auto& candidate : candidates) {
        const auto actual = view->itemAt(candidate.second);
        if (!actual || actual->kind != SelectKind::Face || *actual != candidate.first) continue;
        *point = view->mapTo(m_main.data(), candidate.second);
        *expected = *actual;
        return true;
      }
      m_waitDetail = QStringLiteral("OCCT found %1 face candidates").arg(candidates.size());
      return false;
    }, 6, QStringLiteral("no visible selectable face in the current viewport"), [this, point, expected, line] {
      queueClick(*point, Qt::LeftButton, Qt::NoModifier, 1);
      queue([this, expected, line] {
        auto* window = qobject_cast<MainWindow*>(m_main.data());
        if (!window) { fail(line, QStringLiteral("the document vanished after the face click")); return; }
        window->deliverPendingPicks();
        whenReady([this, expected] {
          const auto* current = qobject_cast<MainWindow*>(m_main.data());
          if (!current || current->modelBusy()) return false;
          const Selection& selected = current->selection();
          if (selected.size() != 1 || selected.first().kind != SelectKind::Face || selected.first() != *expected) {
            m_waitDetail = QStringLiteral("actual selection has %1 items; expected %2")
                .arg(selected.size()).arg(expected->describe());
            return false;
          }
          qInfo().noquote() << QStringLiteral("TestDriver: actual face selected after viewport click: %1")
              .arg(selected.first().describe());
          return true;
        }, 6, QStringLiteral("the actual viewport click did not select the probed face"));
      });
    }, mainWindow);
  } else if (name == QLatin1String("expect-pick-offset") || name == QLatin1String("click-pick-offset")) {
    QRegularExpression target;
    QRegularExpression expected;
    if (!regex(QStringLiteral("^Pick (?:.*)(?:%1)").arg(a.at(0)), target) || !regex(a.at(3), expected)) { return; }
    bool xOk = false;
    bool yOk = false;
    const int dx = a.at(1).toInt(&xOk);
    const int dy = a.at(2).toInt(&yOk);
    if (!xOk || !yOk) { fail(line, QStringLiteral("pick offsets must be logical pixels")); return; }
    const auto at = std::make_shared<QPoint>();
    const auto settle = std::make_shared<QElapsedTimer>();
    settle->start();
    whenReady([this, target, expected, dx, dy, at, settle] {
      if (TestSync::settling() && settle->elapsed() < kSettleLimitMs) { return false; }
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window) { return false; }
      auto& view = window->viewer();
      bool found = false;
      for (const auto& [item, point] : view.pickPoints(SelectKind::Face | SelectKind::Edge | SelectKind::Vertex)) {
        const QString description = QStringLiteral("Pick %1 %2/%3")
            .arg(SelectionItem::kindName(item.kind), item.owner, item.name);
        if (target.match(description).hasMatch()) {
          *at = view.mapTo(m_main.data(), point) + QPoint(dx, dy);
          found = true;
        }
      }
      if (!found) { m_waitDetail = QStringLiteral("no fresh point for %1").arg(target.pattern()); return false; }
      const auto item = view.itemAt(view.mapFrom(m_main.data(), *at));
      const QString actual = item ? item->describe() : QStringLiteral("nothing");
      m_waitDetail = QStringLiteral("pick at offset %1,%2: %3").arg(dx).arg(dy).arg(actual);
      if (!expected.match(actual).hasMatch()) { return false; }
      qInfo().noquote() << QStringLiteral("TestDriver: %1").arg(m_waitDetail);
      return true;
    }, 6, QStringLiteral("offset pick did not match '%1'").arg(a.at(3)),
    [this, at, click = name == QLatin1String("click-pick-offset")] {
      sendMouse(QEvent::MouseMove, *at, Qt::NoButton, Qt::NoButton, Qt::NoModifier);
      if (click) { queueClick(*at, Qt::LeftButton, Qt::NoModifier, 1); }
    }, mainWindow);
  } else if (name == QLatin1String("click-logged") || name == QLatin1String("dblclick-logged") ||
             name == QLatin1String("dialog-click-logged") || name == QLatin1String("dialog-dblclick-logged") ||
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
    const bool dialog = name.startsWith(QLatin1String("dialog-"));
    const int clicks = name == QLatin1String("dblclick-logged") || name == QLatin1String("dialog-dblclick-logged") ? 2 : 1;
    if (mac::test::active() && !pick && clicks == 1 && button == Qt::LeftButton &&
        modifiers == Qt::NoModifier && a.at(0).contains(QStringLiteral(" input "))) {
      const auto identifier = std::make_shared<QString>();
      whenReady([this, re, identifier] {
        QString logged;
        if (findLastLine(re, 0, &logged) < 0) { return false; }
        const auto match = QRegularExpression(QStringLiteral(" input (\\S+) at ")).match(logged);
        if (!match.hasMatch()) { return false; }
        *identifier = QStringLiteral("input_") + match.captured(1);
        return mac::test::controlExists(*identifier);
      }, 6, QStringLiteral("no native input logged for '%1'").arg(a.at(0)), [this, identifier, line] {
        queue([this, identifier, line] {
          QString error;
          if (!mac::test::click(*identifier, error)) { fail(line, error); }
        });
      });
      return;
    }
    const auto at = std::make_shared<QPoint>();
    // Places are read once what is about to be logged is (mitcad#528: a
    // panel logs its places again 50 ms after they moved, on a watched
    // timer), as the Linux and Windows steps wait for the sync; at most as
    // long as the sync waits for them.
    const auto settle = std::make_shared<QElapsedTimer>();
    settle->start();
    whenReady(
        [this, re, pick, at, settle] {
          if (TestSync::settling() && settle->elapsed() < kSettleLimitMs) {
            return false;
          }
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
        [this, at, button, modifiers, clicks, dialog] {
          if (dialog) queueDialogClick(*at, clicks);
          else queueClick(*at, button, modifiers, clicks);
        }, dialog ? targetWindow() : mainWindow);
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
    if (mac::test::active()) {
      const auto identifier = std::make_shared<QString>();
      whenReady([this, re, identifier] {
        QString logged;
        if (findLastLine(re, 0, &logged) < 0) return false;
        const QRegularExpression input(QStringLiteral(" input (\\S+) at "));
        const auto match = input.match(logged);
        if (!match.hasMatch()) return false;
        *identifier = QStringLiteral("input_") + match.captured(1);
        return mac::test::controlExists(*identifier);
      }, 6, QStringLiteral("no native input logged for '%1'").arg(a.at(0)), [this, identifier, text, line] {
        queue([this, identifier, text, line] {
          QString error;
          if (!mac::test::replaceText(*identifier, text, error)) fail(line, error);
        });
      });
      return;
    }
    const auto at = std::make_shared<QPoint>();
    const auto editor = std::make_shared<QPointer<QLineEdit>>();
    whenReady(
        [this, re, at, editor] {
          QString logged;
          if (findLastLine(re, 0, &logged) < 0) { return false; }
          const auto match = QRegularExpression(QStringLiteral(" input (\\S+) at ")).match(logged);
          if (match.hasMatch() && m_main) {
            auto* input = m_main->findChild<QLineEdit*>(QStringLiteral("input_") + match.captured(1));
            if (input && input->isVisible()) {
              // Validation hints can move a field after its logged position.
              // Reveal the actual editor and use its current position.
              for (QWidget* parent = input->parentWidget(); parent; parent = parent->parentWidget()) {
                if (auto* scroll = qobject_cast<QScrollArea*>(parent)) { scroll->ensureWidgetVisible(input); }
              }
              *at = m_main->mapFromGlobal(input->mapToGlobal(input->rect().center()));
              *editor = input;
              return true;
            }
          }
          return loggedPoint(re, 0, *at);
        }, 6,
        QStringLiteral("no place logged for '%1'").arg(a.at(0)),
        [this, at, text, editor] {
          queueClick(*at, Qt::LeftButton, Qt::NoModifier, 1);
          queue([editor] {
            if (*editor) { (*editor)->setFocus(Qt::TabFocusReason); }
          });
          KeyPress selectAll;
          selectAll.key = Qt::Key_A;
          selectAll.modifiers = Qt::ControlModifier;
          queue([this, selectAll] { sendKey(selectAll); });
          queue([this, text] { sendText(text); });
        },
        mainWindow);

    // --- The mouse at sketch points
  } else if (name == QLatin1String("expect-sketch-dimension-clear")) {
    auto* controller = mainWindow->findChild<sketch::SketchController*>();
    if (!controller || !controller->isActive() || !controller->showDimensions() || !controller->showConstraints()) {
      fail(line, QStringLiteral("dimension readability requires an active sketch with dimensions and constraints shown")); return;
    }
    QRegularExpression expression;
    if (!regex(a.at(0), expression)) return;
    const auto& overlay = controller->overlay();
    const auto glyphs = overlay.constraintGlyphBounds();
    if (glyphs.empty()) { fail(line, QStringLiteral("no constraint glyphs are drawn")); return; }
    int matched = 0;
    for (const auto& dimension : controller->model().dimensions) {
      const QString text = overlay.dimensionText(dimension);
      if (!expression.match(text).hasMatch()) continue;
      ++matched;
      const QRectF bounds = overlay.dimensionTextRect(dimension.id);
      if (bounds.isEmpty() || !QRectF(viewer()->rect()).contains(bounds)) {
        fail(line, QStringLiteral("dimension %1 (%2) is not visible inside the viewport").arg(dimension.id, text)); return;
      }
      for (const QRectF& glyph : glyphs) {
        if (glyph.intersects(bounds.adjusted(-2, -2, 2, 2))) {
          fail(line, QStringLiteral("dimension %1 (%2) is covered by a constraint glyph").arg(dimension.id, text)); return;
        }
      }
      const auto hit = overlay.hitTest(bounds.center());
      if (hit.kind != sketch::SketchOverlay::Hit::Kind::Dimension || hit.id != dimension.id) {
        fail(line, QStringLiteral("dimension %1 lost its ordinary pick target").arg(dimension.id)); return;
      }
    }
    if (matched == 0) { fail(line, QStringLiteral("no dimension text matches '%1'").arg(a.at(0))); return; }
    for (const QRectF& glyph : glyphs) {
      const auto kind = overlay.hitTest(glyph.center()).kind;
      if (kind != sketch::SketchOverlay::Hit::Kind::Constraint && kind != sketch::SketchOverlay::Hit::Kind::Fixed) {
        fail(line, QStringLiteral("a drawn constraint glyph lost its ordinary pick target")); return;
      }
    }
    qInfo() << "Sketch dimension readability:" << matched << "matching labels clear of" << glyphs.size()
            << "constraint glyphs; ordinary pick targets retained";
  } else if (name == QLatin1String("expect-sketch-field-dimension")) {
    auto* controller = mainWindow->findChild<sketch::SketchController*>();
    double expected = 0;
    if (!controller || !controller->tool() || !toNumber(a.at(2), expected)) {
      fail(line, QStringLiteral("expected an active sketch drawing tool and a numeric dimension")); return;
    }
    sketch::ToolPreview preview;
    controller->tool()->paint(preview);
    const auto found = std::find_if(preview.fieldDimensions.begin(), preview.fieldDimensions.end(),
                                    [&a](const auto& value) { return value.field == a.at(0); });
    if (found == preview.fieldDimensions.end() || found->dimension.type != a.at(1)) {
      fail(line, QStringLiteral("field %1 has no %2 dimension preview").arg(a.at(0), a.at(1))); return;
    }
    const auto& value = *found;
    auto measured = sketch::measure(value.dimension, value.model);
    if (value.dimension.type == QStringLiteral("arc_length")) {
      const auto* arc = value.model.curve(QStringLiteral("preview"));
      measured = arc ? std::optional<double>(value.dimension.previewSweep.value_or(
                           sketch::sweep(arc->curve.start, arc->curve.end))) : std::nullopt;
    }
    if (!measured || std::abs(*measured - expected) > 1e-6) {
      fail(line, QStringLiteral("field %1 dimension measures %2, expected %3")
                     .arg(a.at(0)).arg(measured.value_or(-1)).arg(expected)); return;
    }
    const QRectF field = controller->fieldRect(value.field);
    const auto at = controller->sketchAtF(field.center());
    if (field.isEmpty() || !at || !QRectF(viewer()->rect()).contains(field)) {
      fail(line, QStringLiteral("field %1 lies outside the view").arg(value.field)); return;
    }
    auto dimension = value.dimension;
    dimension.text = *at;
    const sketch::Projection projection{[controller](const sketch::V2& p) { return controller->toScreen(p); },
                                         controller->pixel()};
    const auto shape = sketch::dimensionShape(dimension, value.model, projection, QString());
    if (!shape.valid || shape.arrows.empty() || (shape.lines.empty() && shape.arcs.empty()) ||
        QLineF(shape.textAt, field.center()).length() > 1.0) {
      fail(line, QStringLiteral("field %1 is missing its dimension graphics: valid=%2 arrows=%3 lines=%4 arcs=%5 text-field delta=%6 px")
                     .arg(value.field).arg(shape.valid).arg(shape.arrows.size()).arg(shape.lines.size())
                     .arg(shape.arcs.size()).arg(QLineF(shape.textAt, field.center()).length(), 0, 'g', 12)); return;
    }
    for (auto* other : viewer()->findChildren<QLineEdit*>()) {
      if (other->isVisible() && other->objectName().startsWith(QStringLiteral("sketchField_")) &&
          other->objectName() != QStringLiteral("sketchField_") + value.field &&
          field.intersects(QRectF(other->geometry()))) {
        fail(line, QStringLiteral("field %1 overlaps %2").arg(value.field, other->objectName())); return;
      }
    }
    for (auto* label : viewer()->findChildren<QLabel*>()) {
      if (!label->isVisible() || !label->objectName().startsWith(QStringLiteral("sketchFieldLabel_"))) {
        continue;
      }
      if (!QRectF(viewer()->rect()).contains(QRectF(label->geometry()))) {
        fail(line, QStringLiteral("field label %1 lies outside the view").arg(label->objectName())); return;
      }
      for (auto* edit : viewer()->findChildren<QLineEdit*>()) {
        if (edit->isVisible() && edit->objectName().startsWith(QStringLiteral("sketchField_")) &&
            QRectF(label->geometry()).intersects(QRectF(edit->geometry()))) {
          fail(line, QStringLiteral("field label %1 covers %2").arg(label->objectName(), edit->objectName())); return;
        }
      }
    }
  } else if (name == QLatin1String("expect-profiles")) {
    double count = 0;
    if (!toNumber(a.at(0), count) || count < 0 || count != std::floor(count)) {
      fail(line, QStringLiteral("expect-profiles <nonnegative-count>")); return;
    }
    whenReady([this, count] {
      auto* view = qobject_cast<OcctViewer*>(viewer());
      return view && view->profileCount() == static_cast<std::size_t>(count);
    }, 6, QStringLiteral("expected %1 displayed profiles").arg(count));
  } else if (name == QLatin1String("sketch-pointer")) {
    const QString phase = a.at(0);
    double x = 0;
    double y = 0;
    if ((phase != QLatin1String("press") && phase != QLatin1String("move") && phase != QLatin1String("release")) ||
        !toNumber(a.at(1), x) || !toNumber(a.at(2), y)) {
      fail(line, QStringLiteral("sketch-pointer press|move|release <x> <y>")); return;
    }
    const auto at = std::make_shared<QPoint>();
    whenReady([this, x, y, at] { return sketchPoint(x, y, *at); }, 6,
              QStringLiteral("the app logged no sketch view"), [this, phase, at] {
      if (phase == QLatin1String("press")) {
        sendMouse(QEvent::MouseMove, *at, Qt::NoButton, Qt::NoButton, Qt::NoModifier);
        sendMouse(QEvent::MouseButtonPress, *at, Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
      } else if (phase == QLatin1String("move")) {
        sendMouse(QEvent::MouseMove, *at, Qt::NoButton, Qt::LeftButton, Qt::NoModifier);
      } else {
        sendMouse(QEvent::MouseButtonRelease, *at, Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
      }
    }, mainWindow);
  } else if (name == QLatin1String("sketch-click") || name == QLatin1String("sketch-dblclick") ||
             name == QLatin1String("sketch-drag") ||
             name == QLatin1String("sketch-move")) {
    const bool drag = name == QLatin1String("sketch-drag");
    const bool move = name == QLatin1String("sketch-move");
    const int clicks = name == QLatin1String("sketch-dblclick") ? 2 : 1;
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
        [this, drag, move, clicks, from, to, button, modifiers] {
          if (drag) {
            queueDrag(*from, *to, Qt::LeftButton, Qt::NoModifier);
          } else if (move) {
            sendMouse(QEvent::MouseMove, *from, Qt::NoButton, Qt::NoButton, Qt::NoModifier);
          } else {
            queueClick(*from, button, modifiers, clicks);
          }
        },
        mainWindow);

    // --- Commands, menus and buttons
  } else if (name == QLatin1String("test-status-message")) {
    // Exercise the ordinary status path without manufacturing a model
    // failure just to obtain an error's longer lifetime (mitcad#996).
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    if (window == nullptr || (a.at(0) != QLatin1String("hint") && a.at(0) != QLatin1String("error"))) {
      fail(line, QStringLiteral("test-status-message requires hint|error and the main window")); return;
    }
    if (a.at(0) == QLatin1String("error")) window->showError(a.at(1));
    else window->showHint(a.at(1));
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
  } else if (name == QLatin1String("open-file") || name == QLatin1String("open-file-now")) {
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    if (window == nullptr) {
      fail(line, QStringLiteral("open-file requires a document window"));
      return;
    }
    const QPointer<MainWindow> guard(window);
    const QString path = a.at(0);
    queue([guard, path] {
      if (guard != nullptr) {
        guard->openExternalFile(path);
      }
    });
  } else if (name == QLatin1String("file-dialog")) {
    const QString mode = a.at(0);
    if (mode != QLatin1String("save") && mode != QLatin1String("directory")) {
      fail(line, QStringLiteral("file-dialog save|directory <initial> [filters] [selected-filter]"));
      return;
    }
    const QPointer<QWidget> parent(mainWindow);
    queue([parent, mode, a] {
      if (!parent) return;
      if (mode == QLatin1String("directory")) {
        dialogDirectory(parent, QStringLiteral("File Dialog"), a.at(1));
      } else {
        QString selected = a.value(3);
        dialogSaveFileName(parent, QStringLiteral("File Dialog"), a.at(1), a.value(2), &selected);
      }
    });
  } else if (name == QLatin1String("menu") || name == QLatin1String("menu-now")) {
    if (mac::test::active()) {
      const QString path = a.at(0);
      queue([this, path, line] {
        QString error;
        if (!mac::test::triggerMenu(path, error)) fail(line, error);
      });
      return;
    }
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
    if (mac::test::active()) {
      if (a.size() > 1) {
        fail(line, QStringLiteral("button objectName filtering requires Qt controls"));
        return;
      }
      whenReady([re] { return mac::test::buttonExists(re); }, 6,
                QStringLiteral("no native button matching '%1'").arg(a.at(0)), [this, re, line] {
        QString error;
        if (!mac::test::clickButton(re, error)) fail(line, error);
      });
      return;
    }
    const QPointer<QWidget> window = targetWindow();
    const QString identifier = a.size() > 1 ? a.at(1) : QString();
    whenReady(
        [this, re, window, identifier] {
          if (window.isNull()) {
            return false;
          }
          const QList<QAbstractButton*> buttons = window->findChildren<QAbstractButton*>();
          for (QAbstractButton* button : buttons) {
            if (button->isVisibleTo(window.data()) && button->isEnabled() &&
                (identifier.isEmpty() || button->objectName() == identifier) &&
                re.match(plainText(button->text().isEmpty() ? button->accessibleName() : button->text())).hasMatch()) {
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
    if (mac::test::active()) {
      whenReady([re] { return mac::test::dialogExists(re); }, limit,
                QStringLiteral("no native window matching '%1'").arg(a.at(0)), [this, re] {
        mac::test::focusDialog(re);
        m_notBefore = std::max(m_notBefore, m_clock.elapsed() + 300);
      });
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
  } else if (name == QLatin1String("collapse-card")) {
    auto* card = m_main->findChild<GlassCard*>(a.at(0));
    if (!card || (a.at(1) != QLatin1String("true") && a.at(1) != QLatin1String("false"))) {
      fail(line, QStringLiteral("collapse-card needs a card name and true|false")); return;
    }
    if (card->isCollapsed() != (a.at(1) == QLatin1String("true"))) {
      card->findChild<QToolButton*>(QStringLiteral("cardChevron"))->click();
    }
  } else if (name == QLatin1String("expect-projects-layout")) {
    whenReady([this, a] {
      auto* projects = m_main->findChild<QWidget*>(QStringLiteral("projectsDock"));
      auto* browser = m_main->findChild<QWidget*>(QStringLiteral("browserDock"));
      auto* tree = m_main->findChild<QTreeWidget*>(QStringLiteral("projectsTree"));
      if (!projects || !browser || !tree || !projects->isVisible() || !browser->isVisible() ||
          tree->topLevelItemCount() != a.at(0).toInt()) {
        m_waitDetail = QStringLiteral("missing or hidden panels, or unexpected project count");
        return false;
      }
      const QRect actual(projects->mapToGlobal(QPoint()), projects->size());
      const QRect next(browser->mapToGlobal(QPoint()), browser->size());
      const QRect host(m_main->mapToGlobal(QPoint()), m_main->size());
      if (!host.contains(actual) || !host.contains(next)) {
        m_waitDetail = QStringLiteral("Projects or Browser extends outside the main window");
        return false;
      }
      auto* card = qobject_cast<GlassCard*>(projects);
      auto* browserCard = qobject_cast<GlassCard*>(browser);
      if (!card) {
        auto* browserRows = browser->findChild<QTreeWidget*>();
        for (QTreeWidget* rows : {tree, browserRows}) {
          if (!rows || rows->viewport()->height() < std::max(rows->fontMetrics().height() + 6,
                                                            rows->sizeHintForRow(0))) {
            m_waitDetail = QStringLiteral("a docked tree cannot show one complete row");
            return false;
          }
        }
      }
      qInfo() << "Projects layout: actual" << actual << "browser" << next
              << "slot" << (card ? card->slot() : QRect()) << "minimum" << projects->minimumSize();
      if (actual.intersects(next) || actual.bottom() >= next.top()) {
        m_waitDetail = QStringLiteral("Projects bottom %1 reaches Browser top %2")
            .arg(actual.bottom()).arg(next.top());
        return false;
      }
      if (card && (!browserCard || card->slot().size() != card->size() ||
                   card->slot().intersects(browserCard->slot()) ||
                   card->parentWidget()->mapToGlobal(card->slot().topLeft()) != actual.topLeft())) {
        m_waitDetail = QStringLiteral("actual card bounds differ from their slots");
        return false;
      }
      auto* toolbar = m_main->findChild<QToolBar*>(QStringLiteral("projectsToolbar"));
      if (!toolbar || ((!card || !card->isCollapsed()) &&
          (toolbar->height() < toolbar->sizeHint().height() || !actual.contains(
              QRect(toolbar->mapToGlobal(QPoint()), toolbar->size()))))) {
        m_waitDetail = QStringLiteral("toolbar height %1, preferred %2, Projects height %3")
            .arg(toolbar ? toolbar->height() : -1).arg(toolbar ? toolbar->sizeHint().height() : -1)
            .arg(projects->height());
        return false;
      }
      if (!card || !card->isCollapsed()) {
        for (auto* button : toolbar->findChildren<QToolButton*>()) {
          if (button->isVisible() && !toolbar->rect().contains(QRect(
                  button->mapTo(toolbar, QPoint()), button->size()))) {
            m_waitDetail = QStringLiteral("toolbar clips visible button '%1'").arg(button->objectName());
            return false;
          }
        }
      }
      qInfo() << "Projects layout: passed, projects" << tree->topLevelItemCount()
              << "DPR" << projects->devicePixelRatioF();
      return true;
    }, 6, QStringLiteral("Projects/Browser overlap, stale slot, clipped toolbar or project rows not loaded"));
  } else if (name == QLatin1String("expect-tree-rename")) {
    whenReady([this, a] {
      QStringList observed;
      const auto rectText = [](const QRect& rect) {
        return QStringLiteral("%1,%2 %3x%4").arg(rect.x()).arg(rect.y()).arg(rect.width()).arg(rect.height());
      };
      for (QLineEdit* editor : m_main->findChildren<QLineEdit*>(QStringLiteral("treeRenameEditor"))) {
        observed << QStringLiteral("'%1' selected '%2' visible %3")
                        .arg(editor->text(), editor->selectedText()).arg(editor->isVisible());
        if (!editor->isVisible() || editor->text() != a.at(0)) continue;
        m_waitDetail = QStringLiteral("editor '%1', selected '%2', rect %3")
                           .arg(editor->text(), editor->selectedText(), rectText(editor->geometry()));
        const auto reject = [this](const QString& reason) {
          m_waitDetail += QStringLiteral("; %1").arg(reason);
          return false;
        };
        auto* tree = qobject_cast<QTreeWidget*>(editor->parentWidget()->parentWidget());
        if (!tree) return reject(QStringLiteral("editor is not a tree viewport child"));
        const QModelIndex index = tree->currentIndex();
        m_waitDetail += QStringLiteral("; current row %1 column %2 value '%3'")
                            .arg(index.row()).arg(index.column()).arg(index.data().toString());
        auto* delegate = dynamic_cast<TreeItemDelegate*>(tree->itemDelegateForIndex(index));
        if (!delegate) return reject(QStringLiteral("no shared tree delegate"));
        if (!delegate->editing(index)) return reject(QStringLiteral("current index has no tracked editor"));
        if (editor->hasFrame()) return reject(QStringLiteral("editor has a native line-edit frame"));
        if (!editor->testAttribute(Qt::WA_OpaquePaintEvent)) return reject(QStringLiteral("editor lacks opaque paint attribute"));
        if (editor->palette().color(QPalette::Base).alpha() != 255) {
          return reject(QStringLiteral("editor Base alpha %1").arg(editor->palette().color(QPalette::Base).alpha()));
        }
        if (editor->selectedText() != editor->text()) return reject(QStringLiteral("editor text is not fully selected"));
        for (QPalette::ColorGroup group : {QPalette::Active, QPalette::Inactive}) {
          const QPalette colors = editor->palette();
          const QPalette application = QApplication::palette();
          for (QPalette::ColorRole role : {QPalette::Text, QPalette::Highlight, QPalette::HighlightedText}) {
            if (colors.color(group, role) != application.color(group, role)) {
              return reject(QStringLiteral("palette group %1 role %2: %3 instead of platform editor colour %4")
                                .arg(int(group)).arg(int(role))
                                .arg(colors.color(group, role).name(QColor::HexArgb),
                                     application.color(group, role).name(QColor::HexArgb)));
            }
          }
          if (colors.color(group, QPalette::Highlight).alpha() == 0) {
            return reject(QStringLiteral("palette group %1 has a transparent text-selection background").arg(int(group)));
          }
        }
        QStyleOptionViewItem cell;
        cell.initFrom(tree);
        cell.widget = tree;
        cell.rect = tree->visualRect(index);
        cell.font = tree->font();
        cell.fontMetrics = QFontMetrics(cell.font);
        cell.decorationSize = tree->iconSize();
        cell.state |= QStyle::State_Selected;
        // The editor must stay inside its row and cover the original name,
        // including the Browser's custom icon/bulb text margin.
        const QRect nameInk = delegate->nameTextRect(cell, index);
        const int editorTextLeft = editor->x() + editor->textMargins().left();
        m_waitDetail += QStringLiteral("; row %1, name ink %2, editor text left %3, font height %4")
                            .arg(rectText(cell.rect), rectText(nameInk)).arg(editorTextLeft).arg(cell.fontMetrics.height());
        if (!cell.rect.contains(editor->geometry())) return reject(QStringLiteral("editor extends outside its row"));
        if (editor->height() < cell.fontMetrics.height()) return reject(QStringLiteral("editor is shorter than the font"));
        if (nameInk.isEmpty()) return reject(QStringLiteral("old name ink rectangle is empty"));
        if (!editor->geometry().contains(nameInk)) return reject(QStringLiteral("editor does not cover the old name ink rectangle"));
        if (std::abs(editorTextLeft - nameInk.left()) > 2) return reject(QStringLiteral("editor text is horizontally misaligned"));
        const auto painted = [&](const QColor& ink) {
          QImage image(tree->viewport()->size(), QImage::Format_ARGB32_Premultiplied);
          image.fill(Qt::transparent);
          QPainter painter(&image);
          QStyleOptionViewItem option = cell;
          option.palette.setColor(QPalette::Text, ink);
          option.palette.setColor(QPalette::HighlightedText, ink);
          delegate->paint(&painter, option, index);
          return image;
        };
        // Changing only the text ink must not change a single row pixel:
        // the delegate must omit the old name while the editor is active.
        if (painted(Qt::magenta) != painted(Qt::green)) {
          return reject(QStringLiteral("delegate painting changes with text ink while the editor is open"));
        }
        QImage cover(editor->size(), QImage::Format_ARGB32_Premultiplied);
        cover.fill(Qt::transparent);
        editor->render(&cover);
        for (int y = 0; y < cover.height(); ++y) {
          for (int x = 0; x < cover.width(); ++x) {
            if (cover.pixelColor(x, y).alpha() != 255) {
              return reject(QStringLiteral("editor pixel %1,%2 has alpha %3")
                                .arg(x).arg(y).arg(cover.pixelColor(x, y).alpha()));
            }
          }
        }
        qInfo() << "Tree rename: passed" << editor->text() << editor->geometry() << "covers" << nameInk
                << "DPR" << editor->devicePixelRatioF();
        return true;
      }
      m_waitDetail = QStringLiteral("no visible editor with text '%1'; found %2").arg(a.at(0), observed.join(QStringLiteral(" | ")));
      return false;
    }, 6, QStringLiteral("rename editor is missing, translucent, misplaced, unselected or paints the old name"));
  } else if (name == QLatin1String("expect-no-qt-popup")) {
    QWidget* popup = QApplication::activePopupWidget();
    if (popup == nullptr) {
      for (QWidget* candidate : QApplication::topLevelWidgets()) {
        if (candidate->isVisible() && candidate->windowType() == Qt::Popup) {
          popup = candidate;
          break;
        }
      }
    }
    if (popup != nullptr) {
      fail(line, QStringLiteral("Qt popup '%1' (%2) owns input or remains logically visible")
                     .arg(popup->objectName(), QString::fromLatin1(popup->metaObject()->className())));
    }
  } else if (name == QLatin1String("expect-widget-property")) {
    QWidget* window = targetWindow();
    QObject* widget = namedWidget(window, a.at(0));
    const QVariant value = widget ? widget->property(a.at(1).toUtf8().constData()) : QVariant();
    if (!value.isValid() || value.toString() != a.at(2)) {
      fail(line, QStringLiteral("widget %1.%2 is '%3', expected '%4'").arg(a.at(0), a.at(1), value.toString(), a.at(2)));
    }
  } else if (name == QLatin1String("expect-setting")) {
    const QVariant value = QSettings().value(a.at(0));
    if (!value.isValid() || value.toString() != a.at(1)) {
      fail(line, QStringLiteral("setting %1 is '%2', expected '%3'").arg(a.at(0), value.toString(), a.at(1)));
    }
  } else if (name == QLatin1String("expect-dialog-gone")) {
    QRegularExpression re;
    if (!regex(a.at(0), re)) return;
    whenReady([this, re] {
      for (QWidget* widget : QApplication::topLevelWidgets()) {
        if (widget != m_main.data() && widget->isVisible() && dialogMatches(widget, re)) return false;
      }
      return true;
    }, 6, QStringLiteral("a window matching '%1' remains visible").arg(a.at(0)));
  } else if (name == QLatin1String("focus-widget")) {
    QWidget* window = QApplication::activePopupWidget();
    if (!window) window = targetWindow();
    QWidget* widget = namedWidget(window, a.at(0), true);
    const auto describe = [](const QWidget* value) {
      if (value == nullptr) return QStringLiteral("none");
      const QRect rect = value->geometry();
      return QStringLiteral("%1 name='%2' title='%3' visible=%4 hidden=%5 enabled=%6 rect=%7,%8 %9x%10")
          .arg(QString::fromLatin1(value->metaObject()->className()), value->objectName(), value->windowTitle())
          .arg(value->isVisible()).arg(value->isHidden()).arg(value->isEnabled())
          .arg(rect.x()).arg(rect.y()).arg(rect.width()).arg(rect.height());
    };
    // Record the real lookup context before changing focus. A floating
    // command card, a still-active combo popup and a hidden duplicate field
    // otherwise all reduce to the same unhelpful "no visible widget" error.
    qInfo().noquote() << QStringLiteral("Qt field focus context: id='%1'; popup={%2}; active={%3}; target={%4}; search={%5}")
        .arg(a.at(0), describe(QApplication::activePopupWidget()), describe(QApplication::activeWindow()),
             describe(targetWindow()), describe(window));
    int candidates = 0;
    for (QWidget* candidate : QApplication::allWidgets()) {
      if (candidate->objectName() != a.at(0)) continue;
      ++candidates;
      QStringList ancestry;
      for (QWidget* parent = candidate->parentWidget(); parent != nullptr; parent = parent->parentWidget()) {
        ancestry << describe(parent);
      }
      qInfo().noquote() << QStringLiteral("Qt field focus candidate: selected=%1; field={%2}; window={%3}; host={%4}; parents=[%5]")
          .arg(candidate == widget).arg(describe(candidate), describe(candidate->window()),
               describe(GlassCard::hostWindow(candidate)), ancestry.join(QStringLiteral("; ")));
    }
    if (candidates == 0) qInfo().noquote() << QStringLiteral("Qt field focus candidate: none for '%1'").arg(a.at(0));
    if (!widget || !widget->isVisible() || !widget->isEnabled()) {
      fail(line, QStringLiteral("no unique visible enabled widget '%1'").arg(a.at(0))); return;
    }
    m_dialog = widget->window();
    widget->setFocus(Qt::TabFocusReason);
  } else if (name == QLatin1String("dialog-resize")) {
    double width = 0;
    double height = 0;
    QWidget* window = targetWindow();
    if (!window || window == m_main.data() || !toNumber(a.at(0), width) || !toNumber(a.at(1), height) ||
        width < 300 || height < 200 || width > 3000 || height > 2000) {
      fail(line, QStringLiteral("dialog-resize requires a focused dialog and width 300..3000, height 200..2000"));
      return;
    }
    window->resize(qRound(width), qRound(height));
    m_notBefore = std::max(m_notBefore, m_clock.elapsed() + 300);
  } else if (name == QLatin1String("show-dialog-style-fixture")) {
    showDialogStyleFixture(a.at(0));
  } else if (name == QLatin1String("expect-macos-tool-buttons")) {
    testMacToolButtons();
  } else if (name == QLatin1String("expect-sketch-finish-layout")) {
    testSketchFinishLayout();
  } else if (name == QLatin1String("expect-ribbon-labels")) {
    testRibbonLabels(a);
  } else if (name == QLatin1String("test-dialog-controls")) {
    testDialogControls(a.at(0));
  } else if (name == QLatin1String("expect-dialog-appearance")) {
    const bool dark = a.at(0) == QStringLiteral("dark");
    if (!dark && a.at(0) != QStringLiteral("light")) {
      fail(line, QStringLiteral("expect-dialog-appearance light|dark")); return;
    }
    whenReady([this, dark] {
      QWidget* window = targetWindow();
      if (!window || !window->isVisible()) return false;
      const QPalette palette = window->palette();
      const QColor background = palette.color(QPalette::Window);
      const QColor text = palette.color(QPalette::WindowText);
      const double ratio = window->devicePixelRatioF();
      m_waitDetail = QStringLiteral("actual dialog palette window %1, text %2, device scale %3")
                         .arg(background.name(), text.name()).arg(ratio);
      const double minimumScale = qEnvironmentVariableIntValue("MITCAD_UI_REQUIRE_RETINA") == 1 ? 2 : 1;
      const bool matches = isDarkPalette(palette) == dark &&
          (dark ? text.lightness() > background.lightness() : text.lightness() < background.lightness()) &&
          ratio >= minimumScale;
      if (matches) qDebug().noquote() << "TestDriver:" << m_waitDetail;
      return matches;
    }, 6, QStringLiteral("dialog did not adopt the %1 appearance at the required scale").arg(a.at(0)));
  } else if (name == QLatin1String("expect-command-search")) {
    if (a.size() == 3 && a.at(2) != QStringLiteral("disabled") && a.at(2) != QStringLiteral("shortcuts")) {
      fail(line, QStringLiteral("expect-command-search <filter> <current-id|*|-> [disabled|shortcuts]")); return;
    }
    QWidget* popup = m_main ? m_main->findChild<QWidget*>(QStringLiteral("commandSearch")) : nullptr;
    auto* input = popup ? popup->findChild<QLineEdit*>() : nullptr;
    auto* list = popup ? popup->findChild<QTreeWidget*>() : nullptr;
    if (!popup || !popup->isVisible() || !input || !list || input->text() != a.at(0)) {
      fail(line, QStringLiteral("command search is not visible with filter '%1'").arg(a.at(0))); return;
    }
    const auto* selected = list->currentItem();
    const QString current = selected ? selected->data(0, Qt::UserRole).toString() : QStringLiteral("-");
    int enabled = 0;
    int disabled = 0;
    int shortcuts = 0;
    for (int row = 0; row < list->topLevelItemCount(); ++row) {
      const auto* item = list->topLevelItem(row);
      (item->flags() & Qt::ItemIsEnabled) ? ++enabled : ++disabled;
      const QString id = item->data(0, Qt::UserRole).toString();
      const auto* action = m_main->findChild<QAction*>(QStringLiteral("command_") + id);
      if (!action || item->text(1) != action->shortcut().toString(QKeySequence::NativeText)) {
        fail(line, QStringLiteral("search shortcut differs from the native shortcut for %1").arg(id)); return;
      }
      if (item->text(1).contains(QChar(0x2318))) ++shortcuts;
    }
    const bool needDisabled = a.size() == 3 && a.at(2) == QStringLiteral("disabled");
    const bool needShortcuts = a.size() == 3 && a.at(2) == QStringLiteral("shortcuts");
    if ((a.at(1) != QStringLiteral("*") && current != a.at(1)) ||
        (a.at(1) == QStringLiteral("*") && !selected) || (selected && !(selected->flags() & Qt::ItemIsEnabled)) ||
        (needDisabled && disabled == 0) || (needShortcuts && shortcuts == 0)) {
      fail(line, QStringLiteral("search current %1, enabled %2, disabled %3, Command glyphs %4")
                     .arg(current).arg(enabled).arg(disabled).arg(shortcuts));
    }
  } else if (name == QLatin1String("click-parameter") || name == QLatin1String("expect-parameter-cell")) {
    QWidget* window = targetWindow();
    auto* table = window ? window->findChild<QTreeWidget*>(QStringLiteral("parametersTable")) : nullptr;
    bool validColumn = false;
    const int column = a.at(1).toInt(&validColumn);
    if (!table || !validColumn || column < 0 || column >= table->columnCount()) {
      fail(line, QStringLiteral("parameter cell requires the parameter table and a valid column")); return;
    }
    QTreeWidgetItem* row = nullptr;
    for (QTreeWidgetItemIterator it(table); *it; ++it) {
      if ((*it)->data(0, Qt::UserRole).toString() == a.at(0)) { row = *it; break; }
    }
    if (name == QLatin1String("expect-parameter-cell")) {
      const QString state = a.at(3);
      if (state == QStringLiteral("absent") && !row) return;
      QRegularExpression re;
      if (!regex(a.at(2), re)) return;
      const bool visible = row && !row->isHidden();
      const auto* message = window->findChild<QLabel*>(QStringLiteral("parametersMessage"));
      const bool refused = row && !row->toolTip(column).isEmpty() && message && message->isVisible() &&
          message->text() == a.at(0) + QStringLiteral(": ") + row->toolTip(column);
      if (!row || !re.match(row->text(column)).hasMatch() ||
          (state == QStringLiteral("hidden") ? visible : !visible) ||
          (state == QStringLiteral("refused") && !refused) || state == QStringLiteral("absent") ||
          (state != QStringLiteral("visible") && state != QStringLiteral("hidden") && state != QStringLiteral("refused"))) {
        fail(line, QStringLiteral("parameter %1 column %2 did not match '%3' (%4)")
                       .arg(a.at(0)).arg(column).arg(a.at(2), state));
      }
      return;
    }
    if (!row || row->isHidden()) {
      fail(line, QStringLiteral("parameter %1 is not visible").arg(a.at(0))); return;
    }
    if (mac::test::active()) {
      const QString identifier = QStringLiteral("parameter.%1.%2").arg(a.at(0)).arg(column);
      whenReady([identifier] { return mac::test::controlExists(identifier); }, 6,
                QStringLiteral("no native parameter cell '%1'").arg(identifier), [this, identifier, line] {
        queue([this, identifier, line] {
          QString error;
          if (!mac::test::click(identifier, error)) { fail(line, error); }
        });
      });
      return;
    }
    const QRect rect = table->visualItemRect(row);
    const QPoint local(table->header()->sectionViewportPosition(column) + table->columnWidth(column) / 2,
                       rect.center().y());
    if (!table->viewport()->rect().contains(local)) {
      fail(line, QStringLiteral("parameter %1 cell is outside the visible table").arg(a.at(0))); return;
    }
    window->raise();
    // As sendKey does for shell-launched windows, establish Qt activation
    // synchronously before the click. A native activation request on every
    // cell can arrive after F2 has opened its editor and take its focus away.
    // Direct sendEvent does not supply platform click focus.
    if (QApplication::activeWindow() != window) {
      QT_WARNING_PUSH
      QT_WARNING_DISABLE_DEPRECATED
      QApplication::setActiveWindow(window);
      QT_WARNING_POP
    }
    table->setFocus(Qt::MouseFocusReason);
    const QPointer<QTreeWidget> guardedTable(table);
    whenReady([guardedTable] { return guardedTable && guardedTable->hasFocus() &&
                                     guardedTable->window()->isActiveWindow(); }, 6,
              QStringLiteral("parameter table did not receive mouse focus"), [this, guardedTable, local] {
      const QPointer<QWidget> viewport(guardedTable->viewport());
      for (const auto kind : {QEvent::MouseButtonPress, QEvent::MouseButtonRelease}) {
        queue([viewport, local, kind] {
          if (!viewport) return;
          const QPoint global = viewport->mapToGlobal(local);
          QMouseEvent event(kind, QPointF(local), QPointF(viewport->window()->mapFromGlobal(global)),
                            QPointF(global), Qt::LeftButton,
                            kind == QEvent::MouseButtonPress ? Qt::LeftButton : Qt::NoButton, Qt::NoModifier);
          QCoreApplication::sendEvent(viewport, &event);
        });
      }
    });
  } else if (name == QLatin1String("focus-main")) {
    m_dialog = nullptr;
    if (mac::test::active()) mac::test::focusMain();
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

    // --- A 3D mouse (mitcad#951): the input its backend gives
  } else if (name == QLatin1String("space-mouse")) {
    SpaceMouseAxes axes{};
    double milliseconds = 300;
    bool valid = true;
    for (int axis = 0; axis < 6 && valid; ++axis) {
      valid = toNumber(a.at(axis), axes[static_cast<std::size_t>(axis)]);
    }
    if (!valid || (a.size() > 6 && (!toNumber(a.at(6), milliseconds) || milliseconds < 0))) {
      fail(line, QStringLiteral("space-mouse <tx> <ty> <tz> <rx> <ry> <rz> [milliseconds]"));
      return;
    }
    auto* window = qobject_cast<MainWindow*>(m_main.data());
    if (window == nullptr || window->spaceMouse() == nullptr) {
      fail(line, QStringLiteral("space-mouse requires the main window"));
      return;
    }
    // Held for the time, then released, as a hand on the cap.
    window->spaceMouse()->motion(axes, false);
    const auto clock = std::make_shared<QElapsedTimer>();
    clock->start();
    whenReady([clock, milliseconds] { return clock->elapsed() >= milliseconds; }, milliseconds / 1000.0 + 5,
              QStringLiteral("space-mouse"), [this] {
                if (auto* owner = qobject_cast<MainWindow*>(m_main.data()); owner && owner->spaceMouse()) {
                  owner->spaceMouse()->motion(SpaceMouseAxes{}, false);
                }
              });
  } else if (name == QLatin1String("space-mouse-button")) {
    bool ok = false;
    const int number = a.at(0).toInt(&ok);
    auto* window = qobject_cast<MainWindow*>(m_main.data());
    if (!ok || number < 1 || number > 32 || window == nullptr || window->spaceMouse() == nullptr) {
      fail(line, QStringLiteral("space-mouse-button <1..32> requires the main window"));
      return;
    }
    window->spaceMouse()->button(number - 1, true);
    window->spaceMouse()->button(number - 1, false);

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
  } else if (name == QLatin1String("screenshot") || name == QLatin1String("screenshot-view") ||
             name == QLatin1String("screenshot-widget")) {
    // screenshot-widget <name> <png>: the main window's screenshot cut to
    // a widget, such as the timeline (timelineDock, mitcad#961).
    QWidget* crop = nullptr;
    if (name == QLatin1String("screenshot-widget")) {
      crop = namedWidget(mainWindow, a.at(0));
      if (crop == nullptr || !crop->isVisible()) {
        fail(line, QStringLiteral("no visible widget %1 to take a screenshot of").arg(a.at(0)));
        return;
      }
    }
    const QString path = a.at(crop != nullptr ? 1 : 0);
    if (!saveScreenshot(path, name == QLatin1String("screenshot-view"), crop)) {
      fail(line, QStringLiteral("could not save the screenshot %1").arg(path));
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
    if (mac::test::active() ? !mac::test::screenshot(a.at(0)) : (window == nullptr || !window->grab().save(a.at(0)))) {
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
