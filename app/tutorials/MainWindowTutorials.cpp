// SPDX-License-Identifier: MIT
// In-app tutorials (mitcad#2012, docs/tutorials.md): the tutorial pane, a
// tutorial package's pages on MitCad's web browser component (CEF,
// webbrowser/TutorialPageView in the component's module) at the left of the
// design's window in both chrome styles, at the right or centred over the
// view as its page asks (mitcad#2367), the component downloaded first when
// it is missing; and the bridge window.mitcad that its pages (of the
// package's own folder only) call: guide steps, the pane's place, the page's
// input demonstrations over a step's target and the learner's keys and
// gestures (mitcad#2366), the package's starting design opened, the
// application's state read, read-only queries of the window's design
// (mitcad#2371), Leave Tutorial, and the guide's events back. No file access
// and no model edits through it.
#include "../MainWindow.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <functional>
#include <utility>

#include <QCoreApplication>
#include <QDateTime>
#include <QDir>
#include <QDockWidget>
#include <QFile>
#include <QFileInfo>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QJsonDocument>
#include <QLabel>
#include <QPropertyAnimation>
#include <QPushButton>
#include <QStandardPaths>
#include <QTimer>
#include <QVBoxLayout>
#include <QtLogging>

#include "../files/Libraries.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/FloatingLayout.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/Json.hpp"
#include "../framework/PaneMotion.hpp"
#include "../sketch/SketchController.hpp"
#include "../webbrowser/BrowserComponent.hpp"
#include "../webbrowser/BrowserModuleLoad.hpp"
#include "../webbrowser/ComponentBrowserHost.hpp"
#include "../files/StartPage.hpp"
#include "../framework/Ribbon.hpp"
#include "GuideBindings.hpp"
#include "GuideDemonstration.hpp"
#include "GuideStep.hpp"
#include "TutorialChoice.hpp"
#include "TutorialGuide.hpp"
#include "TutorialSources.hpp"

namespace mitcad {
namespace {

// What the tutorial's page asks of MitCad: the bridge's calls, the
// component's and the profile's folders. A child of the page's widget.
class TutorialHost final : public QObject, public TutorialPageHost {
public:
  using Call = std::function<bool(const QString& request, QString* answer, QString* error)>;
  TutorialHost(Call call, QString folder) : m_call(std::move(call)), m_folder(std::move(folder)) {}
  bool bridgeCall(const QString& request, QString* answer, QString* error) override {
    return m_call(request, answer, error);
  }
  QString componentFolder() override { return m_folder; }
  QString profileFolder() override { return browserProfileFolder(); }

private:
  Call m_call;
  QString m_folder;
};

// Whether `path` is inside `folder` (both as they are on the disk).
bool inside(const QString& folder, const QString& path) {
  const QString root = QFileInfo(folder).canonicalFilePath();
  const QString file = QFileInfo(path).canonicalFilePath();
#ifdef Q_OS_WIN
  const Qt::CaseSensitivity sensitivity = Qt::CaseInsensitive;
#else
  const Qt::CaseSensitivity sensitivity = Qt::CaseSensitive;
#endif
  return !root.isEmpty() && !file.isEmpty() && file.startsWith(root + QLatin1Char('/'), sensitivity);
}

// Where a tutorial's starting designs are copied to be worked on: the
// local data folder's tutorial-designs/<package> (MITCAD_TUTORIALS_WORK_DIR
// for tests).
QString tutorialWorkFolder(const QString& package) {
  const QString given = qEnvironmentVariable("MITCAD_TUTORIALS_WORK_DIR");
  const QString root = given.isEmpty() ? QDir(QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation))
                                             .filePath(QStringLiteral("tutorial-designs"))
                                       : QDir::cleanPath(given);
  return QDir(root).filePath(package);
}

} // namespace

void MainWindow::showTutorials() {
  // The Start page's tutorials (mitcad#2346, TutorialChoice), from a design
  // too: the page shows them in its centre, where Start starts the chosen
  // one in an empty design of its own.
  if (m_ribbon == nullptr) {
    qWarning().noquote() << QStringLiteral("Tutorials: no Start page in this window");
    return;
  }
  createStartPage();
  m_startPage->showTutorials();
  if (!startPageShown()) {
    m_ribbon->showTab(QStringLiteral("START"));
  }
  qInfo().noquote() << QStringLiteral("Start page tutorials shown");
}

void MainWindow::openListedTutorial(const QString& url, const QJsonObject& tutorial) {
  const QString id = tutorial.value(QStringLiteral("id")).toString();
  const QString title = tutorial.value(QStringLiteral("title")).toString();
  const QString oldest = tutorial.value(QStringLiteral("min_mitcad")).toString();
  const QString version = QCoreApplication::applicationVersion();
  if (!version.isEmpty() && compareVersions(version, oldest) < 0) {
    qInfo().noquote() << QStringLiteral("Tutorial: %1 needs MitCad %2, this is %3").arg(id, oldest, version);
    sheetWarning(this, tr("Tutorials"), tr("%1 needs MitCad %2 or newer.").arg(title, oldest));
    return;
  }
  // Installed, or updated, at the version of its source fetched last.
  const QJsonObject answer = libraryCommand({{QStringLiteral("cmd"), QStringLiteral("tutorial_install")},
                                             {QStringLiteral("url"), url},
                                             {QStringLiteral("id"), id},
                                             {QStringLiteral("folder"), tutorialsDirectory()}});
  if (answer.contains(QStringLiteral("error"))) {
    const QString reason = answer.value(QStringLiteral("error")).toObject().value(QStringLiteral("message")).toString();
    qWarning().noquote() << QStringLiteral("Tutorial: %1 was not installed: %2").arg(id, reason);
    sheetWarning(this, tr("Tutorials"), tr("The tutorial could not be installed:\n%1").arg(reason));
    return;
  }
  const QString folder = answer.value(QStringLiteral("folder")).toString();
  qInfo().noquote() << QStringLiteral("Tutorial: installed %1 %2 (%3 files) into %4")
                           .arg(id, tutorial.value(QStringLiteral("version")).toString())
                           .arg(answer.value(QStringLiteral("files")).toInt())
                           .arg(QDir::toNativeSeparators(folder));
  openTutorialInDesign(folder, answer.value(QStringLiteral("start")).toString(), title);
}

void MainWindow::openTutorialInDesign(const QString& folder, const QString& page, const QString& title) {
  if (!startPageShown()) {
    openTutorialPane(folder, page, title);
    return;
  }
  // Over the Start page the web browser component comes first (mitcad#2311):
  // a declined or failed download leaves the page, the design under it and
  // the windows as they were, with no empty new design left open.
  withTutorialComponent([this, folder, page, title](const BrowserComponentInfo& component) {
    return showTutorialInDesign(component, folder, page, title);
  });
}

bool MainWindow::showTutorialInDesign(const BrowserComponentInfo& component, const QString& folder,
                                      const QString& page, const QString& title) {
  if (!startPageShown()) {
    // The page was left while the component was downloaded.
    return showTutorialPane(component, folder, page, title);
  }
  // Over the Start page (mitcad#2168) the tutorial starts in an empty
  // design of its own, never in the design under the page: its first
  // step's target is on the design's toolbar, and the page's own controls
  // are no part of any step.
  if (modelBusy()) {
    whenIdle(this, [this, component, folder, page, title] { showTutorialInDesign(component, folder, page, title); });
    return true;
  }
  if (m_noDesign || emptyUntitledDesign()) {
    // The empty untitled design that waits under the page (the launch's,
    // or the one a close to the page installed): the page is left for it.
    qInfo().noquote() << QStringLiteral("Tutorial: starts in the empty design under the Start page");
    leaveStartPage();
    return showTutorialPane(component, folder, page, title);
  }
  // The user's design: New Design, in a new window or (Open in current
  // window) in this one after its unsaved-changes question.
  qInfo().noquote() << QStringLiteral("Tutorial: starts in a new design");
  QPointer<MainWindow> target = newDocument();
  if (target.isNull()) {
    qInfo().noquote() << QStringLiteral("Tutorial: not opened (New Design was cancelled)");
    return false;
  }
  return target->showTutorialPane(component, folder, page, title);
}

bool MainWindow::tutorialOpen() const { return !m_tutorialPageWidget.isNull(); }

void MainWindow::openTutorialPane(const QString& folder, const QString& page, const QString& title) {
  withTutorialComponent([this, folder, page, title](const BrowserComponentInfo& component) {
    return showTutorialPane(component, folder, page, title);
  });
}

void MainWindow::withTutorialComponent(const std::function<bool(const BrowserComponentInfo&)>& then) {
  withBrowserComponent(tr("Tutorials"),
                       tr("Tutorials are shown in MitCad's web browser component (the Chromium Embedded Framework "
                          "%1), a download of %2. Download it now?"),
                       then);
}

bool MainWindow::showTutorialPane(const BrowserComponentInfo& component, const QString& folder, const QString& page,
                                  const QString& title) {
  QString componentFolder;
  QString error;
  const CreateTutorialPageFunction create = loadTutorialPageModule(component, &componentFolder, &error);
  if (create == nullptr) {
    sheetWarning(this, tr("Tutorials"), tr("The web browser could not be loaded:\n%1").arg(error));
    return false;
  }
  // One tutorial at a time: another one replaces it.
  if (!m_tutorialPageWidget.isNull()) {
    if (m_guide != nullptr) {
      m_guide->end(QStringLiteral("closed"));
    }
    delete m_tutorialPageWidget.data();
  }
  ensureTutorialPane();
  // A tutorial opens at the left (mitcad#2367); its page places it.
  if (m_tutorialPlace != TutorialPlace::Left) {
    placeTutorialPane(TutorialPlace::Left, false);
  }
  auto* host = new TutorialHost(
      [this](const QString& request, QString* answer, QString* reason) {
        return tutorialBridgeCall(request, answer, reason);
      },
      componentFolder);
  TutorialPage* tutorialPage = create(host, folder, m_tutorialPane);
  if (tutorialPage == nullptr) {
    delete host;
    sheetWarning(this, tr("Tutorials"), tr("The web browser could not be started."));
    return false;
  }
  QWidget* widget = tutorialPage->widget();
  host->setParent(widget);
  m_tutorialPage = tutorialPage;
  m_tutorialPageWidget = widget;
  m_tutorialFolder = folder;
  m_tutorialTitle->setText(title);
  static_cast<QVBoxLayout*>(m_tutorialPane->layout())->addWidget(widget, 1);
  // The guide's events to the page while it lives.
  connect(guide(), &TutorialGuide::guideEvent, widget, [this](const QJsonObject& event) {
    if (m_tutorialPage != nullptr && !m_tutorialPageWidget.isNull()) {
      m_tutorialPage->sendEvent(QString::fromUtf8(QJsonDocument(event).toJson(QJsonDocument::Compact)));
    }
  });
  if (auto* card = qobject_cast<GlassCard*>(m_tutorialContainer.data())) {
    card->setTitle(tr("Tutorial"));
    card->setVisible(true);
    m_floating->requestLayout();
  } else if (!m_tutorialContainer.isNull()) {
    m_tutorialContainer->setVisible(true);
  }
  tutorialPage->open(page);
  qInfo().noquote() << QStringLiteral("Tutorial: opened %1 (%2)").arg(title, QDir::toNativeSeparators(page));
  updateActions();
  return true;
}

void MainWindow::ensureTutorialPane() {
  if (!m_tutorialContainer.isNull()) {
    return;
  }
  m_tutorialPane = new QWidget;
  m_tutorialPane->setObjectName(QStringLiteral("tutorialPane"));
  auto* layout = new QVBoxLayout(m_tutorialPane);
  layout->setContentsMargins(0, 0, 0, 0);
  layout->setSpacing(4);
  auto* header = new QHBoxLayout;
  m_tutorialTitle = new QLabel(m_tutorialPane);
  m_tutorialTitle->setObjectName(QStringLiteral("tutorialTitle"));
  m_tutorialTitle->setTextFormat(Qt::PlainText);
  m_tutorialTitle->setWordWrap(true);
  header->addWidget(m_tutorialTitle, 1);
  auto* leave = new QPushButton(tr("Leave Tutorial"), m_tutorialPane);
  leave->setObjectName(QStringLiteral("tutorialLeave"));
  connect(leave, &QPushButton::clicked, this, [this] { trigger(QString::fromLatin1(kLeaveTutorialCommand)); });
  header->addWidget(leave);
  layout->addLayout(header);
  if (m_floating != nullptr) {
    auto* card = new GlassCard;
    card->setObjectName(QStringLiteral("tutorialDock"));
    card->setTitle(tr("Tutorial"));
    card->setContent(m_tutorialPane, QMargins(6, 0, 6, 8));
    // In the left column, as tall as it lets it be.
    m_floating->addCard(card, FloatingLayout::Anchor::TopLeft, {360, [] { return 4000; }, 1.0, false});
    m_tutorialContainer = card;
  } else {
    auto* dock = new QDockWidget(tr("Tutorial"), this);
    dock->setObjectName(QStringLiteral("tutorialDock"));
    // Closed by Leave Tutorial, not by the dock's own button.
    dock->setFeatures(QDockWidget::DockWidgetMovable | QDockWidget::DockWidgetFloatable);
    dock->toggleViewAction()->setVisible(false);
    dock->setWidget(m_tutorialPane);
    addDockWidget(Qt::LeftDockWidgetArea, dock, Qt::Horizontal);
    dock->hide();
    m_tutorialContainer = dock;
    // Its width once it shows (dockTutorialPane).
    QTimer::singleShot(0, this, [this, dock] {
      if (!dock->isFloating()) {
        dockTutorialPane(dock, dockWidgetArea(dock));
      }
    });
  }
}

void MainWindow::dockTutorialPane(QDockWidget* dock, Qt::DockWidgetArea area) {
  if (dock->isFloating() || dockWidgetArea(dock) != area) {
    addDockWidget(area, dock, Qt::Horizontal);
  }
  if (dock->isFloating()) {
    // Docked where addDockWidget put it.
    dock->setFloating(false);
  }
  if (layout() != nullptr) {
    layout()->activate();
  }
  // 360 px wide, but never at the cost of the 3D view between the docks:
  // in a 1024 px window the Browser, the tutorial and the command panel
  // left it 100 px, the XY plane under the command panel (mitcad#2248).
  constexpr int kWidth = 360;
  constexpr int kViewRoom = 320;
  int width = kWidth;
  if (const QWidget* central = centralWidget(); central != nullptr && dock->isVisible()) {
    width = std::min(width, dock->width() + central->width() - kViewRoom);
  }
  width = std::max(width, dock->minimumSizeHint().width());
  resizeDocks({dock}, {width}, Qt::Horizontal);
  if (layout() != nullptr) {
    layout()->activate();
  }
}

void MainWindow::placeTutorialPane(TutorialPlace place, bool animate) {
  if (m_tutorialContainer.isNull()) {
    return;
  }
  QWidget* pane = m_tutorialContainer.data();
  const int move = ++m_tutorialMoves;
  m_tutorialPlace = place;
  const bool shown = pane->isVisible();
  const int moveMs = animate && shown ? paneMoveMs() : 0;
  const QString how = moveMs > 0 ? QStringLiteral("animated %1 ms").arg(moveMs) : QStringLiteral("at once");
  // The view's picks held while the pane is centred over it, as a step
  // holds them.
  if (place == TutorialPlace::Centre || m_guide != nullptr) {
    guide()->holdView(place == TutorialPlace::Centre && shown, QStringLiteral("the tutorial pane is centred"));
  }
  // Where it ends: logged (the UI tests read it), unless another move was
  // asked for meanwhile.
  const auto placed = [this, move](const QString& words) {
    if (move == m_tutorialMoves) {
      logTutorialPlace(words);
    }
  };
  const QString atOnce = QStringLiteral("at once");
  if (auto* card = qobject_cast<GlassCard*>(pane)) {
    // Floating: the card's anchor; the card moves to its new slot.
    FloatingLayout::Sizing sizing;
    FloatingLayout::Anchor anchor = FloatingLayout::Anchor::TopLeft;
    switch (place) {
    case TutorialPlace::Left:
      // In the left column, as tall as it lets it be (ensureTutorialPane).
      sizing = {360, [] { return 4000; }, 1.0, false};
      break;
    case TutorialPlace::Right:
      // The right column from the rows to the bottom card; the Browser's
      // column at the left stays free.
      anchor = FloatingLayout::Anchor::Right;
      sizing = {360, [] { return 4000; }, 1.0, false};
      break;
    case TutorialPlace::Centre:
      anchor = FloatingLayout::Anchor::Center;
      sizing = {0, [] { return 4000; }, 1.0, false};
      break;
    }
    m_floating->setCardAnchor(card, anchor, sizing, moveMs);
    if (card->slotMoving()) {
      connect(card, &GlassCard::slotMoveFinished, this, [placed, how] { placed(how); }, Qt::SingleShotConnection);
    } else {
      placed(atOnce);
    }
    return;
  }
  auto* dock = qobject_cast<QDockWidget*>(pane);
  if (dock == nullptr) {
    return;
  }
  // Docked: a move under way stops where it is; this one starts there.
  if (auto* running = qobject_cast<QPropertyAnimation*>(m_tutorialDockMove.data())) {
    running->stop();
  }
  const QRect from(dock->mapToGlobal(QPoint(0, 0)), dock->size());
  const Qt::DockWidgetArea area = place == TutorialPlace::Right ? Qt::RightDockWidgetArea : Qt::LeftDockWidgetArea;
  QRect to;
  if (place == TutorialPlace::Centre) {
    // A floating pane centred over the window, most of it.
    to = centredPaneRect(QRect(mapToGlobal(QPoint(0, 0)), size()));
    if (!dock->isFloating()) {
      dock->setFloating(true);
    }
  } else {
    // Docked there: where it ends.
    dockTutorialPane(dock, area);
    to = QRect(dock->mapToGlobal(QPoint(0, 0)), dock->size());
  }
  if (moveMs <= 0 || from == to || from.isEmpty() || to.isEmpty()) {
    if (place == TutorialPlace::Centre) {
      dock->setGeometry(to);
    }
    QTimer::singleShot(0, this, [placed, atOnce] { placed(atOnce); });
    return;
  }
  // The move: the dock, floating, goes from where it was to where it ends,
  // and is docked there at the end.
  if (!dock->isFloating()) {
    dock->setFloating(true);
  }
  dock->setGeometry(from);
  dock->show();
  auto* motion = new QPropertyAnimation(dock, "geometry", this);
  motion->setDuration(moveMs);
  motion->setEasingCurve(QEasingCurve::OutCubic);
  motion->setStartValue(from);
  motion->setEndValue(to);
  m_tutorialDockMove = motion;
  connect(motion, &QPropertyAnimation::finished, this, [this, dock, place, area, placed, how] {
    if (place != TutorialPlace::Centre) {
      dockTutorialPane(dock, area);
    }
    QTimer::singleShot(0, this, [placed, how] { placed(how); });
  });
  motion->start(QAbstractAnimation::DeleteWhenStopped);
}

void MainWindow::logTutorialPlace(const QString& how) {
  if (m_tutorialContainer.isNull()) {
    return;
  }
  const QWidget* pane = m_tutorialContainer.data();
  const QString place = tutorialPlaceName(m_tutorialPlace);
  if (!pane->isVisible()) {
    qInfo().noquote() << QStringLiteral("Tutorial: pane placed %1 (hidden)").arg(place);
    return;
  }
  // In the window's coordinates, across windows (a floating dock, a card
  // of native glass).
  const QWidget* host = window();
  const QPoint at = host->mapFromGlobal(pane->mapToGlobal(QPoint(0, 0)));
  const QSize size = pane->size();
  const int hostWidth = std::max(1, host->width());
  const long middle = std::lround(100.0 * (at.x() + size.width() / 2.0) / hostWidth);
  const long wide = std::lround(100.0 * size.width() / hostWidth);
  qInfo().noquote() << QStringLiteral("Tutorial: pane placed %1 (%2) at %3,%4 %5x%6 in the window %7x%8")
                               .arg(place, how)
                               .arg(at.x())
                               .arg(at.y())
                               .arg(size.width())
                               .arg(size.height())
                               .arg(host->width())
                               .arg(host->height()) +
                           QStringLiteral(", its middle at %1% of the window's width, %2% of its width wide")
                               .arg(middle)
                               .arg(wide);
}

void MainWindow::closeTutorialPane() {
  if (m_tutorialPageWidget.isNull()) {
    return;
  }
  // The tutorial closed: its step ends and a hold after a step done is
  // released, always (mitcad#2368); after Leave Tutorial nothing is left.
  if (m_guide != nullptr) {
    m_guide->end(QStringLiteral("closed"));
  }
  m_tutorialPage = nullptr;
  m_tutorialPageWidget->deleteLater();
  m_tutorialPageWidget = nullptr;
  m_tutorialFolder.clear();
  if (auto* card = qobject_cast<GlassCard*>(m_tutorialContainer.data())) {
    card->setVisible(false);
    m_floating->requestLayout();
  } else if (!m_tutorialContainer.isNull()) {
    m_tutorialContainer->setVisible(false);
  }
  // Leaving restores the layout as it was (mitcad#2367): the pane, hidden,
  // back at the left, where the next tutorial opens; the view free.
  if (m_tutorialPlace != TutorialPlace::Left || (m_guide != nullptr && m_guide->viewHeld())) {
    placeTutorialPane(TutorialPlace::Left, false);
  }
  qInfo().noquote() << QStringLiteral("Tutorial: pane closed");
  updateActions();
}

bool MainWindow::tutorialBridgeCall(const QString& request, QString* answer, QString* error) {
  const QJsonObject parsed = QJsonDocument::fromJson(request.toUtf8()).object();
  const QString call = parsed.value(QStringLiteral("call")).toString();
  const QJsonObject args = parsed.value(QStringLiteral("args")).toObject();
  qInfo().noquote() << QStringLiteral("Tutorial bridge: %1").arg(call);
  QJsonObject result;
  if (call == QLatin1String("start_step")) {
    if (!startGuideStep(args.value(QStringLiteral("step")).toObject(), error)) {
      qInfo().noquote() << QStringLiteral("Tutorial bridge: the step was refused: %1").arg(*error);
      return false;
    }
    result = guide()->state();
  } else if (call == QLatin1String("end_step")) {
    guide()->end(QStringLiteral("ended"));
    result = guide()->state();
  } else if (call == QLatin1String("release_hold")) {
    // The input held after a step with "hold" is free again (mitcad#2368);
    // nothing happens when none holds.
    guide()->releaseHold(QStringLiteral("released"));
    result = guide()->state();
  } else if (call == QLatin1String("state")) {
    static const char* const modes[] = {"idle", "pick_plane", "sketch", "command"};
    QJsonArray picked;
    for (const SelectionItem& item : m_selection) {
      QString kind = SelectionItem::kindName(item.kind);
      kind.replace(QLatin1Char(' '), QLatin1Char('_'));
      picked.append(kind);
    }
    result = {{QStringLiteral("guide"), guide()->state()},
              {QStringLiteral("mode"), QLatin1String(modes[static_cast<int>(m_mode)])},
              {QStringLiteral("command"), m_session ? QJsonValue(m_session->def().id) : QJsonValue()},
              {QStringLiteral("picked"), picked},
              {QStringLiteral("design"), QFileInfo(m_filePath).completeBaseName()},
              {QStringLiteral("modified"), isWindowModified()},
              {QStringLiteral("version"), QCoreApplication::applicationVersion()},
              // Where the tutorial was left (mitcad#2346): a page continues
              // from it.
              {QStringLiteral("progress"), loadTutorialProgress(QFileInfo(m_tutorialFolder).fileName()).toJson()},
              // Where the pane is (mitcad#2367).
              {QStringLiteral("place"), tutorialPlaceName(m_tutorialPlace)}};
  } else if (call == QLatin1String("set_progress")) {
    // The step the user is at, or done (mitcad#2346): the Start page's card
    // shows it, and a page continues from it.
    TutorialProgress progress;
    if (!tutorialProgressFromCall(args, &progress, error)) {
      return false;
    }
    const QString id = QFileInfo(m_tutorialFolder).fileName();
    if (id.isEmpty()) {
      *error = QStringLiteral("set_progress needs an open tutorial");
      return false;
    }
    if (!(loadTutorialProgress(id) == progress)) {
      saveTutorialProgress(id, progress);
      qInfo().noquote() << QStringLiteral("Tutorial: progress %1: %2").arg(id, progress.words());
      TutorialChoice::progressChanged(id);
    }
    result = progress.toJson();
  } else if (call == QLatin1String("open_design")) {
    // A starting design of the package, as a copy of its own: the package
    // stays as it was downloaded.
    const QString relative = args.value(QStringLiteral("path")).toString();
    const QString source = QDir(m_tutorialFolder).filePath(relative);
    if (relative.isEmpty() || QDir::isAbsolutePath(relative) || !inside(m_tutorialFolder, source) ||
        !source.endsWith(QLatin1String(".mitcad"), Qt::CaseInsensitive)) {
      *error = QStringLiteral("open_design takes a .mitcad file of the tutorial's own folder, by its relative path");
      return false;
    }
    const QString work = tutorialWorkFolder(QFileInfo(m_tutorialFolder).fileName());
    QDir().mkpath(work);
    QString target = QDir(work).filePath(QFileInfo(source).fileName());
    if (QFileInfo::exists(target)) {
      // Each start of the tutorial on a fresh copy; the earlier one stays.
      target = QDir(work).filePath(QStringLiteral("%1-%2.mitcad")
                                       .arg(QFileInfo(source).completeBaseName(),
                                            QDateTime::currentDateTime().toString(QStringLiteral("yyyyMMdd-HHmmss"))));
    }
    if (!QFile::copy(source, target)) {
      *error = QStringLiteral("The starting design could not be copied to the tutorials' folder");
      return false;
    }
    QFile(target).setPermissions(QFile(target).permissions() | QFileDevice::WriteOwner);
    qInfo().noquote() << QStringLiteral("Tutorial: opens %1 as %2")
                             .arg(relative, QDir::toNativeSeparators(target));
    // After the answer: the open may ask about unsaved changes.
    QTimer::singleShot(0, this, [this, target] { openPath(target); });
    result = {{QStringLiteral("opened"), QFileInfo(target).fileName()}};
  } else if (call == QLatin1String("query")) {
    // What the page reads of this window's design (mitcad#2371): the model's
    // allow-list of read-only queries (core/model-api tutorial_bridge.rs),
    // refused in words for a command, another query or another design.
    if (modelBusy() || !m_import.isNull()) {
      *error = QStringLiteral("The design is computing; ask again in a moment");
      qInfo().noquote() << QStringLiteral("Tutorial bridge: query refused: %1").arg(*error);
      return false;
    }
    const QString openSketch = m_sketch != nullptr && m_sketch->isActive() ? m_sketch->uid() : QString();
    try {
      const QJsonDocument read = parseJson(tutorial_query(idleDocument(), rustStr(compactJson(args)),
                                                          rustStr(openSketch.toUtf8()),
                                                          rustStr(QFileInfo(m_filePath).completeBaseName().toUtf8())));
      result = read.object();
    } catch (const std::exception& failure) {
      *error = errorText(failure);
      qInfo().noquote() << QStringLiteral("Tutorial bridge: query refused: %1").arg(modelText(failure));
      return false;
    }
    qInfo().noquote() << QStringLiteral("Tutorial bridge: query %1 answered")
                             .arg(args.value(QStringLiteral("query")).toString());
  } else if (call == QLatin1String("place")) {
    // Where the pane goes (mitcad#2367): left, right or centre, moved there
    // after the answer; the page goes on meanwhile.
    TutorialPlace place = TutorialPlace::Left;
    if (!parseTutorialPlace(args.value(QStringLiteral("side")).toString(), &place)) {
      *error = QStringLiteral("place takes \"left\", \"right\" or \"centre\"");
      return false;
    }
    qInfo().noquote() << QStringLiteral("Tutorial: the page places the pane %1").arg(tutorialPlaceName(place));
    QTimer::singleShot(0, this, [this, place] {
      if (tutorialOpen()) {
        placeTutorialPane(place, true);
      }
    });
    result = {{QStringLiteral("place"), tutorialPlaceName(place)}};
  } else if (call == QLatin1String("demonstrate")) {
    // The page's input demonstration over the step's target (mitcad#2366):
    // its pictures by their paths in the tutorial's folder or as data:
    // addresses.
    const QString folder = m_tutorialFolder;
    const GuideDemonstration::Loader load = [folder](const QString& path, QByteArray* bytes, QString* reason) {
      const QString file = QDir::cleanPath(QDir(folder).filePath(path));
      const auto refuse = [reason] {
        *reason = QStringLiteral("a picture is a file of the tutorial's own folder, by its relative path, "
                                 "or a data: address");
        return false;
      };
      if (folder.isEmpty() || QDir::isAbsolutePath(path) ||
          !file.startsWith(QDir::cleanPath(folder) + QLatin1Char('/'))) {
        return refuse();
      }
      if (!QFileInfo::exists(file)) {
        *reason = QStringLiteral("the tutorial has no picture %1").arg(path);
        return false;
      }
      if (!inside(folder, file)) {
        return refuse(); // a link out of the folder
      }
      QFile input(file);
      if (input.size() > 2 * 1024 * 1024) {
        *reason = QStringLiteral("the picture is larger than 2 MB");
        return false;
      }
      if (!input.open(QIODevice::ReadOnly)) {
        *reason = QStringLiteral("%1 could not be read").arg(path);
        return false;
      }
      *bytes = input.readAll();
      return true;
    };
    const GuideDemonstration demonstration = GuideDemonstration::parse(args, load, devicePixelRatioF(), error);
    if (demonstration.frames.isEmpty() || !guide()->demonstrate(demonstration, error)) {
      qInfo().noquote() << QStringLiteral("Tutorial bridge: the demonstration was refused: %1").arg(*error);
      return false;
    }
    result = guide()->state();
  } else if (call == QLatin1String("stop_demonstration")) {
    const bool playing = guide()->demonstrating();
    guide()->stopDemonstration(QStringLiteral("stopped"));
    result = {{QStringLiteral("stopped"), playing}};
  } else if (call == QLatin1String("shortcut")) {
    // A command's keys as the learner has them now (mitcad#2366, GitHub
    // mitcad/Mitcad#625; mitcad#2439): an empty list for a command with
    // none; read at each call, so a key changed in Keyboard Shortcuts is in
    // the next answer.
    if (!guideShortcutAnswer(*m_registry, args.value(QStringLiteral("command")).toString(), &result, error)) {
      return false;
    }
  } else if (call == QLatin1String("navigation")) {
    // The view's gestures of the learner's navigation scheme (mitcad#2366),
    // the mouse's and the trackpad's each on its own (mitcad#2439): the
    // view's settings now, so a scheme or gesture changed in Preferences is
    // in the next answer.
    if (m_viewer == nullptr) {
      *error = QStringLiteral("This window has no 3D view");
      return false;
    }
    result = guideNavigationJson(m_viewer->navigation());
  } else if (call == QLatin1String("leave")) {
    // After the answer: the pane and its page go.
    QTimer::singleShot(0, this, [this] { trigger(QString::fromLatin1(kLeaveTutorialCommand)); });
    result = {{QStringLiteral("left"), true}};
  } else {
    *error = QStringLiteral("window.mitcad has no call %1").arg(call);
    return false;
  }
  *answer = QString::fromUtf8(QJsonDocument(result).toJson(QJsonDocument::Compact));
  return true;
}

} // namespace mitcad
