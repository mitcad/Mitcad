// SPDX-License-Identifier: MIT
// Test steps for picks and the selection: the selection and its info, where a click picks,
// clicks on visible faces, edges and items, pick offsets and a held pick.
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
    {"expect-pick-offset", 4, 4, TestDriver::kInputOfMain},
    {"click-pick-offset", 4, 4, TestDriver::kInputOfMain},
    {"hold-pick", 1, 1, TestDriver::kInputOfMain},
    {"click-visible-face", 0, 0, TestDriver::kInputOfMain},
    {"click-visible-item", 1, 1, TestDriver::kInputOfMain}, // mitcad#1704
    {"click-edge-release", 2, 2, TestDriver::kInputOfMain},
    {"expect-selection", 2, 2, TestDriver::kIdle},
    {"expect-selection-info", 1, 3, TestDriver::kIdle},
    // Where a click with no command picks each face, edge and vertex, as
    // "Pick places:" (mitcad#1955: a surface body's free edges).
    {"idle-pick-places", 0, 0, TestDriver::kIdle},
};

} // namespace

const TestDriver::StepArea TestDriver::kPickSteps{
    kStepSpecs, std::size(kStepSpecs), &TestDriver::runPickStep};

void TestDriver::runPickStep(const QString& name, const QStringList& a, int line) {
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
  if (name == QLatin1String("expect-selection-info")) {
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
  } else if (name == QLatin1String("idle-pick-places")) {
    // The idle selection's places, for hover-logged and click-pick with no
    // command (mitcad#1955); logged again by each click-pick.
    whenReady([this] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      return window != nullptr && !window->modelBusy() && window->logIdlePickPlaces();
    }, 6, QStringLiteral("no pick places logged with no command"));
  } else if (name == QLatin1String("click-edge-release")) {
    bool ok = false;
    const int index = a.at(0).toInt(&ok);
    const QString order = a.at(1);
    const bool modifierFirst = order == QLatin1String("modifier-first");
    // key-up-moved (mitcad#1571): Ctrl released while the button is down,
    // then a one-pixel move before the release. f4-elsewhere (mitcad#1573):
    // F4 pressed on the view, the keyboard taken elsewhere and F4 released
    // there, then a plain click.
    const bool keyUpMoved = order == QLatin1String("key-up-moved");
    const bool f4Elsewhere = order == QLatin1String("f4-elsewhere");
    if (!ok || index < 1 ||
        (!modifierFirst && !keyUpMoved && !f4Elsewhere && order != QLatin1String("button-first"))) {
      fail(line, QStringLiteral("click-edge-release <positive index> modifier-first|button-first|key-up-moved|f4-elsewhere"));
      return;
    }
    const auto point = std::make_shared<QPoint>();
    whenReady([this, index, point] {
      auto* view = qobject_cast<OcctViewer*>(viewer());
      if (!view || !view->isValid()) return false;
      Selection seen;
      for (const auto& candidate : view->pickPoints(SelectKind::Edge)) {
        const auto actual = view->itemAt(candidate.second);
        if (!actual || actual->kind != SelectKind::Edge || *actual != candidate.first || seen.contains(*actual)) continue;
        seen.append(*actual);
        if (seen.size() != index) continue;
        *point = view->mapTo(topWindow(), candidate.second);
        qInfo().noquote() << QStringLiteral("Release-order edge %1: %2").arg(index).arg(actual->logDescription());
        return true;
      }
      m_waitDetail = QStringLiteral("only %1 clean visible edges for index %2").arg(seen.size()).arg(index);
      return false;
    }, 6, QStringLiteral("no fresh visible edge for the release-order click"),
        [this, point, modifierFirst, keyUpMoved, f4Elsewhere] {
      if (f4Elsewhere) {
        queue([this] {
          auto* view = viewer();
          if (!view) return;
          QKeyEvent press(QEvent::KeyPress, Qt::Key_F4, Qt::NoModifier);
          QCoreApplication::sendEvent(view, &press);
        });
        queue([this] {
          // The keyboard leaves the view (a panel's field takes it) and
          // F4's release goes to that other widget, never to the view.
          auto* view = viewer();
          QWidget* other = topWindow();
          if (!view || !other || other == view) return;
          QFocusEvent out(QEvent::FocusOut, Qt::TabFocusReason);
          QCoreApplication::sendEvent(view, &out);
          QKeyEvent release(QEvent::KeyRelease, Qt::Key_F4, Qt::NoModifier);
          QCoreApplication::sendEvent(other, &release);
        });
        queue([this, point] { sendMouse(QEvent::MouseMove, *point, Qt::NoButton, Qt::NoButton, Qt::NoModifier); });
        queue([this, point] { sendMouse(QEvent::MouseButtonPress, *point, Qt::LeftButton, Qt::LeftButton, Qt::NoModifier); });
        queue([this, point] { sendMouse(QEvent::MouseButtonRelease, *point, Qt::LeftButton, Qt::NoButton, Qt::NoModifier); });
        return;
      }
      if (keyUpMoved) {
        const QPoint moved = *point + QPoint(1, 0);
        queue([this, point] { sendMouse(QEvent::MouseMove, *point, Qt::NoButton, Qt::NoButton, Qt::ControlModifier); });
        queue([this, point] { sendMouse(QEvent::MouseButtonPress, *point, Qt::LeftButton, Qt::LeftButton, Qt::ControlModifier); });
        queue([this] {
          QKeyEvent event(QEvent::KeyRelease, Qt::Key_Control, Qt::NoModifier);
          if (auto* view = viewer()) QCoreApplication::sendEvent(view, &event);
        });
        queue([this, moved] { sendMouse(QEvent::MouseMove, moved, Qt::NoButton, Qt::LeftButton, Qt::NoModifier); });
        queue([this, moved] { sendMouse(QEvent::MouseButtonRelease, moved, Qt::LeftButton, Qt::NoButton, Qt::NoModifier); });
        return;
      }
      queue([this, point] { sendMouse(QEvent::MouseMove, *point, Qt::NoButton, Qt::NoButton, Qt::ControlModifier); });
      queue([this, point] { sendMouse(QEvent::MouseButtonPress, *point, Qt::LeftButton, Qt::LeftButton, Qt::ControlModifier); });
      queue([this, point, modifierFirst] {
        const auto releaseKey = [this] {
          QKeyEvent event(QEvent::KeyRelease, Qt::Key_Control, Qt::NoModifier);
          if (auto* view = viewer()) QCoreApplication::sendEvent(view, &event);
        };
        if (modifierFirst) releaseKey();
        sendMouse(QEvent::MouseButtonRelease, *point, Qt::LeftButton, Qt::NoButton,
                  modifierFirst ? Qt::NoModifier : Qt::ControlModifier);
        if (!modifierFirst) releaseKey();
      });
    }, mainWindow);
  } else if (name == QLatin1String("click-visible-face") || name == QLatin1String("click-visible-item")) {
    // click-visible-item <kind> (mitcad#1704): the same for another kind
    // the view picks outside commands, by its kindName.
    SelectKind kind = SelectKind::Face;
    if (name == QLatin1String("click-visible-item")) {
      kind = SelectKind::None;
      for (const SelectKind candidate : {SelectKind::Face, SelectKind::Edge, SelectKind::Vertex, SelectKind::Body,
                                         SelectKind::Profile, SelectKind::SketchCurve, SelectKind::SketchPoint}) {
        if (SelectionItem::kindName(candidate) == a.at(0)) {
          kind = candidate;
        }
      }
      if (kind == SelectKind::None) {
        fail(line, QStringLiteral("click-visible-item: unknown kind '%1'").arg(a.at(0)));
        return;
      }
    }
    const QString kindName = SelectionItem::kindName(kind);
    const auto point = std::make_shared<QPoint>();
    const auto expected = std::make_shared<SelectionItem>();
    whenReady([this, point, expected, kind, kindName] {
      auto* view = qobject_cast<OcctViewer*>(viewer());
      if (!view || !m_main || !view->isValid()) return false;
      // Ask the current OCCT selector after this lifecycle transition;
      // idle views do not emit the command-only "Pick places" log.
      const auto candidates = view->pickPoints(kind);
      for (const auto& candidate : candidates) {
        const auto actual = view->itemAt(candidate.second);
        if (!actual || actual->kind != kind || *actual != candidate.first) continue;
        *point = view->mapTo(topWindow(), candidate.second);
        *expected = *actual;
        return true;
      }
      m_waitDetail = QStringLiteral("OCCT found %1 %2 candidates").arg(candidates.size()).arg(kindName);
      return false;
    }, 6, QStringLiteral("no visible selectable %1 in the current viewport").arg(kindName),
              [this, point, expected, line, kind, kindName] {
      queueClick(*point, Qt::LeftButton, Qt::NoModifier, 1);
      queue([this, expected, line, kind, kindName] {
        auto* window = qobject_cast<MainWindow*>(m_main.data());
        if (!window) { fail(line, QStringLiteral("the document vanished after the %1 click").arg(kindName)); return; }
        window->deliverPendingPicks();
        whenReady([this, expected, kind, kindName] {
          const auto* current = qobject_cast<MainWindow*>(m_main.data());
          if (!current || current->modelBusy()) return false;
          const Selection& selected = current->selection();
          if (selected.size() != 1 || selected.first().kind != kind || selected.first() != *expected) {
            m_waitDetail = QStringLiteral("actual selection has %1 items; expected %2")
                .arg(selected.size()).arg(expected->logDescription());
            return false;
          }
          qInfo().noquote() << QStringLiteral("TestDriver: actual %1 selected after viewport click: %2")
              .arg(kindName, selected.first().logDescription());
          return true;
        }, 6, QStringLiteral("the actual viewport click did not select the probed %1").arg(kindName));
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
          *at = view.mapTo(topWindow(), point) + QPoint(dx, dy);
          found = true;
        }
      }
      if (!found) { m_waitDetail = QStringLiteral("no fresh point for %1").arg(target.pattern()); return false; }
      const auto item = view.itemAt(view.mapFrom(topWindow(), *at));
      const QString actual = item ? item->logDescription() : QStringLiteral("nothing");
      m_waitDetail = QStringLiteral("pick at offset %1,%2: %3").arg(dx).arg(dy).arg(actual);
      if (!expected.match(actual).hasMatch()) { return false; }
      qInfo().noquote() << QStringLiteral("TestDriver: %1").arg(m_waitDetail);
      return true;
    }, 6, QStringLiteral("offset pick did not match '%1'").arg(a.at(3)),
    [this, at, click = name == QLatin1String("click-pick-offset")] {
      sendMouse(QEvent::MouseMove, *at, Qt::NoButton, Qt::NoButton, Qt::NoModifier);
      if (click) { queueClick(*at, Qt::LeftButton, Qt::NoModifier, 1); }
    }, mainWindow);
  } else if (name == QLatin1String("hold-pick")) {
    // A press held still on an item (mitcad#1703: the view's pick list):
    // where a click picks it, as click-pick-offset probes it (profiles
    // too), held until the list opens, then let go.
    QRegularExpression target;
    if (!regex(QStringLiteral("^Pick (?:.*)(?:%1)").arg(a.at(0)), target)) { return; }
    const auto at = std::make_shared<QPoint>();
    const auto settle = std::make_shared<QElapsedTimer>();
    settle->start();
    whenReady([this, target, at, settle] {
      if (TestSync::settling() && settle->elapsed() < kSettleLimitMs) { return false; }
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window) { return false; }
      auto& view = window->viewer();
      const SelectFilter kinds = SelectKind::Face | SelectKind::Edge | SelectKind::Vertex | SelectKind::Profile;
      for (const auto& [item, point] : view.pickPoints(kinds)) {
        const QString description = QStringLiteral("Pick %1 %2/%3")
            .arg(SelectionItem::kindName(item.kind), item.owner, item.name);
        if (target.match(description).hasMatch()) {
          *at = view.mapTo(topWindow(), point);
          m_waitDetail.clear();
          return true;
        }
      }
      m_waitDetail = QStringLiteral("no fresh point for %1").arg(target.pattern());
      return false;
    }, 6, QStringLiteral("no pick place for '%1'").arg(a.at(0)),
    [this, at] {
      sendMouse(QEvent::MouseMove, *at, Qt::NoButton, Qt::NoButton, Qt::NoModifier);
      const int since = lineCount();
      const auto held = std::make_shared<QElapsedTimer>();
      queue([this, at, held] {
        held->start();
        sendMouse(QEvent::MouseButtonPress, *at, Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
      });
      queueHoldRelease(*at, since, 3000, held);
    }, mainWindow);
  }
}

} // namespace mitcad
