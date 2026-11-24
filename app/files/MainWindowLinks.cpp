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

#include <algorithm>
#include <exception>
#include <functional>
#include <optional>
#include <utility>

#include <QDir>
#include <QFileInfo>
#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QPoint>
#include <QRegularExpression>
#include <QSaveFile>
#include <QStandardPaths>
#include <QStringList>
#include <QToolButton>
#include <QtLogging>

#include "../WindowManager.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/Json.hpp"
#include "../framework/MessageBand.hpp"
#include "FileDialogs.hpp"
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
                                  std::function<void()> run, const QString& source, const QString& tooltip) {
  m_statusActionLabel = action;
  m_statusActionTooltip = tooltip;
  m_statusActionRun = std::move(run);
  m_nextStatusSource = source; // showFeatureStatus takes it (mitcad#2090)
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
  // An occurrence of it machined by cuts made in this assembly
  // (mitcad#2131): its own design opens without them.
  std::function<bool(const QVector<DocumentSnapshot::Occurrence>&)> machined =
      [&](const QVector<DocumentSnapshot::Occurrence>& occurrences) {
        return std::any_of(occurrences.cbegin(), occurrences.cend(), [&](const DocumentSnapshot::Occurrence& o) {
          return (o.component == component && !o.machinedNote.isEmpty()) || machined(o.children);
        });
      };
  for (const DocumentSnapshot::Component& c : std::as_const(m_snapshot.components)) {
    if (c.uid == component && !c.link.isEmpty()) {
      if (machined(m_snapshot.occurrences)) {
        qInfo().noquote() << QStringLiteral("Open Linked Design of machined %1").arg(component);
        showHint(tr("%1 opens as its own design, without the cuts this assembly machines it with.")
                     .arg(m_snapshot.componentName(component)));
      }
      openLinkedFile(c.link);
      return;
    }
  }
  showError(tr("%1 is not a linked design.").arg(component));
}

void MainWindow::openLibrarySource(const QString& source) {
  QJsonObject answer;
  try {
    answer = queryObject({{QStringLiteral("query"), QStringLiteral("library_source")}, {QStringLiteral("source"), source}});
  } catch (const std::exception& e) {
    showError(tr("The source design cannot be opened: %1").arg(errorText(e)));
    return;
  }
  const QString text = answer.value(QStringLiteral("text")).toString();
  if (text.isEmpty()) {
    showWarning(tr("The source design is not read yet: save and open this design again, then try once more."));
    return;
  }
  // One file per version of the source (its SHA-256), named as the source
  // so that its tab says what it is: written once, opened read-only.
  QString name = answer.value(QStringLiteral("name")).toString().trimmed();
  static const QRegularExpression unsafe(QStringLiteral("[^A-Za-z0-9 ._()-]+"));
  name.replace(unsafe, QStringLiteral("_"));
  if (name.isEmpty()) {
    name = QStringLiteral("source");
  }
  const QString folder = QStandardPaths::writableLocation(QStandardPaths::CacheLocation) +
                         QStringLiteral("/library-sources/") +
                         answer.value(QStringLiteral("sha256")).toString().left(16);
  const QString file = folder + QLatin1Char('/') + name + QStringLiteral(".mitcad");
  if (!QFileInfo::exists(file)) {
    QSaveFile out(file);
    if (!QDir().mkpath(folder) || !out.open(QIODevice::WriteOnly) || out.write(text.toUtf8()) < 0 || !out.commit()) {
      showError(tr("The source design could not be written to %1.").arg(QDir::toNativeSeparators(folder)));
      return;
    }
  }
  qInfo().noquote() << QStringLiteral("Open Source Design: %1 (%2) read-only from %3")
                           .arg(answer.value(QStringLiteral("name")).toString(), source, file);
  openReadOnlyFile(file);
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

void MainWindow::chooseLinkedParts(const QString& component) {
  // Choose Linked Parts… (mitcad#2130): the parts of its design a linked
  // component holds, chosen again in the list Insert Component's Choose
  // Parts… shows, as one undo step (`set_link_parts`).
  if (!canChangeModel()) {
    showError(changeBlockedReason());
    return;
  }
  const QString base = m_filePath.isEmpty() ? QString() : QFileInfo(m_filePath).absolutePath();
  QJsonObject request{{QStringLiteral("query"), QStringLiteral("link_parts")},
                      {QStringLiteral("component"), component}};
  if (!base.isEmpty()) {
    request.insert(QStringLiteral("base"), base);
  }
  QJsonObject tree;
  try {
    tree = queryObject(request);
  } catch (const std::exception& e) {
    showError(tr("Could not read the linked design: %1").arg(errorText(e)));
    return;
  }
  const std::optional<LinkPartsChoice> choice = chooseLinkParts(this, tr("Choose Linked Parts"), tree, true);
  if (!choice) {
    return;
  }
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_link_parts")},
                      {QStringLiteral("component"), component}};
  command.insert(QStringLiteral("parts"), choice->whole ? QJsonValue() : QJsonValue(choice->parts));
  if (!base.isEmpty()) {
    command.insert(QStringLiteral("base"), base);
  }
  QJsonObject result;
  if (!runModelCommand(command, &result)) {
    return; // the model's refusal is shown
  }
  QStringList names;
  const QJsonValue parts = result.value(QStringLiteral("parts"));
  for (const QJsonValue& value : parts.toObject().value(QStringLiteral("names")).toArray()) {
    names << value.toString();
  }
  qInfo().noquote() << QStringLiteral("Choose Linked Parts of %1: %2; %3 bodies")
                           .arg(component,
                                parts.isObject() ? names.join(QStringLiteral(", ")) : QStringLiteral("the whole design"),
                                QString::number(result.value(QStringLiteral("bodies")).toArray().size()));
  if (!result.value(QStringLiteral("changed")).toBool()) {
    return;
  }
  showLinkMessages(result.value(QStringLiteral("messages")).toArray());
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
  // This design's Save writes it for edits made in place here, and its
  // link records it (mitcad#2321).
  if (!m_inPlaceSaving.isEmpty() && sameFile(m_inPlaceSaving, path)) {
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
