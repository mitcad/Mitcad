// SPDX-License-Identifier: MIT
#include "VersionHistory.hpp"

#include <algorithm>
#include <exception>

#include <QDir>
#include <QFileInfo>
#include <QFontDatabase>
#include <QHBoxLayout>
#include <QHash>
#include <QHeaderView>
#include <QJsonArray>
#include <QLabel>
#include <QPixmap>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QRadioButton>
#include <QStandardPaths>
#include <QThread>
#include <QTreeWidget>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Json.hpp"
#include "mitcad_bridge/lib.h"

namespace mitcad {
namespace {

// The preview's size (the longer side), as saved and as shown.
constexpr int kPreviewSize = 256;
// At most this many versions are named in one line of the log.
constexpr int kLoggedVersions = 30;

enum Column { kVersion, kSaved, kAuthor, kDescription, kChanges, kColumns };

// A comparison's text in one line of the log.
QString oneLine(const QString& text) {
  return text.split(QLatin1Char('\n'), Qt::SkipEmptyParts).join(QStringLiteral(" | "));
}

// The versions named in the log: "v3 abc1234 <text>", the newest first.
template <typename Text> QString logged(const std::vector<FileVersion>& versions, Text text) {
  QStringList parts;
  for (const FileVersion& version : versions) {
    if (parts.size() == kLoggedVersions) {
      parts << QStringLiteral("... %1 more").arg(versions.size() - kLoggedVersions);
      break;
    }
    parts << text(version);
  }
  return parts.join(QStringLiteral(" | "));
}

} // namespace

QString FileVersion::label() const { return QStringLiteral("v%1").arg(number); }

QString FileVersion::labelWithId() const { return QStringLiteral("v%1 (%2)").arg(number).arg(shortId); }

std::vector<FileVersion> fileVersions(const QJsonObject& history) {
  const QJsonArray list = history.value(QStringLiteral("versions")).toArray();
  std::vector<FileVersion> out;
  out.reserve(static_cast<std::size_t>(list.size()));
  for (qsizetype i = 0; i < list.size(); ++i) {
    const QJsonObject entry = list[i].toObject();
    FileVersion version;
    version.number = static_cast<int>(list.size() - i);
    version.id = entry.value(QStringLiteral("id")).toString();
    version.shortId = entry.value(QStringLiteral("short_id")).toString();
    version.time = QDateTime::fromSecsSinceEpoch(entry.value(QStringLiteral("time")).toInteger()).toLocalTime();
    const QJsonObject author = entry.value(QStringLiteral("author")).toObject();
    version.authorName = author.value(QStringLiteral("name")).toString();
    version.authorEmail = author.value(QStringLiteral("email")).toString();
    version.summary = entry.value(QStringLiteral("summary")).toString();
    version.message = entry.value(QStringLiteral("message")).toString();
    version.path = entry.value(QStringLiteral("path")).toString();
    version.renamedFrom = entry.value(QStringLiteral("renamed_from")).toString();
    version.blob = entry.value(QStringLiteral("blob")).toString(); // null: deleted
    out.push_back(version);
  }
  return out;
}

QString versionThumbnailPath(const QString& blob) {
  QString folder = qEnvironmentVariable("MITCAD_THUMBNAIL_DIR"); // tests
  if (folder.isEmpty()) {
    const QString data = QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation);
    if (!data.isEmpty()) {
      folder = QDir(data).filePath(QStringLiteral("thumbnails"));
    }
  }
  if (folder.isEmpty() || blob.isEmpty()) {
    return {};
  }
  return QDir(folder).filePath(QStringLiteral("%1.png").arg(blob));
}

VersionHistoryDialog::VersionHistoryDialog(QWidget* parent, const QString& file, bool modified,
                                           VersionHistoryHost host)
    : QDialog(parent), m_file(file), m_name(QFileInfo(file).fileName()), m_modified(modified),
      m_host(std::move(host)) {
  setWindowTitle(tr("Version History - %1").arg(m_name));
  auto* layout = new QVBoxLayout(this);
  m_heading = new QLabel(tr("Reading the versions of %1...").arg(m_name));
  m_heading->setWordWrap(true);
  layout->addWidget(m_heading);

  m_list = new QTreeWidget;
  m_list->setColumnCount(kColumns);
  m_list->setHeaderLabels({tr("Version"), tr("Saved"), tr("Author"), tr("Description"), tr("Changes")});
  m_list->setRootIsDecorated(false);
  m_list->setUniformRowHeights(true);
  m_list->setAllColumnsShowFocus(true);
  m_list->setSelectionMode(QAbstractItemView::SingleSelection);
  m_list->header()->setStretchLastSection(true);
  layout->addWidget(m_list, 3);

  auto* middle = new QHBoxLayout;
  m_details = new QPlainTextEdit;
  m_details->setReadOnly(true);
  middle->addWidget(m_details, 1);
  m_preview = new QLabel(tr("No preview"));
  m_preview->setFixedSize(kPreviewSize, kPreviewSize);
  m_preview->setAlignment(Qt::AlignCenter);
  m_preview->setFrameShape(QFrame::StyledPanel);
  m_preview->setToolTip(tr("The view as it was when this version was saved on this computer"));
  middle->addWidget(m_preview);
  layout->addLayout(middle, 2);

  auto* compareRow = new QHBoxLayout;
  compareRow->addWidget(new QLabel(tr("Compare with:")));
  m_withBefore = new QRadioButton(tr("the &previous version"));
  m_withOpen = new QRadioButton(modified ? tr("the open &design (with its unsaved changes)") : tr("the open &design"));
  m_withBefore->setChecked(true);
  compareRow->addWidget(m_withBefore);
  compareRow->addWidget(m_withOpen);
  compareRow->addStretch(1);
  m_geometry = new QPushButton(tr("Compare &Geometry"));
  m_geometry->setToolTip(tr("Computes both designs and compares their bodies' volumes and areas"));
  compareRow->addWidget(m_geometry);
  layout->addLayout(compareRow);

  m_comparison = new QPlainTextEdit;
  m_comparison->setReadOnly(true);
  m_comparison->setLineWrapMode(QPlainTextEdit::NoWrap);
  m_comparison->setFont(QFontDatabase::systemFont(QFontDatabase::FixedFont));
  layout->addWidget(m_comparison, 3);

  auto* buttons = new QHBoxLayout;
  m_open = new QPushButton(tr("&Open"));
  m_open->setToolTip(tr("Opens this version as a new, untitled design; the file stays as it is"));
  m_restore = new QPushButton(tr("&Restore..."));
  m_restore->setToolTip(tr("Makes this version the latest: it is recorded as a new version, and the versions "
                           "since stay in the history"));
  m_copy = new QPushButton(tr("Save &Copy As..."));
  m_copy->setToolTip(tr("Saves this version as a file of its own"));
  auto* close = new QPushButton(tr("Close"));
  for (QPushButton* button : {m_geometry, m_open, m_restore, m_copy, close}) {
    button->setAutoDefault(false); // Enter in the list opens nothing
  }
  buttons->addWidget(m_open);
  buttons->addWidget(m_restore);
  buttons->addWidget(m_copy);
  buttons->addStretch(1);
  buttons->addWidget(close);
  layout->addLayout(buttons);

  connect(m_list, &QTreeWidget::currentItemChanged, this, &VersionHistoryDialog::selected);
  connect(m_withBefore, &QRadioButton::toggled, this, [this](bool on) {
    if (on) {
      selected();
    }
  });
  connect(m_withOpen, &QRadioButton::toggled, this, [this](bool on) {
    if (on) {
      selected();
    }
  });
  connect(m_geometry, &QPushButton::clicked, this, &VersionHistoryDialog::compareGeometry);
  connect(m_open, &QPushButton::clicked, this, [this] { finish(Choice::Open); });
  connect(m_restore, &QPushButton::clicked, this, [this] { finish(Choice::Restore); });
  connect(m_copy, &QPushButton::clicked, this, [this] {
    if (const FileVersion* version = current(); version != nullptr && m_host.saveCopy) {
      qInfo().noquote() << QStringLiteral("Version History save copy %1").arg(version->labelWithId());
      m_host.saveCopy(this, *version);
    }
  });
  connect(close, &QPushButton::clicked, this, &QDialog::reject);
  selected(); // nothing yet: the buttons off
  resize(980, 760);
  m_list->setFocus();
  qInfo().noquote() << QStringLiteral("Version History dialog: %1, reading its versions").arg(m_name);

  // The history on a thread of its own, with a project of its own (a
  // project is used by one thread): the list, then what each version
  // changed, which reads every version.
  const QByteArray path = file.toUtf8();
  m_loader = QThread::create([this, path] {
    try {
      const rust::Box<Project> project = open_project(rustStr(path));
      const auto history = [&](bool summaries) {
        const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("history")},
                                  {QStringLiteral("path"), QString::fromUtf8(path)},
                                  {QStringLiteral("summaries"), summaries}};
        return parseObject(project->command(rustStr(compactJson(command))));
      };
      const QJsonObject list = history(false);
      QMetaObject::invokeMethod(this, [this, list] { showVersions(list); }, Qt::QueuedConnection);
      if (QThread::currentThread()->isInterruptionRequested()) {
        return;
      }
      const QJsonObject changes = history(true);
      QMetaObject::invokeMethod(this, [this, changes] { showChanges(changes); }, Qt::QueuedConnection);
    } catch (const std::exception& e) {
      const QString error = errorText(e);
      QMetaObject::invokeMethod(this, [this, error] { showLoadError(error); }, Qt::QueuedConnection);
    }
  });
  m_loader->start();
}

VersionHistoryDialog::~VersionHistoryDialog() {
  // What it posts after this is dropped with the dialog.
  m_loader->requestInterruption();
  m_loader->wait();
  delete m_loader;
}

void VersionHistoryDialog::showVersions(const QJsonObject& history) {
  m_versions = fileVersions(history);
  m_changes.assign(m_versions.size(), QString());
  const QSignalBlocker blocker(m_list);
  m_list->clear();
  for (const FileVersion& version : m_versions) {
    auto* item = new QTreeWidgetItem(m_list);
    item->setText(kVersion, version.number == versionCount() ? tr("%1 (latest)").arg(version.label())
                                                              : version.label());
    item->setToolTip(kVersion, version.id);
    item->setText(kSaved, version.time.toString(QStringLiteral("yyyy-MM-dd HH:mm")));
    item->setText(kAuthor, version.authorName);
    item->setToolTip(kAuthor, QStringLiteral("%1 <%2>").arg(version.authorName, version.authorEmail));
    QString description = version.summary;
    if (version.deleted()) {
      description += tr(" (deleted)");
    } else if (!version.renamedFrom.isEmpty()) {
      description += tr(" (renamed from %1)").arg(version.renamedFrom);
    }
    item->setText(kDescription, description);
    item->setToolTip(kDescription, version.message);
    item->setText(kChanges, tr("..."));
  }
  for (int column = 0; column < kChanges; ++column) {
    m_list->resizeColumnToContents(column);
  }
  m_list->setColumnWidth(kDescription, std::min(m_list->columnWidth(kDescription), 360));
  const QString root = QFileInfo(m_file).absolutePath();
  if (m_versions.empty()) {
    m_heading->setText(tr("%1 has no version yet: Save records the first.").arg(m_name));
  } else {
    m_heading->setText(tr("%1 has %n version(s), the newest first. Open shows one as a new design, Restore makes "
                          "one the latest version again; the history is never rewritten.",
                          nullptr, versionCount())
                           .arg(m_name));
  }
  m_heading->setToolTip(QDir::toNativeSeparators(root));
  qInfo().noquote() << QStringLiteral("Version History: %1 versions of %2: %3")
                           .arg(versionCount())
                           .arg(m_name,
                                logged(m_versions, [](const FileVersion& version) {
                                  QString text = QStringLiteral("%1 %2 %3").arg(version.label(), version.shortId,
                                                                                version.summary);
                                  if (version.deleted()) {
                                    text += QStringLiteral(" (deleted)");
                                  } else if (!version.renamedFrom.isEmpty()) {
                                    text += QStringLiteral(" (renamed from %1)").arg(version.renamedFrom);
                                  }
                                  return text;
                                }));
  if (m_list->topLevelItemCount() > 0) {
    m_list->setCurrentItem(m_list->topLevelItem(0));
  }
  selected();
}

void VersionHistoryDialog::showChanges(const QJsonObject& history) {
  // By id: the list is the one shown, unless a version came meanwhile.
  QHash<QString, QJsonObject> byId;
  for (const QJsonValue& entry : history.value(QStringLiteral("versions")).toArray()) {
    byId.insert(entry.toObject().value(QStringLiteral("id")).toString(), entry.toObject());
  }
  for (std::size_t row = 0; row < m_versions.size(); ++row) {
    const FileVersion& version = m_versions[row];
    const QJsonObject entry = byId.value(version.id);
    QString text;
    if (version.deleted()) {
      text = tr("deleted");
    } else if (entry.isEmpty()) {
      text = tr("not in the history any more");
    } else if (entry.value(QStringLiteral("changes")).isString()) {
      text = entry.value(QStringLiteral("changes")).toString();
    } else if (entry.contains(QStringLiteral("changes_error"))) {
      text = tr("cannot compare: %1").arg(entry.value(QStringLiteral("changes_error")).toString());
    } else if (row + 1 == m_versions.size()) {
      text = tr("first version");
    } else {
      text = tr("added again");
    }
    m_changes[row] = text;
    if (QTreeWidgetItem* item = m_list->topLevelItem(static_cast<int>(row))) {
      item->setText(kChanges, text);
      item->setToolTip(kChanges, text);
    }
  }
  std::size_t row = 0;
  qInfo().noquote() << QStringLiteral("Version History changes: %1").arg(logged(m_versions, [&](const FileVersion& v) {
    return QStringLiteral("%1 %2").arg(v.label(), m_changes[row++]);
  }));
}

void VersionHistoryDialog::showLoadError(const QString& error) {
  m_heading->setText(tr("Could not read the versions of %1: %2").arg(m_name, error));
  qWarning().noquote() << QStringLiteral("Version History failed: %1").arg(error);
}

const FileVersion* VersionHistoryDialog::current() const {
  const int row = m_list->indexOfTopLevelItem(m_list->currentItem());
  if (row < 0 || static_cast<std::size_t>(row) >= m_versions.size()) {
    return nullptr;
  }
  return &m_versions[static_cast<std::size_t>(row)];
}

const FileVersion* VersionHistoryDialog::before(const FileVersion& version) const {
  // Newest first: the one before is the next row.
  const auto row = static_cast<std::size_t>(versionCount() - version.number) + 1;
  return row < m_versions.size() ? &m_versions[row] : nullptr;
}

void VersionHistoryDialog::selected() {
  const FileVersion* version = current();
  const bool there = version != nullptr && !version->deleted();
  m_open->setEnabled(there);
  m_copy->setEnabled(there);
  // The latest version is the file's content already.
  m_restore->setEnabled(there && version->number != versionCount());
  const FileVersion* older = there ? before(*version) : nullptr;
  m_geometry->setEnabled(there && (m_withOpen->isChecked() || (older != nullptr && !older->deleted())));
  if (version == nullptr) {
    m_details->clear();
    m_comparison->clear();
    m_preview->setPixmap(QPixmap());
    m_preview->setText(tr("No preview"));
    return;
  }
  QString details = tr("%1 of %2, %3").arg(version->label(), version->path, version->id) + QLatin1Char('\n');
  details += tr("Saved %1 by %2 <%3>")
                 .arg(version->time.toString(QStringLiteral("yyyy-MM-dd HH:mm:ss")), version->authorName,
                      version->authorEmail) +
             QLatin1Char('\n');
  if (!version->renamedFrom.isEmpty()) {
    details += tr("Renamed from %1").arg(version->renamedFrom) + QLatin1Char('\n');
  }
  if (version->deleted()) {
    details += tr("This version deleted the file.") + QLatin1Char('\n');
  }
  details += QLatin1Char('\n') + version->message;
  m_details->setPlainText(details);
  const QPixmap preview(versionThumbnailPath(version->blob));
  if (preview.isNull()) {
    m_preview->setPixmap(QPixmap());
    m_preview->setText(version->deleted() ? QString() : tr("No preview"));
  } else {
    m_preview->setPixmap(preview.scaled(m_preview->contentsRect().size(), Qt::KeepAspectRatio,
                                        Qt::SmoothTransformation));
  }
  qInfo().noquote() << QStringLiteral("Version History selected %1: %2")
                           .arg(version->labelWithId(),
                                preview.isNull() ? QStringLiteral("no preview") : QStringLiteral("preview"));
  compare();
}

void VersionHistoryDialog::showComparison(const QString& heading, const QString& text) {
  m_comparison->setPlainText(heading.isEmpty() ? text : heading + QLatin1Char('\n') + text);
}

void VersionHistoryDialog::compare() {
  const FileVersion* version = current();
  if (version == nullptr) {
    return;
  }
  if (version->deleted()) {
    showComparison(QString(), tr("%1 deleted %2: there is no design to compare.").arg(version->label(), m_name));
    return;
  }
  QString heading;
  QString text;
  QString with;
  if (m_withOpen->isChecked()) {
    with = QStringLiteral("the open design");
    heading = m_modified ? tr("Changes from %1 to the open design (with its unsaved changes):").arg(version->labelWithId())
                         : tr("Changes from %1 to the open design:").arg(version->labelWithId());
    text = m_host.compareWithOpen ? m_host.compareWithOpen(*version) : QString();
  } else {
    const FileVersion* older = before(*version);
    if (older == nullptr || older->deleted()) {
      showComparison(QString(), older == nullptr ? tr("%1 is the first version of %2.").arg(version->label(), m_name)
                                                 : tr("%1 added %2 again.").arg(version->label(), m_name));
      qInfo().noquote() << QStringLiteral("Version History compare %1: nothing before it").arg(version->label());
      return;
    }
    with = older->label();
    heading = tr("Changes from %1 to %2:").arg(older->labelWithId(), version->labelWithId());
    // Nothing computed: the two versions' project files, read from their
    // commits (P12c, the version history's diff command).
    try {
      const QByteArray path = m_file.toUtf8();
      const rust::Box<Project> project = open_project(rustStr(path));
      const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("diff")},
                                {QStringLiteral("path"), m_file},
                                {QStringLiteral("from"), older->id},
                                {QStringLiteral("to"), version->id}};
      text = parseObject(project->command(rustStr(compactJson(command)))).value(QStringLiteral("text")).toString();
    } catch (const std::exception& e) {
      text = tr("Could not compare them: %1").arg(errorText(e));
    }
  }
  showComparison(heading, text);
  qInfo().noquote() << QStringLiteral("Version History compare %1 with %2: %3")
                           .arg(version->label(), with, oneLine(heading + QLatin1Char('\n') + text));
}

void VersionHistoryDialog::compareGeometry() {
  const FileVersion* version = current();
  if (version == nullptr || version->deleted() || !m_host.compareGeometry) {
    return;
  }
  const bool withOpen = m_withOpen->isChecked();
  const FileVersion* older = withOpen ? nullptr : before(*version);
  if (!withOpen && older == nullptr) {
    return;
  }
  const QString with = withOpen ? QStringLiteral("the open design") : older->label();
  const QString heading =
      withOpen ? tr("Changes from %1 to the open design, with the geometry:").arg(version->labelWithId())
               : tr("Changes from %1 to %2, with the geometry:").arg(older->labelWithId(), version->labelWithId());
  qInfo().noquote() << QStringLiteral("Version History geometry %1 with %2: computing").arg(version->label(), with);
  std::optional<QString> text;
  try {
    text = m_host.compareGeometry(older, *version);
  } catch (const std::exception& e) {
    text = tr("Could not compare the geometry: %1").arg(errorText(e));
  }
  if (!text) {
    qInfo().noquote() << QStringLiteral("Version History geometry %1 with %2: cancelled").arg(version->label(), with);
    return;
  }
  showComparison(heading, *text);
  qInfo().noquote() << QStringLiteral("Version History geometry %1 with %2: %3")
                           .arg(version->label(), with, oneLine(*text));
}

void VersionHistoryDialog::finish(Choice choice) {
  const FileVersion* version = current();
  if (version == nullptr) {
    return;
  }
  m_choice = choice;
  m_chosen = *version;
  qInfo().noquote() << QStringLiteral("Version History %1 %2")
                           .arg(choice == Choice::Open ? QStringLiteral("open") : QStringLiteral("restore"),
                                version->labelWithId());
  accept();
}

} // namespace mitcad
