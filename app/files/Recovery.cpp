// SPDX-License-Identifier: MIT
#include "Recovery.hpp"

#include <algorithm>
#include <utility>

#include <QAbstractItemView>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QHeaderView>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
#include <QLocale>
#include <QLockFile>
#include <QMessageBox>
#include <QPushButton>
#include <QStringList>
#include <QTreeWidget>
#include <QUuid>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "Autosave.hpp"

namespace mitcad {
namespace {

QString unknownDocument() { return QObject::tr("Unknown document"); }

// What the session's files tell; a reason it cannot be restored goes to
// its damage.
void readSession(RecoverableSession& found) {
  const QString metadata = QDir(found.directory).filePath(found.session + QLatin1String(".json"));
  const QFileInfo project(found.projectFile());
  found.document = unknownDocument();
  if (!QFileInfo::exists(metadata)) {
    // The project file is written first: the session's first write ended
    // before its metadata.
    found.savedAt = project.lastModified().toUTC();
    found.size = project.size();
    found.damage = QObject::tr("no metadata, the first autosave did not finish");
    return;
  }
  found.savedAt = QFileInfo(metadata).lastModified().toUTC();
  if (QFileInfo(metadata).isSymLink()) {
    found.damage = QObject::tr("the metadata is a symbolic link");
    return;
  }
  QFile file(metadata);
  const QJsonObject about =
      file.open(QIODevice::ReadOnly) ? QJsonDocument::fromJson(file.readAll()).object() : QJsonObject();
  if (about.value(QStringLiteral("format")).toString() != QLatin1String("mitcad-autosave")) {
    found.damage = QObject::tr("the metadata cannot be read");
    return;
  }
  const QString document = about.value(QStringLiteral("document")).toString();
  if (!document.isEmpty()) {
    found.document = document;
  }
  found.path = about.value(QStringLiteral("path")).toString();
  found.baseDigest = about.value(QStringLiteral("base_digest")).toString();
  const QDateTime savedAt = QDateTime::fromString(about.value(QStringLiteral("saved_at")).toString(), Qt::ISODate);
  if (savedAt.isValid()) {
    found.savedAt = savedAt.toUTC();
  }
  found.size = about.value(QStringLiteral("size")).toInteger(-1);
  found.digest = about.value(QStringLiteral("digest")).toString();
  const int version = about.value(QStringLiteral("version")).toInt();
  if (version != 1 && version != 2) {
    found.damage = QObject::tr("written by another version of Mitcad");
    return;
  }
  const QString snapshot = about.value(QStringLiteral("project")).toString();
  if (about.value(QStringLiteral("session")).toString() != found.session ||
      !validAutosaveProject(found.session, snapshot, version)) {
    found.damage = QObject::tr("the metadata names another project file");
    return;
  }
  found.project = snapshot;
  if (QFileInfo(found.projectFile()).isSymLink()) {
    found.damage = QObject::tr("the project file is a symbolic link");
    return;
  }
  QFile data(found.projectFile());
  if (!data.open(QIODevice::ReadOnly)) {
    found.damage = QObject::tr("the project file is missing");
    return;
  }
  const QByteArray bytes = data.readAll();
  if (bytes.size() != found.size || fileDigest(bytes) != found.digest) {
    found.damage = QObject::tr("the project file is not the one its metadata describes");
  }
}

// Whether the document's file changed after the session opened or saved it.
void compareWithFile(RecoverableSession& found) {
  if (found.path.isEmpty() || found.damaged()) {
    return;
  }
  QFile file(found.path);
  if (!file.exists()) {
    found.fileMissing = true;
    return;
  }
  if (!found.baseDigest.isEmpty() && file.open(QIODevice::ReadOnly)) {
    found.fileChanged = fileDigest(file.readAll()) != found.baseDigest;
  }
}

QString noteOf(const RecoverableSession& session) {
  if (session.damaged()) {
    return QObject::tr("Damaged: %1").arg(session.damage);
  }
  if (session.fileMissing) {
    return QObject::tr("The file is gone");
  }
  if (session.fileChanged) {
    return QObject::tr("The file has changed since");
  }
  return QString();
}

} // namespace

RecoverableSession::RecoverableSession() = default;
RecoverableSession::~RecoverableSession() = default;
RecoverableSession::RecoverableSession(RecoverableSession&&) noexcept = default;
RecoverableSession& RecoverableSession::operator=(RecoverableSession&&) noexcept = default;

QString RecoverableSession::projectFile() const {
  return QDir(directory).filePath(project.isEmpty() ? session + QLatin1String(".mitcad") : project);
}

QString RecoverableSession::describe() const {
  QStringList parts{path.isEmpty() ? QStringLiteral("never saved") : path,
                    QStringLiteral("autosaved ") + savedAt.toString(Qt::ISODate),
                    QStringLiteral("%1 bytes").arg(size)};
  if (fileMissing) {
    parts << QStringLiteral("the file is gone");
  }
  if (fileChanged) {
    parts << QStringLiteral("the file has changed since");
  }
  if (damaged()) {
    parts << QStringLiteral("damaged: ") + damage;
  }
  return QStringLiteral("%1: %2").arg(document, parts.join(QStringLiteral(", ")));
}

std::vector<RecoverableSession> findRecoverable(const QString& directory, const QString& own) {
  std::vector<RecoverableSession> found;
  const QDir dir(directory);
  if (directory.isEmpty() || !dir.exists()) {
    return found;
  }
  QStringList ids;
  const QStringList names = dir.entryList({QStringLiteral("*.json"), QStringLiteral("*.mitcad"), QStringLiteral("*.lock")},
                                          QDir::Files | QDir::Hidden, QDir::Name);
  for (const QString& name : names) {
    const QString id = name.section(QLatin1Char('.'), 0, 0);
    // Only what autosave names (a session's UUID); nothing else there is
    // touched.
    if (id != own && !ids.contains(id) && !QUuid::fromString(id).isNull() &&
        QUuid::fromString(id).toString(QUuid::WithoutBraces) == id) {
      ids << id;
    }
  }
  for (const QString& id : std::as_const(ids)) {
    auto lock = std::make_unique<QLockFile>(dir.filePath(id + QLatin1String(".lock")));
    // Taken only when its process is gone, however old the lock is.
    lock->setStaleLockTime(0);
    if (!lock->tryLock(0)) {
      continue; // a running instance's session
    }
    RecoverableSession session;
    session.directory = dir.absolutePath();
    session.session = id;
    session.lock = std::move(lock);
    if (!QFileInfo::exists(dir.filePath(id + QLatin1String(".json"))) &&
        !QFileInfo::exists(session.projectFile())) {
      // No published snapshot. Remove any abandoned generation safely.
      removeSessionFiles(directory, id);
      session.lock->unlock();
      qDebug().noquote() << QStringLiteral("Recovery: removed the lock of session %1").arg(id);
      continue;
    }
    readSession(session);
    compareWithFile(session);
    found.push_back(std::move(session));
  }
  std::stable_sort(found.begin(), found.end(), [](const RecoverableSession& a, const RecoverableSession& b) {
    return a.savedAt > b.savedAt;
  });
  return found;
}

void discardSession(RecoverableSession& session) {
  removeSessionFiles(session.directory, session.session);
  if (session.lock) {
    session.lock->unlock(); // removes the lock file
    session.lock.reset();
  }
  qInfo().noquote() << QStringLiteral("Discarded recovery of %1").arg(session.document);
}

std::optional<std::size_t> askRecovery(QWidget* parent, std::vector<RecoverableSession>& sessions) {
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Recover Unsaved Work"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* text = new QLabel(QObject::tr("<b>Mitcad ended without closing these documents.</b> Their changes that "
                                      "were not saved are kept."));
  text->setWordWrap(true);
  layout->addWidget(text);
  auto* table = new QTreeWidget;
  table->setRootIsDecorated(false);
  table->setAllColumnsShowFocus(true);
  table->setSelectionMode(QAbstractItemView::ExtendedSelection);
  table->setHeaderLabels({QObject::tr("Document"), QObject::tr("Autosaved"), QObject::tr("Size"),
                          QObject::tr("File"), QObject::tr("Note")});
  table->header()->setSectionResizeMode(QHeaderView::ResizeToContents);
  layout->addWidget(table, 1);
  auto* info = new QLabel(QObject::tr("Restore opens the selected document with these changes, not saved yet and "
                                      "without undo history. One document is restored at a time; the others stay "
                                      "until you restore or discard them (File > Recover Documents)."));
  info->setWordWrap(true);
  layout->addWidget(info);
  auto* buttons = new QDialogButtonBox;
  QPushButton* restore = buttons->addButton(QObject::tr("&Restore"), QDialogButtonBox::AcceptRole);
  QPushButton* discard = buttons->addButton(QObject::tr("&Discard..."), QDialogButtonBox::DestructiveRole);
  buttons->addButton(QObject::tr("&Later"), QDialogButtonBox::RejectRole);
  restore->setDefault(true);
  layout->addWidget(buttons);

  // Row i is sessions[i]; the newest first, selected.
  const auto fill = [&] {
    table->clear();
    for (const RecoverableSession& session : sessions) {
      const QString file = session.path.isEmpty() ? QObject::tr("Never saved") : QDir::toNativeSeparators(session.path);
      auto* row = new QTreeWidgetItem(table, {session.document,
                                              QLocale().toString(session.savedAt.toLocalTime(), QLocale::ShortFormat),
                                              QLocale().formattedDataSize(session.size), file, noteOf(session)});
      row->setToolTip(3, file);
      row->setToolTip(4, noteOf(session));
    }
    if (table->topLevelItemCount() > 0) {
      table->setCurrentItem(table->topLevelItem(0));
    }
  };
  const auto selected = [&] {
    std::vector<std::size_t> rows;
    for (QTreeWidgetItem* item : table->selectedItems()) {
      rows.push_back(static_cast<std::size_t>(table->indexOfTopLevelItem(item)));
    }
    std::sort(rows.begin(), rows.end());
    return rows;
  };
  const auto restorable = [&]() -> std::optional<std::size_t> {
    const std::vector<std::size_t> rows = selected();
    if (rows.size() == 1 && rows.front() < sessions.size() && !sessions[rows.front()].damaged()) {
      return rows.front();
    }
    return std::nullopt;
  };
  const auto update = [&] {
    restore->setEnabled(restorable().has_value());
    discard->setEnabled(!selected().empty());
  };
  std::optional<std::size_t> chosen;
  const auto accept = [&] {
    chosen = restorable();
    if (chosen) {
      dialog.accept();
    }
  };
  QObject::connect(table, &QTreeWidget::itemSelectionChanged, &dialog, update);
  QObject::connect(table, &QTreeWidget::itemDoubleClicked, &dialog, accept);
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  QObject::connect(discard, &QPushButton::clicked, &dialog, [&] {
    const std::vector<std::size_t> rows = selected();
    if (rows.empty()) {
      return;
    }
    QMessageBox box(QMessageBox::Warning, QObject::tr("Discard"),
                    rows.size() == 1
                        ? QObject::tr("Discard the unsaved changes of %1?").arg(sessions[rows.front()].document)
                        : QObject::tr("Discard the unsaved changes of %1 documents?").arg(rows.size()),
                    QMessageBox::Discard | QMessageBox::Cancel, &dialog);
    box.setInformativeText(QObject::tr("They cannot be recovered afterwards."));
    box.button(QMessageBox::Discard)->setText(QObject::tr("&Discard"));
    box.setDefaultButton(QMessageBox::Cancel);
    prepareModal(&box);
    if (box.exec() != QMessageBox::Discard) {
      return;
    }
    for (auto row = rows.rbegin(); row != rows.rend(); ++row) {
      discardSession(sessions[*row]);
      sessions.erase(sessions.begin() + static_cast<std::ptrdiff_t>(*row));
    }
    if (sessions.empty()) {
      dialog.reject();
      return;
    }
    fill();
    update();
  });
  fill();
  update();
  table->setFocus();
  dialog.resize(820, 340);
  prepareModal(&dialog);
  if (dialog.exec() != QDialog::Accepted) {
    return std::nullopt;
  }
  return chosen;
}

} // namespace mitcad
