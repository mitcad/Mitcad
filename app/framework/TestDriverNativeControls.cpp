// SPDX-License-Identifier: MIT
// Test steps for the native (AppKit) controls through the native test input: the native
// history, tooltips, menus, dialogs, layouts, keys and the pointer.
// Split from TestDriver.cpp (mitcad#2173), so that a UI test depends on the
// step areas it uses; the steps are described in TestDriver.hpp.
#include "TestDriver.hpp"
#include "TestDriverSupport.hpp"
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

using namespace testdriver;

namespace {

const TestDriver::StepSpec kStepSpecs[] = {
    {"native-viewport-click-pick", 1, 1, TestDriver::kInputOfMain},
    {"native-viewport-click-logged", 1, 1, TestDriver::kInputOfMain},
    {"native-click", 1, 1, TestDriver::kInputOfTarget},
    {"native-focus", 1, 1, TestDriver::kInputOfTarget},
    {"native-hover", 2, 2, TestDriver::kInputOfTarget},
    {"native-type", 2, 2, TestDriver::kInputOfTarget},
    {"native-choice", 2, 2, TestDriver::kInputOfTarget},
    {"expect-native-duplicate-choices", 0, 0, TestDriver::kIdle},
    {"expect-native-widget-refresh", 0, 0, TestDriver::kIdle},
    {"expect-native-picker-fallback", 0, 0, TestDriver::kIdle},
    {"expect-native-application-delegate", 0, 0, TestDriver::kIdle},
    {"native-toolbar-item", 2, 2, TestDriver::kInputOfMain},
    {"expect-native-toolbar-item", 2, 2, TestDriver::kIdle},
    {"expect-native-form-controls", 0, 0, TestDriver::kIdle},
    {"expect-native-disclosure-layout", 0, 0, TestDriver::kIdle},
    {"native-action", 1, 1, TestDriver::kInputOfMain},
    {"native-activate", 1, 1, TestDriver::kInputOfTarget},
    {"native-select", 1, 2, TestDriver::kInputOfTarget},
    {"native-expand", 1, 1, TestDriver::kInputOfTarget},
    {"native-click-now", 1, 1, TestDriver::kPlain},
    {"native-type-now", 2, 2, TestDriver::kPlain},
    {"native-file-name", 1, 1, TestDriver::kInputOfTarget},
    {"native-file-directory", 1, 1, TestDriver::kInputOfTarget},
    {"native-file-cancel", 0, 0, TestDriver::kInputOfTarget},
    {"native-file-state", 0, 1, TestDriver::kPlain},
    {"native-system-key", 1, 1, TestDriver::kInputOfTarget},
    {"native-system-type", 1, 1, TestDriver::kInputOfTarget},
    {"native-system-pointer", 1, 1, TestDriver::kInputOfTarget},
    {"native-move", 2, 2, TestDriver::kInputOfMain},
    {"native-appearance", 1, 1, TestDriver::kInputOfMain},
    {"native-context-menu", 2, 2, TestDriver::kInputOfMain},
    {"native-popup", 2, 2, TestDriver::kInputOfMain},
    {"expect-native-text", 2, 2, TestDriver::kIdle},
    {"expect-native-accessibility", 2, 2, TestDriver::kIdle},
    {"expect-native-enabled", 2, 2, TestDriver::kIdle},
    {"expect-native-visible", 2, 2, TestDriver::kIdle},
    {"expect-native-present", 2, 2, TestDriver::kIdle},
    {"expect-native-attached", 3, 3, TestDriver::kIdle},
    {"expect-native-cube-arrows", 0, 0, TestDriver::kIdle},
    {"expect-native-style", 2, 2, TestDriver::kIdle},
    {"expect-native-columns", 1, 2, TestDriver::kIdle},
    {"native-dialog-size", 2, 2, TestDriver::kInputOfTarget},
    // Native tile grids, group lists and utility panels (mitcad#813, #819).
    {"native-tiles", 2, 4, TestDriver::kInputOfTarget},
    {"expect-native-tiles", 1, 4, TestDriver::kIdle},
    {"expect-native-layout", 0, 0, TestDriver::kIdle},
    {"expect-native-history", 0, -1, TestDriver::kIdle},
    {"expect-native-history-wheel", 0, 0, TestDriver::kIdle},
    {"native-history-drag", 1, 2, TestDriver::kInputOfMain},
    {"native-history-drag-now", 1, 2, TestDriver::kPlain},
    {"expect-native-dialog-gone", 1, 2, TestDriver::kPlain},
    {"native-drag", 2, 2, TestDriver::kInputOfTarget},
    {"expect-native-focus", 1, 2, TestDriver::kIdle},
    {"expect-native-selection", 2, 2, TestDriver::kIdle},
    {"native-card-width", 2, 2, TestDriver::kInputOfMain},
    {"native-screen-limit", 2, 2, TestDriver::kIdle},
    // Compact AppKit selection and command cards (mitcad#815).
    {"expect-native-selection-card", 1, 1, TestDriver::kIdle},
    {"expect-native-command-layout", 0, -1, TestDriver::kIdle},
    // Shared layout bounds, real pointer input and button order (mitcad#812, #817).
    {"expect-native-bounds", 2, 3, TestDriver::kIdle},
    {"expect-native-form-layout", 1, -1, TestDriver::kIdle},
    {"expect-native-configuration-layout", 0, 1, TestDriver::kIdle},
    {"expect-native-spanned-heading", 2, 2, TestDriver::kIdle},
    {"expect-native-first-row", 2, 2, TestDriver::kIdle},
    {"expect-native-color-field", 4, 5, TestDriver::kIdle},
    {"expect-native-button-order", 1, -1, TestDriver::kIdle},
    {"expect-native-control-focus", 1, 1, TestDriver::kIdle},
    {"expect-native-layout-contract", 0, 0, TestDriver::kIdle},
    {"expect-native-settings-layout", 1, 1, TestDriver::kIdle},
    {"native-pointer-click", 1, 3, TestDriver::kInputOfTarget},
    {"native-pointer-popup", 2, 3, TestDriver::kInputOfTarget},
    {"native-pointer-dblclick", 1, 1, TestDriver::kInputOfTarget},
    {"native-pointer-choice", 4, 34, TestDriver::kInputOfTarget},
    {"native-pointer-button", 1, 1, TestDriver::kInputOfTarget},
    {"native-pointer-drag", 3, 3, TestDriver::kInputOfTarget},
    {"native-header-resize", 3, 3, TestDriver::kInputOfTarget},
    {"expect-native-header-resize-retained", 2, 2, TestDriver::kIdle},
    {"expect-native-split-retained", 1, 1, TestDriver::kIdle},
    {"native-dialog-resize", 2, 2, TestDriver::kInputOfTarget},
    // The tooltip of a native control (the timeline's cells, mitcad#1076).
    {"expect-native-tooltip", 2, 2, TestDriver::kIdle},
    // The AppKit left column (mitcad#1020, TestDriverWindowFit.cpp).
    {"native-column-split", 2, 2, TestDriver::kInputOfMain},
    // A Qt widget over the AppKit frontend's 3D view against an AppKit view
    // of the window: above it, or at its top (mitcad#1358).
    {"expect-native-qt-place", 3, 3, TestDriver::kIdle},
    // The wheel over a native control, an AppKit value field (mitcad#1350).
    {"native-wheel", 2, 3, TestDriver::kInputOfTarget},
};

} // namespace

const TestDriver::StepArea TestDriver::kNativeControlSteps{
    kStepSpecs, std::size(kStepSpecs), &TestDriver::runNativeControlStep};

void TestDriver::runNativeControlStep(const QString& name, const QStringList& a, int line) {
  QWidget* mainWindow = m_main.data();
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
  if (name == QLatin1String("expect-native-history-wheel")) {
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
      const QSet<QString> filtered = timeline ? timeline->filteredFeatures() : QSet<QString>();
      // The features of the folded timeline groups (mitcad#1077), as the
      // shared timeline folds them.
      QHash<QString, QString> folded;
      const QJsonArray features = state.value(QStringLiteral("features")).toArray();
      for (const TimelineWidget::ShownGroup& group : timeline ? timeline->shownGroups() : QVector<TimelineWidget::ShownGroup>()) {
        if (!group.collapsed) continue;
        bool inside = false;
        for (const QJsonValue& feature : features) {
          const QString uid = feature.toObject().value(QStringLiteral("uid")).toString();
          inside = inside || uid == group.first;
          if (inside && !filtered.contains(uid)) folded.insert(uid, group.name);
          if (uid == group.last) break;
        }
      }
      return mac::test::historyLayout(state, filtered, selected, checkSelection, m_waitDetail, folded);
    }, 6, QStringLiteral("native history does not match the real model timeline"));
  } else if (name == QLatin1String("expect-native-tooltip")) {
    if (!mac::test::active()) { fail(line, QStringLiteral("expect-native-tooltip requires the native macOS UI")); return; }
    QRegularExpression expected;
    if (!regex(a.at(1), expected)) return;
    const QString identifier = a.at(0);
    whenReady([this, identifier, expected] {
      QString text;
      if (!mac::test::toolTipOf(identifier, text)) {
        m_waitDetail = QStringLiteral("no native control '%1'").arg(identifier);
        return false;
      }
      m_waitDetail = text;
      return expected.match(text).hasMatch();
    }, 6, QStringLiteral("the native tooltip of '%1' does not match").arg(identifier));
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
  } else if (name == QLatin1String("expect-native-widget-refresh") ||
             name == QLatin1String("expect-native-picker-fallback")) {
    QString error;
    // The contract pumps presenter events. Keep later script steps from
    // entering while its synthetic controls are mounted.
    m_timer->stop();
    const bool passed = mac::test::active() &&
        (name == QLatin1String("expect-native-picker-fallback") ? mac::test::pickerFallback(error)
                                                               : mac::test::widgetRefreshContract(error));
    if (!m_finished) m_timer->start();
    if (!passed) {
      fail(line, error.isEmpty() ? QStringLiteral("native presenter contract requires the native UI") : error);
    }
  } else if (name == QLatin1String("expect-native-split-retained")) {
    if (!mac::test::active()) { fail(line, QStringLiteral("native split checks require the native UI")); return; }
    const QString id = a.at(0);
    whenReady([this, id] { return mac::test::splitDragRetained(id, m_waitDetail); }, 6,
              QStringLiteral("native split '%1' lost its dragged position").arg(id));
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
  } else if (name == QLatin1String("expect-native-accessibility")) {
    if (!mac::test::active()) {
      fail(line, QStringLiteral("'%1' requires the native macOS UI").arg(name));
      return;
    }
    QRegularExpression re;
    if (!regex(a.at(1), re)) return;
    const QString id = a.at(0);
    whenReady([this, id, re] {
      QString label;
      if (!mac::test::accessibilityLabelOf(id, label)) {
        m_waitDetail = QStringLiteral("no visible native accessibility label for '%1'").arg(id);
        return false;
      }
      m_waitDetail = QStringLiteral("its accessibility label is '%1'").arg(label);
      return re.match(label).hasMatch();
    }, 6, QStringLiteral("native accessibility label '%1' did not match '%2'").arg(id, a.at(1)));
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
      // The text found goes into the failure's detail.
      whenReady([this, id, re] {
        QString text;
        if (!mac::test::textOf(id, text)) {
          m_waitDetail = QStringLiteral("no native control or cell '%1'").arg(id);
          return false;
        }
        m_waitDetail = QStringLiteral("its text is '%1'").arg(text);
        return re.match(text).hasMatch();
      }, 6, QStringLiteral("native text '%1' did not match '%2'").arg(id, a.at(1)));
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
  } else if (name == QLatin1String("expect-native-attached")) {
    // A search popover at the control that opened it (mitcad#1021).
    if (!mac::test::active()) {
      fail(line, QStringLiteral("'%1' requires the native macOS UI").arg(name));
      return;
    }
    if (a.at(2) != QLatin1String("above") && a.at(2) != QLatin1String("below")) {
      fail(line, QStringLiteral("expect-native-attached <identifier> <anchor identifier> above|below")); return;
    }
    const QString id = a.at(0);
    const QString anchor = a.at(1);
    const bool above = a.at(2) == QLatin1String("above");
    whenReady([this, id, anchor, above] { return mac::test::attached(id, anchor, above, m_waitDetail); }, 6,
              QStringLiteral("native '%1' is not directly %2 '%3'").arg(id, a.at(2), anchor));
  } else if (name == QLatin1String("expect-native-cube-arrows")) {
    // The cube's AppKit arrows at the hidden Qt arrows' places, also below
    // a notice bar (mitcad#1317).
    if (!mac::test::active()) {
      fail(line, QStringLiteral("'%1' requires the native macOS UI").arg(name));
      return;
    }
    whenReady([this] {
      auto* view = qobject_cast<OcctViewer*>(viewer());
      if (view == nullptr || view->shownCubeArrows().empty()) {
        m_waitDetail = QStringLiteral("the view shows no cube arrows");
        return false;
      }
      for (const OcctViewer::CubeArrow& arrow : view->shownCubeArrows()) {
        const QRect place(view->mapTo(view->window(), arrow.geometry.topLeft()), arrow.geometry.size());
        if (!mac::test::viewportFrame(QStringLiteral("native.cubeArrow.") + arrow.id, place.adjusted(2, 2, -2, -2),
                                      m_waitDetail)) {
          return false;
        }
      }
      return true;
    }, 6, QStringLiteral("the cube's AppKit arrows are not at the Qt arrows' places"));
  } else if (name == QLatin1String("expect-native-qt-place")) {
    // A Qt widget over the embedded 3D view and an AppKit view, both in the
    // 3D view's host (mitcad#1358): "above", the widget's bottom at or above
    // the view's top (the Start page's footer over the message band); "top",
    // the widget's top at the view's top (the Start page right below the
    // toolbar, native.window.content).
    if (!mac::test::active()) {
      fail(line, QStringLiteral("'%1' requires the native macOS UI").arg(name));
      return;
    }
    if (a.at(1) != QLatin1String("above") && a.at(1) != QLatin1String("top")) {
      fail(line, QStringLiteral("expect-native-qt-place <widget> above|top <native view>"));
      return;
    }
    const QString widgetName = a.at(0);
    const bool above = a.at(1) == QLatin1String("above");
    const QString nativeView = a.at(2);
    whenReady([this, widgetName, above, nativeView] {
      QWidget* view = viewer();
      QWidget* widget = view != nullptr ? namedWidget(view, widgetName) : nullptr;
      if (widget == nullptr || !widget->isVisible()) {
        m_waitDetail = QStringLiteral("no visible widget %1 over the 3D view").arg(widgetName);
        return false;
      }
      QRectF frame;
      if (!mac::test::viewportRect(nativeView, frame, m_waitDetail)) return false;
      const QRect place(widget->mapTo(view->window(), QPoint(0, 0)), widget->size());
      m_waitDetail = QStringLiteral("%1 at %2,%3 %4x%5, native '%6' at %7,%8 %9x%10 in the 3D view's host")
                         .arg(widgetName)
                         .arg(place.x())
                         .arg(place.y())
                         .arg(place.width())
                         .arg(place.height())
                         .arg(nativeView)
                         .arg(frame.x())
                         .arg(frame.y())
                         .arg(frame.width())
                         .arg(frame.height());
      return above ? place.y() + place.height() <= frame.top() + 0.5 : std::abs(place.y() - frame.top()) <= 1;
    }, 6, QStringLiteral("widget %1 is not %2 native '%3'").arg(widgetName, a.at(1), nativeView));
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
      if (!a.isEmpty() && a.first() != QLatin1String("frames")) {
        fail(line, QStringLiteral("expect-native-configuration-layout [frames]")); return;
      }
      const bool framesOnly = !a.isEmpty();
      whenReady([this, framesOnly] { return mac::test::configurationLayout(m_waitDetail, framesOnly); }, 6,
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
      // Wait for real application activation before pointer delivery. A
      // transient popover can leave its owner key, so the keyboard event's
      // exact key-window identity is not a mouse readiness condition.
      mac::test::requestPointerEventActivation();
      if (byTitle) {
        whenReady([this, re] {
          if (!mac::test::pointerEventActivationReady()) { m_waitDetail = mac::test::keyEventFocusDiagnostic(); return false; }
          return mac::test::buttonExists(re);
        }, 6, QStringLiteral("no active native button matching '%1'").arg(target),
                  [this, perform] { queue(perform); });
      } else {
        whenReady([this, target] {
          if (!mac::test::pointerEventActivationReady()) { m_waitDetail = mac::test::keyEventFocusDiagnostic(); return false; }
          return mac::test::controlExists(target, &m_waitDetail);
        }, 6, QStringLiteral("no active native control '%1'").arg(target),
                  [this, perform] { queue(perform); });
      }
    }
  } else if (name == QLatin1String("native-system-pointer")) {
    if (!mac::test::active()) {
      fail(line, QStringLiteral("native-system-pointer requires the native macOS UI")); return;
    }
    const QString identifier = a.at(0);
    mac::test::requestPointerEventActivation();
    whenReady([this, identifier] {
      if (!mac::test::pointerEventActivationReady()) { m_waitDetail = mac::test::keyEventFocusDiagnostic(); return false; }
      return mac::test::controlExists(identifier, &m_waitDetail);
    }, 6, QStringLiteral("no active visible native system pointer target '%1'").arg(identifier), [this, identifier, line] {
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
             name != QLatin1String("native-system-type") &&
             name != QLatin1String("native-viewport-click-pick") &&
             name != QLatin1String("native-viewport-click-logged")) {
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
    // native-wheel <id> <notches> [mods] (mitcad#1350).
    double notches = 0;
    if (name == QLatin1String("native-wheel") &&
        (!toNumber(a.at(1), notches) || qRound(notches) == 0 || (a.size() > 2 && !parseModifiers(a.at(2), modifiers)))) {
      fail(line, QStringLiteral("native-wheel <id> <notches> [mods]")); return;
    }
    const auto perform = [this, line, name, identifier, a, modifiers, notches] {
      QString error;
      const bool ok = (name == QLatin1String("native-click") || name == QLatin1String("native-click-now")) ? mac::test::click(identifier, error) :
                      name == QLatin1String("native-focus") ? mac::test::focusControl(identifier, error) :
                      name == QLatin1String("native-hover") ? mac::test::hover(identifier, a.at(1) == QStringLiteral("enter"), error) :
                      (name == QLatin1String("native-type") || name == QLatin1String("native-type-now")) ? mac::test::replaceText(identifier, a.at(1), error) :
                      name == QLatin1String("native-choice") ? mac::test::choose(identifier, a.at(1), error) :
                      name == QLatin1String("native-activate") ? mac::test::activate(identifier, error) :
                      name == QLatin1String("native-select") ? mac::test::select(identifier, error, modifiers) :
                      name == QLatin1String("native-expand") ? mac::test::expand(identifier, error) :
                      name == QLatin1String("native-wheel") ? mac::test::wheel(identifier, qRound(notches), modifiers, error) :
                      mac::test::triggerAction(identifier, error);
      if (!ok) fail(line, error.isEmpty() ? QStringLiteral("native input failed for '%1'").arg(identifier) : error);
    };
    if (name == QLatin1String("native-action")) queue(perform);
    else whenReady([identifier] { return mac::test::controlExists(identifier); }, 6,
                   QStringLiteral("no native control '%1'").arg(identifier), [this, perform] { queue(perform); });
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
  } else if (name == QLatin1String("native-viewport-click-pick") ||
             name == QLatin1String("native-viewport-click-logged")) {
    if (!mac::test::active()) {
      fail(line, QStringLiteral("'%1' requires the native macOS UI").arg(name));
      return;
    }
    const bool pick = name == QLatin1String("native-viewport-click-pick");
    QRegularExpression re;
    if (!regex(pick ? QStringLiteral("^Pick (?:.*)(?:%1)").arg(a.at(0)) : a.at(0), re)) return;
    const auto at = std::make_shared<QPoint>();
    const auto settle = std::make_shared<QElapsedTimer>();
    settle->start();
    whenReady([this, re, pick, at, settle] {
      if (TestSync::settling() && settle->elapsed() < kSettleLimitMs) return false;
      int from = 0;
      if (pick) {
        static const QRegularExpression places(QStringLiteral("^Pick places:"));
        from = findLastLine(places, 0);
        if (from < 0) { m_waitDetail = QStringLiteral("no current pick-place batch"); return false; }
      }
      return loggedPoint(re, from, *at);
    }, 6, QStringLiteral("no viewport point logged for '%1'").arg(a.at(0)), [this, at, line] {
      queue([this, at, line] {
        auto* window = qobject_cast<MainWindow*>(m_main.data());
        if (!window) { fail(line, QStringLiteral("the document vanished before native viewport input")); return; }
        auto& view = window->viewer();
        const QPoint inView = view.mapFrom(topWindow(), *at);
        QString error;
        m_timer->stop();
        if (!mac::test::pointerClickViewport(&view, QPointF(inView), error)) fail(line, error);
        if (!m_finished) m_timer->start();
      });
    }, mainWindow);
  }
}

} // namespace mitcad
