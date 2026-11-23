// SPDX-License-Identifier: MIT
// Scripts and macros (mitcad#2245, app/COMMANDS.md "Macros and scripts"):
// File › Export Timeline as Script… writes the design's timeline as a
// script (the model's export_script) that rebuilds it on an empty design
// and checks the result; Tools › Macro › Record Macro keeps the model
// commands the application sends from then on (framework/MacroRecorder,
// noted by DocumentSession), Stop Recording ends it, Save Macro… writes the
// steps as a script file, and Run Script… runs a script file into the open
// design as one undo step named after the file (the model's run_script),
// which stops at the first failing step and says which. While a macro is
// recorded an indicator beside the project indicator says so; a click on
// it stops the recording.
#include "../MainWindow.hpp"

#include <functional>
#include <utility>

#include <QCoreApplication>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonObject>
#include <QSaveFile>
#include <QToolButton>
#include <QtLogging>

#include "../framework/CommandRegistry.hpp"
#include "../framework/DialogFolders.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/DocumentSession.hpp"
#include "../framework/Icons.hpp"
#include "../framework/MacroRecorder.hpp"

namespace mitcad {
namespace {

// "1 step", "3 steps": the log's and the indicator's counts.
QString stepsText(int count) {
  return count == 1 ? QStringLiteral("1 step") : QStringLiteral("%1 steps").arg(count);
}

} // namespace

void MainWindow::registerScriptCommands() {
  const auto def = [](const char* id, const QString& name, const char* icon, const QString& tooltip,
                      std::function<void()> run) {
    CommandDef d;
    d.id = QString::fromLatin1(id);
    d.name = name;
    d.icon = QString::fromLatin1(icon);
    d.tooltip = tooltip;
    d.kind = CommandDef::Kind::Action;
    d.mode = CommandDef::Mode::Model;
    d.run = std::move(run);
    return d;
  };
  // File › Export Timeline as Script… (after Export…).
  CommandDef exportScript =
      def("file.export_script", tr("Export Timeline as Script…"), "export-script",
          tr("Writes the timeline as a script that builds the design again and checks it"),
          [this] { exportTimelineScript(); });
  exportScript.keywords = {QStringLiteral("script"), QStringLiteral("timeline"), QStringLiteral("history"),
                           QStringLiteral("json"), QStringLiteral("replay"), QStringLiteral("macro")};
  exportScript.enabled = [this] { return m_controller != nullptr; };
  m_registry->add(exportScript);

  // Tools › Macro.
  CommandDef record = def("tools.macro_record", tr("Record Macro"), "macro-record",
                          tr("Keeps the commands you give from now on, to save and run again"),
                          [this] { startMacroRecording(); });
  record.mode = CommandDef::Mode::Any;
  record.keywords = {QStringLiteral("macro"), QStringLiteral("record"), QStringLiteral("script"),
                     QStringLiteral("automate"), QStringLiteral("repeat")};
  record.enabled = [this] { return !m_documentSession->macro().recording(); };
  record.disabledReason = [] { return tr("a macro is being recorded"); };
  m_registry->add(record);
  CommandDef stop = def("tools.macro_stop", tr("Stop Recording"), "macro-record",
                        tr("Ends the macro's recording; Save Macro writes it"), [this] { stopMacroRecording(); });
  stop.mode = CommandDef::Mode::Any;
  stop.duringCommands = true;
  stop.keywords = {QStringLiteral("macro"), QStringLiteral("record"), QStringLiteral("end")};
  stop.enabled = [this] { return m_documentSession->macro().recording(); };
  stop.disabledReason = [] { return tr("no macro is being recorded"); };
  m_registry->add(stop);
  CommandDef save = def("tools.macro_save", tr("Save Macro…"), "save",
                        tr("Writes the recorded commands to a script file"), [this] { saveMacro(); });
  save.mode = CommandDef::Mode::Any;
  save.keywords = {QStringLiteral("macro"), QStringLiteral("script"), QStringLiteral("json")};
  save.enabled = [this] { return m_documentSession->macro().count() > 0; };
  save.disabledReason = [] { return tr("nothing recorded"); };
  m_registry->add(save);
  CommandDef run = def("tools.run_script", tr("Run Script…"), "open",
                       tr("Runs a script file's commands in the open design as one undo step"),
                       [this] { runScriptFile(); });
  run.keywords = {QStringLiteral("macro"), QStringLiteral("script"), QStringLiteral("json"),
                  QStringLiteral("play"), QStringLiteral("replay")};
  run.enabled = [this] { return m_controller != nullptr && !windowReadOnly(); };
  m_registry->add(run);
}

void MainWindow::createMacroIndicator() {
  m_macroIndicator = new QToolButton(this);
  m_macroIndicator->setObjectName(QStringLiteral("macroIndicator"));
  m_macroIndicator->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
  m_macroIndicator->setIcon(themeIcon(QStringLiteral("macro-record")));
  m_macroIndicator->setAutoRaise(true);
  m_macroIndicator->setFocusPolicy(Qt::NoFocus);
  m_macroIndicator->setToolTip(tr("A macro is being recorded: click to stop the recording"));
  m_macroIndicator->setAccessibleName(tr("Macro recording"));
  m_macroIndicator->hide();
  connect(m_macroIndicator, &QToolButton::clicked, this, [this] { stopMacroRecording(); });
  connect(m_documentSession, &DocumentSession::macroStepRecorded, this, [this](const QString& name, int count) {
    qInfo().noquote() << QStringLiteral("Macro: recorded %1 (%2)").arg(name, stepsText(count));
    updateMacroIndicator();
  });
}

void MainWindow::updateMacroIndicator() {
  if (m_macroIndicator == nullptr) {
    return;
  }
  const MacroRecorder& macro = m_documentSession->macro();
  if (!macro.recording()) {
    m_macroIndicator->hide();
    return;
  }
  const int count = macro.count();
  m_macroIndicator->setText(count == 1 ? tr("Recording macro: 1 step")
                                       : tr("Recording macro: %1 steps").arg(count));
  m_macroIndicator->show();
}

void MainWindow::startMacroRecording() {
  MacroRecorder& macro = m_documentSession->macro();
  if (macro.recording()) {
    return;
  }
  macro.start();
  qInfo().noquote() << QStringLiteral("Macro: recording");
  updateMacroIndicator();
  updateActions();
  showHint(tr("Recording a macro: Tools › Macro › Stop Recording ends it."));
}

void MainWindow::stopMacroRecording() {
  MacroRecorder& macro = m_documentSession->macro();
  if (!macro.recording()) {
    return;
  }
  macro.stop();
  qInfo().noquote() << QStringLiteral("Macro: stopped, %1").arg(stepsText(macro.count()));
  updateMacroIndicator();
  updateActions();
  showHint(macro.count() == 1 ? tr("Macro recorded: 1 step. Save Macro writes it to a file.")
                              : tr("Macro recorded: %1 steps. Save Macro writes it to a file.").arg(macro.count()));
}

void MainWindow::saveMacro() {
  const MacroRecorder& macro = m_documentSession->macro();
  if (macro.count() == 0) {
    showHint(tr("Save Macro: nothing is recorded. Record Macro starts a recording."));
    return;
  }
  const QString suggested = QDir(dialogFolder(DialogGroup::Export)).filePath(QStringLiteral("macro.json"));
  const QString path = dialogSaveFileName(this, tr("Save Macro"), suggested, tr("MitCad scripts (*.json)"));
  if (path.isEmpty()) {
    return;
  }
  rememberDialogFolder(DialogGroup::Export, path);
  const QString comment = QStringLiteral("Macro recorded in MitCad %1 on %2: %3")
                              .arg(QCoreApplication::applicationVersion(),
                                   QDateTime::currentDateTime().toString(Qt::ISODate), stepsText(macro.count()));
  QSaveFile file(path);
  if (!file.open(QIODevice::WriteOnly) || file.write(MacroRecorder::fileJson(macro.steps(), comment)) < 0 ||
      !file.commit()) {
    qWarning().noquote() << QStringLiteral("Macro: could not save %1: %2").arg(path, file.errorString());
    sheetWarning(this, tr("Save Macro"), tr("Could not write %1:\n%2").arg(QDir::toNativeSeparators(path),
                                                                          file.errorString()));
    return;
  }
  qInfo().noquote() << QStringLiteral("Macro: saved %1 to %2").arg(stepsText(macro.count()), path);
  showHint(tr("Saved the macro to %1.").arg(QDir::toNativeSeparators(path)));
}

void MainWindow::runScriptFile() {
  if (!canChangeModel(false)) {
    showStatus(changeBlockedReason(), NoticeLevel::Error);
    return;
  }
  const QString path = dialogOpenFileName(this, tr("Run Script"), dialogFolder(DialogGroup::Import),
                                          tr("MitCad scripts (*.json)"));
  if (path.isEmpty()) {
    return;
  }
  rememberDialogFolder(DialogGroup::Import, path);
  QFile file(path);
  if (!file.open(QIODevice::ReadOnly)) {
    qWarning().noquote() << QStringLiteral("Script: could not read %1: %2").arg(path, file.errorString());
    showError(tr("Could not read %1: %2").arg(QDir::toNativeSeparators(path), file.errorString()));
    return;
  }
  QString error;
  const QJsonArray steps = MacroRecorder::readSteps(file.readAll(), &error);
  if (!error.isEmpty()) {
    qWarning().noquote() << QStringLiteral("Script: %1 refused: %2").arg(path, error);
    showError(tr("Run Script: %1").arg(error));
    return;
  }
  const QString name = QFileInfo(path).completeBaseName();
  QJsonObject result;
  if (!runModelCommand({{QStringLiteral("cmd"), QStringLiteral("run_script")},
                        {QStringLiteral("steps"), steps},
                        {QStringLiteral("label"), tr("Run %1").arg(name)}},
                       &result)) {
    // runCommand showed the model's words: the failing step's number and
    // its message; nothing of the script stays.
    qWarning().noquote() << QStringLiteral("Script: %1 failed: %2").arg(path, m_lastError);
    return;
  }
  const int ran = result.value(QStringLiteral("steps")).toInt(static_cast<int>(steps.size()));
  qInfo().noquote() << QStringLiteral("Script: ran %1 from %2 as one undo step (Run %3)")
                           .arg(stepsText(ran), path, name);
  showHint(tr("Ran %1: one undo step takes it back.").arg(QFileInfo(path).fileName()));
}

void MainWindow::exportTimelineScript() {
  const QString stem = m_filePath.isEmpty() ? documentName() : QFileInfo(m_filePath).completeBaseName();
  const QString suggested = QDir(dialogFolder(DialogGroup::Export))
                                .filePath((stem.isEmpty() ? QStringLiteral("timeline") : stem) +
                                          QStringLiteral(".json"));
  const QString path = dialogSaveFileName(this, tr("Export Timeline as Script"), suggested,
                                          tr("MitCad scripts (*.json)"));
  if (path.isEmpty()) {
    return;
  }
  rememberDialogFolder(DialogGroup::Export, path);
  QJsonObject result;
  if (!runCommand({{QStringLiteral("cmd"), QStringLiteral("export_script")}, {QStringLiteral("path"), path}},
                  &result)) {
    qWarning().noquote() << QStringLiteral("Script: could not export %1: %2").arg(path, m_lastError);
    sheetWarning(this, tr("Export Timeline as Script"), tr("Could not export:\n%1").arg(m_lastError));
    return;
  }
  const int features = result.value(QStringLiteral("features")).toInt();
  const int steps = result.value(QStringLiteral("steps")).toInt();
  qInfo().noquote() << QStringLiteral("Script: exported %1 features, %2 to %3")
                           .arg(features)
                           .arg(stepsText(steps), path);
  QStringList omitted;
  for (const QJsonValue& line : result.value(QStringLiteral("omitted")).toArray()) {
    omitted << line.toString();
    qInfo().noquote() << QStringLiteral("Script: omitted: %1").arg(line.toString());
  }
  const QString file = QDir::toNativeSeparators(path);
  if (omitted.isEmpty()) {
    showHint(tr("Exported the timeline to %1.").arg(file));
  } else {
    // What a replay of the script does not build again, in the model's words.
    showStatus(tr("Exported the timeline to %1. Not in the script: %2").arg(file, omitted.join(QStringLiteral("; "))),
               NoticeLevel::Warning);
  }
}

} // namespace mitcad
