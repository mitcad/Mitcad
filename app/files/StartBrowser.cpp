// SPDX-License-Identifier: MIT
#include "StartBrowser.hpp"

#include <QDir>
#include <QFileInfo>
#include <QSettings>

namespace mitcad::start {
namespace {

QString cleaned(const QString& path) { return QDir::cleanPath(QDir::fromNativeSeparators(path)); }

bool holdsPath(const QStringList& paths, const QString& path) {
  for (const QString& each : paths) {
    if (samePath(each, path)) {
      return true;
    }
  }
  return false;
}

QString kindWord(Row::Kind kind) {
  switch (kind) {
  case Row::Kind::Pinned:
    return QStringLiteral("pinned");
  case Row::Kind::Project:
    return QStringLiteral("project");
  case Row::Kind::Recent:
    break;
  }
  return QStringLiteral("recent");
}

} // namespace

bool Row::operator==(const Row& other) const {
  if (kind != other.kind) {
    return false;
  }
  return kind != Kind::Project || (samePath(root, other.root) && folder == other.folder);
}

QString rowKey() { return QStringLiteral("start/browseRow"); }
QString rootKey() { return QStringLiteral("start/browseRoot"); }
QString folderKey() { return QStringLiteral("start/browseFolder"); }

Row loadRow() {
  const QSettings settings;
  const QString kind = settings.value(rowKey()).toString();
  Row row;
  if (kind == QLatin1String("pinned")) {
    row.kind = Row::Kind::Pinned;
  } else if (kind == QLatin1String("project")) {
    row.kind = Row::Kind::Project;
    row.root = settings.value(rootKey()).toString();
    row.folder = settings.value(folderKey()).toString();
    if (row.root.isEmpty()) {
      row = Row();
    }
  }
  return row;
}

void saveRow(const Row& row) {
  QSettings settings;
  settings.setValue(rowKey(), kindWord(row.kind));
  if (row.kind == Row::Kind::Project) {
    settings.setValue(rootKey(), row.root);
    settings.setValue(folderKey(), row.folder);
  } else {
    settings.remove(rootKey());
    settings.remove(folderKey());
  }
}

Row shownRow(const Row& row, const QVector<ProjectRow>& projects) {
  if (row.kind != Row::Kind::Project) {
    return row;
  }
  const ProjectRow* project = findProject(projects, row.root);
  if (project == nullptr) {
    return Row();
  }
  Row shown;
  shown.kind = Row::Kind::Project;
  shown.root = project->root;
  shown.folder = placeExists(projects, {project->root, row.folder}) ? row.folder : QString();
  return shown;
}

bool samePath(const QString& a, const QString& b) {
#ifdef _WIN32
  return cleaned(a).compare(cleaned(b), Qt::CaseInsensitive) == 0;
#else
  return cleaned(a) == cleaned(b);
#endif
}

Card recentCard(const RecentDesign& design) {
  Card card;
  card.path = design.path;
  card.name = design.name;
  card.type = design.type;
  card.location = design.folder;
  card.key = design.name;
  card.when = design.opened;
  card.pinned = design.pinned;
  card.missing = design.missing;
  card.preview = design.preview;
  return card;
}

QString designPath(const BrowserEntry& entry) { return cleaned(QDir(entry.root).filePath(entry.path)); }

QString locationText(const BrowserEntry& entry) {
  QStringList parts{entry.projectName};
  parts << entry.folder().split(QLatin1Char('/'), Qt::SkipEmptyParts);
  return parts.join(QStringLiteral(" / "));
}

Card projectCard(const BrowserEntry& entry, const QStringList& pinned) {
  Card card;
  card.path = designPath(entry);
  card.name = lastPart(entry.path);
  card.type = recent::typeText(entry.path);
  card.location = locationText(entry);
  card.key = entry.key();
  card.when = entry.design.modified.isValid()   ? entry.design.modified
              : entry.design.versionTime > 0 ? QDateTime::fromSecsSinceEpoch(entry.design.versionTime)
                                             : QDateTime();
  card.pinned = holdsPath(pinned, card.path);
  card.inProject = true;
  return card;
}

QVector<Card> placeCards(const QVector<BrowserEntry>& entries, const QStringList& pinned) {
  QVector<Card> cards;
  for (const BrowserEntry& entry : entries) {
    if (entry.kind == BrowserEntry::Kind::Design) {
      cards << projectCard(entry, pinned);
    }
  }
  return cards;
}

QVector<BrowserEntry> placeFolders(const QVector<BrowserEntry>& entries) {
  QVector<BrowserEntry> folders;
  for (const BrowserEntry& entry : entries) {
    if (entry.kind == BrowserEntry::Kind::Folder) {
      folders << entry;
    }
  }
  return folders;
}

void findLocally(Card* card) {
  if (card == nullptr || card->missing || card->path.isEmpty()) {
    return;
  }
  if (card->preview.isEmpty()) {
    const QString preview = recent::designPreviewPath(card->path);
    if (!preview.isEmpty() && QFileInfo(preview).isFile()) {
      card->preview = preview;
    }
  }
  if (!card->when.isValid()) {
    const QFileInfo info(card->path);
    if (info.isFile()) {
      card->when = info.lastModified();
    }
  }
}

QVector<Card> searchCards(const QVector<ProjectRow>& projects, const QHash<QString, QString>& keys,
                          const QList<RecentDesign>& recent, const QString& text, const QStringList& pinned) {
  QVector<Card> cards;
  const QString wanted = text.trimmed();
  if (wanted.isEmpty()) {
    return cards;
  }
  for (const BrowserEntry& entry : searchDesigns(projects, keys, wanted, QString())) {
    cards << projectCard(entry, pinned);
  }
  const qsizetype inProjects = cards.size();
  for (const RecentDesign& design : recent) {
    if (!QFileInfo(design.name).completeBaseName().contains(wanted, Qt::CaseInsensitive)) {
      continue;
    }
    bool listed = false;
    for (qsizetype i = 0; i < inProjects && !listed; ++i) {
      listed = samePath(cards.at(i).path, design.path);
    }
    if (!listed) {
      cards << recentCard(design);
    }
  }
  return cards;
}

QString projectsLine(const QVector<ProjectRow>& projects) {
  QStringList listed;
  for (const ProjectRow& row : projects) {
    listed << QStringLiteral("%1 (%2)").arg(row.name, row.kind == ProjectRow::Kind::Cloud ? QStringLiteral("cloud")
                                                                                         : QStringLiteral("local"));
  }
  return QStringLiteral("Start page projects: %1")
      .arg(listed.isEmpty() ? QStringLiteral("none") : listed.join(QStringLiteral(" | ")));
}

QString placeLine(const QString& placeKey, const QVector<BrowserEntry>& entries) {
  int folders = 0;
  int designs = 0;
  for (const BrowserEntry& entry : entries) {
    if (entry.kind == BrowserEntry::Kind::Folder) {
      ++folders;
    } else {
      ++designs;
    }
  }
  return QStringLiteral("Start page place: %1 (%2 folders, %3 designs)").arg(placeKey).arg(folders).arg(designs);
}

QString searchLine(const QVector<Card>& cards) {
  QStringList keys;
  for (const Card& card : cards) {
    keys << card.key;
  }
  QString line = QStringLiteral("Start page search results: %1").arg(cards.size());
  if (!keys.isEmpty()) {
    line += QStringLiteral(" (%1)").arg(keys.join(QStringLiteral(" | ")));
  }
  return line;
}

} // namespace mitcad::start
