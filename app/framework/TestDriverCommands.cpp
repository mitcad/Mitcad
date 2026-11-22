// SPDX-License-Identifier: MIT
// Test steps for the toolbar and the commands: the toolbar's height, the
// command search, a command's state and icon, the parameter table's cells.
// Split from TestDriver.cpp (mitcad#2173), so that a UI test depends on the
// step areas it uses; the steps are described in TestDriver.hpp.
#include "TestDriver.hpp"
#include "Diagnostics.hpp"

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

namespace {

const TestDriver::StepSpec kStepSpecs[] = {
    {"capture-toolbar-height", 1, 1, TestDriver::kIdle},
    {"expect-command-search", 2, 3, TestDriver::kIdle},
    {"expect-command-enabled", 2, 2, TestDriver::kIdle},
    {"expect-command-icon", 2, 3, TestDriver::kIdle},
    {"click-parameter", 2, 2, TestDriver::kInputOfTarget},
    {"expect-parameter-cell", 4, 4, TestDriver::kIdle},
};

} // namespace

const TestDriver::StepArea TestDriver::kCommandSteps{
    kStepSpecs, std::size(kStepSpecs), &TestDriver::runCommandStep};

void TestDriver::runCommandStep(const QString& name, const QStringList& a, int line) {
  // A regular expression argument.
  const auto regex = [&](const QString& pattern, QRegularExpression& re) {
    re = QRegularExpression(pattern);
    if (!re.isValid()) {
      fail(line, QStringLiteral("bad regular expression '%1': %2").arg(pattern, re.errorString()));
      return false;
    }
    return true;
  };
  if (name == QLatin1String("capture-toolbar-height")) {
    // The toolbar's height as laid out into ${var} (mitcad#2092): the
    // Floating layout's toolbar panel, the Docked layout's toolbar (the
    // ribbon either way).
    const auto* ribbon = m_main ? m_main->findChild<Ribbon*>() : nullptr;
    if (ribbon == nullptr || !ribbon->isVisible()) {
      fail(line, QStringLiteral("capture-toolbar-height needs the toolbar shown"));
      return;
    }
    m_vars.insert(a.at(0), QString::number(ribbon->height()));
    qDebug().noquote() << QStringLiteral("TestDriver: toolbar height %1: %2 px (%3, %4)")
                              .arg(a.at(0))
                              .arg(ribbon->height())
                              .arg(ribbon->isPanel() ? QStringLiteral("panel") : QStringLiteral("toolbar"),
                                   ribbon->buttonLabels() ? QStringLiteral("labels") : QStringLiteral("icons"));
  } else if (name == QLatin1String("expect-command-search")) {
    // `hint`: the first row is the mode's hint (mitcad#1681: "Open a
    // sketch first: ..."), a row without a command; `no-hint`: no such row.
    static const QStringList searchOptions = {QStringLiteral("disabled"), QStringLiteral("shortcuts"),
                                              QStringLiteral("hint"), QStringLiteral("no-hint")};
    if (a.size() == 3 && !searchOptions.contains(a.at(2))) {
      fail(line, QStringLiteral("expect-command-search <filter> <current-id|*|-> [disabled|shortcuts|hint|no-hint]"));
      return;
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
    int hints = 0;
    bool hintFirst = false;
    for (int row = 0; row < list->topLevelItemCount(); ++row) {
      const auto* item = list->topLevelItem(row);
      const QString id = item->data(0, Qt::UserRole).toString();
      if (id.isEmpty()) {
        // The mode's hint: a row without a command, never selectable.
        ++hints;
        hintFirst = hintFirst || row == 0;
        if (item->flags() & Qt::ItemIsEnabled) {
          fail(line, QStringLiteral("the command search's hint row '%1' is selectable").arg(item->text(0))); return;
        }
        continue;
      }
      (item->flags() & Qt::ItemIsEnabled) ? ++enabled : ++disabled;
      const auto* action = m_main->findChild<QAction*>(QStringLiteral("command_") + id);
      if (!action || item->text(2) != action->shortcut().toString(QKeySequence::NativeText)) {
        fail(line, QStringLiteral("search shortcut differs from the native shortcut for %1").arg(id)); return;
      }
      if (item->text(2).contains(QChar(0x2318))) ++shortcuts;
    }
    const bool needDisabled = a.size() == 3 && a.at(2) == QStringLiteral("disabled");
    const bool needShortcuts = a.size() == 3 && a.at(2) == QStringLiteral("shortcuts");
    const bool needHint = a.size() == 3 && a.at(2) == QStringLiteral("hint");
    const bool needNoHint = a.size() == 3 && a.at(2) == QStringLiteral("no-hint");
    if ((a.at(1) != QStringLiteral("*") && current != a.at(1)) ||
        (a.at(1) == QStringLiteral("*") && !selected) || (selected && !(selected->flags() & Qt::ItemIsEnabled)) ||
        (needDisabled && disabled == 0) || (needShortcuts && shortcuts == 0) ||
        (needHint && (hints != 1 || !hintFirst || disabled == 0)) || (needNoHint && hints != 0)) {
      fail(line, QStringLiteral("search current %1, enabled %2, disabled %3, Command glyphs %4, hint rows %5")
                     .arg(current).arg(enabled).arg(disabled).arg(shortcuts).arg(hints));
    }
  } else if (name == QLatin1String("expect-command-enabled")) {
    if (a.at(1) != QLatin1String("true") && a.at(1) != QLatin1String("false")) {
      fail(line, QStringLiteral("expect-command-enabled <id> true|false"));
      return;
    }
    // Registry actions belong to the main window, independently of the
    // buttons that expose them in the current layout.
    const QList<QAction*> actions = m_main
        ? m_main->findChildren<QAction*>(QStringLiteral("command_") + a.at(0), Qt::FindDirectChildrenOnly)
        : QList<QAction*>();
    if (actions.size() != 1) {
      fail(line, QStringLiteral("expected one registered action for command '%1', found %2")
                     .arg(a.at(0)).arg(actions.size()));
      return;
    }
    const bool enabled = actions.first()->isEnabled();
    if (enabled != (a.at(1) == QLatin1String("true"))) {
      fail(line, QStringLiteral("command %1 enabled is %2, expected %3")
                     .arg(a.at(0), enabled ? QStringLiteral("true") : QStringLiteral("false"), a.at(1)));
      return;
    }
    qDebug().noquote() << QStringLiteral("TestDriver: command %1 enabled %2").arg(a.at(0), a.at(1));
  } else if (name == QLatin1String("expect-command-icon")) {
    // The command's icon as the ribbon, the menus and the command search
    // show it (they all take the command's action's icon), compared with an
    // icon of app/icons by its rendering; with a second command, the two
    // look different (mitcad#1096). The toolbar panel's buttons draw that
    // icon with the thin outline (commandButtonIcon, mitcad#2092), every
    // other button and the action with the sources' outline.
    const auto rendered = [](const QIcon& icon) {
      return icon.pixmap(QSize(24, 24), 1.0).toImage().convertToFormat(QImage::Format_ARGB32);
    };
    const auto* action = m_main ? m_main->findChild<QAction*>(QStringLiteral("command_") + a.at(0)) : nullptr;
    const QIcon expected = themeIcon(a.at(1));
    if (action == nullptr || expected.isNull()) {
      fail(line, QStringLiteral("expect-command-icon needs the command '%1' and the icon '%2'").arg(a.at(0), a.at(1)));
      return;
    }
    const QImage want = rendered(expected);
    const QImage wantThin = rendered(themeIcon(a.at(1), IconBackdrop::Palette, kThinOutlineWeight));
    if (rendered(action->icon()) != want) {
      fail(line, QStringLiteral("the command %1 does not show the icon %2").arg(a.at(0), a.at(1)));
      return;
    }
    int buttons = 0;
    int thin = 0;
    for (const QToolButton* button : m_main->findChildren<QToolButton*>()) {
      if (button->defaultAction() != action) {
        continue;
      }
      ++buttons;
      const QIcon drawn = commandButtonIcon(button);
      // A button of the toolbar panel (Ribbon.cpp's kThinOutlineProperty).
      const bool toolbar = button->property("mitcadThinOutline").toBool();
      thin += toolbar ? 1 : 0;
      if (rendered(drawn) != (toolbar ? wantThin : want) || rendered(button->icon()) != want) {
        fail(line, QStringLiteral("a button of %1 (%2) does not show the icon %3%4")
                       .arg(a.at(0), button->objectName(), a.at(1),
                            toolbar ? QStringLiteral(" with the thin outline") : QString()));
        return;
      }
    }
    if (a.size() == 3) {
      const auto* other = m_main->findChild<QAction*>(QStringLiteral("command_") + a.at(2));
      if (other == nullptr || rendered(other->icon()) == want) {
        fail(line, QStringLiteral("the command %1 shows the same icon as %2").arg(a.at(2), a.at(0)));
        return;
      }
    }
    qDebug().noquote() << QStringLiteral("TestDriver: command %1 shows the icon %2 (%3 buttons, %4 thin)")
                              .arg(a.at(0), a.at(1))
                              .arg(buttons)
                              .arg(thin);
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
      // The refusal with the name before it, or as the model's words that
      // name the parameter already (mitcad#2181).
      const bool refused = row && !row->toolTip(column).isEmpty() && message && message->isVisible() &&
          (message->text() == a.at(0) + QStringLiteral(": ") + row->toolTip(column) ||
           message->text() == row->toolTip(column));
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
  }
}

} // namespace mitcad
