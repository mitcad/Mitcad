// SPDX-License-Identifier: MIT
// In-app tutorials (mitcad#2012, docs/tutorials.md): the tutorial pane, a
// tutorial package's pages on MitCad's web browser component (CEF,
// webbrowser/TutorialPageView in the component's module) docked at the left
// of the design's window in both chrome styles, the component downloaded
// first when it is missing; and the bridge window.mitcad that its pages (of
// the package's own folder only) call: guide steps, the package's starting
// design opened, the application's state read, Leave Tutorial, and the
// guide's events back. No file access and no model edits through it.
#include "../MainWindow.hpp"

#include <algorithm>
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
#include "../webbrowser/BrowserComponent.hpp"
#include "../webbrowser/BrowserModuleLoad.hpp"
#include "../webbrowser/ComponentBrowserHost.hpp"
#include "GuideStep.hpp"
#include "TutorialGuide.hpp"
#include "TutorialSources.hpp"
#include "TutorialsDialog.hpp"

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
  TutorialsDialog dialog(this);
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted || !dialog.chosen()) {
    return;
  }
  openListedTutorial(dialog.chosen()->url, dialog.chosen()->tutorial);
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
  // Over the Start page (mitcad#2168) the tutorial starts in an empty
  // design of its own, never in the design under the page: its first
  // step's target is on the design's toolbar, and the page's own controls
  // are no part of any step.
  if (modelBusy()) {
    whenIdle(this, [this, folder, page, title] { openTutorialInDesign(folder, page, title); });
    return;
  }
  if (m_noDesign || emptyUntitledDesign()) {
    // The empty untitled design that waits under the page (the launch's,
    // or the one a close to the page installed): the page is left for it.
    qInfo().noquote() << QStringLiteral("Tutorial: starts in the empty design under the Start page");
    leaveStartPage();
    openTutorialPane(folder, page, title);
    return;
  }
  // The user's design: New Design, in a new window or (Open in current
  // window) in this one after its unsaved-changes question.
  qInfo().noquote() << QStringLiteral("Tutorial: starts in a new design");
  QPointer<MainWindow> target = newDocument();
  if (target.isNull()) {
    qInfo().noquote() << QStringLiteral("Tutorial: not opened (New Design was cancelled)");
    return;
  }
  target->openTutorialPane(folder, page, title);
}

bool MainWindow::tutorialOpen() const { return !m_tutorialPageWidget.isNull(); }

void MainWindow::openTutorialPane(const QString& folder, const QString& page, const QString& title) {
  withBrowserComponent(tr("Tutorials"),
                       tr("Tutorials are shown in MitCad's web browser component (the Chromium Embedded Framework "
                          "%1), a download of %2. Download it now?"),
                       [this, folder, page, title](const BrowserComponentInfo& component) {
                         return showTutorialPane(component, folder, page, title);
                       });
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
    // 360 px wide, but never at the cost of the 3D view between the docks:
    // in a 1024 px window the Browser, the tutorial and the command panel
    // left it 100 px, the XY plane under the command panel (mitcad#2248).
    QTimer::singleShot(0, this, [this, dock] {
      constexpr int kWidth = 360;
      constexpr int kViewRoom = 320;
      int width = kWidth;
      if (const QWidget* central = centralWidget(); central != nullptr && dock->isVisible()) {
        width = std::min(width, dock->width() + central->width() - kViewRoom);
      }
      width = std::max(width, dock->minimumSizeHint().width());
      resizeDocks({dock}, {width}, Qt::Horizontal);
    });
  }
}

void MainWindow::closeTutorialPane() {
  if (m_tutorialPageWidget.isNull()) {
    return;
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
              {QStringLiteral("version"), QCoreApplication::applicationVersion()}};
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
