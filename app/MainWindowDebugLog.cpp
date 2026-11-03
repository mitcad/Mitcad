// SPDX-License-Identifier: MIT
// The debug log in the window (mitcad#269, framework/DebugLog.hpp): Help >
// Open Log Folder, the facts of its files' header, "Debug log on" in the
// project indicator while it runs.
#include "MainWindow.hpp"
#include "WindowManager.hpp"

#include <QAction>
#include <QDir>
#include <QList>
#include <QStringList>
#include <QUrl>
#include <QtLogging>

#include "files/ProjectIndicator.hpp"
#include "framework/Command.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/DebugLog.hpp"
#include "framework/DocumentSession.hpp"
#include "report/ReportCenter.hpp"

namespace mitcad {

void MainWindow::registerDebugLogCommands() {
  CommandDef open;
  open.id = QStringLiteral("help.open_log_folder");
  open.name = tr("Open Log Folder");
  open.icon = QStringLiteral("folder");
  open.kind = CommandDef::Kind::Action;
  open.mode = CommandDef::Mode::Any;
  open.tab.clear(); // the Help menu
  open.tooltip = tr("Opens the folder of the debug log's files");
  open.keywords = {QStringLiteral("debug log"), QStringLiteral("logs"), QStringLiteral("diagnostics"),
                   QStringLiteral("trace")};
  open.duringCommands = true;
  open.run = [this] { openLogFolder(); };
  m_registry->add(open);
}

void MainWindow::openLogFolder() {
  const QString folder = DebugLog::instance().folder();
  QDir().mkpath(folder);
  qInfo().noquote() << QStringLiteral("Open Log Folder: %1").arg(QDir::toNativeSeparators(folder));
  openExternalUrl(QUrl::fromLocalFile(folder));
}

void MainWindow::createDebugLog() {
  DebugLog& log = DebugLog::instance();
  // The process log describes all documents, not whichever worker changed last.
  const auto updateDesigns = [this] {
    QStringList designs;
    const auto windows = windowManager() ? windowManager()->windows() : QList<MainWindow*>{this};
    for (MainWindow* window : windows) {
      designs.append(window->documentFilePath().isEmpty() ? QStringLiteral("none (not saved yet)")
                                                        : QDir::toNativeSeparators(window->documentFilePath()));
    }
    DebugLog::instance().setFact(QStringLiteral("Design"), designs.join(QStringLiteral("; ")));
  };
  connect(m_documentSession, &DocumentSession::changed, this, updateDesigns);
  if (WindowManager* manager = WindowManager::instance())
    connect(manager, &WindowManager::windowsChanged, this, updateDesigns);
  m_debugLogOff = new QAction(tr("Turn Off Debug Log"), this);
  m_debugLogOff->setObjectName(QStringLiteral("debugLogOff"));
  m_debugLogOff->setMenuRole(QAction::NoRole);
  connect(m_debugLogOff, &QAction::triggered, this, [] {
    qInfo().noquote() << QStringLiteral("Debug log turned off from the project indicator");
    DebugLog::setSettingOn(false);
    DebugLog::instance().stop();
  });
  connect(&log, &DebugLog::runningChanged, this, &MainWindow::debugLogChanged);
  debugLogChanged(log.running());
}

void MainWindow::debugLogChanged(bool running) {
  const QString file = QDir::toNativeSeparators(DebugLog::instance().currentFile());
  m_indicator->setDebugLog(running ? tr("Debug log on") : QString(),
                           tr("A debug log is written to %1 (Help › Open Log Folder). Preferences: Diagnostics or "
                              "this menu turns it off.")
                               .arg(file));
  QList<QAction*> actions;
  if (running) {
    if (QAction* open = m_registry->action(QStringLiteral("help.open_log_folder"))) {
      actions << open;
    }
    actions << m_debugLogOff;
    showHint(tr("Debug log on: %1").arg(file));
  }
  m_indicator->setDebugLogActions(actions);
}

} // namespace mitcad
