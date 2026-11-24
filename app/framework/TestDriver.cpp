// SPDX-License-Identifier: MIT
#include "TestDriver.hpp"
#include "Diagnostics.hpp"
#include "TestDriverSupport.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdlib>
#include <limits>
#include <memory>
#include <unordered_set>
#include <utility>
#include <vector>

#include <BRep_Tool.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Poly_Polygon3D.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_TShape.hxx>

#include <QAbstractButton>
#include <QAbstractItemView>
#include <QAction>
#include <QAbstractScrollArea>
#include <QApplication>
#include <QClipboard>
#include <QColor>
#include <QComboBox>
#include <QContextMenuEvent>
#include <QDir>
#include <QDockWidget>
#include <QEnterEvent>
#include <QEvent>
#include <QFile>
#include <QFileInfo>
#include <QFormLayout>
#include <QHash>
#include <QHeaderView>
#include <QIcon>
#include <QImage>
#include <QJsonDocument>
#include <QKeyEvent>
#include <QKeySequence>
#include <QLabel>
#include <QLayout>
#include <QLineEdit>
#include <QPlainTextEdit>
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
#include <QStyleHints>
#include <QThread>
#include <QTimer>
#include <QToolBar>
#include <QToolButton>
#include <QToolTip>
#include <QTreeWidget>
#include <QTreeWidgetItemIterator>
#include <QTextDocument>
#include <QWheelEvent>
#include <QWidget>
#include <QWindow>
#include <QtGlobal>
#include <QtLogging>

#include "../platform/MacChrome.hpp"
#include "../platform/MacTestApplication.hpp"
#include "../platform/NativeTestInput.hpp"
#include "../OcctViewer.hpp"
#include "../DocumentFrame.hpp"
#include "../MainWindow.hpp"
#include "../WindowManager.hpp"
#include "../update/UpdateController.hpp"
#include "../view/SpaceMouseController.hpp"
#include "../view/ViewController.hpp"
#include "../view/ViewEdges.hpp"
#include "../sketch/SketchController.hpp"
#include "../sketch/SketchOverlay.hpp"
#include "../sketch/SketchTool.hpp"
#include "../browser/TimelineWidget.hpp"
#include "GlassCard.hpp"
#include "Ribbon.hpp"
#include "TestSync.hpp"
#include "HangWatchdog.hpp"
#include "SessionLog.hpp"
#include "Dialogs.hpp"
#include "DisplayMeshes.hpp"
#include "Icons.hpp"
#include "ComputeProgress.hpp"
#include "Theme.hpp"
#include "TreeItemDelegate.hpp"
#include "mitcad/geometry/geometry.hpp"
#include "mitcad/io/mesh.hpp"

namespace mitcad {

using namespace testdriver;

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

const TestDriver::StepSpec kSpecs[] = {
    {"key", 1, -1, TestDriver::kInputOfTarget},
    {"key-now", 1, -1, TestDriver::kPlain},
    {"key-down", 1, 1, TestDriver::kInputOfTarget},
    {"key-up", 1, 1, TestDriver::kInputOfTarget},
    {"key-chord", 2, 5, TestDriver::kInputOfTarget},
    {"qt-type-now", 2, 2, TestDriver::kPlain},
    {"qt-choice", 2, 2, TestDriver::kInputOfTarget},
    {"type", 1, -1, TestDriver::kInputOfTarget},
    {"click", 2, 4, TestDriver::kInputOfMain},
    {"dblclick", 2, 4, TestDriver::kInputOfMain},
    {"click-now", 2, 4, TestDriver::kPlain},
    {"drag", 4, 6, TestDriver::kInputOfMain},
    {"wheel", 3, 4, TestDriver::kInputOfMain},
    {"click-logged", 1, 3, TestDriver::kInputOfMain},
    {"dblclick-logged", 1, 2, TestDriver::kInputOfMain},
    {"dialog-click-logged", 1, 1, TestDriver::kInputOfTarget},
    {"dialog-dblclick-logged", 1, 1, TestDriver::kInputOfTarget},
    // A drag in a dialog (mitcad#2011): the logged place and two offsets.
    {"dialog-drag-logged", 5, 5, TestDriver::kInputOfTarget},
    {"click-pick", 1, 2, TestDriver::kInputOfMain},
    {"drag-logged", 2, 3, TestDriver::kInputOfMain},
    {"drag-from", 3, 3, TestDriver::kInputOfMain},
    {"type-in", 2, 2, TestDriver::kInputOfMain},
    {"command", 1, 1, TestDriver::kInputOfMain},
    {"open-file", 1, 1, TestDriver::kInputOfMain},
    {"open-file-now", 1, 1, TestDriver::kPlain},
    {"file-dialog", 2, 4, TestDriver::kInputOfMain},
    {"menu", 1, 1, TestDriver::kInputOfMain},
    {"menu-now", 1, 1, TestDriver::kPlain},
    {"popup", 1, 1, TestDriver::kInputOfTarget},
    // A command's row of the open menu made the active one and its row
    // menu opened by the Menu key's Shift+F10 (the ribbon's group menus,
    // mitcad#1001, mitcad#1951): `row-menu <command id>`, whichever row the
    // pointer left over the menu had made active.
    {"row-menu", 1, 1, TestDriver::kInputOfTarget},
    {"button", 1, 2, TestDriver::kInputOfTarget},
    {"native-key-now", 1, -1, TestDriver::kPlain},
    {"native-key-event", 1, -1, TestDriver::kInputOfTarget},
    {"focus-dialog", 1, 2, TestDriver::kPlain},
    {"focus-main", 0, 0, TestDriver::kPlain},
    {"workspace", 1, 1, TestDriver::kIdle},
    {"show-workspace", 1, 1, TestDriver::kInputOfMain},
    {"leave-start-page", 0, 0, TestDriver::kInputOfMain},
    {"mark", 0, 0, TestDriver::kPlain},
    {"wait-log", 1, 2, TestDriver::kPlain},
    {"expect-log", 1, 2, TestDriver::kPlain},
    {"expect-no-log", 1, 1, TestDriver::kIdle},
    {"expect-ever", 1, 2, TestDriver::kPlain},
    {"expect-never", 1, 1, TestDriver::kIdle},
    {"capture", 2, 3, TestDriver::kPlain},
    {"set", 2, 2, TestDriver::kPlain},
    {"expect-near", 2, 4, TestDriver::kPlain},
    {"expect-less", 2, 2, TestDriver::kPlain},
    {"wait-idle", 0, 1, TestDriver::kPlain},
    {"sleep", 1, 1, TestDriver::kPlain},
    {"screenshot", 1, 1, TestDriver::kIdle},
    {"screenshot-view", 1, 1, TestDriver::kIdle},
    {"screenshot-screen", 1, 1, TestDriver::kIdle},
    {"screenshot-dialog", 1, 1, TestDriver::kIdle},
    {"screenshot-widget", 2, 2, TestDriver::kIdle},
    {"quit", 0, 1, TestDriver::kPlain},
    {"kill", 0, 0, TestDriver::kPlain},
    {"stay", 0, 0, TestDriver::kPlain},
    // The wheel over a logged place, a panel's value field (mitcad#1254).
    {"wheel-logged", 2, 3, TestDriver::kInputOfMain},
    // The wheel over a logged place without the model-idle gate: a panel's
    // value field while its preview computes (mitcad#1407).
    {"wheel-logged-now", 2, 3, TestDriver::kPlain},
};

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

} // namespace

namespace testdriver {

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

} // namespace testdriver

namespace {

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
      // Shift on its own: the view's selection scope widens (mitcad#217;
      // tools/ui-scripts/paste-busy-keys.mitcad-ui, mitcad#1360).
      {QStringLiteral("shift"), Qt::Key_Shift},
      // Ctrl (the Command key on macOS) on its own: held, it hides a
      // command's preview (mitcad#1421, key-down).
      {QStringLiteral("ctrl"), Qt::Key_Control},
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

} // namespace

namespace testdriver {

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

} // namespace testdriver

namespace {

// Where a click types into a widget, in its own coordinates: its middle,
// but in a line edit the middle of its text, left of its trailing buttons
// (the fx and the step buttons, mitcad#2327), as CommandPanel::placeOf
// logs a field's place: a narrow panel field's middle lies on its fx.
QPoint typingPoint(const QWidget* input) {
  QRect area = input->rect();
  if (qobject_cast<const QLineEdit*>(input) != nullptr) {
    int right = area.right();
    for (const QWidget* child : input->findChildren<QWidget*>(QString(), Qt::FindDirectChildrenOnly)) {
      // A trailing button: a tool button or the step buttons, off the
      // field's left edge (a leading one's place).
      const bool button = child->inherits("QToolButton") || child->objectName() == QStringLiteral("parameterSteps");
      if (button && child->isVisibleTo(input) && child->geometry().left() > 8) {
        right = std::min(right, child->geometry().left() - 1);
      }
    }
    if (right - area.left() >= 4) {
      area.setRight(right);
    }
  }
  return area.center();
}

// The popups as the popup stage sees them, for its timeout message: the
// active one and every menu Qt reports visible, with whether its window is
// really shown (a menu effect shows the menu's window only when it ends).
QString popupState() {
  const auto describe = [](const QWidget* widget) {
    if (widget == nullptr) return QStringLiteral("none");
    QString text = QStringLiteral("%1 '%2'").arg(QString::fromLatin1(widget->metaObject()->className()),
                                                  plainText(widget->windowTitle()));
    const QWindow* handle = widget->windowHandle();
    text += QStringLiteral(" window shown=%1").arg(handle != nullptr && handle->isVisible());
    if (const auto* menu = qobject_cast<const QMenu*>(widget)) {
      QStringList entries;
      for (const QAction* action : menu->actions()) {
        if (!action->isSeparator()) entries << plainText(action->text());
      }
      text += QStringLiteral(" [%1]").arg(entries.join(QStringLiteral(" | ")));
    }
    return text;
  };
  QStringList visible;
  for (QWidget* widget : QApplication::topLevelWidgets()) {
    if (widget->isVisible() && (qobject_cast<QMenu*>(widget) != nullptr || widget->windowType() == Qt::ToolTip)) {
      visible << describe(widget);
    }
  }
  return QStringLiteral("menu effects %1, fade %2; active popup: %3; visible: %4")
      .arg(QApplication::isEffectEnabled(Qt::UI_AnimateMenu))
      .arg(QApplication::isEffectEnabled(Qt::UI_FadeMenu))
      .arg(describe(QApplication::activePopupWidget()), visible.isEmpty() ? QStringLiteral("none")
                                                                       : visible.join(QStringLiteral("; ")));
}

} // namespace

namespace testdriver {

// Stacked workspaces can reuse object names. Resolve the visible instance
// inside the existing lookup scope, never a hidden page's first match.
// Property assertions also inspect disabled controls and, when there is no
// visible match, a unique hidden control (for example visible=false).
QWidget* namedWidget(QWidget* scope, const QString& name, bool forInput) {
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

// A command panel's row or note button by its logged place ("Panel <name>
// input <key> at x,y", "Panel <name> action <key>.<index> at x,y"),
// scrolled into the panel's view first (mitcad#1643: Send to Slicer's rows
// below the fold were logged where the scrolled panel no longer shows
// them), and its message ("Panel <name> message at x,y", mitcad#1877: a
// long Flange panel's refusal below the fold, hovered for its places) and
// a table's add button ("Panel <name> add <key> at x,y", mitcad#2082: the
// Docked top bar leaves the Command panel shorter, and the add buttons of
// Flange and Sheet Rules lay under its OK row): its middle now, in `top`'s
// coordinates; false when the line names no such widget or it is in view,
// and the logged place stands.
bool revealedPanelPlace(QWidget* main, QWidget* top, const QString& logged, QPoint& at) {
  static const QRegularExpression line(QStringLiteral("^Panel .* (input|action|add) (\\S+?)(?:[.](\\d+))? at "));
  static const QRegularExpression message(QStringLiteral("^Panel .* message at "));
  const QRegularExpressionMatch match = line.match(logged);
  const bool isMessage = !match.hasMatch() && message.match(logged).hasMatch();
  if ((!match.hasMatch() && !isMessage) || main == nullptr || top == nullptr) {
    return false;
  }
  const bool isAdd = !isMessage && match.captured(1) == QStringLiteral("add");
  QWidget* row = isMessage ? namedWidget(main, QStringLiteral("commandMessage"))
                 : isAdd   ? namedWidget(main, QStringLiteral("add_") + match.captured(2))
                           : namedWidget(main, QStringLiteral("input_") + match.captured(2));
  if (row == nullptr || !row->isVisible()) {
    return false;
  }
  QWidget* target = row;
  if (!isMessage && match.captured(1) == QStringLiteral("action")) {
    target = namedWidget(row, QStringLiteral("note_action_") + match.captured(3));
    if (target == nullptr || !target->isVisible()) {
      return false;
    }
  }
  // In view already: the logged place stands (it may be a part of the
  // row, its field rather than its label).
  if (target->visibleRegion().contains(target->rect().center())) {
    return false;
  }
  for (QWidget* parent = target->parentWidget(); parent; parent = parent->parentWidget()) {
    if (auto* scroll = qobject_cast<QScrollArea*>(parent)) {
      scroll->ensureWidgetVisible(target);
    }
  }
  at = top->mapFromGlobal(target->mapToGlobal(typingPoint(target)));
  return true;
}

} // namespace testdriver

// The step areas (mitcad#2173): each area file's specs, with the chain that
// runs them there (those of the files with a run function of their own run
// before, in run()).
const TestDriver::StepArea* TestDriver::areaOf(const QString& name) {
  static const StepArea* const areas[] = {
      &kDocumentWindowSteps,
      &kModelSteps,
      &kViewShapeSteps,
      &kViewPixelSteps,
      &kViewCameraSteps,
      &kNativeControlSteps,
      &kSketchSteps,
      &kPointerSteps,
      &kPickSteps,
      &kWidgetSteps,
      &kCommandSteps,
      &kEnvironmentSteps,
      &kFileSteps,
      &kNavigationSteps,
      &kAssistantSteps,
      &kFormulaSteps,
      &kManipulatorSteps,
      &kDialogSteps,
      &kMacControlSteps,
      &kChromeSteps,
      &kWindowFitSteps,
      &kDocumentFrameSteps,
      &kDockSteps,
      &kWalkSteps,
      &kOrbitCenterSteps,
      &kNativeWindowSteps,
  };
  for (const StepArea* area : areas) {
    for (std::size_t i = 0; i < area->count; ++i) {
      if (name == QLatin1String(area->specs[i].name)) {
        return area;
      }
    }
  }
  return nullptr;
}

const TestDriver::StepSpec* TestDriver::findSpec(const QString& name) {
  for (const StepSpec& spec : kSpecs) {
    if (name == QLatin1String(spec.name)) {
      return &spec;
    }
  }
  if (const StepArea* area = areaOf(name)) {
    for (std::size_t i = 0; i < area->count; ++i) {
      if (name == QLatin1String(area->specs[i].name)) {
        return &area->specs[i];
      }
    }
  }
  return nullptr;
}

// ---------------------------------------------------------------------------
// Start and script

TestDriver::TestDriver() {
  const QString script = qEnvironmentVariable("MITCAD_TEST_INPUT");
  if (script.isEmpty()) {
    return;
  }
  m_active = true;
  if (headlessUiTest()) {
    m_stepMs = 10;
    m_settleMs = 20;
  }
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

// A drawing sheet's point (mm) in window pixels, from "Drawing sheet view
// x0,y0 x1,y1 x2,y2" (the sheet points (0, 0), (100, 0) and (0, 100);
// mitcad#1841).
bool TestDriver::drawingSheetPoint(double x, double y, QPoint& out) const {
  // The unrounded origin and zoom, logged with the rounded points below: a
  // sheet shown small (a narrow window beside a wide panel) is too few
  // pixels per 100 mm for those to place a click within a pick's reach.
  static const QRegularExpression exact(
      QStringLiteral("^Drawing sheet origin ([-0-9.]+),([-0-9.]+) zoom ([-0-9.]+)"));
  QRegularExpressionMatch found;
  if (findLastLine(exact, 0, nullptr, &found) >= 0) {
    const double zoom = found.captured(3).toDouble();
    out = QPoint(qRound(found.captured(1).toDouble() + x * zoom), qRound(found.captured(2).toDouble() - y * zoom));
    return true;
  }
  static const QRegularExpression view(QStringLiteral(
      "^Drawing sheet view ([-0-9.]+),([-0-9.]+) ([-0-9.]+),([-0-9.]+) ([-0-9.]+),([-0-9.]+)"));
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
  // A message box's question identifies the document even when all its
  // windows share an application title. Native sheets match that text too.
  return box != nullptr &&
         (re.match(box->text()).hasMatch() || re.match(box->informativeText()).hasMatch());
}

bool TestDriver::findMain() {
  // Follow the same active document as the application's menus and MCP.
  // A MainWindow may be embedded in a document frame, so it need not be a
  // top-level widget. Cached controls always belong to the selected one.
  if (WindowManager* manager = WindowManager::instance()) {
    if (MainWindow* current = manager->activeWindow()) {
      if (m_main != current) {
        m_main = current;
        m_viewer.clear();
        m_dialog.clear();
        if (current->usesNativeUi() && mac::test::active()) mac::test::focusMain();
      }
      return true;
    }
  }
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

// The top-level window that shows the document: its document frame, with
// the frame's menu bar and tab bar above the MainWindow (mitcad#1172), or
// the MainWindow itself where it is a window. Logged places are in its
// coordinates (as the ribbon's are), and so are the points of the steps.
QWidget* TestDriver::topWindow() const {
  return m_main.isNull() ? nullptr : m_main->window();
}

QWidget* TestDriver::viewer() const {
  if (headlessUiTest()) return nullptr;
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
  out = view->mapTo(topWindow(), QPoint(qRound(view->width() * xPercent / 100.0),
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
  if (auto* window = qobject_cast<MainWindow*>(m_main.data())) {
    auto& session = window->documentSession();
    if (session.m_testHoldPreview->load() || session.m_testHoldEdit->load() || session.m_testHoldDisplay->load()) {
      session.cancelJob();
      session.m_testHoldPreview->store(false);
      session.m_testHoldEdit->store(false);
      session.m_testHoldDisplay->store(false);
    }
  }
  QObject::disconnect(m_jobPickConnection);
  m_jobPickLabel.clear();
  QObject::disconnect(m_nativeJobClickConnection);
  m_nativeJobClickLabel.clear();
  QObject::disconnect(m_nativeJobHistoryDragConnection);
  m_nativeJobHistoryDragLabel.clear();
  m_finished = true;
  m_left = true;
  m_exitStatus = code;
  if (m_timer != nullptr) {
    m_timer->stop();
  }
  // Closes no window (no closeEvent, no question about unsaved work); every
  // event loop that runs, a dialog's too, ends. Before the main event loop
  // runs, main() reads left() instead (mitcad#1933).
  QCoreApplication::exit(code);
}

bool TestDriver::verifyHeadlessCompletion() {
  if (headlessUiTest()) {
    auto* window = qobject_cast<MainWindow*>(m_main.data());
    if (QGuiApplication::platformName() != QStringLiteral("offscreen") || !window ||
        window->usesNativeUi() || window->viewer().context() != nullptr) {
      fail(m_line, QStringLiteral("headless workflow requires Qt offscreen without an OpenGL context"));
      return false;
    }
    QString platformError;
    if (!mac::headlessTestApplicationIsolated(platformError)) {
      fail(m_line, platformError);
      return false;
    }
    qInfo("Headless UI test verified: Qt offscreen without an OpenGL context");
  }
  return true;
}

void TestDriver::finish() {
  if (!verifyHeadlessCompletion()) {
    return;
  }
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
      fail(m_line, QStringLiteral("the preview worker gate was not released"));
      return;
    }
    if (window->documentSession().m_testHoldEdit->load()) {
      fail(m_line, QStringLiteral("the feature edit worker gate was not released"));
      return;
    }
    if (window->documentSession().m_testHoldDisplay->load()) {
      fail(m_line, QStringLiteral("the display worker gate was not released"));
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
  QWidget* window = nullptr;
  QWidget* receiver = keyReceiver(window);
  if (receiver == nullptr) {
    return;
  }
  const bool popup = QApplication::activePopupWidget() != nullptr;
  const Qt::KeyboardModifiers modifiers = press.modifiers;
  // A key is a shortcut unless the focused widget takes it (a line edit
  // asks to keep the letters): what the platform does before it delivers
  // a key press.
  QKeyEvent offer(QEvent::ShortcutOverride, press.key, modifiers, press.text);
  offer.ignore();
  QCoreApplication::sendEvent(receiver, &offer);
  if (!popup && !offer.isAccepted() && triggerShortcut(window, press)) {
    return;
  }
  m_sequenceKeys.clear();
  QKeyEvent down(QEvent::KeyPress, press.key, modifiers, press.text);
  QCoreApplication::sendEvent(receiver, &down);
  QKeyEvent up(QEvent::KeyRelease, press.key, modifiers, press.text);
  QCoreApplication::sendEvent(receiver, &up);
}

// A key held or let go on its own (key-down, key-up, mitcad#1421): its
// shortcut offer and its press, or its release, for the widget with the
// focus; no shortcut is triggered (a modifier alone makes none).
void TestDriver::sendKeyHalf(const KeyPress& press, bool down) {
  QWidget* window = nullptr;
  QWidget* receiver = keyReceiver(window);
  if (receiver == nullptr) {
    return;
  }
  if (down) {
    QKeyEvent offer(QEvent::ShortcutOverride, press.key, press.modifiers, press.text);
    offer.ignore();
    QCoreApplication::sendEvent(receiver, &offer);
    QKeyEvent event(QEvent::KeyPress, press.key, press.modifiers, press.text);
    QCoreApplication::sendEvent(receiver, &event);
  } else {
    QKeyEvent event(QEvent::KeyRelease, press.key, press.modifiers, press.text);
    QCoreApplication::sendEvent(receiver, &event);
  }
}

QWidget* TestDriver::keyReceiver(QWidget*& window) {
  window = targetWindow();
  if (window == nullptr) {
    return nullptr;
  }
  // The document's MainWindow sits in a document frame (mitcad#1172): the
  // frame is the window the system activates and that hosts the panels.
  QWidget* host = window->window();
  QWidget* popup = QApplication::activePopupWidget();
  QWidget* active = QApplication::activeWindow();
  if (popup == nullptr && active != host && (active == nullptr || GlassCard::hostWindow(active) != host)) {
    // A window the system did not activate (a session without a user at the
    // console; macOS does not let a process started from a shell take the
    // focus) has no focused widget for Qt: QWidget::hasFocus() is false, and
    // the application's code that checks it (the sketch's value field that
    // selects its live text, so that typing replaces it) acts differently
    // from a window the user sees. Qt's own activation, as QTest does it.
    qWarning().noquote() << QStringLiteral("TestDriver: note: '%1' is not the active window; activated by Qt")
                                .arg(host->windowTitle());
    // QApplication::setActiveWindow is deprecated in favour of
    // QWidget::activateWindow, which asks the system and is what findMain
    // tried; only the former sets Qt's own state without it.
    QT_WARNING_PUSH
    QT_WARNING_DISABLE_DEPRECATED
    QApplication::setActiveWindow(host);
    QT_WARNING_POP
  }
  QWidget* receiver = popup != nullptr ? popup : window;
  QWidget* focused = QApplication::focusWidget();
  if (popup == nullptr && focused != nullptr && GlassCard::hostWindow(focused) == host) {
    receiver = focused; // in the main window or in one of its panels' windows
  } else if (receiver->focusWidget() != nullptr) {
    receiver = receiver->focusWidget();
  }
  return receiver;
}

// With native glass the floating panels are windows of their own: the one
// that has the position (in the main window's coordinates), if any. A card
// that lets the mouse through (the status pill) has none.
GlassCard* TestDriver::cardAt(const QPoint& windowPos) const {
  QWidget* top = topWindow();
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
  const QKeyCombination key(modifiers, static_cast<Qt::Key>(press.key));
  // A sequence begun in another window does not go on in this one.
  if (!m_sequenceKeys.isEmpty() && m_sequenceWindow.data() != window) {
    dropSequence(QStringLiteral("another window"));
  }
  QList<QWidget*> widgets = window->findChildren<QWidget*>();
  widgets.prepend(window);
  // As Qt's shortcut map: the keys so far and this one; a key that
  // continues no sequence is tried again on its own.
  for (int attempt = 0; attempt < 2; ++attempt) {
    QList<QKeyCombination> keys = m_sequenceKeys;
    keys.append(key);
    while (keys.size() > 4) keys.removeFirst();
    const QKeySequence wanted(keys.value(0), keys.value(1, QKeyCombination::fromCombined(0)),
                              keys.value(2, QKeyCombination::fromCombined(0)),
                              keys.value(3, QKeyCombination::fromCombined(0)));
    bool partial = false;
    for (QWidget* widget : std::as_const(widgets)) {
      const QList<QAction*> actions = widget->actions();
      for (QAction* action : actions) {
        if (!action->isEnabled() || !action->isVisible() || action->shortcutContext() == Qt::WidgetShortcut ||
            action->shortcutContext() == Qt::WidgetWithChildrenShortcut) {
          continue;
        }
        const QList<QKeySequence> shortcuts = action->shortcuts();
        if (shortcuts.contains(wanted)) {
          m_sequenceKeys.clear();
          action->trigger();
          return true;
        }
        for (const QKeySequence& shortcut : shortcuts) {
          partial = partial || wanted.matches(shortcut) == QKeySequence::PartialMatch;
        }
      }
    }
    const QList<QShortcut*> shortcuts = window->findChildren<QShortcut*>();
    for (QShortcut* shortcut : shortcuts) {
      if (shortcut->isEnabled() && shortcut->context() != Qt::WidgetShortcut &&
          shortcut->context() != Qt::WidgetWithChildrenShortcut && shortcut->key() == wanted) {
        m_sequenceKeys.clear();
        emit shortcut->activated();
        return true;
      }
    }
    if (keys.size() == 1) {
      const QList<QAbstractButton*> buttons = window->findChildren<QAbstractButton*>();
      for (QAbstractButton* button : buttons) {
        if (button->isEnabled() && button->isVisibleTo(window) && !button->shortcut().isEmpty() &&
            button->shortcut() == wanted) {
          m_sequenceKeys.clear();
          button->click();
          return true;
        }
      }
    }
    if (partial) {
      // The start of a sequence: Qt keeps the key and waits for the next.
      m_sequenceKeys = keys;
      m_sequenceWindow = window;
      qDebug().noquote() << QStringLiteral("TestDriver: key sequence so far: %1")
                                .arg(wanted.toString(QKeySequence::PortableText));
      return true;
    }
    const bool started = !m_sequenceKeys.isEmpty();
    m_sequenceKeys.clear();
    if (!started) {
      break;
    }
  }
#ifdef Q_OS_MACOS
  // AppKit dispatches the global menu's Close even when a Qt tool window is key.
  if (window != m_main.data() && m_main) {
    if (auto* close = m_main->findChild<QAction*>(QStringLiteral("command_file.close"));
        close && close->isEnabled() && close->shortcuts().contains(QKeySequence(key))) {
      close->trigger();
      return true;
    }
  }
#endif
  return false;
}

void TestDriver::dropSequence(const QString& why) {
  if (m_sequenceKeys.isEmpty()) return;
  const QKeySequence started(m_sequenceKeys.value(0), m_sequenceKeys.value(1, QKeyCombination::fromCombined(0)),
                             m_sequenceKeys.value(2, QKeyCombination::fromCombined(0)),
                             m_sequenceKeys.value(3, QKeyCombination::fromCombined(0)));
  qDebug().noquote() << QStringLiteral("TestDriver: key sequence dropped: %1 (%2)")
                            .arg(started.toString(QKeySequence::PortableText), why);
  m_sequenceKeys.clear();
  m_sequenceWindow.clear();
}

void TestDriver::sendText(const QString& text) {
  // Typed text is text, not the rest of a started sequence (mitcad#2096).
  dropSequence(QStringLiteral("typed text"));
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
  QWidget* top = topWindow();
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
  // Posted input has no system cursor transition. A move from the view
  // to a floating card must leave the old receiver and its ancestors, as
  // real pointer input does: otherwise a drawing tool keeps snapping to
  // the place the pointer left while values are typed in the card.
  if (kind == QEvent::MouseMove && buttons == Qt::NoButton && !m_grab) {
    const QPointer<QWidget> nextTarget(target);
    const QPointer<QWidget> nextWindow(window);
    QPointer<QWidget> previous = m_pointerTarget;
    m_pointerTarget = nextTarget;
    while (previous && nextTarget && previous != nextTarget && !previous->isAncestorOf(nextTarget)) {
      const QPointer<QWidget> parent(previous->parentWidget());
      QEvent leave(QEvent::Leave);
      QCoreApplication::sendEvent(previous, &leave);
      previous = parent;
    }
    if (!nextTarget || !nextWindow) {
      return;
    }
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
  const QPoint global = topWindow()->mapToGlobal(at);
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

void TestDriver::queueDialogDrag(const QPoint& from, const QPoint& to) {
  const QPointer<QWidget> window(targetWindow());
  if (!window || window == m_main) {
    fail(m_line, QStringLiteral("dialog pointer input requires focus-dialog"));
    return;
  }
  const QPoint start = topWindow()->mapToGlobal(from);
  const QPoint end = topWindow()->mapToGlobal(to);
  const QPointer<QWidget> receiver(window->childAt(window->mapFromGlobal(start)));
  if (!receiver || !receiver->isVisible()) {
    fail(m_line, QStringLiteral("logged dialog point is outside its visible controls"));
    return;
  }
  const auto send = [window, receiver](QEvent::Type type, const QPoint& global, Qt::MouseButton button,
                                       Qt::MouseButtons buttons) {
    if (!window || !receiver) return;
    QMouseEvent event(type, receiver->mapFromGlobal(global), window->mapFromGlobal(global), global, button, buttons,
                      Qt::NoModifier);
    QCoreApplication::sendEvent(receiver, &event);
  };
  queue([send, start] { send(QEvent::MouseMove, start, Qt::NoButton, Qt::NoButton); });
  queue([send, start] { send(QEvent::MouseButtonPress, start, Qt::LeftButton, Qt::LeftButton); });
  for (int i = 1; i <= kDragSteps; ++i) {
    const QPoint at(start.x() + (end.x() - start.x()) * i / kDragSteps,
                    start.y() + (end.y() - start.y()) * i / kDragSteps);
    queue([send, at] { send(QEvent::MouseMove, at, Qt::NoButton, Qt::LeftButton); });
  }
  queue([] {}); // a pause before the button goes up
  queue([send, end] { send(QEvent::MouseButtonRelease, end, Qt::LeftButton, Qt::NoButton); });
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

void TestDriver::queueHoldRelease(const QPoint& at, int since, qint64 limitMs, std::shared_ptr<QElapsedTimer> held) {
  queue([this, at, since, limitMs, held] {
    // Held until the view opened its list (or long enough that it would
    // have), then let go where it was pressed.
    whenReady(
        [this, since, limitMs, held] {
          static const QRegularExpression opened(QStringLiteral("^Pick list at "));
          return findLine(opened, since) >= 0 || held->elapsed() >= limitMs;
        },
        static_cast<double>(limitMs) / 1000.0 + 3.0, QStringLiteral("the held press was not let go"),
        [this, at] { sendMouse(QEvent::MouseButtonRelease, at, Qt::LeftButton, Qt::NoButton, Qt::NoModifier); });
  });
}

void TestDriver::sendWheel(const QPoint& windowPos, int notches, Qt::KeyboardModifiers modifiers) {
  QWidget* top = topWindow();
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
  // A wheel the widget under the pointer leaves goes on to its parents, as
  // the user's does: Qt passes it on only for the system's own events, not
  // for one sent, so the driver does it here. (The 3D Sketch Palette's
  // buttons leave it to the card's scroll area, which scrolls; sent to the
  // button alone it scrolled nothing, mitcad#1652.)
  for (QWidget* w = target; w != nullptr; w = w->parentWidget()) {
    QWheelEvent event(QPointF(w->mapFromGlobal(global)), QPointF(global), QPoint(), QPoint(0, 120 * notches),
                      Qt::NoButton, modifiers, Qt::NoScrollPhase, false);
    event.ignore();
    QCoreApplication::sendEvent(w, &event);
    if (event.isAccepted() || w->isWindow() || w->testAttribute(Qt::WA_NoMousePropagation)) {
      break;
    }
  }
}

bool TestDriver::saveScreenshot(const QString& path, bool viewOnly, QWidget* crop) {
  QDir().mkpath(QFileInfo(path).absolutePath());
  QOpenGLWidget* gl = qobject_cast<QOpenGLWidget*>(viewer());
  if (viewOnly) {
    // What --screenshot saves.
    return gl != nullptr && gl->grabFramebuffer().save(path);
  }
  if (mac::test::active()) return crop == nullptr && mac::test::screenshot(path, gl);
  QWidget* top = m_main ? m_main->window() : nullptr;
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
    // An open popup menu (the view's context menu, mitcad#1713's review
    // shots): a window of its own over this one, drawn at its place.
    if (QWidget* popup = QApplication::activePopupWidget(); popup != nullptr && popup->isVisible()) {
      painter.drawPixmap(top->mapFromGlobal(popup->geometry().topLeft()), popup->grab());
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

bool TestDriver::saveHeadlessScreen(const QString& path) const {
  // The visible top-level Qt windows at their places on the offscreen
  // screen: the main window first, then tool windows and dialogs, popups
  // last, as the window server would stack them.
  QList<QWidget*> windows;
  for (QWidget* widget : QApplication::topLevelWidgets()) {
    if (widget->isVisible() && !widget->size().isEmpty()) windows << widget;
  }
  const auto rank = [this](const QWidget* widget) {
    if (widget == m_main.data()) return 0;
    if (widget->windowType() == Qt::Popup || widget->windowType() == Qt::ToolTip) return 2;
    return 1;
  };
  std::stable_sort(windows.begin(), windows.end(),
                   [&rank](const QWidget* a, const QWidget* b) { return rank(a) < rank(b); });
  QRect bounds;
  for (const QWidget* window : windows) bounds |= window->frameGeometry();
  if (windows.isEmpty() || bounds.isEmpty()) return false;
  QImage image(bounds.size(), QImage::Format_ARGB32_Premultiplied);
  image.fill(Qt::transparent);
  QPainter painter(&image);
  for (QWidget* window : windows) {
    painter.drawPixmap(window->geometry().topLeft() - bounds.topLeft(), window->grab());
  }
  painter.end();
  qDebug().noquote() << QStringLiteral("TestDriver: headless screen capture of %1 Qt windows, %2x%3")
                            .arg(windows.size()).arg(bounds.width()).arg(bounds.height());
  return image.save(path);
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
  if (runDocumentFrameStep(name, a, line)) {
    return;
  }
  // The Docked layout's docks (mitcad#1455).
  if (runDockStep(name, a, line)) {
    return;
  }
  // The interface walked (mitcad#1473).
  if (runWalkStep(name, a, line)) {
    return;
  }
  // The orbit's center (mitcad#1791).
  if (runOrbitCenterStep(name, a, line)) {
    return;
  }
  // The design window's native windows (mitcad#2152).
  if (runNativeWindowStep(name, a, line)) {
    return;
  }

  // A step of an area file (mitcad#2173): its chain there.
  if (const StepArea* area = areaOf(name); area != nullptr && area->run != nullptr) {
    (this->*(area->run))(name, a, line);
    return;
  }

  // --- Keys and text
  bool stepMatched = true;
  if (name == QLatin1String("qt-choice")) {
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
    // The live state of the persistent row in the real popup. Under load
    // (several UI tests at once) a popup can be visible to Qt while it is
    // still being shown or laying out its rows, so the opening and each
    // pointer phase wait for this state instead of failing on one look.
    // The window's exposure is reported only: a Windows service session
    // (session 0) never reports the popup's window as exposed.
    struct LiveRow {
      QRect rect;
      QRect exposed;
      QPoint point;
    };
    const auto liveRow = [this, list, index](const QString& phase, bool scroll, LiveRow& live) {
      if (!list || !index.isValid()) {
        m_waitDetail = QStringLiteral("%1: the combo's popup row was destroyed").arg(phase);
        return false;
      }
      if (!list->isVisible()) {
        m_waitDetail = QStringLiteral("%1: the combo's item view is not a visible popup").arg(phase);
        return false;
      }
      if (scroll) list->scrollTo(index, QAbstractItemView::PositionAtCenter);
      list->doItemsLayout();
      QWidget* viewport = list->viewport();
      const QWindow* handle = list->window()->windowHandle();
      const bool windowExposed = handle != nullptr && handle->isExposed();
      const bool activePopup = QApplication::activePopupWidget() == list->window();
      live.rect = list->visualRect(index);
      // A macOS menu-style row can include trailing layout space beyond
      // its narrow viewport while its label is fully shown. Hit the real
      // exposed row, not the center of that non-visible layout space.
      live.exposed = live.rect.intersected(viewport->rect());
      live.point = live.exposed.center();
      QWidget* hit = QApplication::widgetAt(viewport->mapToGlobal(live.point));
      const bool indexHit = list->indexAt(live.point) == index;
      const bool widgetHit = hit == viewport || (hit && viewport->isAncestorOf(hit));
      m_waitDetail = QStringLiteral("%1: popup row %2 at %3,%4, viewport %5x%6, exposed %7x%8, window exposed=%9, "
                                    "active popup=%10, index-hit=%11, widget-hit=%12")
          .arg(phase).arg(index.row()).arg(live.point.x()).arg(live.point.y())
          .arg(viewport->width()).arg(viewport->height()).arg(live.exposed.width()).arg(live.exposed.height())
          .arg(windowExposed).arg(activePopup).arg(indexHit).arg(widgetHit);
      return !live.exposed.isEmpty() && indexHit && activePopup && widgetHit;
    };
    // One pointer phase: wait within the step's limit until the row is live
    // again, then send the event from that same look. A press can change
    // the motion type, shrink the floating card and move its popup before
    // the release, so an earlier phase's screen point is never reused.
    const auto sendWhenLive = [this, combo, list, index, identifier, label, rowInputSent, line, liveRow]
        (QEvent::Type type, const std::function<void()>& beforeSend) {
      const QString phase = type == QEvent::MouseMove ? QStringLiteral("move") : type == QEvent::MouseButtonPress
          ? QStringLiteral("press") : QStringLiteral("release");
      const auto live = std::make_shared<LiveRow>();
      whenReady([combo, list, index, liveRow, phase, live] {
        // A destroyed combo or row fails at once below, without waiting.
        return !combo || !list || !index.isValid() || liveRow(phase, false, *live);
      }, 6, QStringLiteral("Qt choice '%1' in '%2' has no exposed popup row").arg(label, identifier),
      [this, combo, list, index, identifier, label, rowInputSent, line, type, beforeSend, phase, live] {
        if (!combo || !list || !index.isValid()) {
          fail(line, QStringLiteral("Qt choice '%1' in '%2' lost its live popup row").arg(label, identifier)); return;
        }
        const QPointer<QWidget> receiver(list->viewport());
        const QPoint global = receiver->mapToGlobal(live->point);
        QWidget* popup = receiver->window();
        qInfo().noquote() << QStringLiteral("Qt choice pointer: %1 phase=%2 row=%3 rect=%4,%5 %6x%7 global=%8,%9 popup-visible=%10 current='%11' exposed=%12,%13 %14x%15 index-hit=1 widget-hit=1")
            .arg(identifier, phase).arg(index.row()).arg(live->rect.x()).arg(live->rect.y()).arg(live->rect.width())
            .arg(live->rect.height()).arg(global.x()).arg(global.y()).arg(popup->isVisible()).arg(combo->currentText())
            .arg(live->exposed.x()).arg(live->exposed.y()).arg(live->exposed.width()).arg(live->exposed.height());
        QMouseEvent event(type, receiver->mapFromGlobal(global), popup->mapFromGlobal(global), global,
                          type == QEvent::MouseMove ? Qt::NoButton : Qt::LeftButton,
                          type == QEvent::MouseButtonPress ? Qt::LeftButton : Qt::NoButton, Qt::NoModifier);
        // Mark only after the release passed every live row/hit guard, just
        // before dispatch: its handler may open a dialog in a nested loop.
        if (type == QEvent::MouseButtonRelease) *rowInputSent = true;
        if (beforeSend) beforeSend();
        QCoreApplication::sendEvent(receiver, &event);
        if (type == QEvent::MouseButtonRelease) {
          qInfo().noquote() << QStringLiteral("Qt choice: clicked '%1' in '%2' popup").arg(label, identifier);
        }
      });
    };
    // Install the wait before opening: a native style may track its popup
    // in a nested loop. A key sent to the main window must never confirm
    // the command when its NoFocus combo did not receive that key.
    const auto opened = std::make_shared<LiveRow>();
    whenReady([nativeSelected, liveRow, opened] {
      return *nativeSelected || liveRow(QStringLiteral("open"), true, *opened);
    }, 6, QStringLiteral("the real Qt choice popup did not display '%1'").arg(label),
    [this, identifier, label, nativeSelected, waitClosed, sendWhenLive] {
      if (*nativeSelected) {
        waitClosed();
        qInfo().noquote() << QStringLiteral("Qt choice: selected '%1' in '%2' native popup").arg(label, identifier);
        return;
      }
      // Each phase starts in its own driver tick; the next phase is queued
      // just before this one's event. The close wait is installed before
      // the release is dispatched, as that release may run a nested loop.
      queue([this, waitClosed, sendWhenLive] {
        sendWhenLive(QEvent::MouseMove, [this, waitClosed, sendWhenLive] {
          queue([this, waitClosed, sendWhenLive] {
            sendWhenLive(QEvent::MouseButtonPress, [this, waitClosed, sendWhenLive] {
              queue([waitClosed, sendWhenLive] { sendWhenLive(QEvent::MouseButtonRelease, waitClosed); });
            });
          });
        });
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

  // --- A key held on its own, and a chord with it (mitcad#1421: Ctrl held
  // alone hides a command's preview; Ctrl+Z and Ctrl+S show it again)
  if (name == QLatin1String("key-down") || name == QLatin1String("key-up") || name == QLatin1String("key-chord")) {
    KeyPress held;
    QString error;
    if (!parseKeySpec(a.at(0), held.key, held.modifiers, held.text, error)) {
      fail(line, error);
      return;
    }
    // A modifier key carries its own modifier while it is down, as the
    // platforms deliver it, and none once it is released.
    Qt::KeyboardModifiers own;
    if (held.key == Qt::Key_Control) {
      own = Qt::ControlModifier;
    } else if (held.key == Qt::Key_Shift) {
      own = Qt::ShiftModifier;
    } else if (held.key == Qt::Key_Alt) {
      own = Qt::AltModifier;
    } else if (held.key == Qt::Key_Meta) {
      own = Qt::MetaModifier;
    }
    KeyPress down = held;
    down.modifiers |= own;
    KeyPress up = held;
    up.modifiers &= ~own;
    if (name == QLatin1String("key-down")) {
      queue([this, down] { sendKeyHalf(down, true); });
      return;
    }
    if (name == QLatin1String("key-up")) {
      queue([this, up] { sendKeyHalf(up, false); });
      return;
    }
    if (a.at(1) == QLatin1String("wheel")) {
      double x = 0;
      double y = 0;
      double notches = 0;
      if (a.size() != 5 || !toNumber(a.at(2), x) || !toNumber(a.at(3), y) || !toNumber(a.at(4), notches)) {
        fail(line, QStringLiteral("key-chord <key> wheel <x%> <y%> <notches>"));
        return;
      }
      QPoint at;
      if (!viewPoint(x, y, at)) {
        fail(line, QStringLiteral("no 3D view"));
        return;
      }
      // One turn of the event loop: no timer runs between them.
      queue([this, down, up, at, notches] {
        sendKeyHalf(down, true);
        sendMouse(QEvent::MouseMove, at, Qt::NoButton, Qt::NoButton, down.modifiers);
        sendWheel(at, qRound(notches), down.modifiers);
        sendKeyHalf(up, false);
      });
      return;
    }
    KeyPress second;
    if (a.size() != 2 || !parseKeySpec(a.at(1), second.key, second.modifiers, second.text, error)) {
      fail(line, error.isEmpty() ? QStringLiteral("key-chord <key> <key2> | <key> wheel <x%> <y%> <notches>") : error);
      return;
    }
    second.modifiers |= down.modifiers;
    if (second.modifiers & (Qt::ControlModifier | Qt::AltModifier | Qt::MetaModifier)) {
      second.text.clear(); // a shortcut's key, no text
    }
    queue([this, down, up, second] {
      sendKeyHalf(down, true);
      sendKey(second);
      sendKeyHalf(up, false);
    });
    return;
  }

  // --- The mouse in the 3D view
  if (name == QLatin1String("click") || name == QLatin1String("dblclick") || name == QLatin1String("click-now")) {
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
    queueClick(at, button, modifiers, name == QLatin1String("dblclick") ? 2 : 1);
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
  } else if (name == QLatin1String("wheel-logged") || name == QLatin1String("wheel-logged-now")) {
    // Over a logged place (a panel's value field, mitcad#1254): the widget
    // there gets the wheel, as the one under the mouse does. The -now form
    // does not wait for the model (its step kind): the wheel over a field
    // while the panel's preview computes (mitcad#1407).
    QRegularExpression re;
    if (!regex(a.at(0), re)) {
      return;
    }
    double notches = 0;
    Qt::KeyboardModifiers modifiers;
    if (!toNumber(a.at(1), notches) || (a.size() > 2 && !parseModifiers(a.at(2), modifiers))) {
      fail(line, QStringLiteral("%1 <regex> <notches> [mods]").arg(name));
      return;
    }
    const auto point = std::make_shared<QPoint>();
    whenReady([this, re, point] { return loggedPoint(re, 0, *point); }, 6,
              QStringLiteral("no place logged for '%1'").arg(a.at(0)), [this, point, notches, modifiers] {
      sendMouse(QEvent::MouseMove, *point, Qt::NoButton, Qt::NoButton, Qt::NoModifier);
      sendWheel(*point, qRound(notches), modifiers);
    }, mainWindow);
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
            auto* window = qobject_cast<MainWindow*>(m_main.data());
            if (window == nullptr || !window->refreshLoggedPickPlaces()) {
              return false;
            }
            static const QRegularExpression places(QStringLiteral("^Pick places:"));
            from = std::max(findLastLine(places, 0), 0);
          }
          // A panel's row or note button scrolled into view first.
          QString logged;
          if (!pick && findLastLine(re, from, &logged) >= 0 &&
              revealedPanelPlace(m_main, topWindow(), logged, *at)) {
            return true;
          }
          return loggedPoint(re, from, *at);
        },
        6, QStringLiteral("no place logged for '%1'").arg(a.at(0)),
        [this, at, button, modifiers, clicks, dialog] {
          if (dialog) queueDialogClick(*at, clicks);
          else queueClick(*at, button, modifiers, clicks);
        }, dialog ? targetWindow() : mainWindow);
  } else if (name == QLatin1String("dialog-drag-logged")) {
    QRegularExpression re;
    if (!regex(a.at(0), re)) {
      return;
    }
    double d[4] = {0, 0, 0, 0};
    for (int i = 0; i < 4; ++i) {
      if (!toNumber(a.at(i + 1), d[i])) {
        fail(line, QStringLiteral("dialog-drag-logged <regex> <dx1> <dy1> <dx2> <dy2>"));
        return;
      }
    }
    const auto at = std::make_shared<QPoint>();
    whenReady([this, re, at] { return loggedPoint(re, 0, *at); }, 6,
              QStringLiteral("no place logged for '%1'").arg(a.at(0)),
              [this, at, d0 = d[0], d1 = d[1], d2 = d[2], d3 = d[3]] {
                queueDialogDrag(*at + QPoint(qRound(d0), qRound(d1)), *at + QPoint(qRound(d2), qRound(d3)));
              },
              targetWindow());
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
    const auto editor = std::make_shared<QPointer<QWidget>>();
    whenReady(
        [this, re, at, editor] {
          QString logged;
          if (findLastLine(re, 0, &logged) < 0) { return false; }
          const auto match = QRegularExpression(QStringLiteral(" input (\\S+) at ")).match(logged);
          if (match.hasMatch() && m_main) {
            const QString name = QStringLiteral("input_") + match.captured(1);
            QWidget* input = m_main->findChild<QLineEdit*>(name);
            if (input == nullptr) {
              // A text box of several lines (mitcad#2029).
              input = m_main->findChild<QPlainTextEdit*>(name);
            }
            if (input == nullptr) {
              // A field with a slider before it: the name is the row's (the
              // slider and the field), the editor inside it.
              if (auto* both = m_main->findChild<QWidget*>(name)) { input = both->findChild<QLineEdit*>(); }
            }
            if (input && input->isVisible()) {
              // Validation hints can move a field after its logged position.
              // Reveal the actual editor and use its current position.
              for (QWidget* parent = input->parentWidget(); parent; parent = parent->parentWidget()) {
                if (auto* scroll = qobject_cast<QScrollArea*>(parent)) { scroll->ensureWidgetVisible(input); }
              }
              *at = topWindow()->mapFromGlobal(input->mapToGlobal(typingPoint(input)));
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
  } else if (name == QLatin1String("row-menu")) {
    const QString id = a.at(0);
    const auto found = std::make_shared<QPointer<QMenu>>();
    whenReady(
        [this, id, found] {
          auto* menu = qobject_cast<QMenu*>(QApplication::activePopupWidget());
          m_waitDetail = popupState();
          if (menu == nullptr || !menu->isVisible()) {
            return false;
          }
          for (QAction* action : menu->actions()) {
            if (action->objectName() == QStringLiteral("command_") + id && action->isVisible()) {
              *found = menu;
              return true;
            }
          }
          m_waitDetail = QStringLiteral("%1; no row of %2 in it").arg(m_waitDetail, id);
          return false;
        },
        6, QStringLiteral("no row of %1 in an open menu").arg(id),
        [this, id, found, line] {
          auto* ribbon = m_main->findChild<Ribbon*>();
          if (ribbon == nullptr || !ribbon->openRowMenu(found->data(), id)) {
            fail(line, QStringLiteral("no row menu of %1: the open menu is no group menu with its row").arg(id));
            return;
          }
          qDebug().noquote() << QStringLiteral("TestDriver: row menu of %1").arg(id);
        });
  } else if (name == QLatin1String("popup")) {
    const QString entry = a.at(0);
    // "Sub/Item" opens the submenu Sub and chooses Item in it (Material ▸
    // More…, mitcad#1046); a whole entry with a slash still matches first.
    const auto rest = std::make_shared<QString>();
    // The rest of "Sub/Item" is chosen in the submenu Sub opened, not in
    // Qt's active popup: a focus event the platform delivers late (under
    // load) makes the parent menu Qt's active popup again while Sub stays
    // open over it (mitcad#1443), and the entry was then never found.
    const bool inSubmenu = std::exchange(m_inSubmenu, false);
    const QPointer<QWidget> submenu = std::exchange(m_submenu, nullptr);
    const auto found = std::make_shared<QPointer<QMenu>>();
    whenReady(
        [this, entry, rest, inSubmenu, submenu, found] {
          QMenu* menu = inSubmenu ? qobject_cast<QMenu*>(submenu.data())
                                  : qobject_cast<QMenu*>(QApplication::activePopupWidget());
          m_waitDetail = popupState();
          if (menu == nullptr || !menu->isVisible()) {
            return false;
          }
          *found = menu;
          rest->clear();
          for (QAction* action : menu->actions()) {
            if (plainText(action->text()).compare(plainText(entry), Qt::CaseInsensitive) == 0 &&
                action->isEnabled()) {
              m_found = action;
              return true;
            }
          }
          // "Rim: *" chooses the first entry that starts with "Rim: " (an
          // entry whose message the test cannot know whole, mitcad#1935).
          if (entry.endsWith(QLatin1Char('*'))) {
            const QString head = plainText(entry.chopped(1));
            for (QAction* action : menu->actions()) {
              if (action->isEnabled() && action->menu() == nullptr &&
                  plainText(action->text()).startsWith(head, Qt::CaseInsensitive)) {
                m_found = action;
                return true;
              }
            }
          }
          const int slash = entry.indexOf(QLatin1Char('/'));
          if (slash > 0) {
            const QString head = entry.left(slash);
            for (QAction* action : menu->actions()) {
              if (action->menu() != nullptr && action->isEnabled() &&
                  plainText(action->text()).section(QLatin1Char('\t'), 0, 0).compare(plainText(head),
                                                                                  Qt::CaseInsensitive) == 0) {
                m_found = action;
                *rest = entry.mid(slash + 1);
                return true;
              }
            }
          }
          return false;
        },
        6, QStringLiteral("no entry '%1' in an open menu").arg(entry),
        [this, rest, line, found] {
          QMenu* menu = found->data();
          QAction* action = qobject_cast<QAction*>(m_found.data());
          if (menu == nullptr || action == nullptr) {
            fail(line, QStringLiteral("the popup menu closed before its entry was chosen"));
            return;
          }
          // A key for the menu that has the entry, as the keyboard's would
          // be for it while it is the open menu that has the input.
          const auto press = [menu](int key, const QString& text) {
            if (mac::test::sendKey(key, Qt::NoModifier, text)) return;
            QKeyEvent offer(QEvent::ShortcutOverride, key, Qt::NoModifier, text);
            offer.ignore();
            QCoreApplication::sendEvent(menu, &offer);
            QKeyEvent down(QEvent::KeyPress, key, Qt::NoModifier, text);
            QCoreApplication::sendEvent(menu, &down);
            QKeyEvent up(QEvent::KeyRelease, key, Qt::NoModifier, text);
            QCoreApplication::sendEvent(menu, &up);
          };
          if (!rest->isEmpty()) {
            // The submenu current, which opens it (else Right does, as the
            // keyboard opens one), then the rest in it.
            menu->setActiveAction(action);
            if (!action->menu()->isVisible()) {
              press(Qt::Key_Right, QString());
            }
            m_submenu = action->menu();
            m_inSubmenu = true;
            const QString next = *rest;
            queue([this, line, next] { run(Step{line, {}, QStringLiteral("popup"), {next}}); });
            return;
          }
          // As the keyboard does: the entry current, then Return.
          menu->setActiveAction(action);
          press(Qt::Key_Return, QStringLiteral("\r"));
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
            if (widget == m_main.data() || widget == topWindow() || qobject_cast<GlassCard*>(widget) != nullptr ||
                !widget->isVisible() || !dialogMatches(widget, re)) {
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
  } else if (name == QLatin1String("workspace")) {
    // workspace DESIGN|MAKE|RENDER|DRAWING: the toolbar panel's workspace chosen as
    // its selector's list chooses it (mitcad#1990; the Docked tab row's
    // selector too, mitcad#2082), so that a script's next step finds that
    // workspace's tabs; nothing in a layout without the selector (the macOS
    // Floating title bar), where every tab is in sight. The list itself is
    // clicked by the UI tests (tools/ui-workspaces-test.sh).
    auto* ribbon = m_main ? m_main->findChild<Ribbon*>() : nullptr;
    if (ribbon == nullptr || !ribbon->hasWorkspaceSelector()) {
      qDebug().noquote() << QStringLiteral("TestDriver: workspace %1: no selector in this layout").arg(a.at(0));
    } else {
      ribbon->setWorkspace(a.at(0).toUpper());
    }
  } else if (name == QLatin1String("show-workspace")) {
    // show-workspace DESIGN|RENDER: that workspace shown in either layout
    // (mitcad#2092): chosen in the toolbar panel's selector as `workspace`
    // does, or in the Docked layout by a click on its tab (RENDER, or SOLID
    // for the Design workspace).
    auto* ribbon = m_main ? m_main->findChild<Ribbon*>() : nullptr;
    const QString workspace = a.at(0).toUpper();
    if (workspace != QLatin1String("DESIGN") && workspace != QLatin1String("RENDER")) {
      fail(line, QStringLiteral("show-workspace DESIGN|RENDER"));
      return;
    }
    if (ribbon != nullptr && ribbon->isPanel()) {
      ribbon->setWorkspace(workspace);
    } else {
      Step click = step;
      click.name = QStringLiteral("click-logged");
      click.args = QStringList{workspace == QLatin1String("RENDER") ? QStringLiteral("^Ribbon tab RENDER at ")
                                                                    : QStringLiteral("^Ribbon tab SOLID at ")};
      run(click);
    }
  } else if (name == QLatin1String("leave-start-page")) {
    // The Start page left as a user does (mitcad#1990): a click on the
    // design's tab in the Floating title row, where the toolbar panel is
    // hidden over the page; on the SOLID tab elsewhere. A click-logged step
    // at that place.
    auto* ribbon = m_main ? m_main->findChild<Ribbon*>() : nullptr;
    Step click = step;
    click.name = QStringLiteral("click-logged");
    click.args = QStringList{ribbon != nullptr && ribbon->isPanel() ? QStringLiteral("^Document tab shown at ")
                                                                    : QStringLiteral("^Ribbon tab SOLID at ")};
    run(click);
  } else if (name == QLatin1String("focus-main")) {
    m_dialog = nullptr;
    if (mac::test::active()) {
      mac::test::focusMain();
      mac::test::requestKeyEventFocus();
      whenReady([this] {
        if (mac::test::keyEventFocusReady()) return true;
        m_waitDetail = mac::test::keyEventFocusDiagnostic();
        return false;
      }, 6, QStringLiteral("native main window did not become the active input target"));
    }
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
  } else if (name == QLatin1String("expect-less")) {
    double smaller = 0;
    double larger = 0;
    if (!toNumber(a.at(0), smaller) || !toNumber(a.at(1), larger)) {
      fail(line, QStringLiteral("expect-less <smaller> <larger>: not a number"));
      return;
    }
    if (!(smaller < larger)) {
      fail(line, QStringLiteral("%1 is not less than %2").arg(smaller, 0, 'g', 12).arg(larger, 0, 'g', 12));
      return;
    }
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
    if (headlessUiTest()) {
      // No window server: the visible Qt windows as the offscreen screen
      // shows them, without native chrome, glass or the 3D scene.
      QDir().mkpath(QFileInfo(a.at(0)).absolutePath());
      if (!saveHeadlessScreen(a.at(0))) {
        fail(line, QStringLiteral("could not save the screenshot %1 (no visible Qt window)").arg(a.at(0)));
        return;
      }
    } else {
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
    }
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
    if (static_cast<int>(code) == 0 && !verifyHeadlessCompletion()) {
      return;
    }
    qDebug().noquote() << QStringLiteral("TestDriver: quit (%1 of %2 steps)").arg(m_next).arg(m_steps.size());
    leave(static_cast<int>(code));
  } else if (name == QLatin1String("kill")) {
    if (!verifyHeadlessCompletion()) {
      return;
    }
    qDebug().noquote() << QStringLiteral("TestDriver: kill (%1 of %2 steps)").arg(m_next).arg(m_steps.size());
    std::_Exit(0);
  } else if (name == QLatin1String("stay")) {
    m_stay = true;
  }
}

} // namespace mitcad
