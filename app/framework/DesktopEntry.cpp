// SPDX-License-Identifier: MIT
#include "DesktopEntry.hpp"

#include <QCoreApplication>
#include <QDir>
#include <QDirIterator>
#include <QFile>
#include <QFileInfo>
#include <QProcess>
#include <QSaveFile>
#include <QStandardPaths>
#include <QtLogging>

#include <algorithm>
#include <optional>
#include <vector>

namespace mitcad {
namespace {

const QString kDesktopFile = QStringLiteral("applications/mitcad.desktop");
const QString kIcons = QStringLiteral("icons/hicolor");
// File managers' thumbnails of designs (mitcad#1539).
const QString kThumbnailer = QStringLiteral("usr/bin/mitcad-thumbnailer");
const QString kThumbnailerCopy = QStringLiteral("mitcad/bin/mitcad-thumbnailer");
const QString kThumbnailerEntry = QStringLiteral("thumbnailers/mitcad.thumbnailer");
const QString kMimePackage = QStringLiteral("mime/packages/mitcad.xml");

// A desktop file's Exec argument: quoted, with the specification's escapes
// inside the quotes, and then a value's (a backslash doubled).
QString execArgument(const QString& program) {
  QString quoted = QStringLiteral("\"");
  for (const QChar c : program) {
    if (c == QLatin1Char('"') || c == QLatin1Char('`') || c == QLatin1Char('$') || c == QLatin1Char('\\')) {
      quoted += QLatin1Char('\\');
    }
    quoted += c;
  }
  quoted += QLatin1Char('"');
  return quoted.replace(QLatin1Char('\\'), QStringLiteral("\\\\"));
}

// A desktop file's string value with its escapes undone (\s, \n, \t, \r,
// \\), and made again (a space stays a space).
QString unescapeValue(const QString& value) {
  QString plain;
  for (qsizetype i = 0; i < value.size(); ++i) {
    const QChar c = value.at(i);
    if (c != QLatin1Char('\\') || i + 1 == value.size()) {
      plain += c;
      continue;
    }
    const QChar next = value.at(++i);
    if (next == QLatin1Char('s')) {
      plain += QLatin1Char(' ');
    } else if (next == QLatin1Char('n')) {
      plain += QLatin1Char('\n');
    } else if (next == QLatin1Char('t')) {
      plain += QLatin1Char('\t');
    } else if (next == QLatin1Char('r')) {
      plain += QLatin1Char('\r');
    } else {
      plain += next;
    }
  }
  return plain;
}

QString escapeValue(const QString& plain) {
  QString value;
  for (const QChar c : plain) {
    if (c == QLatin1Char('\\')) {
      value += QStringLiteral("\\\\");
    } else if (c == QLatin1Char('\n')) {
      value += QStringLiteral("\\n");
    } else if (c == QLatin1Char('\t')) {
      value += QStringLiteral("\\t");
    } else if (c == QLatin1Char('\r')) {
      value += QStringLiteral("\\r");
    } else {
      value += c;
    }
  }
  return value;
}

// One argument of an Exec value (its string escapes undone): where it is
// and what it says, its quotes and their escapes undone.
struct ExecArgument {
  qsizetype begin = 0;
  qsizetype end = 0;
  QString text;
};

std::vector<ExecArgument> execArguments(const QString& exec) {
  std::vector<ExecArgument> arguments;
  qsizetype i = 0;
  while (i < exec.size()) {
    if (exec.at(i) == QLatin1Char(' ')) {
      ++i;
      continue;
    }
    ExecArgument argument;
    argument.begin = i;
    if (exec.at(i) == QLatin1Char('"')) {
      for (++i; i < exec.size() && exec.at(i) != QLatin1Char('"'); ++i) {
        if (exec.at(i) == QLatin1Char('\\') && i + 1 < exec.size()) {
          ++i;
        }
        argument.text += exec.at(i);
      }
      i = std::min(i + 1, exec.size());
    } else {
      for (; i < exec.size() && exec.at(i) != QLatin1Char(' '); ++i) {
        argument.text += exec.at(i);
      }
    }
    argument.end = i;
    arguments.push_back(argument);
  }
  return arguments;
}

// A desktop file's lines by group: the lines before the first group under
// an empty header, then each group's header and its lines.
struct Group {
  QString header;
  QStringList lines;
};

std::vector<Group> groupsOf(const QString& text) {
  std::vector<Group> groups(1);
  for (QString line : text.split(QLatin1Char('\n'))) {
    if (line.endsWith(QLatin1Char('\r'))) {
      line.chop(1);
    }
    if (line.startsWith(QLatin1Char('['))) {
      groups.push_back(Group{line.trimmed(), {}});
    } else {
      groups.back().lines << line;
    }
  }
  return groups;
}

// A line's key (with its locale, Name[fi]); empty for comments and blank
// lines.
QString keyOf(const QString& line) {
  if (line.startsWith(QLatin1Char('#'))) {
    return {};
  }
  const qsizetype equals = line.indexOf(QLatin1Char('='));
  return equals <= 0 ? QString() : line.left(equals).trimmed();
}

QString valueOf(const QString& line) {
  return line.mid(line.indexOf(QLatin1Char('=')) + 1).trimmed();
}

const Group* findGroup(const std::vector<Group>& groups, const QString& header) {
  for (const Group& group : groups) {
    if (!group.header.isEmpty() && group.header == header) {
      return &group;
    }
  }
  return nullptr;
}

// The value of a key in a group; nullopt when the group has no such key.
std::optional<QString> valueIn(const Group* group, const QString& key) {
  if (group) {
    for (const QString& line : group->lines) {
      if (keyOf(line) == key) {
        return valueOf(line);
      }
    }
  }
  return std::nullopt;
}

// Where a group's own lines end: before the blank lines that separate it
// from the next group.
qsizetype groupEnd(const QStringList& lines) {
  qsizetype end = lines.size();
  while (end > 0 && lines.at(end - 1).trimmed().isEmpty()) {
    --end;
  }
  return end;
}

const QString kEntryGroup = QStringLiteral("[Desktop Entry]");
const QString kTryExec = QStringLiteral("TryExec");
const QString kAppImageName = QStringLiteral("X-AppImage-Name");
const QString kAppImageVersion = QStringLiteral("X-AppImage-Version");

// The Exec line running the program: when the earlier entry's Exec runs
// the same program, what an AppImage manager put around it stays (env
// DESKTOPINTEGRATION=1 before it, field codes after it); otherwise the
// bundled entry's arguments follow it.
QString execLine(const QString& bundledExec, const std::optional<QString>& earlierExec, const QString& program) {
  const QString bundled = unescapeValue(bundledExec);
  const std::vector<ExecArgument> bundledArguments = execArguments(bundled);
  QString before;
  QString after = bundledArguments.empty() ? QString() : bundled.mid(bundledArguments.front().end);
  if (earlierExec) {
    const QString earlier = unescapeValue(*earlierExec);
    for (const ExecArgument& argument : execArguments(earlier)) {
      if (argument.text == program) {
        before = earlier.left(argument.begin);
        if (!earlier.mid(argument.end).trimmed().isEmpty()) {
          after = earlier.mid(argument.end);
        }
        break;
      }
    }
  }
  return QStringLiteral("Exec=") + escapeValue(before) + execArgument(program) + escapeValue(after);
}

// Writes a file when its content differs; false with the reason on failure.
bool writeIfChanged(const QString& path, const QByteArray& content, QStringList& written, QString& error) {
  QFile current(path);
  if (current.open(QIODevice::ReadOnly) && current.readAll() == content) {
    return true;
  }
  current.close();
  QDir().mkpath(QFileInfo(path).absolutePath());
  QSaveFile file(path);
  if (!file.open(QIODevice::WriteOnly) || file.write(content) != content.size() || !file.commit()) {
    error = QStringLiteral("cannot write %1: %2").arg(path, file.errorString());
    return false;
  }
  written << path;
  return true;
}

} // namespace

QString desktopEntryFor(const QString& bundled, const QString& program, const QString& version,
                        const QString& earlier) {
  std::vector<Group> groups = groupsOf(bundled);
  const std::vector<Group> earlierGroups = earlier.isEmpty() ? std::vector<Group>() : groupsOf(earlier);
  for (Group& group : groups) {
    if (group.header.isEmpty()) {
      continue;
    }
    const Group* earlierGroup = findGroup(earlierGroups, group.header);
    if (group.header == kEntryGroup) {
      // The keys written for AppImage managers: the program they list
      // (TryExec), its name (a manager's own name stays) and the running
      // AppImage's version (it updates itself).
      const QString bundledName = valueIn(&group, QStringLiteral("Name")).value_or(QStringLiteral("MitCad"));
      const QString name = valueIn(earlierGroup, kAppImageName).value_or(bundledName);
      const QString appVersion =
          version.isEmpty() ? valueIn(earlierGroup, kAppImageVersion).value_or(QString()) : version;
      QStringList lines;
      const qsizetype end = groupEnd(group.lines);
      for (qsizetype i = 0; i < end; ++i) {
        const QString& line = group.lines.at(i);
        const QString key = keyOf(line);
        if (key == kTryExec || key == kAppImageName || key == kAppImageVersion) {
          continue;
        }
        if (key == QLatin1String("Exec")) {
          lines << execLine(valueOf(line), valueIn(earlierGroup, key), program);
          // A string value: only a backslash is escaped.
          lines << kTryExec + QLatin1Char('=') + QString(program).replace(QLatin1Char('\\'), QStringLiteral("\\\\"));
        } else {
          lines << line;
        }
      }
      lines << kAppImageName + QLatin1Char('=') + name;
      if (!appVersion.isEmpty()) {
        lines << kAppImageVersion + QLatin1Char('=') + appVersion;
      }
      group.lines = lines + group.lines.mid(end);
    }
    if (!earlierGroup) {
      continue;
    }
    // The earlier entry's keys this one does not define (an AppImage
    // manager's TryExec and X-AppImage-* keys, a user's NoDisplay), after
    // this one's own.
    QStringList defined;
    for (const QString& line : group.lines) {
      defined << keyOf(line);
    }
    QStringList kept;
    for (const QString& line : earlierGroup->lines) {
      const QString key = keyOf(line);
      if (!key.isEmpty() && !defined.contains(key)) {
        kept << line;
        defined << key;
      }
    }
    const qsizetype end = groupEnd(group.lines);
    group.lines = group.lines.mid(0, end) + kept + group.lines.mid(end);
  }
  // The earlier entry's groups this one does not have, at the end.
  for (const Group& group : earlierGroups) {
    if (!group.header.isEmpty() && !findGroup(groups, group.header)) {
      if (!groups.back().lines.isEmpty() && !groups.back().lines.back().isEmpty()) {
        groups.back().lines << QString();
      }
      groups.push_back(Group{group.header, group.lines.mid(0, groupEnd(group.lines))});
      groups.back().lines << QString();
    }
  }
  QStringList lines = groups.front().lines;
  for (std::size_t i = 1; i < groups.size(); ++i) {
    lines << groups[i].header << groups[i].lines;
  }
  return lines.join(QLatin1Char('\n'));
}

QString thumbnailerEntryFor(const QString& bundled, const QString& program) {
  QStringList lines = bundled.split(QLatin1Char('\n'));
  for (QString& line : lines) {
    if (line.startsWith(QStringLiteral("TryExec="))) {
      // A string value: only a backslash is escaped.
      line = QStringLiteral("TryExec=") + QString(program).replace(QLatin1Char('\\'), QStringLiteral("\\\\"));
    } else if (line.startsWith(QStringLiteral("Exec="))) {
      const qsizetype space = line.indexOf(QLatin1Char(' '));
      line = QStringLiteral("Exec=") + execArgument(program) + (space < 0 ? QString() : line.mid(space));
    }
  }
  return lines.join(QLatin1Char('\n'));
}

DesktopIntegration integrateAppImage(const QString& appImage, const QString& appDir, const QString& dataHome,
                                     const QString& version) {
  DesktopIntegration result;
  const QDir share(QDir(appDir).filePath(QStringLiteral("usr/share")));
  QFile bundled(share.filePath(kDesktopFile));
  if (!bundled.open(QIODevice::ReadOnly)) {
    result.error = QStringLiteral("no desktop file %1").arg(bundled.fileName());
    return result;
  }
  const QDir data(dataHome);
  // The icons first, so that the desktop finds them when the desktop file
  // appears.
  QDirIterator icons(share.filePath(kIcons), {QStringLiteral("mitcad.*")}, QDir::Files,
                     QDirIterator::Subdirectories);
  while (icons.hasNext()) {
    const QString icon = icons.next();
    QFile file(icon);
    if (!file.open(QIODevice::ReadOnly)) {
      result.error = QStringLiteral("cannot read %1").arg(icon);
      return result;
    }
    if (!writeIfChanged(data.filePath(share.relativeFilePath(icon)), file.readAll(), result.written, result.error)) {
      return result;
    }
  }
  // File managers' thumbnails of designs (mitcad#1539), when the AppImage
  // has them (older ones do not): the program first, then the MIME type,
  // then the entry that runs the program for it.
  QFile thumbnailer(QDir(appDir).filePath(kThumbnailer));
  QFile thumbnailerEntry(share.filePath(kThumbnailerEntry));
  QFile mimePackage(share.filePath(kMimePackage));
  if (thumbnailer.open(QIODevice::ReadOnly) && thumbnailerEntry.open(QIODevice::ReadOnly) &&
      mimePackage.open(QIODevice::ReadOnly)) {
    const QString copy = data.filePath(kThumbnailerCopy);
    if (!writeIfChanged(copy, thumbnailer.readAll(), result.written, result.error)) {
      return result;
    }
    // Run by the file manager: executable, as an installed program is.
    const QFileDevice::Permissions executable = QFileDevice::ReadOwner | QFileDevice::WriteOwner |
                                                QFileDevice::ExeOwner | QFileDevice::ReadUser |
                                                QFileDevice::WriteUser | QFileDevice::ExeUser |
                                                QFileDevice::ReadGroup | QFileDevice::ExeGroup |
                                                QFileDevice::ReadOther | QFileDevice::ExeOther;
    if (!QFile::setPermissions(copy, executable)) {
      result.error = QStringLiteral("cannot make %1 executable").arg(copy);
      return result;
    }
    if (!writeIfChanged(data.filePath(kMimePackage), mimePackage.readAll(), result.written, result.error)) {
      return result;
    }
    const QString entry = thumbnailerEntryFor(QString::fromUtf8(thumbnailerEntry.readAll()), copy);
    if (!writeIfChanged(data.filePath(kThumbnailerEntry), entry.toUtf8(), result.written, result.error)) {
      return result;
    }
  }
  // Merged into the entry there, which an AppImage manager may have
  // written (mitcad#2413).
  QFile earlier(data.filePath(kDesktopFile));
  const QString earlierEntry = earlier.open(QIODevice::ReadOnly) ? QString::fromUtf8(earlier.readAll()) : QString();
  earlier.close();
  const QString entry = desktopEntryFor(QString::fromUtf8(bundled.readAll()), appImage, version, earlierEntry);
  writeIfChanged(data.filePath(kDesktopFile), entry.toUtf8(), result.written, result.error);
  return result;
}

void integrateDesktop() {
#ifdef __linux__
  const QString appImage = qEnvironmentVariable("APPIMAGE");
  const QString appDir = qEnvironmentVariable("APPDIR");
  if (appImage.isEmpty() || appDir.isEmpty()) {
    return;
  }
  const QString dataHome = QStandardPaths::writableLocation(QStandardPaths::GenericDataLocation);
  const DesktopIntegration result =
      integrateAppImage(appImage, appDir, dataHome, QCoreApplication::applicationVersion());
  if (!result.error.isEmpty()) {
    qWarning().noquote() << QStringLiteral("Desktop entry: %1").arg(result.error);
  } else if (result.written.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Desktop entry: up to date in %1").arg(dataHome);
  } else {
    for (const QString& path : result.written) {
      qDebug().noquote() << QStringLiteral("Desktop entry: wrote %1").arg(path);
    }
  }
  // A new or changed MIME type counts once the MIME database file managers
  // read is made again from the packages (mitcad#1539).
  const QString mime = QDir(dataHome).filePath(kMimePackage);
  if (result.written.contains(mime)) {
    const QString update = QStandardPaths::findExecutable(QStringLiteral("update-mime-database"));
    const QString database = QDir(dataHome).filePath(QStringLiteral("mime"));
    if (update.isEmpty() || !QProcess::startDetached(update, {database})) {
      qWarning().noquote() << QStringLiteral("Desktop entry: the MIME database in %1 was not updated").arg(database);
    } else {
      qDebug().noquote() << QStringLiteral("Desktop entry: updating the MIME database in %1").arg(database);
    }
  }
#endif
}

} // namespace mitcad
