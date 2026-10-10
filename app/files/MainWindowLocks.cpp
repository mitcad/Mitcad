// SPDX-License-Identifier: MIT
// Edit locks in the window (mitcad#89): the lock controller of the open
// design (files/LockController.hpp) and the window's read-only mode. A
// read-only window refuses every edit where it passes rather than in each
// widget: the commands' availability (isAvailable: the ribbon, the menus,
// the command search and the shortcuts), the model commands
// (MainWindow::command, which every change of the document goes through:
// the browser, the timeline, the parameters, sketches, drags), the
// design's file (writeFile), sketch mode and command panels.
#include "../MainWindow.hpp"

#include <QApplication>
#include <QDir>
#include <QFileInfo>
#include <QMessageBox>
#include <QPushButton>
#include <QSet>
#include <QtLogging>

#include "../framework/AppSettings.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Json.hpp"
#include "Autosave.hpp"
#include "LiveController.hpp"
#include "LockController.hpp"
#include "ProjectIndicator.hpp"
#include "RemoteController.hpp"

namespace mitcad {

void MainWindow::createLocks() {
  LockController::Host host;
  host.project = [this] { return m_project; };
  host.session = [this] { return m_autosave != nullptr ? m_autosave->session() : QString(); };
  // While a job computes the document it is not read: taken as changed.
  host.modified = [this] { return modelBusy() || isModified(); };
  host.autosaveOn = [] { return GeneralSettings::load().autosave; };
  host.canSaveNow = [this] { return m_mode == Mode::Idle && !m_session && !modelBusy(); };
  host.saveVersion = [this](const QString& message) {
    if (m_filePath.isEmpty() || modelBusy()) {
      return false;
    }
    return writeFile(m_filePath, message);
  };
  host.showVersion = [this](const QString& commit, QString& error) { return showLockedVersion(commit, error); };
  host.readOnlyChanged = [this] { readOnlyChanged(); };
  host.status = [this](const QString& message, bool error) {
    if (error) {
      showError(message);
    } else {
      showHint(message);
    }
  };
  host.saveAsCopy = [this] { saveAs(); };
  host.saveAsNewVersion = [this] { saveAsNewVersion(); };
  host.whenIdle = [this](std::function<void()> call) { whenIdle(m_locks, std::move(call)); };
  host.busy = [this] { return modelBusy(); };
  host.liveDetails = [this](const QString& root) { return m_live != nullptr ? m_live->details(root) : QString(); };
  m_locks = new LockController(*this, *m_remote, *m_indicator, std::move(host), this);
  // A view change is activity (an idle lock is released), a frame drawn
  // for a highlight under the mouse is not: the camera that came to rest.
  connect(m_viewer, &OcctViewer::cameraRested, m_locks, &LockController::activity);
}

bool MainWindow::windowReadOnly() const { return m_locks != nullptr && m_locks->isReadOnly(); }

QString MainWindow::readOnlyReason() const { return windowReadOnly() ? m_locks->readOnlyReason() : QString(); }

bool MainWindow::readOnlyCommand(const CommandDef& def) {
  // The view's commands (also during commands) and inspections change no
  // design.
  if (def.duringCommands || def.inspect) {
    return true;
  }
  static const QSet<QString> allowed{
      QStringLiteral("file.new_project"),     QStringLiteral("file.open_project"),
      QStringLiteral("file.open_read_only"),  QStringLiteral("file.open_remote"),
      QStringLiteral("file.project_settings"), QStringLiteral("file.export"),
      QStringLiteral("file.version_history"), QStringLiteral("file.sync"),
      QStringLiteral("file.check_remote"),    QStringLiteral("file.remote_browser"),
      QStringLiteral("file.render_image"),    QStringLiteral("file.recover"),
      QStringLiteral("make.print3d"),         QStringLiteral("assemble.animate_joint"),
      QStringLiteral("tools.search"),         QStringLiteral("tools.shortcuts"),
      QStringLiteral("tools.libraries"),      QStringLiteral("tools.community_library"),
      QStringLiteral("tools.library_parts"),  QStringLiteral("tools.publish_library"),
  };
  return allowed.contains(def.id) || def.id.startsWith(QLatin1String("view.")) ||
         def.id.startsWith(QLatin1String("inspect.")) || def.id.startsWith(QLatin1String("help."));
}

bool MainWindow::readOnlyModelCommand(const QString& name) {
  // What changes no design: reading and computing it, its file's state,
  // the display state of this computer (isolation, the origin), the
  // section analyses shown (never saved over the file), exports.
  static const QSet<QString> allowed{
      QStringLiteral("recompute"),        QStringLiteral("update_links"),     QStringLiteral("mark_saved"),
      QStringLiteral("clear_cache"),      QStringLiteral("export"),           QStringLiteral("export_sketch"),
      QStringLiteral("set_isolation"),    QStringLiteral("set_origin_visible"), QStringLiteral("add_analysis"),
      QStringLiteral("edit_analysis"),    QStringLiteral("delete_analysis"),  QStringLiteral("rename_analysis"),
      QStringLiteral("set_analysis_visible"), QStringLiteral("merge_undo"),
  };
  return allowed.contains(name);
}

bool MainWindow::refuseReadOnly(const QString& what) {
  if (!windowReadOnly()) {
    return false;
  }
  qInfo().noquote() << QStringLiteral("Read-only: refused %1").arg(what);
  showError(m_locks->readOnlyReason());
  return true;
}

void MainWindow::readOnlyChanged() {
  const bool on = windowReadOnly();
  qInfo().noquote() << QStringLiteral("Read-only: %1%2")
                           .arg(on ? QStringLiteral("on") : QStringLiteral("off"),
                                on ? QStringLiteral(" (%1)").arg(m_locks->readOnlyReason()) : QString());
  if (on) {
    // Editing ends: a command panel open now is cancelled, a sketch
    // finished (its own model commands still pass: they put back what the
    // edit had rolled back).
    m_writeAnyway = true;
    if (m_session) {
      m_session->cancel();
    }
    if (m_mode == Mode::Sketch) {
      finishSketch();
    }
    if (m_mode == Mode::PickPlane) {
      cancel();
    }
    m_writeAnyway = false;
  }
  updateActions();
  updateWindowTitle();
}

bool MainWindow::showLockedVersion(const QString& commit, QString& error) {
  if (m_filePath.isEmpty() || modelBusy() || m_mode != Mode::Idle) {
    error = tr("the window is busy");
    return false;
  }
  const QString file = m_filePath;
  const QString name = QFileInfo(file).fileName();
  const QByteArray path = file.toUtf8();
  const QByteArray id = commit.toUtf8();
  const DocumentMaker read = [&](ModelJob& job) {
    job.setStage(tr("Reading %1").arg(name));
    // From the fetched commit; the file in the folder stays as it is.
    const rust::Box<Project> project = open_project(rustStr(path));
    rust::Box<Document> document = load_version(*project, rustStr(id), rustStr(path));
    document->command(rustStr(compactJson({{QStringLiteral("cmd"), QStringLiteral("mark_saved")}})));
    return document;
  };
  m_keepView = true;
  const bool shown = installDocument(name, file, read, error);
  m_keepView = false;
  if (shown) {
    // What Save compares with stays the file's.
    m_versionBase.reset();
  }
  return shown;
}

bool MainWindow::saveAsNewVersion() {
  if (m_filePath.isEmpty()) {
    return saveAs();
  }
  qInfo().noquote() << QStringLiteral("Save as New Version: %1 (read-only)").arg(m_filePath);
  m_writeAnyway = true;
  const bool saved = writeFile(m_filePath);
  m_writeAnyway = false;
  if (saved) {
    m_locks->changesSaved();
    showHint(tr("Saved %1 as a new version; someone else holds its edit lock, so Sync asks which version to keep.")
                 .arg(QFileInfo(m_filePath).fileName()));
  }
  return saved;
}

std::optional<bool> MainWindow::readOnlySave(bool closing) {
  if (!windowReadOnly()) {
    return std::nullopt;
  }
  if (!m_locks->keptChanges()) {
    // Nothing of the editing: what changed in the window (isolation,
    // section analyses) is not saved over the file.
    if (!closing) {
      refuseReadOnly(QStringLiteral("save"));
      return false;
    }
    return true;
  }
  const QString name = documentName();
  QMessageBox box(QMessageBox::Warning, tr("Mitcad"),
                  closing ? tr("Keep your changes to %1?").arg(name)
                          : tr("%1 is read-only: someone else holds its edit lock.").arg(name),
                  QMessageBox::Cancel, this);
  box.setInformativeText(tr("Save as Copy keeps them in a file of their own; Save as New Version records them as a "
                            "version of %1 all the same, and Sync then asks which version to keep.")
                             .arg(name));
  QPushButton* copy = box.addButton(tr("Save as &Copy..."), QMessageBox::AcceptRole);
  QPushButton* version = m_filePath.isEmpty() ? nullptr : box.addButton(tr("Save as New &Version"), QMessageBox::ApplyRole);
  QPushButton* discard = closing ? box.addButton(tr("&Don't Save"), QMessageBox::DestructiveRole) : nullptr;
  box.setDefaultButton(copy);
  prepareModal(&box);
  qInfo().noquote() << QStringLiteral("Read-only: unsaved changes of %1 asked about").arg(name);
  box.exec();
  if (box.clickedButton() == copy) {
    return saveAs();
  }
  if (version != nullptr && box.clickedButton() == version) {
    return saveAsNewVersion();
  }
  if (discard != nullptr && box.clickedButton() == discard) {
    return true;
  }
  return false;
}

} // namespace mitcad
