// SPDX-License-Identifier: MIT
#include "Print3d.hpp"

#include <algorithm>
#include <utility>

#include <QComboBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QDirIterator>
#include <QFile>
#include <QFileDialog>
#include <QFileInfo>
#include <QFormLayout>
#include <QJsonObject>
#include <QJsonValue>
#include <QLabel>
#include <QProcess>
#include <QPushButton>
#include <QSet>
#include <QSettings>
#include <QStandardPaths>
#include <QTextStream>
#include <QVBoxLayout>
#include <QVariantMap>
#include <QtLogging>

#include "../framework/Numbers.hpp"

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

Slicer slicerForProgram(const QString& name, const QString& program) {
  if (program.endsWith(QStringLiteral(".app"), Qt::CaseInsensitive) && QFileInfo(program).isDir()) {
    return Slicer{name, kOpen, {QStringLiteral("-a"), program}, {}};
  }
  return Slicer{name, QDir::toNativeSeparators(program), {}, {}};
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
      if (!flatpak.isEmpty() && QFileInfo(dir + QLatin1Char('/') + id).isDir()) {
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
  print.format = format == QStringLiteral("3mf") ? format : QStringLiteral("stl");
  const QString refinement = settings.value(QStringLiteral("refinement"), print.refinement).toString();
  if (QStringList{QStringLiteral("low"), QStringLiteral("medium"), QStringLiteral("high"), QStringLiteral("custom")}
          .contains(refinement)) {
    print.refinement = refinement;
  }
  print.deviation = std::clamp(settings.value(QStringLiteral("deviation"), print.deviation).toDouble(), 0.001, 10.0);
  print.angle = std::clamp(settings.value(QStringLiteral("angle"), print.angle).toDouble(), 0.5, 90.0);
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

QVector<PrintPiece> printPieces(const QStringList& bodies, const QStringList& names, const QJsonArray& instances) {
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
    for (const QJsonObject& instance : placed) {
      QStringList path;
      for (const QJsonValue& occurrence : instance.value(QStringLiteral("path")).toArray()) {
        path << occurrence.toString();
      }
      pieces.append({bodies[i], path.join(QLatin1Char('/')),
                     QStringLiteral("%1 (%2)").arg(names[i], instance.value(QStringLiteral("occurrence")).toString())});
    }
  }
  return pieces;
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
  QDir dir(folder);
  if (dir.exists() && !dir.removeRecursively()) {
    *error = QObject::tr("The files of the last send in %1 could not be removed.").arg(QDir::toNativeSeparators(folder));
    return QString();
  }
  if (!QDir().mkpath(folder)) {
    *error = QObject::tr("The folder %1 could not be made.").arg(QDir::toNativeSeparators(folder));
    return QString();
  }
  return folder;
}

// ---------------------------------------------------------------------------
// The slicer's combo box, the dialog and Preferences

namespace {

const QString kBrowse = QStringLiteral("browse");

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
  combo->addItem(QObject::tr("Other program..."), kBrowse);
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
    const QString program = QFileDialog::getOpenFileName(combo->window(), QObject::tr("Slicer Program"));
    if (program.isEmpty()) {
      combo->setCurrentIndex(combo->property("previous").toInt());
      return;
    }
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

std::optional<PrintChoice> askPrint(QWidget* parent, const QString& bodies, const QString& leftOut,
                                    const QVector<Slicer>& found) {
  const PrintSettings settings = PrintSettings::load();
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("3D Print"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* form = new QFormLayout;
  layout->addLayout(form);
  auto* what = new QLabel(bodies);
  what->setWordWrap(true);
  form->addRow(QObject::tr("Bodies:"), what);
  if (!leftOut.isEmpty()) {
    auto* left = new QLabel(leftOut);
    left->setWordWrap(true);
    form->addRow(QObject::tr("Left out:"), left);
  }
  auto* format = new QComboBox;
  format->addItem(QObject::tr("STL, one file per body"), QStringLiteral("stl"));
  format->addItem(QObject::tr("3MF, one object with parts"), QStringLiteral("3mf"));
  format->setCurrentIndex(format->findData(settings.format));
  format->setToolTip(QObject::tr("3MF keeps the bodies together as the parts of one object in their places; "
                                 "STL gives the slicer a file per body"));
  form->addRow(QObject::tr("&Format:"), format);
  auto* refinement = new QComboBox;
  refinement->addItem(QObject::tr("Low"), QStringLiteral("low"));
  refinement->addItem(QObject::tr("Medium"), QStringLiteral("medium"));
  refinement->addItem(QObject::tr("High"), QStringLiteral("high"));
  refinement->addItem(QObject::tr("Custom"), QStringLiteral("custom"));
  refinement->setCurrentIndex(refinement->findData(settings.refinement));
  form->addRow(QObject::tr("&Refinement:"), refinement);
  auto* deviation = new DecimalSpinBox;
  deviation->setDecimals(3);
  deviation->setRange(0.001, 10.0);
  deviation->setSingleStep(0.005);
  deviation->setSuffix(QStringLiteral(" mm"));
  deviation->setValue(settings.deviation);
  deviation->setToolTip(QObject::tr("The largest distance between the triangles and the surface"));
  auto* angle = new DecimalSpinBox;
  angle->setDecimals(1);
  angle->setRange(0.5, 90.0);
  angle->setSuffix(QStringLiteral(" deg"));
  angle->setValue(settings.angle);
  angle->setToolTip(QObject::tr("The largest angle between neighbouring triangles"));
  form->addRow(QObject::tr("&Deviation:"), deviation);
  form->addRow(QObject::tr("&Angle:"), angle);
  const auto showCustom = [&] {
    const bool custom = refinement->currentData().toString() == QStringLiteral("custom");
    form->setRowVisible(deviation, custom);
    form->setRowVisible(angle, custom);
  };
  showCustom();
  QObject::connect(refinement, &QComboBox::currentIndexChanged, &dialog, showCustom);
  const Slicer current = settings.slicer.isValid() ? settings.slicer : found.value(0);
  QComboBox* slicer = slicerComboBox(found, current, QObject::tr("None: open the files' folder"), &dialog);
  form->addRow(QObject::tr("&Slicer:"), slicer);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  buttons->button(QDialogButtonBox::Ok)->setText(QObject::tr("Send"));
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  layout->addWidget(buttons);
  format->setFocus();
  QStringList slicers;
  for (int i = 0; i < slicer->count(); ++i) {
    slicers << slicer->itemText(i);
  }
  qDebug().noquote() << QStringLiteral("3D Print dialog: %1; format %2, refinement %3, slicer %4; slicers %5")
                            .arg(bodies, settings.format, settings.refinement, slicerOf(slicer).describe(),
                                 slicers.join(QStringLiteral(" | ")));
  if (dialog.exec() != QDialog::Accepted) {
    return std::nullopt;
  }
  PrintChoice choice;
  choice.format = format->currentData().toString();
  choice.refinement = refinement->currentData().toString();
  choice.deviation = deviation->value();
  choice.angle = angle->value();
  choice.slicer = slicerOf(slicer);
  // Remembered; "None" this time keeps the slicer of the settings.
  PrintSettings chosen = settings;
  chosen.format = choice.format;
  chosen.refinement = choice.refinement;
  chosen.deviation = choice.deviation;
  chosen.angle = choice.angle;
  if (choice.slicer.isValid()) {
    chosen.slicer = choice.slicer;
  }
  chosen.save();
  return choice;
}

SlicerPreferencesBox::SlicerPreferencesBox(QWidget* parent) : QGroupBox(tr("3D Print"), parent) {
  auto* form = new QFormLayout(this);
  const QVector<Slicer> found = findSlicers();
  const Slicer current = PrintSettings::load().slicer;
  m_slicers = slicerComboBox(found, current,
                             found.isEmpty() ? tr("None found: open the files' folder")
                                             : tr("Automatic: %1").arg(found.first().name),
                             this);
  m_slicers->setToolTip(tr("The program 3D Print sends the files to"));
  form->addRow(tr("Sli&cer:"), m_slicers);
}

Slicer SlicerPreferencesBox::chosen() const { return slicerOf(m_slicers); }

} // namespace mitcad
