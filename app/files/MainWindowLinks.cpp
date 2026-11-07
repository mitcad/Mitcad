// SPDX-License-Identifier: MIT
// Linked designs while their holder is open (mitcad#1416, app/COMMANDS.md
// "Linked designs"): an edit refused on a linked component says where to
// change it, with Open Linked Design; the context menus of a linked
// design's occurrence open it, read its file again now (Update Link) or
// every linked design's (Update All Links), each one undo step; what
// reading the linked files on opening did goes to the message band; and a
// linked file that changes on disk, or is saved in another tab, is offered
// in the band as "<file> changed" with Update. Nothing updates on its own.
#include "../MainWindow.hpp"

#include <optional>
#include <utility>

#include <QDir>
#include <QFileInfo>
#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QPoint>
#include <QStringList>
#include <QToolButton>
#include <QtLogging>

#include "../WindowManager.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/Json.hpp"
#include "../framework/MessageBand.hpp"
#include "LinkWatcher.hpp"
#include "LinkedFiles.hpp"

namespace mitcad {
namespace {

QString fileName(const QString& path) { return QFileInfo(path).fileName(); }

// A line of `update_links` or `update_link` about a file that was not
// taken (not followed, not read, not a design): a warning; the others
// (taken, up to date) are information.
bool linkProblem(const QString& line) {
  return !line.contains(QStringLiteral(" is updated from ")) && !line.contains(QStringLiteral(" is up to date"));
}

} // namespace

void MainWindow::showActionStatus(const QString& message, NoticeLevel level, const QString& action,
                                  std::function<void()> run) {
  m_statusActionLabel = action;
  m_statusActionRun = std::move(run);
  showFeatureStatus(message, level, QString());
}

void MainWindow::logStatusActionPlace() {
  // As the Floating band logs its button, so that a UI test clicks either.
  if (m_statusActionButton == nullptr) {
    return;
  }
  QString line;
  if (m_statusActionButton->isVisible()) {
    const QPoint at = GlassCard::mapToHost(m_statusActionButton, m_statusActionButton->rect().center());
    line = QStringLiteral("Message band action %1 at %2,%3").arg(m_statusActionButton->text()).arg(at.x()).arg(at.y());
  }
  if (line != m_loggedStatusAction) {
    m_loggedStatusAction = line;
    if (!line.isEmpty()) {
      qDebug().noquote() << line;
    }
  }
}

bool MainWindow::showLinkedRefusal(const QString& message) {
  const std::optional<LinkedRefusal> refusal = linkedRefusal(message);
  if (!refusal) {
    return false;
  }
  showLinkedNotice(refusal->component, refusal->path);
  return true;
}

void MainWindow::showLinkedNotice(const QString& component, const QString& path) {
  qInfo().noquote() << QStringLiteral("Edit of linked %1 refused; Open Linked Design offered (%2)").arg(component, path);
  showActionStatus(tr("%1 is linked from %2").arg(component, fileName(path)), NoticeLevel::Information,
                   tr("Open Linked Design"), [this, path] { openLinkedFile(path); });
}

void MainWindow::openLinkedDesign(const QString& component) {
  for (const DocumentSnapshot::Component& c : std::as_const(m_snapshot.components)) {
    if (c.uid == component && !c.link.isEmpty()) {
      openLinkedFile(c.link);
      return;
    }
  }
  showError(tr("%1 is not a linked design.").arg(component));
}

void MainWindow::openLinkedFile(const QString& path) {
  const QString file = linkedFilePath(m_filePath, path);
  if (file.isEmpty()) {
    showWarning(tr("%1 cannot be opened: this design is not saved yet, so its link has no folder.").arg(path));
    return;
  }
  if (!QFileInfo::exists(file)) {
    showWarning(tr("%1 is not there (%2).").arg(fileName(path), QDir::toNativeSeparators(file)));
    return;
  }
  qInfo().noquote() << QStringLiteral("Open Linked Design: %1").arg(file);
  QString error;
  if (m_windowManager != nullptr) {
    m_windowManager->openFileBeside(this, file, error);
  } else {
    // One window only: the linked design takes this one's place.
    if (!maybeSave()) {
      return;
    }
    openFile(file, error);
  }
  if (!error.isEmpty()) {
    showError(tr("Could not open %1: %2").arg(fileName(file), error));
  }
}

void MainWindow::updateLink(const QString& component) {
  if (m_filePath.isEmpty()) {
    showWarning(tr("Save the design first: its links are followed from its folder."));
    return;
  }
  if (!canChangeModel()) {
    showError(changeBlockedReason());
    return;
  }
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("update_link")},
                      {QStringLiteral("base"), QFileInfo(m_filePath).absolutePath()}};
  if (!component.isEmpty()) {
    command.insert(QStringLiteral("component"), component);
  }
  QJsonObject result;
  if (!runModelCommand(command, &result)) {
    return; // the model's refusal is shown
  }
  QStringList lines;
  bool problem = false;
  for (const QJsonValue& value : result.value(QStringLiteral("messages")).toArray()) {
    lines << value.toString();
    problem = problem || linkProblem(value.toString());
  }
  const int updated = static_cast<int>(result.value(QStringLiteral("updated")).toArray().size());
  qInfo().noquote() << QStringLiteral("%1: %2 updated; %3")
                           .arg(component.isEmpty() ? QStringLiteral("Update All Links")
                                                    : QStringLiteral("Update Link %1").arg(component))
                           .arg(updated)
                           .arg(lines.join(QStringLiteral(" | ")));
  showStatus(lines.join(QStringLiteral("; ")), problem ? NoticeLevel::Warning : NoticeLevel::Information);
}

void MainWindow::showLinkMessages(const QJsonArray& messages) {
  for (const QJsonValue& value : messages) {
    const QString line = value.toString();
    if (!line.isEmpty()) {
      showStatus(line, linkProblem(line) ? NoticeLevel::Warning : NoticeLevel::Information);
    }
  }
}

void MainWindow::checkLinkedFiles(const QString& file) {
  if (m_filePath.isEmpty()) {
    return; // no folder: no link is followed
  }
  if (modelBusy()) {
    whenIdle(this, [this, file] { checkLinkedFiles(file); });
    return;
  }
  const QString folder = QFileInfo(m_filePath).absolutePath();
  QJsonObject answer;
  try {
    answer = queryObject({{QStringLiteral("query"), QStringLiteral("links")}, {QStringLiteral("base"), folder}});
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("Linked files not checked: %1").arg(errorText(e));
    return;
  }
  // The changed files, each with the components that link it.
  QStringList changed;
  QHash<QString, QStringList> components;
  for (const QJsonValue& value : answer.value(QStringLiteral("links")).toArray()) {
    const QJsonObject link = value.toObject();
    const QString linked = link.value(QStringLiteral("file")).toString();
    if (!link.value(QStringLiteral("changed")).toBool() || linked.isEmpty()) {
      continue;
    }
    if (!file.isEmpty() && !sameFile(linked, file)) {
      continue;
    }
    if (!components.contains(linked)) {
      changed << linked;
    }
    components[linked] << link.value(QStringLiteral("component")).toString();
  }
  for (const QString& linked : std::as_const(changed)) {
    const QString name = fileName(linked);
    const QString text = tr("%1 changed").arg(name);
    // Said once: a save seen on disk and by its tab come together.
    if (m_messages != nullptr) {
      const MessageEntry* full = m_messages->log().full();
      if (full != nullptr && full->text == text && !full->action.isEmpty()) {
        continue;
      }
    }
    const QStringList uids = components.value(linked);
    qInfo().noquote() << QStringLiteral("Linked file changed: %1 (%2); Update offered")
                             .arg(name, uids.join(QStringLiteral(", ")));
    showActionStatus(text, NoticeLevel::Information, tr("Update"), [this, uids, name, folder] {
      if (uids.size() == 1) {
        updateLink(uids.first());
        return;
      }
      // Several components link the file: one undo step for them all.
      QJsonArray commands;
      for (const QString& uid : uids) {
        commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("update_link")},
                                    {QStringLiteral("component"), uid},
                                    {QStringLiteral("base"), folder}});
      }
      if (runNamedModelCommands(commands, tr("Update Link %1").arg(name))) {
        qInfo().noquote() << QStringLiteral("Update Link %1: %2 components").arg(name).arg(uids.size());
        showStatus(tr("Updated from %1").arg(name), NoticeLevel::Information);
      }
    });
  }
}

void MainWindow::linkedDesignSaved(const QString& path) {
  if (m_linkWatcher == nullptr) {
    return;
  }
  for (const QString& file : m_linkWatcher->files()) {
    if (sameFile(file, path)) {
      qInfo().noquote() << QStringLiteral("Linked design saved in another tab: %1").arg(fileName(path));
      checkLinkedFiles(file);
      return;
    }
  }
}

void MainWindow::refreshLinkWatch() {
  QStringList files;
  for (const DocumentSnapshot::Component& c : std::as_const(m_snapshot.components)) {
    if (c.linked && !c.library) {
      const QString file = linkedFilePath(m_filePath, c.link);
      if (!file.isEmpty()) {
        files << file;
      }
    }
  }
  if (files.isEmpty() && m_linkWatcher == nullptr) {
    return;
  }
  if (m_linkWatcher == nullptr) {
    m_linkWatcher = new LinkWatcher(this);
    connect(m_linkWatcher, &LinkWatcher::changed, this, [this](const QString& file) { checkLinkedFiles(file); });
  }
  m_linkWatcher->setFiles(files);
}

} // namespace mitcad
