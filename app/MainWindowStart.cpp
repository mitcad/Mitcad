// SPDX-License-Identifier: MIT
// The Start tab (mitcad#1258, GitHub mitcad/Mitcad#125; app/COMMANDS.md
// "The Start tab", files/StartPage.hpp): the ribbon's START tab before
// SOLID, BUILD and MAKE shows the Start page over the 3D view, with the
// window's cards (or docks) hidden while it is there; the page at launch
// by its Show start page at launch; a save's preview of the design; the
// projects' browser's session and state icons (mitcad#1301).
#include "MainWindow.hpp"

#include <utility>

#include <QCoreApplication>
#include <QDir>
#include <QDockWidget>
#include <QFileInfo>
#include <QImage>
#include <QKeySequence>
#include <QSaveFile>
#include <QStringList>
#include <QTabBar>
#include <QUrl>
#include <QtLogging>

#include "OcctViewer.hpp"
#include "framework/AppSettings.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/FloatingLayout.hpp"
#include "framework/Ribbon.hpp"
#include "files/Autosave.hpp"
#include "files/FileFormats.hpp"
#include "files/ProjectIndicator.hpp"
#include "files/RecentDesigns.hpp"
#include "files/RemoteController.hpp"
#include "files/StartPage.hpp"
#include "projects/ProjectRowText.hpp"
#include "projects/ProjectsTree.hpp"
#include "report/ReportCenter.hpp"

#ifdef MITCAD_MACOS_NATIVE_UI
#include "platform/NativeMacWindow.hpp"
#endif

namespace mitcad {
namespace {

const QString kStartTab = QStringLiteral("START");

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
  host.tabBar = [this]() -> QWidget* { return m_ribbon != nullptr ? m_ribbon->tabBar() : nullptr; };
  // The Floating layout's message band over the view's bottom (mitcad#1042):
  // the page's footer keeps above it.
  host.bottomBar = [this]() -> QWidget* {
    return m_floating != nullptr ? m_floating->findChild<QWidget*>(QStringLiteral("messageBandCard")) : nullptr;
  };
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
  host.version = QCoreApplication::applicationVersion();
  host.issuesUrl = issueList();
  // The version's release page; nothing is fetched (GitHub's releases of
  // the repository README.md names, the tag v<version>).
  host.releaseUrl = QStringLiteral("https://github.com/mitcad/Mitcad/releases/tag/v%1").arg(host.version);
  m_startPage = new StartPage(m_viewer, std::move(host));
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
    m_startPage->hidePage();
    if (m_floating != nullptr) {
      // A drawing that opened from the page keeps its browser (mitcad#1008).
      m_floating->showOnlyCards(drawingOpen() ? QStringList{QStringLiteral("browserDock")} : QStringList());
    } else if (!m_nativeUi) {
      for (const QString& name : std::as_const(m_startHiddenDocks)) {
        if (auto* dock = findChild<QDockWidget*>(name, Qt::FindDirectChildrenOnly)) {
          dock->show();
        }
      }
      m_startHiddenDocks.clear();
      m_viewer->setFocus(Qt::OtherFocusReason);
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

void MainWindow::leaveStartPage() {
  if (startPageShown() && m_ribbon != nullptr) {
    m_ribbon->showTab(QStringLiteral("SOLID"));
  }
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

void MainWindow::saveDesignPreview(const QString& path) {
  const QString preview = recent::designPreviewPath(path);
  if (preview.isEmpty() || m_viewer == nullptr || !m_viewer->isVisible()) {
    return;
  }
  // The view as it is now, as a version's preview (saveVersionThumbnail);
  // the Start page over it is not in the view's frame.
  const QImage frame = m_viewer->grabFramebuffer();
  if (frame.isNull()) {
    return;
  }
  const QImage image =
      frame.scaled(recent::kPreviewSize, recent::kPreviewSize, Qt::KeepAspectRatio, Qt::SmoothTransformation);
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
