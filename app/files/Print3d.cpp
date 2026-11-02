// SPDX-License-Identifier: MIT
#include "../framework/DialogFolders.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Theme.hpp"
#include "Print3d.hpp"

#include <algorithm>
#include <cmath>
#include <utility>

#include <QComboBox>
#include <QCryptographicHash>
#include <QDir>
#include <QDirIterator>
#include <QFile>
#include <QFileInfo>
#include <QFormLayout>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QProcess>
#include <QSet>
#include <QSettings>
#include <QStandardPaths>
#include <QTextStream>
#include <QVariantMap>
#include <QtLogging>

#ifndef _WIN32
#include <unistd.h>
#endif

namespace mitcad {
namespace {

// The slicers 3D Print knows by name.
struct KnownSlicer {
  const char* name;
  // In the registry's DisplayName and a desktop file's Name.
  const char* label;
  // Windows: the folder under Program Files ('*' ends a folder name that
  // goes on with a version) and the program in it.
  const char* folder;
  const char* windowsProgram;
  // The program's names on PATH.
  const char* programs;
  const char* flatpak;
  // macOS: the application bundles' names in the Applications folders.
  const char* bundles;
};

const KnownSlicer kKnownSlicers[] = {
    {"Bambu Studio", "Bambu Studio", "Bambu Studio", "bambu-studio.exe", "bambu-studio,BambuStudio",
     "com.bambulab.BambuStudio", "BambuStudio.app,Bambu Studio.app"},
    {"OrcaSlicer", "OrcaSlicer", "OrcaSlicer", "orca-slicer.exe", "orca-slicer,OrcaSlicer,orcaslicer",
     "io.github.softfever.OrcaSlicer", "OrcaSlicer.app"},
    // QIDI Studio (mitcad#640): no Flatpak.
    {"QIDI Studio", "QIDI Studio", "QIDIStudio", "qidi-studio.exe", "qidi-studio,QIDIStudio,qidistudio", "",
     "QIDIStudio.app,QIDI Studio.app"},
    {"PrusaSlicer", "PrusaSlicer", "Prusa3D/PrusaSlicer", "prusa-slicer.exe", "prusa-slicer,PrusaSlicer",
     "com.prusa3d.PrusaSlicer", "PrusaSlicer.app,Original Prusa Drivers/PrusaSlicer.app"},
    {"UltiMaker Cura", "Cura", "UltiMaker Cura*", "UltiMaker-Cura.exe", "cura,UltiMaker-Cura,ultimaker-cura",
     "com.ultimaker.cura", "UltiMaker Cura.app,Ultimaker Cura.app"},
};

const QString kGroup = QStringLiteral("print");
const QString kOpen = QStringLiteral("/usr/bin/open");

void addSlicer(QVector<Slicer>& found, Slicer slicer) {
  if (slicer.isValid() && !found.contains(slicer)) {
    found.append(std::move(slicer));
  }
}

// A program file in `dir`, following a '*' at the end of the folder's last
// name (the newest version: the last name in order).
QString programIn(const QString& base, const KnownSlicer& known) {
  QString folder = QString::fromLatin1(known.folder);
  QString dir = base;
  if (folder.endsWith(QLatin1Char('*'))) {
    const QString parent = QFileInfo(base + QLatin1Char('/') + folder).path();
    const QString pattern = QFileInfo(folder).fileName();
    QStringList versions = QDir(parent).entryList({pattern}, QDir::Dirs | QDir::NoDotAndDotDot, QDir::Name);
    if (versions.isEmpty()) {
      return QString();
    }
    dir = parent + QLatin1Char('/') + versions.last();
  } else {
    dir = base + QLatin1Char('/') + folder;
  }
  const QString program = dir + QLatin1Char('/') + QString::fromLatin1(known.windowsProgram);
  return QFileInfo(program).isFile() ? QDir::toNativeSeparators(program) : QString();
}

#ifdef _WIN32
// The installed programs the registry lists (read only): a known slicer's
// install folder, or the program its icon is taken from.
void searchRegistry(QVector<Slicer>& found) {
  const QString uninstall = QStringLiteral("\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall");
  const QPair<QString, QSettings::Format> places[] = {
      {QStringLiteral("HKEY_LOCAL_MACHINE") + uninstall, QSettings::Registry64Format},
      {QStringLiteral("HKEY_LOCAL_MACHINE") + uninstall, QSettings::Registry32Format},
      {QStringLiteral("HKEY_CURRENT_USER") + uninstall, QSettings::NativeFormat},
  };
  for (const auto& [key, format] : places) {
    QSettings registry(key, format);
    for (const QString& entry : registry.childGroups()) {
      const QString name = registry.value(entry + QStringLiteral("/DisplayName")).toString();
      for (const KnownSlicer& known : kKnownSlicers) {
        if (!name.contains(QString::fromLatin1(known.label), Qt::CaseInsensitive)) {
          continue;
        }
        QString program;
        const QString location = registry.value(entry + QStringLiteral("/InstallLocation")).toString();
        const QString inFolder = location + QLatin1Char('/') + QString::fromLatin1(known.windowsProgram);
        if (!location.isEmpty() && QFileInfo(inFolder).isFile()) {
          program = inFolder;
        } else {
          // DisplayIcon: "C:\...\program.exe,0", maybe quoted.
          QString icon = registry.value(entry + QStringLiteral("/DisplayIcon")).toString();
          icon = icon.section(QLatin1Char(','), 0, 0).remove(QLatin1Char('"')).trimmed();
          if (icon.endsWith(QStringLiteral(".exe"), Qt::CaseInsensitive) && QFileInfo(icon).isFile()) {
            program = icon;
          }
        }
        if (!program.isEmpty()) {
          addSlicer(found, Slicer{QString::fromLatin1(known.name), QDir::toNativeSeparators(program), {}, {}});
        }
      }
    }
  }
}
#endif

// A desktop file's Name and Exec (its main group).
struct DesktopEntry {
  QString name;
  QString exec;
};

DesktopEntry readDesktopEntry(const QString& path) {
  DesktopEntry entry;
  QFile file(path);
  if (!file.open(QIODevice::ReadOnly | QIODevice::Text)) {
    return entry;
  }
  QTextStream in(&file);
  bool main = false;
  while (!in.atEnd()) {
    const QString line = in.readLine().trimmed();
    if (line.startsWith(QLatin1Char('['))) {
      main = line == QStringLiteral("[Desktop Entry]");
    } else if (main && line.startsWith(QStringLiteral("Name="))) {
      entry.name = line.mid(5).trimmed();
    } else if (main && line.startsWith(QStringLiteral("Exec="))) {
      entry.exec = line.mid(5).trimmed();
    }
  }
  return entry;
}

// An instance's placement: its path of occurrence uids ("O1/O4"), as a
// pick names it; empty in the root component.
QString placementOf(const QJsonObject& instance) {
  QStringList path;
  for (const QJsonValue& occurrence : instance.value(QStringLiteral("path")).toArray()) {
    path << occurrence.toString();
  }
  return path.join(QLatin1Char('/'));
}

} // namespace

QString Slicer::describe() const {
  if (!isValid()) {
    return QStringLiteral("none");
  }
  QStringList command{QDir::toNativeSeparators(program)};
  command << arguments << QStringLiteral("<files>") << argumentsAfter;
  return QStringLiteral("%1 (%2)").arg(name.isEmpty() ? QFileInfo(program).fileName() : name,
                                        command.join(QLatin1Char(' ')));
}

QString Slicer::location() const {
  if (program == kOpen && arguments.size() == 2 && arguments.first() == QStringLiteral("-a")) {
    return arguments.last();
  }
  return program;
}

QString slicerDialect(const Slicer& slicer) {
  // By its name, else its program's file name (a program chosen by hand).
  const QString name = slicer.name + QLatin1Char(' ') + QFileInfo(slicer.location()).fileName();
  const QString lower = name.toLower();
  if (lower.contains(QStringLiteral("bambu"))) {
    return QStringLiteral("bambu_studio");
  }
  if (lower.contains(QStringLiteral("orca"))) {
    return QStringLiteral("orcaslicer");
  }
  if (lower.contains(QStringLiteral("qidi"))) {
    return QStringLiteral("qidi_studio");
  }
  if (lower.contains(QStringLiteral("prusa"))) {
    return QStringLiteral("prusaslicer");
  }
  if (lower.contains(QStringLiteral("cura"))) {
    return QStringLiteral("cura");
  }
  return QString();
}

Slicer slicerForProgram(const QString& name, const QString& program) {
  if (program.endsWith(QStringLiteral(".app"), Qt::CaseInsensitive) && QFileInfo(program).isDir()) {
    return Slicer{name, kOpen, {QStringLiteral("-a"), program}, {}};
  }
  return Slicer{name, QDir::toNativeSeparators(program), {}, {}};
}

SlicerHandOver slicerHandOver(const Slicer& slicer) {
  // An application bundle: `open -a` exits at once, and macOS gives the
  // running application the files itself.
  if (slicer.program == kOpen) {
    return SlicerHandOver::BringForward;
  }
  const QString dialect = slicerDialect(slicer);
  if (dialect == QStringLiteral("prusaslicer") || dialect == QStringLiteral("cura")) {
    return SlicerHandOver::SingleInstanceSwitch;
  }
  if (dialect == QStringLiteral("bambu_studio") || dialect == QStringLiteral("orcaslicer") ||
      dialect == QStringLiteral("qidi_studio")) {
    return SlicerHandOver::OwnPreference;
  }
  return SlicerHandOver::BringForward;
}

QStringList slicerArguments(const Slicer& slicer, const QStringList& files, bool singleInstance) {
  QStringList arguments = slicer.arguments;
  if (singleInstance && !arguments.contains(QStringLiteral("--single-instance"))) {
    // A Flatpak's arguments end with the file forwarding's "@@": the
    // switch goes before it, to the application.
    const qsizetype at =
        !arguments.isEmpty() && arguments.constLast() == QStringLiteral("@@") ? arguments.size() - 1 : arguments.size();
    arguments.insert(at, QStringLiteral("--single-instance"));
  }
  for (const QString& file : files) {
    arguments << QDir::toNativeSeparators(file);
  }
  arguments << slicer.argumentsAfter;
  return arguments;
}

bool slicerSingleInstanceSetting(const QByteArray& text) {
  // The JSON without the comment lines ("# MD5 checksum …").
  QByteArray json;
  for (const QByteArray& line : text.split('\n')) {
    if (!line.trimmed().startsWith('#')) {
      json += line + '\n';
    }
  }
  const QJsonObject app = QJsonDocument::fromJson(json).object().value(QStringLiteral("app")).toObject();
  const QJsonValue value = app.value(QStringLiteral("single_instance"));
  if (value.isBool()) {
    return value.toBool();
  }
  const QString setting = value.toString().trimmed().toLower();
  return setting == QStringLiteral("true") || setting == QStringLiteral("1");
}

QStringList slicerSettingsFiles(const Slicer& slicer) {
  if (slicerHandOver(slicer) != SlicerHandOver::OwnPreference) {
    return {};
  }
  // The slicer's folder and file: BambuStudio/BambuStudio.conf, ….
  const QString dialect = slicerDialect(slicer);
  const QString name = dialect == QStringLiteral("bambu_studio") ? QStringLiteral("BambuStudio")
                       : dialect == QStringLiteral("orcaslicer") ? QStringLiteral("OrcaSlicer")
                                                                 : QStringLiteral("QIDIStudio");
  const QString file = QStringLiteral("/%1/%1.conf").arg(name);
  QStringList files;
  // A Flatpak keeps its own: ~/.var/app/<id>/config.
  const qsizetype forwarding = slicer.arguments.indexOf(QStringLiteral("--file-forwarding"));
  if (forwarding >= 0 && forwarding + 1 < slicer.arguments.size()) {
    files << QDir::homePath() + QStringLiteral("/.var/app/") + slicer.arguments.at(forwarding + 1) +
                 QStringLiteral("/config") + file;
  }
#ifdef _WIN32
  const QString appData = qEnvironmentVariable("APPDATA");
  if (!appData.isEmpty()) {
    files << QDir::fromNativeSeparators(appData) + file;
  }
#elif defined(__APPLE__)
  files << QDir::homePath() + QStringLiteral("/Library/Application Support") + file;
#else
  const QString config = qEnvironmentVariable("XDG_CONFIG_HOME");
  files << (config.isEmpty() ? QDir::homePath() + QStringLiteral("/.config") : config) + file;
#endif
  return files;
}

bool slicerKeepsOneInstance(const Slicer& slicer) {
  for (const QString& path : slicerSettingsFiles(slicer)) {
    QFile file(path);
    if (file.open(QIODevice::ReadOnly)) {
      return slicerSingleInstanceSetting(file.readAll());
    }
  }
  return false;
}

SlicerSearch SlicerSearch::system() {
  SlicerSearch search;
  search.path = qEnvironmentVariable("PATH").split(QDir::listSeparator(), Qt::SkipEmptyParts);
#ifdef _WIN32
  for (const char* variable : {"ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"}) {
    const QString dir = qEnvironmentVariable(variable);
    if (!dir.isEmpty() && !search.programDirectories.contains(dir, Qt::CaseInsensitive)) {
      search.programDirectories << dir;
    }
  }
  const QString local = qEnvironmentVariable("LOCALAPPDATA");
  if (!local.isEmpty()) {
    search.programDirectories << local + QStringLiteral("/Programs");
  }
  search.registry = true;
#elif defined(__APPLE__)
  search.bundleDirectories = {QStringLiteral("/Applications"), QDir::homePath() + QStringLiteral("/Applications")};
#else
  search.applicationDirectories = QStandardPaths::standardLocations(QStandardPaths::ApplicationsLocation);
  search.flatpakDirectories = {QStringLiteral("/var/lib/flatpak/app"),
                               QDir::homePath() + QStringLiteral("/.local/share/flatpak/app")};
#endif
  return search;
}

QVector<Slicer> findSlicers(const SlicerSearch& search) {
  QVector<Slicer> found;
  for (const KnownSlicer& known : kKnownSlicers) {
    const QString name = QString::fromLatin1(known.name);
    for (const QString& dir : search.programDirectories) {
      addSlicer(found, Slicer{name, programIn(dir, known), {}, {}});
    }
    for (const QString& dir : search.bundleDirectories) {
      for (const QString& bundle : QString::fromLatin1(known.bundles).split(QLatin1Char(','))) {
        const QString path = dir + QLatin1Char('/') + bundle;
        if (QFileInfo(path).isDir()) {
          addSlicer(found, slicerForProgram(name, path));
        }
      }
    }
    for (const QString& program : QString::fromLatin1(known.programs).split(QLatin1Char(','))) {
      const QString path = QStandardPaths::findExecutable(program, search.path);
      if (!path.isEmpty()) {
        addSlicer(found, Slicer{name, QDir::toNativeSeparators(path), {}, {}});
      }
    }
  }
#ifdef _WIN32
  if (search.registry) {
    searchRegistry(found);
  }
#endif
  // Flatpaks: run with the files forwarded into the sandbox (its /tmp is
  // its own).
  const QString flatpak = QStandardPaths::findExecutable(QStringLiteral("flatpak"), search.path);
  for (const QString& dir : search.flatpakDirectories) {
    for (const KnownSlicer& known : kKnownSlicers) {
      const QString id = QString::fromLatin1(known.flatpak);
      if (!flatpak.isEmpty() && !id.isEmpty() && QFileInfo(dir + QLatin1Char('/') + id).isDir()) {
        addSlicer(found, Slicer{QStringLiteral("%1 (Flatpak)").arg(QString::fromLatin1(known.name)), flatpak,
                                {QStringLiteral("run"), QStringLiteral("--file-forwarding"), id, QStringLiteral("@@")},
                                {QStringLiteral("@@")}});
      }
    }
  }
  // Desktop files of the slicers (an AppImage integrated into the
  // desktop); Flatpaks' are found above.
  for (const QString& dir : search.applicationDirectories) {
    QDirIterator files(dir, {QStringLiteral("*.desktop")}, QDir::Files);
    while (files.hasNext()) {
      const DesktopEntry entry = readDesktopEntry(files.next());
      for (const KnownSlicer& known : kKnownSlicers) {
        if (!entry.name.contains(QString::fromLatin1(known.label), Qt::CaseInsensitive)) {
          continue;
        }
        QStringList command = QProcess::splitCommand(entry.exec);
        // Field codes (%f, %F, %u, %U, ...) stand for what is opened.
        command.erase(std::remove_if(command.begin(), command.end(),
                                     [](const QString& part) { return part.startsWith(QLatin1Char('%')); }),
                      command.end());
        if (command.isEmpty() || QFileInfo(command.first()).fileName() == QStringLiteral("flatpak")) {
          continue;
        }
        QString program = command.takeFirst();
        if (QFileInfo(program).isRelative()) {
          program = QStandardPaths::findExecutable(program, search.path);
        }
        if (!program.isEmpty() && QFileInfo(program).isFile()) {
          addSlicer(found, Slicer{QString::fromLatin1(known.name), QDir::toNativeSeparators(program), command, {}});
        }
      }
    }
  }
  return found;
}

PrintSettings PrintSettings::load() {
  QSettings settings;
  settings.beginGroup(kGroup);
  PrintSettings print;
  print.slicer.name = settings.value(QStringLiteral("slicerName")).toString();
  print.slicer.program = settings.value(QStringLiteral("slicerProgram")).toString();
  print.slicer.arguments = settings.value(QStringLiteral("slicerArguments")).toStringList();
  print.slicer.argumentsAfter = settings.value(QStringLiteral("slicerArgumentsAfter")).toStringList();
  const QString format = settings.value(QStringLiteral("format"), print.format).toString();
  print.format = format == QStringLiteral("stl") ? format : QStringLiteral("3mf");
  const QString refinement = settings.value(QStringLiteral("refinement"), print.refinement).toString();
  if (QStringList{QStringLiteral("low"), QStringLiteral("medium"), QStringLiteral("high"), QStringLiteral("custom")}
          .contains(refinement)) {
    print.refinement = refinement;
  }
  print.deviation = std::clamp(settings.value(QStringLiteral("deviation"), print.deviation).toDouble(), 0.001, 10.0);
  print.angle = std::clamp(settings.value(QStringLiteral("angle"), print.angle).toDouble(), 0.5, 90.0);
  print.reuseWindow = settings.value(QStringLiteral("reuseWindow"), print.reuseWindow).toBool();
  return print;
}

void PrintSettings::save() const {
  QSettings settings;
  settings.beginGroup(kGroup);
  settings.setValue(QStringLiteral("slicerName"), slicer.name);
  settings.setValue(QStringLiteral("slicerProgram"), slicer.program);
  settings.setValue(QStringLiteral("slicerArguments"), slicer.arguments);
  settings.setValue(QStringLiteral("slicerArgumentsAfter"), slicer.argumentsAfter);
  settings.setValue(QStringLiteral("format"), format);
  settings.setValue(QStringLiteral("refinement"), refinement);
  settings.setValue(QStringLiteral("deviation"), deviation);
  settings.setValue(QStringLiteral("angle"), angle);
  settings.setValue(QStringLiteral("reuseWindow"), reuseWindow);
}

namespace {

// The settings' group of a design's Center on Plate and Drop to Bed
// (mitcad#1109): "print/placement/<the SHA-1 of its file's path>", the
// path being no key QSettings takes as it is; "unsaved" without a file.
QString placementGroup(const QString& design) {
  const QString key =
      design.isEmpty()
          ? QStringLiteral("unsaved")
          : QString::fromLatin1(
                QCryptographicHash::hash(QDir::cleanPath(design).toUtf8(), QCryptographicHash::Sha1).toHex());
  return QStringLiteral("%1/placement/%2").arg(kGroup, key);
}

} // namespace

PrintPlacement PrintPlacement::load(const QString& design) {
  QSettings settings;
  settings.beginGroup(placementGroup(design));
  PrintPlacement placement;
  placement.center = settings.value(QStringLiteral("center"), placement.center).toBool();
  placement.drop = settings.value(QStringLiteral("drop"), placement.drop).toBool();
  return placement;
}

void PrintPlacement::save(const QString& design) const {
  QSettings settings;
  settings.beginGroup(placementGroup(design));
  settings.setValue(QStringLiteral("center"), center);
  settings.setValue(QStringLiteral("drop"), drop);
}

QString safeFileName(const QString& name) {
  QString safe;
  for (const QChar c : name.trimmed()) {
    const bool reserved = c.unicode() < 0x20 || QStringLiteral("<>:\"/\\|?*").contains(c);
    safe += reserved ? QLatin1Char('_') : c;
  }
  // Windows drops trailing dots and spaces.
  while (safe.endsWith(QLatin1Char('.')) || safe.endsWith(QLatin1Char(' '))) {
    safe.chop(1);
  }
  if (safe.isEmpty()) {
    return QStringLiteral("Body");
  }
  // Names Windows keeps for devices, also with an extension.
  static const QStringList devices = {QStringLiteral("CON"), QStringLiteral("PRN"), QStringLiteral("AUX"),
                                      QStringLiteral("NUL")};
  const QString stem = safe.section(QLatin1Char('.'), 0, 0).toUpper();
  const bool numbered = stem.size() == 4 && (stem.startsWith(QStringLiteral("COM")) || stem.startsWith(QStringLiteral("LPT"))) &&
                        stem.at(3).isDigit();
  if (devices.contains(stem) || numbered) {
    safe.prepend(QLatin1Char('_'));
  }
  return safe;
}

QStringList printFileNames(const QStringList& bodies, const QString& extension) {
  QStringList names;
  QSet<QString> taken;
  for (const QString& body : bodies) {
    const QString base = safeFileName(body);
    QString name = base + QLatin1Char('.') + extension;
    for (int n = 2; taken.contains(name.toLower()); ++n) {
      name = QStringLiteral("%1_%2.%3").arg(base).arg(n).arg(extension);
    }
    taken.insert(name.toLower());
    names << name;
  }
  return names;
}

QVector<PrintPiece> printPieces(const QStringList& bodies, const QStringList& names, const QJsonArray& instances,
                                const Selection& chosen) {
  QVector<PrintPiece> pieces;
  for (int i = 0; i < bodies.size(); ++i) {
    QVector<QJsonObject> mine;
    QVector<QJsonObject> shown;
    for (const QJsonValue& value : instances) {
      const QJsonObject instance = value.toObject();
      if (instance.value(QStringLiteral("body")).toString() == bodies[i]) {
        mine << instance;
        if (instance.value(QStringLiteral("visible")).toBool()) {
          shown << instance;
        }
      }
    }
    const QVector<QJsonObject>& placed = shown.isEmpty() ? mine : shown;
    if (placed.size() <= 1) {
      pieces.append({bodies[i], QString(), names[i]});
      continue;
    }
    // The placements chosen (mitcad#234), all when none of them is.
    QVector<QJsonObject> sent;
    for (const QJsonObject& instance : placed) {
      if (chosen.contains(SelectionItem{SelectKind::Body, bodies[i], QString(), QString(), placementOf(instance)})) {
        sent << instance;
      }
    }
    for (const QJsonObject& instance : sent.isEmpty() ? placed : sent) {
      pieces.append({bodies[i], placementOf(instance),
                     QStringLiteral("%1 (%2)").arg(names[i], instance.value(QStringLiteral("occurrence")).toString())});
    }
  }
  return pieces;
}

namespace {

// A number of the bed's log line: three decimals, no "-0.000".
QString bedNumber(double value) {
  return QString::number(std::abs(value) < 0.0005 ? 0.0 : value, 'f', 3);
}

} // namespace

QString printBedLog(const QJsonObject& bed) {
  const QJsonArray plate = bed.value(QStringLiteral("plate")).toArray();
  const QString centre = QStringLiteral("%1,%2").arg(bedNumber(plate.at(0).toDouble()),
                                                     bedNumber(plate.at(1).toDouble()));
  QStringList rows;
  for (const QJsonValue& row : bed.value(QStringLiteral("transform")).toArray()) {
    QStringList numbers;
    for (const QJsonValue& number : row.toArray()) {
      numbers << bedNumber(number.toDouble());
    }
    rows << numbers.join(QLatin1Char(' '));
  }
  // A face the automatic orientation chose (mitcad#1109).
  const QString automatic = bed.value(QStringLiteral("auto")).toBool() ? QStringLiteral(" (automatic)") : QString();
  QString line;
  if (bed.value(QStringLiteral("body")).isString()) {
    line = QStringLiteral("%1 on %2%3, plate %4, transform %5")
               .arg(bed.value(QStringLiteral("name")).toString(), bed.value(QStringLiteral("face")).toString(),
                    automatic, centre, rows.join(QStringLiteral(" | ")));
  } else {
    if (!bed.value(QStringLiteral("kept")).toArray().isEmpty()) {
      QStringList kept;
      for (const QJsonValue& uid : bed.value(QStringLiteral("kept")).toArray()) {
        kept << uid.toString();
      }
      line = QStringLiteral("kept as assembled: %1").arg(kept.join(QStringLiteral(", ")));
    } else {
      line = QStringLiteral("as in the design");
    }
    // Centred and dropped all the same (mitcad#1109).
    if (!rows.isEmpty()) {
      line += QStringLiteral(", plate %1, transform %2").arg(centre, rows.join(QStringLiteral(" | ")));
    }
  }
  // The file's other objects, each on the bed by itself (mitcad#856): with
  // no object of the rest only they.
  QStringList objects;
  for (const QJsonValue& value : bed.value(QStringLiteral("objects")).toArray()) {
    const QJsonObject object = value.toObject();
    const QString objectName = object.value(QStringLiteral("name")).toString();
    const QString objectAutomatic =
        object.value(QStringLiteral("auto")).toBool() ? QStringLiteral(" (automatic)") : QString();
    objects << (object.value(QStringLiteral("face")).isString()
                    ? QStringLiteral("%1 on %2%3")
                          .arg(objectName, object.value(QStringLiteral("face")).toString(), objectAutomatic)
                    : QStringLiteral("%1 kept as assembled").arg(objectName));
  }
  if (!objects.isEmpty()) {
    const bool rest = bed.value(QStringLiteral("body")).isString() ||
                      !bed.value(QStringLiteral("kept")).toArray().isEmpty();
    line = (rest ? line + QStringLiteral("; ") : QString()) +
           QStringLiteral("objects %1").arg(objects.join(QStringLiteral(", ")));
  }
  for (const QJsonValue& value : bed.value(QStringLiteral("problems")).toArray()) {
    const QJsonObject problem = value.toObject();
    line += QStringLiteral("; %1: %2").arg(problem.value(QStringLiteral("name")).toString(),
                                           problem.value(QStringLiteral("reason")).toString());
  }
  return line;
}

QColor printBedColor() { return viewColor(ColorRole::PrintBed, false); }

QString PrintBedFace::describe() const {
  return occurrenceName.isEmpty() ? QStringLiteral("%1 %2").arg(name, face)
                                  : QStringLiteral("%1 %2 in %3").arg(name, face, occurrenceName);
}

QVector<PrintBedAsk> printBedAsks(const QJsonObject& bed) {
  QVector<PrintBedAsk> asks;
  const auto add = [&asks](const QString& piece, const QJsonArray& choices) {
    if (choices.size() < 2) {
      return;
    }
    PrintBedAsk ask;
    ask.piece = piece;
    QStringList keys;
    for (const QJsonValue& value : choices) {
      const QJsonObject choice = value.toObject();
      PrintBedFace face;
      face.body = choice.value(QStringLiteral("body")).toString();
      face.name = choice.value(QStringLiteral("name")).toString();
      face.face = choice.value(QStringLiteral("face")).toString();
      face.occurrence = choice.value(QStringLiteral("occurrence")).toString();
      face.occurrenceName = choice.value(QStringLiteral("occurrence_name")).toString();
      keys << QStringLiteral("%1@%2").arg(face.body, face.occurrence);
      ask.faces.append(face);
    }
    keys.sort();
    ask.key = piece + QLatin1Char('|') + keys.join(QLatin1Char(','));
    asks.append(ask);
  };
  add(QString(), bed.value(QStringLiteral("choices")).toArray());
  for (const QJsonValue& value : bed.value(QStringLiteral("objects")).toArray()) {
    const QJsonObject object = value.toObject();
    add(object.value(QStringLiteral("name")).toString(), object.value(QStringLiteral("choices")).toArray());
  }
  return asks;
}

QString PrintBedAsk::pieceName() const {
  if (!piece.isEmpty()) {
    return piece;
  }
  // The occurrence names the faces' paths start with alike.
  QStringList shared;
  bool first = true;
  for (const PrintBedFace& face : faces) {
    const QStringList names = face.occurrenceName.split(QLatin1Char('/'), Qt::SkipEmptyParts);
    if (first) {
      shared = names;
      first = false;
      continue;
    }
    qsizetype same = 0;
    while (same < shared.size() && same < names.size() && shared.at(same) == names.at(same)) {
      ++same;
    }
    shared = shared.mid(0, same);
  }
  return shared.join(QLatin1Char('/'));
}

namespace {

// The body of a face to choose, by name; with its place when the piece has
// faces of the body in several places (mitcad#857).
QString bedFaceOwner(const PrintBedAsk& ask, const PrintBedFace& face) {
  const auto sameBody = std::count_if(ask.faces.begin(), ask.faces.end(),
                                      [&face](const PrintBedFace& other) { return other.body == face.body; });
  if (sameBody < 2 || face.occurrenceName.isEmpty()) {
    return face.name;
  }
  return QStringLiteral("%1 (%2)").arg(face.name, face.occurrenceName);
}

} // namespace

QString printBedQuestion(const PrintBedAsk& ask) {
  const QString piece = ask.pieceName();
  return QObject::tr("Not sent yet: %1 has %2 print bed faces. Click the green one to put on the bed.")
      .arg(piece.isEmpty() ? QObject::tr("the piece") : piece)
      .arg(ask.faces.size());
}

QString printBedHover(const PrintBedAsk& ask, const PrintBedFace& face) {
  return QObject::tr("%1: print bed face").arg(bedFaceOwner(ask, face));
}

QString printBedRefused() { return QObject::tr("Not one of the print bed faces: click a green face."); }

QString printBedChosen(const PrintBedAsk& ask, const PrintBedFace& face) {
  const QString piece = ask.pieceName();
  return QObject::tr("%1 on %2's print bed face.")
      .arg(piece.isEmpty() ? QObject::tr("The piece") : piece, bedFaceOwner(ask, face));
}

QString printBedLine(const QString& message, int onFaces, const QStringList& chosen) {
  QString line = message;
  for (const QString& face : chosen) {
    line += QLatin1Char(' ') + face;
  }
  const int others = onFaces - static_cast<int>(chosen.size());
  if (others <= 0) {
    return line;
  }
  if (chosen.isEmpty()) {
    return line + QLatin1Char(' ') +
           (others == 1 ? QObject::tr("1 body on its print bed face.")
                        : QObject::tr("%1 bodies on their print bed faces.").arg(others));
  }
  return line + QLatin1Char(' ') +
         (others == 1 ? QObject::tr("1 other body on its print bed face.")
                      : QObject::tr("%1 other bodies on their print bed faces.").arg(others));
}

QString preparePrintFolder(const QString& root, const QString& design, QString* error) {
  // mitcad-print is the user's own: on Linux the temporary folder is
  // shared, and a folder or link someone else put there must not decide
  // what is emptied.
  const QString base = QDir(root).filePath(QStringLiteral("mitcad-print"));
  if (!QFileInfo::exists(base) &&
      !QDir().mkdir(base, QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner)) {
    *error = QObject::tr("The folder %1 could not be made.").arg(QDir::toNativeSeparators(base));
    return QString();
  }
  const QFileInfo baseInfo(base);
#ifdef _WIN32
  const bool own = true; // the temporary folder is the user's
#else
  const bool own = baseInfo.ownerId() == ::getuid();
#endif
  if (baseInfo.isSymLink() || !baseInfo.isDir() || !own) {
    *error = QObject::tr("%1 is not a folder of yours; the files are not written there.")
                 .arg(QDir::toNativeSeparators(base));
    return QString();
  }
  const QString folder = QDir(base).filePath(safeFileName(design));
  if (QFileInfo(folder).isSymLink()) {
    QFile::remove(folder);
  }
  // Emptied, not removed (mitcad#1162): on Windows a slicer still open
  // from the last send has the folder as its working folder, which keeps
  // the folder itself from being removed.
  QDir dir(folder);
  if (dir.exists()) {
    bool emptied = true;
    for (const QFileInfo& entry :
         dir.entryInfoList(QDir::AllEntries | QDir::Hidden | QDir::System | QDir::NoDotAndDotDot)) {
      const bool removed = entry.isDir() && !entry.isSymLink() ? QDir(entry.filePath()).removeRecursively()
                                                               : QFile::remove(entry.filePath());
      emptied = emptied && removed;
    }
    if (!emptied) {
      *error =
          QObject::tr("The files of the last send in %1 could not be removed.").arg(QDir::toNativeSeparators(folder));
      return QString();
    }
  }
  if (!QDir().mkpath(folder)) {
    *error = QObject::tr("The folder %1 could not be made.").arg(QDir::toNativeSeparators(folder));
    return QString();
  }
  return folder;
}

// ---------------------------------------------------------------------------
// The slicer's combo box, the panel's rows and Preferences

namespace {

const QString kBrowse = QStringLiteral("browse");
// The panel's Slicer choices besides the slicers (slicerChoice): none, and
// another program, chosen in a file dialog.
const QString kNoSlicer = QStringLiteral("none");
const QString& kOtherSlicer = kBrowse;

QVariant slicerData(const Slicer& slicer) {
  return QVariantMap{{QStringLiteral("name"), slicer.name},
                     {QStringLiteral("program"), slicer.program},
                     {QStringLiteral("arguments"), slicer.arguments},
                     {QStringLiteral("after"), slicer.argumentsAfter}};
}

Slicer slicerFromData(const QVariant& data) {
  const QVariantMap map = data.toMap();
  return Slicer{map.value(QStringLiteral("name")).toString(), map.value(QStringLiteral("program")).toString(),
                map.value(QStringLiteral("arguments")).toStringList(), map.value(QStringLiteral("after")).toStringList()};
}

QString itemLabel(const Slicer& slicer) {
  return QStringLiteral("%1 - %2").arg(slicer.name.isEmpty() ? QFileInfo(slicer.location()).fileName() : slicer.name,
                                        QDir::toNativeSeparators(slicer.location()));
}

} // namespace

QComboBox* slicerComboBox(const QVector<Slicer>& found, const Slicer& current, const QString& noneLabel,
                          QWidget* parent) {
  auto* combo = new QComboBox(parent);
  combo->addItem(noneLabel, slicerData(Slicer{}));
  QVector<Slicer> listed = found;
  if (current.isValid() && !listed.contains(current)) {
    listed.prepend(current);
  }
  for (const Slicer& slicer : std::as_const(listed)) {
    combo->addItem(itemLabel(slicer), slicerData(slicer));
    combo->setItemData(combo->count() - 1, slicer.describe(), Qt::ToolTipRole);
  }
  combo->addItem(QObject::tr("Other program…"), kBrowse);
  const int chosen = current.isValid() ? static_cast<int>(listed.indexOf(current)) + 1 : 0;
  combo->setCurrentIndex(chosen);
  combo->setProperty("previous", chosen);
  QObject::connect(combo, &QComboBox::activated, combo, [combo](int index) {
    if (combo->itemData(index).toString() != kBrowse) {
      combo->setProperty("previous", index);
      return;
    }
    // Another program: an AppImage, a slicer installed elsewhere, an
    // application bundle.
    const QString program = dialogOpenFileName(combo->window(), QObject::tr("Slicer Program"),
                                               dialogStartFolder(DialogGroup::Program, QString()));
    if (program.isEmpty()) {
      combo->setCurrentIndex(combo->property("previous").toInt());
      return;
    }
    rememberDialogFolder(DialogGroup::Program, program);
    const Slicer chosenSlicer = slicerForProgram(QFileInfo(program).completeBaseName(), program);
    combo->insertItem(index, itemLabel(chosenSlicer), slicerData(chosenSlicer));
    combo->setCurrentIndex(index);
    combo->setProperty("previous", index);
  });
  return combo;
}

Slicer slicerOf(const QComboBox* combo) {
  const QVariant data = combo->currentData();
  return data.toString() == kBrowse ? Slicer{} : slicerFromData(data);
}

QString slicerChoice(const Slicer& slicer) {
  if (!slicer.isValid()) {
    return kNoSlicer;
  }
  return QString::fromUtf8(QJsonDocument(QJsonObject{{QStringLiteral("name"), slicer.name},
                                                     {QStringLiteral("program"), slicer.program},
                                                     {QStringLiteral("arguments"),
                                                      QJsonArray::fromStringList(slicer.arguments)},
                                                     {QStringLiteral("after"),
                                                      QJsonArray::fromStringList(slicer.argumentsAfter)}})
                               .toJson(QJsonDocument::Compact));
}

Slicer slicerOfChoice(const QString& choice) {
  const QJsonObject object = QJsonDocument::fromJson(choice.toUtf8()).object();
  const auto strings = [&object](const char* key) {
    QStringList list;
    for (const QJsonValue& value : object.value(QLatin1String(key)).toArray()) {
      list << value.toString();
    }
    return list;
  };
  return Slicer{object.value(QStringLiteral("name")).toString(), object.value(QStringLiteral("program")).toString(),
                strings("arguments"), strings("after")};
}

QVector<QPair<QString, QString>> slicerChoices(const QVector<Slicer>& found, const Slicer& current,
                                               const Slicer& other) {
  QVector<QPair<QString, QString>> choices{{kNoSlicer, QObject::tr("Folder only")}};
  QVector<Slicer> listed;
  for (const Slicer& slicer : {current, other}) {
    if (slicer.isValid() && !found.contains(slicer) && !listed.contains(slicer)) {
      listed << slicer;
    }
  }
  listed += found;
  for (const Slicer& slicer : std::as_const(listed)) {
    choices.append({slicerChoice(slicer), itemLabel(slicer)});
  }
  choices.append({kOtherSlicer, QObject::tr("Other program…")});
  return choices;
}

QString otherSlicerChoice() { return kOtherSlicer; }

Selection printStartBodies(const Selection& selection, const QJsonArray& instances, bool* selected) {
  Selection bodies;
  const auto add = [&bodies](const QString& body, const QString& occurrence) {
    const SelectionItem item{SelectKind::Body, body, QString(), QString(), occurrence};
    if (!bodies.contains(item)) {
      bodies.append(item);
    }
  };
  for (const SelectionItem& item : selection) {
    if (item.kind == SelectKind::Body || item.kind == SelectKind::Face || item.kind == SelectKind::Edge ||
        item.kind == SelectKind::Vertex) {
      add(item.owner, item.occurrence);
    } else if (item.kind == SelectKind::Component) {
      for (const QJsonValue& value : instances) {
        const QJsonObject instance = value.toObject();
        const QString occurrence = placementOf(instance);
        if (instance.value(QStringLiteral("visible")).toBool() &&
            (occurrence == item.occurrence || occurrence.startsWith(item.occurrence + QLatin1Char('/')))) {
          add(instance.value(QStringLiteral("body")).toString(), occurrence);
        }
      }
    }
  }
  *selected = !bodies.isEmpty();
  if (bodies.isEmpty()) {
    for (const QJsonValue& value : instances) {
      const QJsonObject instance = value.toObject();
      if (instance.value(QStringLiteral("visible")).toBool()) {
        add(instance.value(QStringLiteral("body")).toString(), placementOf(instance));
      }
    }
  }
  return bodies;
}

SlicerPreferencesBox::SlicerPreferencesBox(QWidget* parent) : QGroupBox(tr("3D Print"), parent) {
  auto* form = new QFormLayout(this);
  const QVector<Slicer> found = findSlicers();
  const Slicer current = PrintSettings::load().slicer;
  m_slicers = slicerComboBox(found, current,
                             found.isEmpty() ? tr("Folder only")
                                             : tr("Automatic: %1").arg(found.first().name),
                             this);
  m_slicers->setToolTip(tr("The program 3D Print sends the files to"));
  // Its entries name programs with their paths: a long path neither widens
  // Preferences nor sets its minimum width (mitcad#690); each entry's
  // tooltip names its program whole.
  m_slicers->setSizeAdjustPolicy(QComboBox::AdjustToMinimumContentsLengthWithIcon);
  m_slicers->setMinimumContentsLength(32);
  // The same words as 3D Print's Open in (mitcad#655).
  form->addRow(tr("Open &in:"), m_slicers);
}

Slicer SlicerPreferencesBox::chosen() const { return slicerOf(m_slicers); }

} // namespace mitcad
