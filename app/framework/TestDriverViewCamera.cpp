// SPDX-License-Identifier: MIT
// Test steps for the 3D view's camera: its orientation, pan, zoom and height, a pinch, a
// trackpad's scroll, a burst of wheel notches and the 3D mouse.
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
    {"wheel-burst", 4, 5, TestDriver::kInputOfMain},
    {"trackpad-scroll", 5, 6, TestDriver::kInputOfMain},
    {"native-pinch", 3, 3, TestDriver::kInputOfMain},
    {"qt-pinch", 3, 3, TestDriver::kInputOfMain},
    {"capture-view-height", 1, 1, TestDriver::kIdle},
    {"expect-view-zoom", 2, 4, TestDriver::kIdle},
    {"expect-view-pan", 2, 2, TestDriver::kIdle},
    // A pan's direction against an earlier one (mitcad#1873).
    {"expect-view-pan-along", 4, 4, TestDriver::kIdle},
    {"capture-view-orientation", 1, 1, TestDriver::kIdle},
    {"expect-view-orientation", 2, 2, TestDriver::kIdle},
    // A 3D mouse's axes and buttons (mitcad#951).
    {"space-mouse", 6, 7, TestDriver::kInputOfMain},
    {"space-mouse-button", 1, 1, TestDriver::kInputOfMain},
};

} // namespace

const TestDriver::StepArea TestDriver::kViewCameraSteps{
    kStepSpecs, std::size(kStepSpecs), &TestDriver::runViewCameraStep};

void TestDriver::runViewCameraStep(const QString& name, const QStringList& a, int line) {
  if (name == QLatin1String("capture-view-orientation") || name == QLatin1String("expect-view-orientation")) {
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
  } else if (name == QLatin1String("expect-view-pan-along")) {
    // expect-view-pan-along <from> <to> <since> same|opposite (mitcad#1873):
    // the camera target's move since <since> goes the way of the move from
    // <from> to <to>, or the other way (within 45 degrees of it).
    std::array<gp_Pnt, 3> points;
    bool parsed = a.at(3) == QLatin1String("same") || a.at(3) == QLatin1String("opposite");
    for (int i = 0; parsed && i < 3; ++i) {
      const QStringList coordinates = a.at(i).split(QLatin1Char(','));
      double px = 0, py = 0, pz = 0;
      parsed = coordinates.size() == 3 && toNumber(coordinates.at(0), px) && toNumber(coordinates.at(1), py) &&
               toNumber(coordinates.at(2), pz);
      points[std::size_t(i)] = gp_Pnt(px, py, pz);
    }
    if (!parsed) {
      fail(line, QStringLiteral("expect-view-pan-along <from-target> <to-target> <since-target> same|opposite")); return;
    }
    const gp_Vec reference(points[0], points[1]);
    if (reference.Magnitude() <= 1e-6) {
      fail(line, QStringLiteral("expect-view-pan-along: the reference pan did not move the camera target")); return;
    }
    const bool same = a.at(3) == QLatin1String("same");
    const gp_Pnt since = points[2];
    whenReady([this, reference, since, same] {
      auto* view = qobject_cast<OcctViewer*>(viewer());
      if (!view) return false;
      const gp_Vec moved(since, view->camera().target);
      if (moved.Magnitude() <= 1e-6) {
        m_waitDetail = QStringLiteral("the camera target has not moved");
        return false;
      }
      const double cosine = reference.Dot(moved) / (reference.Magnitude() * moved.Magnitude());
      m_waitDetail = QStringLiteral("the cosine between the pans' directions is %1").arg(cosine, 0, 'f', 3);
      return same ? cosine >= std::sqrt(0.5) : cosine <= -std::sqrt(0.5);
    }, 6, QStringLiteral("the camera did not pan the %1 way").arg(same ? QStringLiteral("same") : QStringLiteral("opposite")),
    [same] {
      qInfo().noquote() << QStringLiteral("TestDriver: the view panned the %1 way")
                               .arg(same ? QStringLiteral("same") : QStringLiteral("opposite"));
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
      const QPoint global = topWindow()->mapToGlobal(at);
      QWheelEvent event(QPointF(view->mapFromGlobal(global)), QPointF(global),
                        QPoint(qRound(dx), qRound(dy)), QPoint(), Qt::NoButton,
                        modifiers, momentum ? Qt::ScrollMomentum : Qt::ScrollUpdate, false);
      QCoreApplication::sendEvent(view, &event);
    });
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
    // Held for the time, then released, as a hand on the cap. Given to the
    // process's 3D mouse connection, as the backend gives the device's
    // input (mitcad#2297): it reaches the active design's view only.
    window->spaceMouse()->connection().motion(axes, false);
    const auto clock = std::make_shared<QElapsedTimer>();
    clock->start();
    whenReady([clock, milliseconds] { return clock->elapsed() >= milliseconds; }, milliseconds / 1000.0 + 5,
              QStringLiteral("space-mouse"), [this] {
                if (auto* owner = qobject_cast<MainWindow*>(m_main.data()); owner && owner->spaceMouse()) {
                  owner->spaceMouse()->connection().motion(SpaceMouseAxes{}, false);
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
    window->spaceMouse()->connection().button(number - 1, true);
    window->spaceMouse()->connection().button(number - 1, false);

    // --- The system
  }
}

} // namespace mitcad
