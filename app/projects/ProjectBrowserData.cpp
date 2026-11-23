// SPDX-License-Identifier: MIT
#include "ProjectBrowserData.hpp"

#include <algorithm>

#include <QDir>
#include <QFileInfo>
#include <QSet>

#include "ProjectRowText.hpp"

namespace mitcad {
namespace {

QString cleaned(const QString& path) { return QDir::cleanPath(QDir::fromNativeSeparators(path)); }

// Two project folders are the same (case ignored on Windows), as sameRoot.
bool sameFolder(const QString& a, const QString& b) {
#ifdef _WIN32
  return cleaned(a).compare(cleaned(b), Qt::CaseInsensitive) == 0;
#else
  return cleaned(a) == cleaned(b);
#endif
}

bool byName(const QString& a, const QString& b) {
  const int folded = a.compare(b, Qt::CaseInsensitive);
  return folded != 0 ? folded < 0 : a < b;
}

BrowserEntry designEntry(const ProjectRow& project, const QString& key, const ProjectDesignRow& design) {
  BrowserEntry entry;
  entry.kind = BrowserEntry::Kind::Design;
  entry.root = project.root;
  entry.projectKey = key;
  entry.projectName = project.name;
  entry.path = design.path;
  entry.name = designDisplayName(design.path);
  entry.design = design;
  return entry;
}

} // namespace

bool BrowserPlace::operator==(const BrowserPlace& other) const {
  return sameFolder(root, other.root) && folder == other.folder;
}

QString BrowserEntry::folder() const { return kind == Kind::Folder ? path : dirOf(path); }

QString BrowserEntry::key() const {
  return QStringLiteral("%1/%2%3").arg(projectKey, path,
                                       kind == Kind::Folder ? QStringLiteral("/") : QString());
}

QVector<ProjectRow> browsableProjects(const QVector<ProjectRow>& rows) {
  QVector<ProjectRow> offered;
  for (const ProjectRow& row : rows) {
    if (row.kind == ProjectRow::Kind::Local || row.kind == ProjectRow::Kind::Cloud) {
      offered << row;
    }
  }
  std::stable_sort(offered.begin(), offered.end(), [](const ProjectRow& a, const ProjectRow& b) {
    const int folded = a.name.compare(b.name, Qt::CaseInsensitive);
    return folded != 0 ? folded < 0 : a.root < b.root;
  });
  return offered;
}

QHash<QString, QString> projectKeys(const QVector<ProjectRow>& projects) {
  QHash<QString, QString> keys;
  for (const ProjectRow& row : projects) {
    const QString folder = row.folderName.isEmpty() ? QFileInfo(row.root).fileName() : row.folderName;
    int same = 0;
    for (const ProjectRow& other : projects) {
      const QString otherFolder = other.folderName.isEmpty() ? QFileInfo(other.root).fileName() : other.folderName;
      same += otherFolder == folder ? 1 : 0;
    }
    keys.insert(row.root, same > 1 ? row.root : folder);
  }
  return keys;
}

const ProjectRow* findProject(const QVector<ProjectRow>& projects, const QString& root) {
  if (root.isEmpty()) {
    return nullptr;
  }
  for (const ProjectRow& row : projects) {
    if (sameFolder(row.root, root)) {
      return &row;
    }
  }
  return nullptr;
}

QStringList allFolders(const ProjectRow& project) {
  QSet<QString> seen;
  QStringList folders;
  const auto add = [&](QString folder) {
    // The folder and each folder above it.
    while (!folder.isEmpty() && !seen.contains(folder)) {
      seen.insert(folder);
      folders << folder;
      folder = dirOf(folder);
    }
  };
  for (const QString& folder : project.folders) {
    add(folder);
  }
  for (const ProjectDesignRow& design : project.designs) {
    add(dirOf(design.path));
  }
  std::sort(folders.begin(), folders.end(), byName);
  return folders;
}

QStringList subFolders(const ProjectRow& project, const QString& folder) {
  QStringList inside;
  for (const QString& each : allFolders(project)) {
    if (dirOf(each) == folder) {
      inside << each;
    }
  }
  std::sort(inside.begin(), inside.end(), [](const QString& a, const QString& b) {
    return byName(lastPart(a), lastPart(b));
  });
  return inside;
}

bool placeExists(const QVector<ProjectRow>& projects, const BrowserPlace& place) {
  const ProjectRow* project = findProject(projects, place.root);
  return project != nullptr && (place.folder.isEmpty() || allFolders(*project).contains(place.folder));
}

QVector<BrowserEntry> folderContents(const ProjectRow& project, const QString& projectKey, const QString& folder) {
  QVector<BrowserEntry> entries;
  for (const QString& sub : subFolders(project, folder)) {
    BrowserEntry entry;
    entry.kind = BrowserEntry::Kind::Folder;
    entry.root = project.root;
    entry.projectKey = projectKey;
    entry.projectName = project.name;
    entry.path = sub;
    entry.name = lastPart(sub);
    entries << entry;
  }
  QVector<BrowserEntry> designs;
  for (const ProjectDesignRow& design : project.designs) {
    if (dirOf(design.path) == folder) {
      designs << designEntry(project, projectKey, design);
    }
  }
  std::sort(designs.begin(), designs.end(),
            [](const BrowserEntry& a, const BrowserEntry& b) { return byName(a.name, b.name); });
  entries << designs;
  // Other files (mitcad#2204), after the designs.
  QVector<BrowserEntry> files;
  for (const ProjectFileRow& file : project.files) {
    if (dirOf(file.path) == folder) {
      BrowserEntry entry;
      entry.kind = BrowserEntry::Kind::File;
      entry.root = project.root;
      entry.projectKey = projectKey;
      entry.projectName = project.name;
      entry.path = file.path;
      entry.name = lastPart(file.path);
      entry.file = file;
      files << entry;
    }
  }
  std::sort(files.begin(), files.end(),
            [](const BrowserEntry& a, const BrowserEntry& b) { return byName(a.name, b.name); });
  entries << files;
  return entries;
}

QVector<BrowserEntry> searchDesigns(const QVector<ProjectRow>& projects, const QHash<QString, QString>& keys,
                                    const QString& text, const QString& root) {
  QVector<BrowserEntry> found;
  const QString wanted = text.trimmed();
  if (wanted.isEmpty()) {
    return found;
  }
  for (const ProjectRow& project : projects) {
    if (!root.isEmpty() && !sameFolder(project.root, root)) {
      continue;
    }
    const QString key = keys.value(project.root, project.folderName);
    QVector<BrowserEntry> inProject;
    for (const ProjectDesignRow& design : project.designs) {
      if (designDisplayName(design.path).contains(wanted, Qt::CaseInsensitive)) {
        inProject << designEntry(project, key, design);
      }
    }
    std::sort(inProject.begin(), inProject.end(),
              [](const BrowserEntry& a, const BrowserEntry& b) { return byName(a.path, b.path); });
    found << inProject;
  }
  // The projects are in their listed order (browsableProjects: by name).
  return found;
}

QVector<BrowserPathPart> pathParts(const ProjectRow& project, const QString& folder) {
  QVector<BrowserPathPart> parts;
  parts.push_back({project.name, {project.root, QString()}});
  if (folder.isEmpty()) {
    return parts;
  }
  QString walked;
  for (const QString& part : folder.split(QLatin1Char('/'), Qt::SkipEmptyParts)) {
    walked = walked.isEmpty() ? part : QStringLiteral("%1/%2").arg(walked, part);
    parts.push_back({part, {project.root, walked}});
  }
  return parts;
}

QString placeKey(const QString& projectKey, const QString& folder) {
  return folder.isEmpty() ? projectKey : QStringLiteral("%1/%2/").arg(projectKey, folder);
}

BrowserPlace placeUp(const BrowserPlace& place) { return {place.root, dirOf(place.folder)}; }

QString entryWords(const BrowserEntry& entry) {
  if (entry.kind == BrowserEntry::Kind::Folder) {
    return entry.name + QLatin1Char('/');
  }
  if (entry.kind == BrowserEntry::Kind::File) {
    return QStringLiteral("%1 file, %2").arg(entry.name, fileMarks(entry.file).words);
  }
  return QStringLiteral("%1 %2").arg(lastPart(entry.path), designMarks(entry.design).words);
}

} // namespace mitcad
