// SPDX-License-Identifier: MIT
// Test steps for the pointer over the widgets and the view: hovers and tooltips, a widget
// left, rows hovered or clicked, context menus at logged places, the
// view's pointer.
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
    {"hover-logged", 1, 1, TestDriver::kInputOfMain},
    {"timeline-leave", 0, 0, TestDriver::kInputOfMain},
    // In the window the script targets: the main window, or a modal dialog
    // (mitcad#1816: the Properties window's table).
    {"leave-widget", 1, 1, TestDriver::kInputOfTarget},
    {"hover-row", 2, 2, TestDriver::kInputOfTarget},
    {"click-row", 2, 2, TestDriver::kInputOfTarget},
    {"expect-timeline-hover", 1, 1, TestDriver::kIdle},
    {"expect-timeline-tooltip", 1, 1, TestDriver::kIdle},
    {"context-menu-logged", 2, 2, TestDriver::kInputOfMain},
    {"view-context-menu", 3, 3, TestDriver::kInputOfMain},
    {"view-context-menu-logged", 2, 2, TestDriver::kInputOfMain},
    {"view-pointer", 4, 5, TestDriver::kInputOfMain},
    {"pointer-logged", 3, 5, TestDriver::kInputOfMain},
    // The pointer over a logged place without the model-idle gate: a
    // Timeline item while a command's preview computes (mitcad#1600).
    {"hover-logged-now", 1, 1, TestDriver::kPlain},
};

} // namespace

const TestDriver::StepArea TestDriver::kPointerSteps{
    kStepSpecs, std::size(kStepSpecs), &TestDriver::runPointerStep};

void TestDriver::runPointerStep(const QString& name, const QStringList& a, int line) {
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
  if (name == QLatin1String("hover-logged") || name == QLatin1String("hover-logged-now")) {
    // The -now form does not wait for the model (its step kind): the
    // pointer over a Timeline item while a preview computes (mitcad#1600).
    QRegularExpression re;
    if (!regex(a.at(0), re)) return;
    const auto point = std::make_shared<QPoint>();
    whenReady([this, re, point] {
                QString logged;
                if (findLastLine(re, 0, &logged) >= 0 && revealedPanelPlace(m_main, topWindow(), logged, *point)) {
                  return true;
                }
                return loggedPoint(re, 0, *point);
              }, 6,
              QStringLiteral("no hover position logged"), [this, point] {
      sendMouse(QEvent::MouseMove, *point, Qt::NoButton, Qt::NoButton, Qt::NoModifier);
      // The pointer enters what it is over, as a real move does: a widget
      // that tracks the mouse also when the place lies on a child of it
      // that does not (mitcad#2082: the middle of a panel's note row lay
      // on its Edit button, which takes the move without passing it on,
      // and the row never learnt it was hovered).
      if (QWidget* top = topWindow(); top != nullptr && !m_grab) {
        // In the window sendMouse sent the move to (a card over the view).
        QWidget* window = top;
        if (GlassCard* card = cardAt(*point)) {
          window = card;
        }
        const QPoint global = top->mapToGlobal(*point);
        QList<QWidget*> entered;
        for (QWidget* under = window->childAt(window->mapFromGlobal(global)); under != nullptr && under != window;
             under = under->parentWidget()) {
          if (under->hasMouseTracking() && !under->underMouse()) {
            entered.prepend(under);
          }
        }
        for (QWidget* widget : std::as_const(entered)) {
          QEnterEvent enter(QPointF(widget->mapFromGlobal(global)), QPointF(window->mapFromGlobal(global)),
                            QPointF(global));
          QCoreApplication::sendEvent(widget, &enter);
        }
      }
    }, mainWindow);
  } else if (name == QLatin1String("timeline-leave")) {
    auto* strip = m_main->findChild<QWidget*>(QStringLiteral("timelineStrip"));
    if (!strip) { fail(line, QStringLiteral("timeline strip missing")); return; }
    QEvent leave(QEvent::Leave);
    QCoreApplication::sendEvent(strip, &leave);
  } else if (name == QLatin1String("leave-widget")) {
    // The pointer leaves a widget by its object name (mitcad#1327: a
    // notice's hover ends), in the main window or a panel over it.
    QWidget* widget = m_main->findChild<QWidget*>(a.at(0));
    for (QWidget* top : QApplication::topLevelWidgets()) {
      if (widget == nullptr && top != nullptr) {
        widget = top->findChild<QWidget*>(a.at(0));
      }
    }
    if (widget == nullptr) { fail(line, QStringLiteral("no widget named '%1'").arg(a.at(0))); return; }
    if (auto* area = qobject_cast<QAbstractScrollArea*>(widget)) {
      QEvent leaveViewport(QEvent::Leave);
      QCoreApplication::sendEvent(area->viewport(), &leaveViewport);
    }
    QEvent leave(QEvent::Leave);
    QCoreApplication::sendEvent(widget, &leave);
  } else if (name == QLatin1String("hover-row") || name == QLatin1String("click-row")) {
    // The pointer over a row of a list by its object name and the row's
    // number from 0 (mitcad#1794: a choice's rows, whose hovered row paints
    // what it is about): a move to the middle of the row sent to the list's
    // viewport itself, as the pointer's moves reach it, whatever widget a
    // logged place finds in a session without a desktop; click-row then
    // presses and releases the left button there, which chooses the row as
    // a click does.
    QWidget* widget = m_main->findChild<QWidget*>(a.at(0));
    for (QWidget* top : QApplication::topLevelWidgets()) {
      if (widget == nullptr && top != nullptr) {
        widget = top->findChild<QWidget*>(a.at(0));
      }
    }
    // Any list or tree of rows: a choice's rows, a selection input's list
    // of picks (mitcad#2064).
    auto* list = qobject_cast<QAbstractItemView*>(widget);
    if (list == nullptr || list->model() == nullptr) {
      fail(line, QStringLiteral("no list named '%1'").arg(a.at(0)));
      return;
    }
    bool ok = false;
    const int index = a.at(1).toInt(&ok);
    const QModelIndex item = ok ? list->model()->index(index, 0) : QModelIndex();
    if (!item.isValid()) { fail(line, QStringLiteral("list '%1' has no row %2").arg(a.at(0), a.at(1))); return; }
    // The rows laid out now: a list just shown lays them out later by itself.
    list->doItemsLayout();
    QWidget* viewport = list->viewport();
    const QPoint local = list->visualRect(item).center();
    const QPoint global = viewport->mapToGlobal(local);
    const QPointF scene(viewport->window()->mapFromGlobal(global));
    const auto send = [&](QEvent::Type kind, Qt::MouseButton button, Qt::MouseButtons buttons) {
      QMouseEvent event(kind, QPointF(local), scene, QPointF(global), button, buttons, Qt::NoModifier,
                        QPointingDevice::primaryPointingDevice());
      QCoreApplication::sendEvent(viewport, &event);
    };
    send(QEvent::MouseMove, Qt::NoButton, Qt::NoButton);
    if (name == QLatin1String("click-row")) {
      send(QEvent::MouseButtonPress, Qt::LeftButton, Qt::LeftButton);
      send(QEvent::MouseButtonRelease, Qt::LeftButton, Qt::NoButton);
    }
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
  } else if (name == QLatin1String("context-menu-logged")) {
    QRegularExpression re;
    if (!regex(a.at(0), re)) return;
    const auto at = std::make_shared<QPoint>();
    const auto receiver = std::make_shared<QPointer<QWidget>>();
    const auto window = std::make_shared<QPointer<QWidget>>();
    whenReady([this, re, at, receiver, window] {
      if (TestSync::settling() || !m_main || !loggedPoint(re, 0, *at)) return false;
      *window = cardAt(*at);
      if (!*window) *window = topWindow();
      const QPoint local = (*window)->mapFromGlobal(topWindow()->mapToGlobal(*at));
      *receiver = (*window)->childAt(local);
      return *receiver && (*receiver)->isVisible() && (*receiver)->isEnabled() &&
          (*receiver)->visibleRegion().contains((*receiver)->mapFrom(*window, local));
    }, 6, QStringLiteral("no visible widget at the logged context-menu point '%1'").arg(a.at(0)),
    [this, at, receiver, window, choice = a.at(1), line] {
      const auto open = [this, at, receiver, window, line] {
        if (!*receiver || !*window || !m_main) {
          fail(line, QStringLiteral("the context-menu target was destroyed")); return;
        }
        const QPoint global = topWindow()->mapToGlobal(*at);
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
  } else if (name == QLatin1String("pointer-logged")) {
    const QString phase = a.at(0);
    const bool none = a.at(2) == QStringLiteral("none");
    double dx = 0, dy = 0;
    QRegularExpression re;
    if ((phase != QStringLiteral("press") && phase != QStringLiteral("move") && phase != QStringLiteral("release")) ||
        (!none && a.at(2) != QStringLiteral("left") && a.at(2) != QStringLiteral("right")) ||
        (phase != QStringLiteral("move") && none) || a.size() == 4 ||
        (a.size() == 5 && (!toNumber(a.at(3), dx) || !toNumber(a.at(4), dy)))) {
      fail(line, QStringLiteral("pointer-logged press|move|release <regex> none|left|right [dx dy]")); return;
    }
    if (!regex(a.at(1), re)) return;
    // The right button for a context menu where it is logged (mitcad#2071:
    // the orientation cube's).
    const Qt::MouseButton button = none                               ? Qt::NoButton
                                   : a.at(2) == QStringLiteral("right") ? Qt::RightButton
                                                                        : Qt::LeftButton;
    const auto at = std::make_shared<QPoint>();
    whenReady(
        [this, re, at] {
          if (m_pointerHeld) {
            *at = *m_pointerHeld;
            return true;
          }
          return loggedPoint(re, 0, *at);
        },
        6, QStringLiteral("no place logged for '%1'").arg(a.at(1)),
        [this, phase, at, button, dx, dy] {
          const QPoint to = *at + QPoint(qRound(dx), qRound(dy));
          if (phase == QStringLiteral("press")) {
            m_pointerHeld = to;
          } else if (phase == QStringLiteral("release")) {
            m_pointerHeld.reset();
          }
          queue([this, phase, to, button] {
            const auto type = phase == QStringLiteral("press")     ? QEvent::MouseButtonPress
                              : phase == QStringLiteral("release") ? QEvent::MouseButtonRelease
                                                                   : QEvent::MouseMove;
            sendMouse(type, to, phase == QStringLiteral("move") ? Qt::NoButton : button,
                      phase == QStringLiteral("release") ? Qt::NoButton : button, Qt::NoModifier);
          });
        },
        mainWindow);
  }
}

} // namespace mitcad
