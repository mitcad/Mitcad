// SPDX-License-Identifier: MIT
// Autosave (P8, Autosave.hpp): what the main window tells its autosave
// manager about the document. The window's own hooks are short: a new
// document (installDocument), one opened or saved (loadProject,
// afterSaved), an imported design (startF3dImport) and Preferences.
//
// Recovery (P8c, Recovery.hpp): at start-up (main.cpp) and by File >
// Recover Documents, the work that sessions which did not end normally
// left; a document restored from it, and Save over its file.
#include "../MainWindow.hpp"

#include <cstddef>
#include <utility>

#include <QByteArray>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonObject>
#include <QLockFile>
#include <QMessageBox>
#include <QPushButton>
#include <QTimer>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/AppSettings.hpp"
#include "../framework/Json.hpp"
#include "../view/ViewController.hpp"
#include "Autosave.hpp"
#include "F3dImport.hpp"
#include "Recovery.hpp"
#include "RemoteController.hpp"

namespace mitcad {

void MainWindow::createAutosave() {
  m_autosave = new AutosaveManager(this);
  // The document is the worker's during a job; a sketch command's preview
  // is in the model until its panel closes, and is not the user's yet.
  m_autosave->blocked = [this] {
    if (modelBusy()) {
      return QStringLiteral("busy");
    }
    if (m_session && m_session->previewApplied()) {
      return QStringLiteral("previewing");
    }
    return QString();
  };
  m_autosave->state = [this] {
    const QJsonObject document = queryObject({{QStringLiteral("query"), QStringLiteral("document")}});
    return AutosaveManager::State{document.value(QStringLiteral("modified")).toBool(),
                                  static_cast<quint64>(document.value(QStringLiteral("revision")).toInteger())};
  };
  m_autosave->snapshot = [this] {
    // An open edit is written as it will be when it ends: the marker back
    // where the edit restores it.
    const int marker = autosaveMarker();
    const Document& document = idleDocument();
    const rust::String json = marker >= 0 ? document.to_json_with_marker(static_cast<std::size_t>(marker))
                                          : document.to_json();
    return AutosaveManager::Snapshot{QByteArray(json.data(), static_cast<qsizetype>(json.size())),
                                     documentName(), m_filePath};
  };
  m_autosave->apply(GeneralSettings::load());
  m_viewController->generalChanged = [this] {
    m_autosave->apply(GeneralSettings::load());
    applyCacheSettings(); // the result store (P7d)
    m_remote->settingsChanged(); // the git program, remote checks (P12 remote)
  };
}

int MainWindow::autosaveMarker() const {
  if (m_sketchRestoreMarker >= 0) {
    return m_sketchRestoreMarker; // editing a sketch, also in a command of it
  }
  return m_session ? m_session->restoresMarker() : -1;
}

// ---------------------------------------------------------------------------
// Recovery

void MainWindow::recoverAtStart() {
  if (m_import) {
    // An .f3d design given at start-up comes in first; then the question.
    connect(
        m_import.data(), &F3dImport::finished, this,
        [this] { QTimer::singleShot(0, this, [this] { recoverDocuments(true); }); }, Qt::SingleShotConnection);
    return;
  }
  recoverDocuments(true);
}

void MainWindow::recoverDocuments(bool atStart) {
  if (modelBusy() || m_import) {
    showError(tr("Recover Documents waits until the design is computed or imported."));
    return;
  }
  std::vector<RecoverableSession> sessions = findRecoverable(m_autosave->directory(), m_autosave->session());
  if (sessions.empty()) {
    qInfo().noquote() << QStringLiteral("Recovery: none");
    if (!atStart) {
      sheetInformation(this, tr("Recover Documents"), tr("There are no unsaved changes to recover."));
    }
    return;
  }
  qInfo().noquote() << QStringLiteral("Recovery: %1 document(s) found").arg(sessions.size());
  for (const RecoverableSession& session : sessions) {
    qInfo().noquote() << QStringLiteral("Recoverable %1").arg(session.describe());
  }
  const std::optional<std::size_t> chosen = askRecovery(this, sessions);
  if (!chosen) {
    if (!sessions.empty()) {
      qInfo().noquote() << QStringLiteral("Recovery: later, %1 document(s) kept").arg(sessions.size());
    }
    return;
  }
  restoreRecovered(sessions[*chosen]);
  // The others' locks go with `sessions`: their files stay for later.
}

bool MainWindow::restoreRecovered(RecoverableSession& session) {
  const QString name = session.document;
  if (!maybeSave()) {
    qInfo().noquote() << QStringLiteral("Recovery of %1 cancelled").arg(name);
    return false;
  }
  QFile file(session.projectFile());
  QByteArray json;
  if (file.open(QIODevice::ReadOnly)) {
    json = file.readAll();
  }
  QString error;
  if (json.size() != session.size || fileDigest(json) != session.digest) {
    error = tr("The autosaved file %1 is not as it was written.").arg(QDir::toNativeSeparators(file.fileName()));
  }
  const QString path = session.path;
  QJsonArray messages;
  // On the worker, as a project file is opened: read, its links followed
  // when it has a file (relative to it), computed by installDocument. Not
  // marked saved: its changes are in no file yet.
  const DocumentMaker read = [&](ModelJob& job) {
    job.setStage(tr("Reading the recovered %1").arg(name));
    rust::Box<Document> document = load_document(rustStr(json));
    if (!path.isEmpty()) {
      const QByteArray links = compactJson({{QStringLiteral("cmd"), QStringLiteral("update_links")},
                                            {QStringLiteral("base"), QFileInfo(path).absolutePath()}});
      const AttachedJob attached(*document, job);
      messages = parseObject(document->command(rustStr(links))).value(QStringLiteral("messages")).toArray();
    }
    return document;
  };
  if (error.isEmpty() && !installDocument(name, path, read, error) && error.isEmpty()) {
    // Cancelled: the window's document stays, the session for later.
    qInfo().noquote() << QStringLiteral("Recovering %1 cancelled").arg(name);
    showHint(tr("Recovering %1 cancelled; it can be recovered later.").arg(name));
    return false;
  }
  if (!error.isEmpty()) {
    const QString message = tr("Could not recover %1:\n%2").arg(name, error);
    qWarning().noquote() << QStringLiteral("Recovery of %1 failed: %2").arg(name, error);
    sheetWarning(this, tr("Recover Documents"), message);
    return false;
  }
  // The document is the one it was; its file, if any, is still to be
  // written. The earlier session's files stay until this one has written it.
  if (path.isEmpty()) {
    m_documentName = name;
  } else {
    m_recoveredDigest = session.baseDigest;
  }
  m_autosave->recovered(session.baseDigest, session.session, std::move(session.lock));
  m_autosave->saveSoon();
  updateWindowTitle();
  for (const QJsonValue& message : std::as_const(messages)) {
    qInfo().noquote() << message.toString();
  }
  qInfo().noquote() << QStringLiteral("Recovered %1 from autosave of %2")
                           .arg(name, session.savedAt.toString(Qt::ISODate));
  showHint(tr("Recovered %1. Its changes are not saved yet.").arg(name));
  return true;
}

std::optional<bool> MainWindow::askOverwriteChanged() {
  if (!m_recoveredDigest || m_recoveredDigest->isEmpty()) {
    return true;
  }
  QFile file(m_filePath);
  if (!file.exists() || !file.open(QIODevice::ReadOnly) || fileDigest(file.readAll()) == *m_recoveredDigest) {
    return true; // nothing there to lose
  }
  const QString name = documentName();
  qInfo().noquote() << QStringLiteral("Save conflict: %1 changed since it was opened").arg(m_filePath);
  QMessageBox box(QMessageBox::Warning, tr("Mitcad"),
                  tr("%1 has changed since the recovered document was opened from it.").arg(name),
                  QMessageBox::NoButton, this);
  box.setInformativeText(tr("Overwrite the file with the recovered document, or save the recovered document "
                            "as another file?"));
  QPushButton* overwrite = box.addButton(tr("&Overwrite"), QMessageBox::DestructiveRole);
  QPushButton* another = box.addButton(tr("Save &As..."), QMessageBox::AcceptRole);
  box.addButton(QMessageBox::Cancel);
  box.setDefaultButton(another);
  prepareModal(&box);
  box.exec();
  if (box.clickedButton() == overwrite) {
    qInfo().noquote() << QStringLiteral("Save conflict: overwrite");
    return true;
  }
  if (box.clickedButton() == another) {
    qInfo().noquote() << QStringLiteral("Save conflict: save as");
    return false;
  }
  qInfo().noquote() << QStringLiteral("Save conflict: cancelled");
  return std::nullopt;
}

} // namespace mitcad
