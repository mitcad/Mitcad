// SPDX-License-Identifier: MIT
// Test steps for the design windows and their tabs: windows captured, focused and checked, the
// system window menu and menu bar, a window's state, place and size.
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

// The design's tabs: the system window tabs of the 0.48 macOS presentation,
// else the document frame's own (the shared Qt UI; one design, one tab).
int documentTabCount(MainWindow* window) {
#ifdef MITCAD_MAC_UI
  return mac::documentWindowTabCount(window);
#else
  const DocumentFrame* frame = DocumentFrame::containing(window);
  return frame != nullptr ? int(frame->documents().size()) : 1;
#endif
}

bool documentTabSelected(MainWindow* window) {
#ifdef MITCAD_MAC_UI
  return mac::documentWindowTabSelected(window);
#else
  const DocumentFrame* frame = DocumentFrame::containing(window);
  return frame != nullptr ? frame->activeDocument() == window : window->isActiveWindow();
#endif
}

const TestDriver::StepSpec kStepSpecs[] = {
    {"capture-document-window", 1, 1, TestDriver::kIdle},
    {"focus-document-window", 1, 1, TestDriver::kInputOfMain},
    {"focus-document-path", 1, 1, TestDriver::kInputOfMain},
    {"system-window-menu", 1, 1, TestDriver::kInputOfMain},
#ifdef Q_OS_MACOS
    {"expect-system-menu-bar", 1, -1, TestDriver::kIdle},
    // An item of the AppKit menu bar by its title (a regex) and its key
    // equivalent (none when omitted): a command with a key sequence shows
    // it in its title and has none (mitcad#2095).
    {"expect-system-menu-item", 1, 2, TestDriver::kIdle},
#endif
    {"expect-document-windows", 1, 1, TestDriver::kIdle},
    {"expect-document-window-gone", 1, 1, TestDriver::kIdle},
    {"expect-document-window-active", 1, 1, TestDriver::kIdle},
    {"expect-document-tabs", 1, 1, TestDriver::kIdle},
    {"expect-document-tab-selected", 1, 1, TestDriver::kIdle},
    {"native-resize", 2, 2, TestDriver::kInputOfMain},
    {"window-resize", 2, 2, TestDriver::kInputOfMain},
    {"window-state", 1, 1, TestDriver::kIdle},
    {"expect-window-state", 1, 1, TestDriver::kIdle},
    {"window-move", 2, 2, TestDriver::kIdle},
};

} // namespace

const TestDriver::StepArea TestDriver::kDocumentWindowSteps{
    kStepSpecs, std::size(kStepSpecs), &TestDriver::runDocumentWindowStep};

void TestDriver::runDocumentWindowStep(const QString& name, const QStringList& a, int line) {
  QWidget* mainWindow = m_main.data();
  if (name == QLatin1String("capture-document-window")) {
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    if (!window) { fail(line, QStringLiteral("capture-document-window requires a live document")); return; }
    m_documentWindows.insert(a.at(0), window);
  } else if (name == QLatin1String("focus-document-window")) {
    const QPointer<QWidget> captured = m_documentWindows.value(a.at(0));
    auto* window = qobject_cast<MainWindow*>(captured.data());
    WindowManager* manager = WindowManager::instance();
    if (!window || !manager || !manager->windows().contains(window)) {
      fail(line, QStringLiteral("document window '%1' is not open").arg(a.at(0))); return;
    }
    manager->activateWindow(window);
    findMain();
  } else if (name == QLatin1String("focus-document-path")) {
    WindowManager* manager = WindowManager::instance();
    MainWindow* window = manager ? manager->findByPath(a.at(0)) : nullptr;
    if (!window) { fail(line, QStringLiteral("document path '%1' is not open").arg(a.at(0))); return; }
    manager->activateWindow(window);
    findMain();
#ifdef Q_OS_MACOS
  } else if (name == QLatin1String("expect-system-menu-bar")) {
    const QPointer<DocumentFrame> frame = DocumentFrame::containing(qobject_cast<MainWindow*>(mainWindow));
    whenReady([this, frame, expected = a] {
      return frame && mac::systemMenuBarMatches(*frame->menuBar(), expected, m_waitDetail);
    }, 5, QStringLiteral("the native system menu bar or application roles did not match [%1]").arg(a.join(QStringLiteral(", "))));
  } else if (name == QLatin1String("expect-system-menu-item")) {
    if (QGuiApplication::platformName().startsWith(QLatin1String("offscreen"))) {
      // Qt's offscreen platform has no AppKit menu bar.
      qInfo().noquote() << QStringLiteral("TestDriver: system menu item not checked (offscreen): %1").arg(a.at(0));
    } else {
      whenReady([this, title = a.at(0), key = a.value(1)] {
        return mac::systemMenuItemMatches(title, key, m_waitDetail);
      }, 5, QStringLiteral("no AppKit menu item matched %1 with the key equivalent '%2'").arg(a.at(0), a.value(1)));
    }
#endif
  } else if (name == QLatin1String("system-window-menu")) {
    const QPointer<QWidget> presenter = mainWindow;
    whenReady([this, presenter, entry = a.at(0)] {
      if (!presenter) return false;
      QString error;
      const bool dispatched = mac::triggerDocumentWindowMenu(presenter, entry, error);
      m_waitDetail = error;
      return dispatched;
    }, 6, QStringLiteral("the owned document's system Window menu could not dispatch '%1'").arg(a.at(0)));
  } else if (name == QLatin1String("expect-document-windows")) {
    bool valid = false;
    const int expected = a.at(0).toInt(&valid);
    if (!valid || expected < 0) { fail(line, QStringLiteral("expected a non-negative document count")); return; }
    const QPointer<WindowManager> manager = WindowManager::instance();
    whenReady([manager, expected] { return manager && manager->windows().size() == expected; }, 5,
              QStringLiteral("expected %1 open document windows").arg(expected));
  } else if (name == QLatin1String("expect-document-tabs")) {
    bool valid = false;
    const int expected = a.at(0).toInt(&valid);
    const QPointer<MainWindow> window = qobject_cast<MainWindow*>(mainWindow);
    if (!valid || expected < 1 || !window) {
      fail(line, QStringLiteral("expected a positive native document tab count")); return;
    }
    whenReady([window, expected] { return window && documentTabCount(window) == expected; }, 5,
              QStringLiteral("expected %1 actual native document tabs").arg(expected));
  } else if (name == QLatin1String("expect-document-tab-selected")) {
    const QPointer<MainWindow> captured = qobject_cast<MainWindow*>(m_documentWindows.value(a.at(0)).data());
    const QPointer<WindowManager> manager = WindowManager::instance();
    if (!captured || !manager) {
      fail(line, QStringLiteral("document window '%1' was not captured or is closed").arg(a.at(0))); return;
    }
    whenReady([manager, captured] {
      return manager && captured && manager->windows().contains(captured.data()) &&
          manager->activeWindow() == captured.data() && documentTabSelected(captured);
    }, 5, QStringLiteral("document window '%1' is not both the logical active document and actual selected native tab").arg(a.at(0)));
  } else if (name == QLatin1String("expect-document-window-active")) {
    const QPointer<QWidget> captured = m_documentWindows.value(a.at(0));
    const QPointer<WindowManager> manager = WindowManager::instance();
    if (!captured || !manager) {
      fail(line, QStringLiteral("document window '%1' was not captured or is closed").arg(a.at(0))); return;
    }
    whenReady([manager, captured] { return manager && captured && manager->activeWindow() == captured.data(); },
              5, QStringLiteral("document window '%1' did not become active").arg(a.at(0)));
  } else if (name == QLatin1String("expect-document-window-gone")) {
    if (!m_documentWindows.contains(a.at(0))) {
      fail(line, QStringLiteral("document window '%1' was not captured").arg(a.at(0))); return;
    }
    const QPointer<QWidget> captured = m_documentWindows.value(a.at(0));
    const QPointer<WindowManager> manager = WindowManager::instance();
    whenReady([manager, captured] {
      auto* window = qobject_cast<MainWindow*>(captured.data());
      return !window || (manager && !manager->windows().contains(window));
    }, 5, QStringLiteral("document window '%1' remained open").arg(a.at(0)));
  } else if (name == QLatin1String("window-state") || name == QLatin1String("expect-window-state")) {
    const QString state = a.at(0);
    if (state != QLatin1String("normal") && state != QLatin1String("minimized") && state != QLatin1String("fullscreen") &&
        state != QLatin1String("maximized")) {
      fail(line, QStringLiteral("%1 normal|minimized|maximized|fullscreen").arg(name)); return;
    }
    if (name == QLatin1String("window-state")) {
      queue([this, state, line] {
        if (mac::test::active()) {
          QString error;
          if (!mac::test::setDocumentState(state, error)) fail(line, error);
        } else if (m_main) {
          if (state == QLatin1String("minimized")) m_main->window()->showMinimized();
          else if (state == QLatin1String("fullscreen")) m_main->window()->showFullScreen();
          else if (state == QLatin1String("maximized")) m_main->window()->showMaximized();
          else m_main->window()->showNormal();
        } else fail(line, QStringLiteral("window-state requires a main window"));
      });
    } else {
      whenReady([this, state] {
        if (mac::test::active()) return mac::test::documentState() == state;
        return m_main && (m_main->window()->isMinimized() ? QStringLiteral("minimized") :
                         m_main->window()->isFullScreen() ? QStringLiteral("fullscreen") :
                         m_main->window()->isMaximized() ? QStringLiteral("maximized") : QStringLiteral("normal")) == state;
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
        m_main->window()->move(m_main->screen()->availableGeometry().topLeft() + QPoint(qRound(x), qRound(y)));
      } else fail(line, QStringLiteral("window-move requires a main window and screen"));
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
        m_main->window()->resize(int(width), int(height));
      } else {
        fail(line, QStringLiteral("window-resize requires a main window"));
      }
    });
    m_notBefore = std::max(m_notBefore, m_clock.elapsed() + 300);
  }
}

} // namespace mitcad
