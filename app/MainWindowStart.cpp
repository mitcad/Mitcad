// SPDX-License-Identifier: MIT
// The Start tab (mitcad#1258, GitHub mitcad/Mitcad#125; app/COMMANDS.md
// "The Start tab", files/StartPage.hpp): the ribbon's START tab before
// SOLID, BUILD and MAKE shows the Start page over the 3D view, with the
// window's cards (or docks) hidden while it is there; the page at launch
// by its Show start page at launch; a save's preview of the design; the
// projects' browser's session and state icons (mitcad#1301).
#include "MainWindow.hpp"

#include <algorithm>
#include <utility>

#include <QBuffer>
#include <QByteArray>
#include <QCoreApplication>
#include <QDir>
#include <QDockWidget>
#include <QFileInfo>
#include <QGuiApplication>
#include <QImage>
#include <QJsonObject>
#include <QKeySequence>
#include <QSaveFile>
#include <QStringList>
#include <QTabBar>
#include <QToolBar>
#include <QUrl>
#include <QWindow>
#include <QtLogging>

#include "DocumentFrame.hpp"
#include "OcctViewer.hpp"
#include "WindowManager.hpp"
#include "framework/AppSettings.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/Diagnostics.hpp"
#include "framework/FloatingLayout.hpp"
#include "framework/PaintWatch.hpp"
#include "framework/Ribbon.hpp"
#include "framework/TestSync.hpp"
#include "framework/TitleBar.hpp"
#include "files/Autosave.hpp"
#include "files/FileFormats.hpp"
#include "files/ProjectIndicator.hpp"
#include "files/RecentDesigns.hpp"
#include "files/RemoteController.hpp"
#include "files/StartPage.hpp"
#include "projects/ProjectRowText.hpp"
#include "projects/ProjectsTree.hpp"
#include "report/ReportCenter.hpp"
#include "tutorials/TutorialChoice.hpp"

#ifdef MITCAD_MACOS_NATIVE_UI
#include "platform/NativeMacWindow.hpp"
#endif

namespace mitcad {
namespace {

const QString kStartTab = QStringLiteral("START");

// Where the toolbar panel is once the Start page is left (mitcad#2031),
// for "Toolbar panel after the Start page: ...": "shown at x,y wxh (Floating;
// the window exposed, platform xcb)", or "not shown (<layout>: <why>; ...)":
// the panel or a parent of it hidden, its parts hidden (`partsHidden`, the
// Floating row's), no size, outside the window. The window's exposure and
// the platform tell a panel that is placed but not on the screen (a window
// not exposed yet).
QString toolbarPanelState(const QWidget* panel, const QString& layout, bool partsHidden) {
  if (panel == nullptr) {
    return QStringLiteral("not shown (%1: no toolbar panel)").arg(layout);
  }
  const QWidget* top = panel->window();
  const QWindow* handle = top->windowHandle();
  const QString window = QStringLiteral("%1, platform %2")
                             .arg(handle == nullptr  ? QStringLiteral("the window has no native window")
                                  : handle->isExposed() ? QStringLiteral("the window exposed")
                                                        : QStringLiteral("the window not exposed"),
                                  QGuiApplication::platformName());
  if (!panel->isVisible()) {
    QString hidden = QStringLiteral("the window");
    for (const QWidget* widget = panel; widget != nullptr && widget != top; widget = widget->parentWidget()) {
      if (widget->isHidden()) {
        hidden = widget->objectName().isEmpty() ? QString::fromLatin1(widget->metaObject()->className())
                                                : widget->objectName();
        break;
      }
    }
    return QStringLiteral("not shown (%1: %2 hidden; %3)").arg(layout, hidden, window);
  }
  if (partsHidden) {
    return QStringLiteral("not shown (%1: its parts hidden; %2)").arg(layout, window);
  }
  const QRect place(panel->mapTo(top, QPoint(0, 0)), panel->size());
  if (place.isEmpty()) {
    return QStringLiteral("not shown (%1: no size; %2)").arg(layout, window);
  }
  if (!place.intersects(top->rect())) {
    return QStringLiteral("not shown (%1: outside the window at %2,%3; %4)")
        .arg(layout)
        .arg(place.x())
        .arg(place.y())
        .arg(window);
  }
  return QStringLiteral("shown at %1,%2 %3x%4 (%5; %6)")
      .arg(place.x())
      .arg(place.y())
      .arg(place.width())
      .arg(place.height())
      .arg(layout, window);
}

// The issue list of the tracker the reports go to (a template's address
// without its form: ".../issues/new?title={title}" is ".../issues").
QString issueList() {
  QString tracker = ReportCenter::issueTracker();
  if (tracker.contains(QLatin1Char('{'))) {
    tracker = tracker.section(QLatin1Char('?'), 0, 0);
    if (tracker.endsWith(QLatin1String("/new"))) {
      tracker.chop(4);
    }
  }
  return tracker;
}

} // namespace

void MainWindow::createStartPage() {
  if (m_startPage != nullptr) {
    return;
  }
  StartPage::Host host;
  host.trigger = [this](const QString& id) { trigger(id); };
  host.shortcutText = [this](const QString& id) {
    const QList<QKeySequence> keys = m_registry->effectiveShortcuts(id);
    return keys.isEmpty() ? QString() : keys.constFirst().toString(QKeySequence::NativeText);
  };
  host.open = [this](const QString& path) { openPath(path); };
  host.canOpen = [](const QString& path) { return fileKind(path) != FileKind::None; };
  host.drop = [this](const QStringList& paths) { openDropped(paths); };
  host.defaultUnits = [] { return GeneralSettings::load().newDesignUnits; };
  host.openUrl = [](const QString& url) { openExternalUrl(QUrl(url)); };
  // The workspace tabs, or the toolbar that holds them: the Floating
  // toolbar panel on Windows and Linux (mitcad#1990), the Docked toolbar
  // (its tab row alone over the page, or hidden: updateStartToolbar,
  // mitcad#2082). The page starts right below it.
  host.tabBar = [this]() -> QWidget* {
    if (m_ribbon == nullptr) {
      return nullptr;
    }
    const bool toolbar = m_ribbon->isPanel() || m_ribbon->presentation() == Ribbon::Presentation::Classic;
    return toolbar ? static_cast<QWidget*>(m_ribbon) : m_ribbon->tabBar();
  };
  // The Floating layout's message band over the view's bottom (mitcad#1042):
  // the page's footer keeps above it.
  host.bottomBar = [this]() -> QWidget* {
    return m_floating != nullptr ? m_floating->findChild<QWidget*>(QStringLiteral("messageBandCard")) : nullptr;
  };
  // The AppKit message band is no Qt widget: the room the view's overlays
  // keep free at its bottom, which counts the band while it shows
  // (NativeMacWindow's layoutOverlays, mitcad#1358).
  if (m_nativeUi) {
    host.bottomInset = [this] { return m_viewer->overlayInsets().bottom(); };
  }
  // The projects' browser (mitcad#1301): the projects read with this run's
  // edit-lock session, their state icons as the Projects panel's (the
  // current project's remote as the remote work knows it).
  host.session = [this] { return m_autosave != nullptr ? m_autosave->session() : QString(); };
  host.projectMarks = [this](const ProjectRow& row) {
    if (m_remote != nullptr && m_project.isProject() && sameRoot(row.root, m_project.root)) {
      const ProjectIndicator::Remote remote = m_remote->indicatorState();
      return projectMarks(row, remote.ahead >= 0 ? remote.ahead : row.ahead,
                          remote.behind >= 0 ? remote.behind : row.behind, remote.problem, remote.conflict);
    }
    return projectMarks(row, row.ahead, row.behind, QString(), false);
  };
  // The tutorials to start (mitcad#2346): each installed when needed and
  // started in an empty design of its own (openListedTutorial).
  host.tutorials = [this](std::function<void()> back) -> QWidget* {
    TutorialChoice::Host choice;
    choice.back = std::move(back);
    choice.start = [this](const QString& url, const QJsonObject& tutorial) { openListedTutorial(url, tutorial); };
    return new TutorialChoice(std::move(choice));
  };
  host.version = QCoreApplication::applicationVersion();
  host.issuesUrl = issueList();
  // The version's release page; nothing is fetched (GitHub's releases of
  // the repository README.md names, the tag v<version>).
  host.releaseUrl = QStringLiteral("https://github.com/mitcad/Mitcad/releases/tag/v%1").arg(host.version);
  m_startPage = new StartPage(m_viewer, std::move(host));
  if (m_paintWatch != nullptr) {
    m_paintWatch->watch(m_startPage, QStringLiteral("Start page"));
  }
}

void MainWindow::watchOverlayPaints() {
#ifdef Q_OS_LINUX
  if (m_nativeUi || m_paintWatch != nullptr) {
    return;
  }
  // A headless test's view never draws: a paint counts as presented.
  m_paintWatch = new PaintWatch(headlessUiTest() ? nullptr : m_viewer, this);
  m_paintWatch->watch(m_floating != nullptr ? static_cast<QWidget*>(m_ribbon) : m_dockedPanel,
                      QStringLiteral("toolbar panel"));
  m_paintWatch->watch(m_titleRow, QStringLiteral("title row"));
  if (m_startPage != nullptr) {
    m_paintWatch->watch(m_startPage, QStringLiteral("Start page"));
  }
  if (m_floating != nullptr) {
    // The Floating layout's other rows and cards (the notices, the
    // Browser's card, the navigation bar, ...), by their object names.
    for (QWidget* child : m_floating->findChildren<QWidget*>(QString(), Qt::FindDirectChildrenOnly)) {
      if (child == m_viewer) {
        continue;
      }
      m_paintWatch->watch(child, child->objectName().isEmpty() ? QString::fromLatin1(child->metaObject()->className())
                                                               : child->objectName());
    }
  }
#endif
}

bool MainWindow::startPageShown() const { return m_startPage != nullptr && !m_startPage->isHidden(); }

void MainWindow::updateStartPage(const QString& tab) {
  const bool start = tab == kStartTab;
  if (start == startPageShown()) {
    return;
  }
  if (start) {
    createStartPage();
    // The design's selection goes with the view out of sight (mitcad#1313):
    // nothing selected behind the page, nothing for Delete to remove there
    // or after it; a running command keeps its own selection.
    if (m_mode == Mode::Idle && m_controller != nullptr) {
      clearSelection();
    }
    m_startPage->showPage();
    // The page takes the whole view: no card over it (Floating), no dock
    // beside it (Docked); they come back as they were.
    if (m_floating != nullptr) {
      m_floating->showOnlyCards({QStringLiteral("startPage")});
    } else if (!m_nativeUi) {
      m_startHiddenDocks.clear();
      for (QDockWidget* dock : findChildren<QDockWidget*>(QString(), Qt::FindDirectChildrenOnly)) {
        if (!dock->isHidden()) {
          m_startHiddenDocks << dock->objectName();
          dock->hide();
        }
      }
    }
  } else {
    // What left it (mitcad#2031): a session log tells a page that was left
    // from one that only stopped showing.
    qInfo().noquote() << QStringLiteral("Start page left for %1").arg(tab);
    m_startPage->hidePage();
    // Off the page the window shows a design again (mitcad#2142): the one
    // New or Open installed, or the untitled one that waited under the page.
    if (m_noDesign) {
      m_noDesign = false;
      updateWindowTitle();
      qInfo().noquote() << QStringLiteral("Design shown again after the last design closed");
      if (DocumentFrame* frame = DocumentFrame::containing(this)) {
        frame->documentShownChanged(this);
      }
    }
    if (m_floating != nullptr) {
      // A drawing that opened from the page keeps its Drawings card
      // (mitcad#1008, mitcad#1990).
      m_floating->showOnlyCards(drawingOpen() ? drawingCards() : QStringList());
    } else if (!m_nativeUi) {
      // A drawing opened meanwhile keeps the model's docks hidden
      // (showDocksAgain, mitcad#2082).
      QStringList hidden;
      hidden.swap(m_startHiddenDocks);
      showDocksAgain(hidden);
      m_viewer->setFocus(Qt::OtherFocusReason);
    }
  }
  updateStartToolbar();
  // The toolbar panel hidden by the user (mitcad#2426): no reveal over the
  // page, the top edge's band back after it.
  applyToolbarPanelHidden();
  if (!start) {
    logToolbarPanelAfterStart();
    // Painted and presented, not only placed (PaintWatch, mitcad#2031).
    if (m_paintWatch != nullptr) {
      m_paintWatch->arm(QStringLiteral("after the Start page was left"));
    }
  }
#ifdef MITCAD_MACOS_NATIVE_UI
  if (m_nativeWindow) {
    m_nativeWindow->setStartPageShown(start);
    refreshNativeActions(); // its workspace selector shows the tab
  }
#endif
  updateNavigationBar();
}

void MainWindow::updateStartToolbar() {
  if (m_floating != nullptr || m_nativeUi || m_ribbon == nullptr) {
    return;
  }
  auto* bar = findChild<QToolBar*>(QStringLiteral("ribbonToolbar"), Qt::FindDirectChildrenOnly);
  if (bar == nullptr) {
    return;
  }
  if (m_dockedPanel != nullptr) {
    // The top bar of Windows and Linux (mitcad#2082): the title row stays
    // (Home, and the design's tab that leads back, also with one design);
    // the toolbar panel under it is hidden over the Start page, as the
    // Floating layout hides it (the Start page has no workspace). Also when
    // a kept dock state hid the tool bar (restoreState).
    if (bar->isHidden()) {
      bar->show();
    }
    // Also while View › Toolbar Panel hides it (mitcad#2426); while it is
    // revealed at the top edge the reveal places it.
    const bool start = startPageShown();
    const bool shown = !start && !m_toolbarPanelHidden;
    if (!toolbarPanelRevealed() && m_dockedPanel->isHidden() == shown) {
      m_dockedPanel->setVisible(shown);
    }
    const StartToolbar wanted = start ? StartToolbar::Hidden : StartToolbar::Whole;
    if (wanted != m_startToolbar) {
      m_startToolbar = wanted;
      qDebug().noquote() << (start   ? QStringLiteral("Docked toolbar panel hidden (Start)")
                             : shown ? QStringLiteral("Docked toolbar panel shown")
                                     : QStringLiteral("Docked toolbar panel hidden (View › Toolbar Panel)"));
    }
    return;
  }
  // The Docked toolbar of macOS, without Home: the designs' tabs under the
  // menu bar show for two designs or more
  // (DocumentFrame::showsTabs); with one there is no tab to go back by.
  const DocumentFrame* frame = DocumentFrame::containing(this);
  const bool designTabs = frame != nullptr && frame->showsTabs();
  StartToolbar wanted = StartToolbar::Whole;
  if (startPageShown()) {
    // With no design open (mitcad#2142) there is no workspace to go back
    // to: New and Open lead on.
    wanted = designTabs || m_noDesign ? StartToolbar::Hidden : StartToolbar::TabRow;
  }
  // Also when a kept dock state hid or showed the bar (restoreState). View ›
  // Toolbar Panel hides the whole bar here (mitcad#2426; no reveal at the
  // top edge in this comparison layout).
  const bool hidden = wanted == StartToolbar::Hidden || (wanted == StartToolbar::Whole && m_toolbarPanelHidden);
  if (bar->isHidden() != hidden) {
    bar->setVisible(!hidden);
  }
  m_ribbon->setTabRowOnly(wanted == StartToolbar::TabRow);
  if (wanted == m_startToolbar) {
    return;
  }
  m_startToolbar = wanted;
  switch (wanted) {
  case StartToolbar::Hidden:
    qDebug().noquote() << QStringLiteral("Docked toolbar on the Start page: hidden (the designs' tabs lead back)");
    break;
  case StartToolbar::TabRow:
    qDebug().noquote() << QStringLiteral("Docked toolbar on the Start page: tab row only");
    break;
  case StartToolbar::Whole:
    qDebug().noquote() << QStringLiteral("Docked toolbar back: workspace block, tab row and groups");
    break;
  }
}

void MainWindow::logToolbarPanelAfterStart() {
  if (m_nativeUi || m_ribbon == nullptr) {
    return;
  }
  // Once the layouts that leaving the page queued have placed the panel
  // (FloatingLayout::requestLayout, the tool bar's); watched, so a UI
  // test's sync waits for the line.
  TestSync::singleShot(0, this, [this] {
    if (startPageShown()) {
      return; // back on the page meanwhile
    }
    // The user's state given back (mitcad#2426): hidden, the way back at
    // the top edge.
    if (m_toolbarPanelHidden) {
      qInfo().noquote() << QStringLiteral("Toolbar panel after the Start page: hidden (View › Toolbar Panel)");
      return;
    }
    const QWidget* panel = nullptr;
    QString layout = QStringLiteral("Docked");
    bool partsHidden = false;
    if (m_floating != nullptr) {
      m_floating->layoutNow();
      panel = m_ribbon;
      layout = QStringLiteral("Floating");
      // The row shows while one of its parts does (the workspace selector,
      // the tabs and groups).
      const QList<QWidget*> parts = m_ribbon->panelRowParts();
      partsHidden = std::none_of(parts.cbegin(), parts.cend(),
                                 [](const QWidget* part) { return part != nullptr && !part->isHidden(); });
    } else if (m_dockedPanel != nullptr) {
      panel = m_dockedPanel;
    } else {
      panel = findChild<QToolBar*>(QStringLiteral("ribbonToolbar"), Qt::FindDirectChildrenOnly);
    }
    qInfo().noquote() << QStringLiteral("Toolbar panel after the Start page: %1")
                             .arg(toolbarPanelState(panel, layout, partsHidden));
  });
}

void MainWindow::leaveStartPage() {
  if (startPageShown() && m_ribbon != nullptr) {
    m_ribbon->showTab(QStringLiteral("SOLID"));
  }
}

void MainWindow::leaveStartPageForDesign() {
  if (!startPageShown() || m_ribbon == nullptr) {
    return;
  }
  qDebug().noquote() << QStringLiteral("Start page left by the design's tab");
  if (m_ribbon->hasWorkspaceSelector()) {
    m_ribbon->showWorkspaceTab();
  } else {
    leaveStartPage();
  }
}

bool MainWindow::closeToStartPage() {
  if (m_noDesign) {
    qInfo().noquote() << QStringLiteral("Last design closed already: Start page");
    return true;
  }
  if (deferForNativeCommand([this] { closeToStartPage(); })) {
    return false;
  }
  if (!maybeSave()) {
    if (m_windowManager != nullptr) {
      m_windowManager->cancelQuit(this);
    }
    return false;
  }
  // The design goes (its edit lock with it, installDocument); an empty
  // untitled one waits under the page for New, as the window's design
  // before any is made.
  m_keepStartPage = true;
  const bool emptied = installEmptyDocument(QString());
  m_keepStartPage = false;
  if (!emptied) {
    return false;
  }
  if (m_ribbon != nullptr && !startPageShown()) {
    m_ribbon->showTab(kStartTab);
  }
  m_noDesign = true;
  updateWindowTitle();
  updateStartToolbar();
  if (DocumentFrame* frame = DocumentFrame::containing(this)) {
    frame->documentShownChanged(this);
  }
  qInfo() << "Document closed";
  qInfo().noquote() << QStringLiteral("Last design closed: Start page");
  return true;
}

void MainWindow::showStartPageAtLaunch(const QString& skip) {
  if (m_ribbon == nullptr) {
    return;
  }
  if (!skip.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Start page at launch: off (%1)").arg(skip);
    return;
  }
  if (!recent::showAtLaunch()) {
    qInfo().noquote() << QStringLiteral("Start page at launch: off (Show start page at launch)");
    return;
  }
  qInfo().noquote() << QStringLiteral("Start page at launch: shown");
  m_ribbon->showTab(kStartTab);
}

void MainWindow::newDesignUnitsChanged() {
  // An untitled design nothing is made in yet (the one at start, or one
  // New made: WindowManager::reusable) is a new design still: it starts
  // again in the new default units, as New would make it (mitcad#1452: the
  // units set to in after the start, the untitled design drawn in stayed
  // in mm and took the typed inches as millimetres). A design with
  // anything in it, or read from a file, keeps its units (mitcad#1036).
  const QString units = GeneralSettings::load().newDesignUnits;
  WindowManager* manager = windowManager();
  const QList<MainWindow*> windows = manager != nullptr ? manager->windows() : QList<MainWindow*>{this};
  for (MainWindow* window : windows) {
    if (window == nullptr || manager == nullptr || !manager->reusable(window) || window->lengthUnit() == units) {
      continue;
    }
    // Where it was: under the Start page while that shows.
    window->m_keepStartPage = true;
    const bool restarted = window->installEmptyDocument(window->m_documentName);
    window->m_keepStartPage = false;
    if (restarted) {
      qInfo().noquote() << QStringLiteral("The untitled design starts again in %1").arg(units);
    }
  }
  if (startPageShown()) {
    m_startPage->refresh(); // its New Design card names the units
  }
}

QImage MainWindow::designPreviewImage() const {
  if (m_viewer == nullptr || !m_viewer->isVisible()) {
    return {};
  }
  // The view as it is now, as a version's preview (saveVersionThumbnail);
  // the Start page over it is not in the view's frame.
  const QImage frame = m_viewer->grabFramebuffer();
  if (frame.isNull()) {
    return {};
  }
  return frame.scaled(recent::kPreviewSize, recent::kPreviewSize, Qt::KeepAspectRatio, Qt::SmoothTransformation);
}

QByteArray MainWindow::designPreviewPng(const QImage& image) {
  QByteArray png;
  if (image.isNull()) {
    return png;
  }
  QBuffer buffer(&png);
  if (!buffer.open(QIODevice::WriteOnly) || !image.save(&buffer, "PNG")) {
    return {};
  }
  return png;
}

void MainWindow::saveDesignPreview(const QString& path, const QImage& image) {
  const QString preview = recent::designPreviewPath(path);
  if (preview.isEmpty() || image.isNull()) {
    return;
  }
  QDir().mkpath(QFileInfo(preview).absolutePath());
  QSaveFile out(preview);
  if (!out.open(QIODevice::WriteOnly) || !image.save(&out, "PNG") || !out.commit()) {
    qWarning().noquote() << QStringLiteral("Design preview not saved: %1").arg(out.errorString());
    return;
  }
  qInfo().noquote() << QStringLiteral("Design preview saved: %1 (%2 x %3)")
                           .arg(QFileInfo(path).fileName())
                           .arg(image.width())
                           .arg(image.height());
  if (startPageShown()) {
    m_startPage->refresh();
  }
}

} // namespace mitcad
