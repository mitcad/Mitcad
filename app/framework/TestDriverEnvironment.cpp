// SPDX-License-Identifier: MIT
// Test steps for the application's surroundings: settings, the update offer, the colour
// scheme, test switches, a screen change and a frozen UI.
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
    {"expect-setting", 2, 2, TestDriver::kIdle},
    {"set-setting", 2, 2, TestDriver::kIdle},
    // The update tests' fake offer (mitcad#1890).
    {"update-fake-offer", 1, 1, TestDriver::kIdle},
    // The hang marker and the session log (mitcad#1896).
    {"freeze-ui", 1, 1, TestDriver::kIdle},
    // The colour scheme switched at a step (mitcad#1104).
    {"color-scheme", 1, 1, TestDriver::kPlain},
    // A test switch (MITCAD_TEST_*) set or cleared while the app runs:
    // MITCAD_TEST_GL_FAILURE's failure ends (mitcad#2125).
    {"set-env", 2, 2, TestDriver::kPlain},
    {"unset-env", 1, 1, TestDriver::kPlain},
    // Every shown window told that its screen changed, as when the system
    // announces its screens again (mitcad#2125).
    {"screen-change", 0, 0, TestDriver::kPlain},
};

} // namespace

const TestDriver::StepArea TestDriver::kEnvironmentSteps{
    kStepSpecs, std::size(kStepSpecs), &TestDriver::runEnvironmentStep};

void TestDriver::runEnvironmentStep(const QString& name, const QStringList& a, int line) {
  if (name == QLatin1String("expect-setting")) {
    const QVariant value = QSettings().value(a.at(0));
    if (!value.isValid() || value.toString() != a.at(1)) {
      fail(line, QStringLiteral("setting %1 is '%2', expected '%3'").arg(a.at(0), value.toString(), a.at(1)));
    }
  } else if (name == QLatin1String("set-setting")) {
    // A saved QSettings value, as Preferences would save it (text).
    QSettings().setValue(a.at(0), a.at(1));
    qInfo().noquote() << QStringLiteral("TestDriver: setting %1 = %2").arg(a.at(0), a.at(1));
  } else if (name == QLatin1String("update-fake-offer")) {
    // The update offered from now on is the tests' fake one (mitcad#1890):
    // no request, "downloaded" in the seconds given, never installed.
    double offerSeconds = 0;
    if (!toNumber(a.at(0), offerSeconds) || offerSeconds < 1) {
      fail(line, QStringLiteral("update-fake-offer <seconds of its download, 1 or more>"));
      return;
    }
    UpdateController::application().useTestFakeOffer(static_cast<int>(offerSeconds));
  } else if (name == QLatin1String("freeze-ui")) {
    // A hang of the UI thread for the hang marker (mitcad#1896): the event
    // loop does not run for the time.
    double milliseconds = 0;
    if (!toNumber(a.at(0), milliseconds) || milliseconds < 0 || milliseconds > 60000) {
      fail(line, QStringLiteral("freeze-ui <0..60000 ms>, not '%1'").arg(a.at(0)));
      return;
    }
    const int ms = static_cast<int>(milliseconds);
    HangWatchdog::noteActivity(QStringLiteral("test step freeze-ui %1 ms").arg(ms));
    qDebug().noquote() << QStringLiteral("TestDriver: freezing the UI thread for %1 ms").arg(ms);
    QThread::msleep(static_cast<unsigned long>(ms));
  } else if (name == QLatin1String("color-scheme")) {
    // As MITCAD_TEST_COLOR_SCHEME at start (main.cpp), at this step
    // (mitcad#1104): after rows were styled, say.
    const QString scheme = a.at(0);
    if (scheme != QLatin1String("light") && scheme != QLatin1String("dark")) {
      fail(line, QStringLiteral("color-scheme light|dark, not '%1'").arg(scheme));
      return;
    }
    qDebug().noquote() << QStringLiteral("Asking the platform for the %1 colour scheme").arg(scheme);
    QGuiApplication::styleHints()->setColorScheme(scheme == QLatin1String("dark") ? Qt::ColorScheme::Dark
                                                                                  : Qt::ColorScheme::Light);
  } else if (name == QLatin1String("set-env") || name == QLatin1String("unset-env")) {
    // Only the app's test switches, which it reads again when it needs
    // them (mitcad#2125).
    if (!a.at(0).startsWith(QLatin1String("MITCAD_TEST_"))) {
      fail(line, QStringLiteral("%1 takes a MITCAD_TEST_ variable, not '%2'").arg(name, a.at(0)));
      return;
    }
    const QByteArray variable = a.at(0).toUtf8();
    if (name == QLatin1String("set-env")) {
      qputenv(variable.constData(), a.at(1).toUtf8());
      qDebug().noquote() << QStringLiteral("TestDriver: %1 set to %2").arg(a.at(0), a.at(1));
    } else {
      qunsetenv(variable.constData());
      qDebug().noquote() << QStringLiteral("TestDriver: %1 cleared").arg(a.at(0));
    }
  } else if (name == QLatin1String("screen-change")) {
    // A simulated screen change (mitcad#2125): every shown top-level
    // window is told that its screen changed, on the screen it is on, as
    // when the system announces its screens again (a graphics driver
    // reset). Qt's widgets and whatever listens (Component from Web's
    // view, the 3D view under MITCAD_TEST_GL_FAILURE) take it as a real one.
    int told = 0;
    const QWindowList windows = QGuiApplication::topLevelWindows();
    for (QWindow* window : windows) {
      if (window == nullptr || !window->isVisible() || window->screen() == nullptr) {
        continue;
      }
      emit window->screenChanged(window->screen());
      ++told;
    }
    qDebug().noquote() << QStringLiteral("Screen change told to %1 window(s)").arg(told);
  }
}

} // namespace mitcad
