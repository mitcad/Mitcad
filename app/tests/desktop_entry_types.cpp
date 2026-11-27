// SPDX-License-Identifier: MIT
// The packaging's file types against fileKind() (mitcad#2527), read from
// the source tree:
// - the Linux desktop entry, as MitCad started as the AppImage installs it
//   (desktopEntryFor), runs MitCad with the file (%f, also over an earlier
//   MitCad's entry without it), and its MimeType lists a type for every
//   extension fileKind() takes but .sc (Scala source's glob): the system's
//   type where shared-mime-info has one (the table below), else one that
//   packaging/linux/mitcad-mime.xml declares with that glob; no other
//   types;
// - the MIME package declares only types the entry lists, none of the
//   system's again, globs only of extensions fileKind() takes, and no
//   *.sc;
// - the Windows installer's Open with list (packaging/windows/filetype.nsh)
//   is every extension fileKind() takes but .mitcad, which has its own
//   file type.
#include "UnitTests.hpp"
#include "files/FileFormats.hpp"
#include "framework/DesktopEntry.hpp"

#include <cstdio>
#include <map>
#include <set>

#include <QFile>
#include <QRegularExpression>
#include <QString>
#include <QStringList>
#include <QXmlStreamReader>

#ifndef MITCAD_PACKAGING_DIR
// Set by tests/unit/desktop_entry_types.cmake; without it no packaging
// file is found and every check of a file's text fails by name.
#define MITCAD_PACKAGING_DIR "MITCAD_PACKAGING_DIR-not-set"
#endif

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line, const QString& detail = QString()) {
  if (!condition) {
    std::fprintf(stderr, "desktop_entry_types.cpp:%d: check failed: %s %s\n", line, expression, qPrintable(detail));
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)
#define CHECK_FOR(expr, detail) check((expr), #expr, __LINE__, (detail))

using Types = std::map<QString, std::set<QString>>; // type: its extensions, lower case

// The types shared-mime-info (2.4) defines for extensions MitCad opens;
// mitcad-mime.xml does not declare them again.
Types systemTypes() {
  return {
      {QStringLiteral("model/iges"), {QStringLiteral("iges"), QStringLiteral("igs")}},
      {QStringLiteral("model/stl"), {QStringLiteral("stl")}},
      {QStringLiteral("model/obj"), {QStringLiteral("obj")}},
      {QStringLiteral("model/3mf"), {QStringLiteral("3mf")}},
      {QStringLiteral("model/gltf-binary"), {QStringLiteral("glb")}},
      {QStringLiteral("model/gltf+json"), {QStringLiteral("gltf")}},
      {QStringLiteral("image/vnd.dxf"), {QStringLiteral("dxf")}},
      {QStringLiteral("image/vnd.dwg"), {QStringLiteral("dwg")}},
      {QStringLiteral("image/svg+xml"), {QStringLiteral("svg")}},
  };
}

QString packagingFile(const char* relative) {
  return QStringLiteral(MITCAD_PACKAGING_DIR "/") + QLatin1String(relative);
}

QString readText(const QString& path) {
  QFile file(path);
  if (!file.open(QIODevice::ReadOnly)) {
    return QString();
  }
  return QString::fromUtf8(file.readAll());
}

// A key's value in a desktop entry's text (the first line with it).
QString valueOf(const QString& entry, const QString& key) {
  for (QString line : entry.split(QLatin1Char('\n'))) {
    if (line.endsWith(QLatin1Char('\r'))) {
      line.chop(1);
    }
    if (line.startsWith(key + QLatin1Char('='))) {
      return line.mid(key.size() + 1);
    }
  }
  return QString();
}

// The MIME package's types with their globs' extensions.
Types packageTypes(const QString& xml) {
  Types types;
  QXmlStreamReader reader(xml);
  QString type;
  while (!reader.atEnd()) {
    reader.readNext();
    if (reader.isStartElement() && reader.name() == QLatin1String("mime-type")) {
      type = reader.attributes().value(QLatin1String("type")).toString();
      types[type];
    } else if (reader.isStartElement() && reader.name() == QLatin1String("glob")) {
      const QString pattern = reader.attributes().value(QLatin1String("pattern")).toString();
      CHECK_FOR(pattern.startsWith(QStringLiteral("*.")), pattern);
      types[type].insert(pattern.mid(2).toLower());
    } else if (reader.isEndElement() && reader.name() == QLatin1String("mime-type")) {
      type.clear();
    }
  }
  CHECK_FOR(!reader.hasError(), reader.errorString());
  return types;
}

void linuxTests() {
  const QString bundled = readText(packagingFile("linux/mitcad.desktop"));
  CHECK_FOR(!bundled.isEmpty(), packagingFile("linux/mitcad.desktop"));
  CHECK(valueOf(bundled, QStringLiteral("Exec")) == QStringLiteral("mitcad %f"));

  // As the AppImage installs it: the file after the AppImage, also over
  // the entry an earlier MitCad wrote (Exec only the AppImage).
  const QString program = QStringLiteral("/apps/MitCad.AppImage");
  const QString installed = mitcad::desktopEntryFor(bundled, program, QStringLiteral("1.2.3"), QString());
  CHECK_FOR(valueOf(installed, QStringLiteral("Exec")) == QStringLiteral("\"/apps/MitCad.AppImage\" %f"),
            valueOf(installed, QStringLiteral("Exec")));
  const QString updated = mitcad::desktopEntryFor(
      bundled, program, QStringLiteral("1.2.3"),
      QStringLiteral("[Desktop Entry]\nName=MitCad\nExec=\"/apps/MitCad.AppImage\"\nIcon=mitcad\n"));
  CHECK_FOR(valueOf(updated, QStringLiteral("Exec")) == QStringLiteral("\"/apps/MitCad.AppImage\" %f"),
            valueOf(updated, QStringLiteral("Exec")));
  CHECK(valueOf(updated, QStringLiteral("MimeType")) == valueOf(bundled, QStringLiteral("MimeType")));

  const QString mimeType = valueOf(installed, QStringLiteral("MimeType"));
  CHECK(mimeType.endsWith(QLatin1Char(';')));
  const QStringList listed = mimeType.split(QLatin1Char(';'), Qt::SkipEmptyParts);
  CHECK(listed.contains(QStringLiteral("application/x-mitcad")));

  const QString xml = readText(packagingFile("linux/mitcad-mime.xml"));
  CHECK_FOR(!xml.isEmpty(), packagingFile("linux/mitcad-mime.xml"));
  const Types declared = packageTypes(xml);
  const Types system = systemTypes();
  CHECK(declared.count(QStringLiteral("application/x-mitcad")) == 1U);

  // Every listed type is the system's or declared; the package declares
  // only listed types, none of the system's, and no glob MitCad does not
  // open, nor .sc.
  for (const QString& type : listed) {
    CHECK_FOR(system.count(type) + declared.count(type) == 1U, type);
    CHECK_FOR(listed.count(type) == 1, type);
  }
  for (const auto& [type, suffixes] : declared) {
    CHECK_FOR(listed.contains(type), type);
    CHECK_FOR(!suffixes.empty(), type);
    for (const QString& suffix : suffixes) {
      CHECK_FOR(suffix != QLatin1String("sc"), type);
      CHECK_FOR(mitcad::fileKind(QStringLiteral("a.") + suffix) != mitcad::FileKind::None, type + QLatin1Char(' ') + suffix);
    }
  }

  // Every extension MitCad opens but .sc has a listed type.
  for (const QString& suffix : mitcad::fileKindSuffixes()) {
    if (suffix == QLatin1String("sc")) {
      continue;
    }
    bool found = false;
    for (const QString& type : listed) {
      const auto own = declared.find(type);
      const auto systems = system.find(type);
      found = found || (own != declared.end() && own->second.count(suffix) > 0) ||
              (systems != system.end() && systems->second.count(suffix) > 0);
    }
    CHECK_FOR(found, suffix);
  }
}

void windowsTests() {
  const QString nsh = readText(packagingFile("windows/filetype.nsh"));
  CHECK_FOR(!nsh.isEmpty(), packagingFile("windows/filetype.nsh"));
  // The extensions of MitcadImportExtensions.
  std::set<QString> listed;
  const QRegularExpression line(QStringLiteral("^\\s*!insertmacro \\$\\{OPERATION\\} (\\S+)\\s*$"),
                                QRegularExpression::MultilineOption);
  for (auto it = line.globalMatch(nsh); it.hasNext();) {
    const QString suffix = it.next().captured(1);
    CHECK_FOR(listed.insert(suffix).second, suffix);
  }
  std::set<QString> expected;
  for (const QString& suffix : mitcad::fileKindSuffixes()) {
    if (suffix != QLatin1String("mitcad")) {
      expected.insert(suffix);
    }
  }
  for (const QString& suffix : expected) {
    CHECK_FOR(listed.count(suffix) == 1U, suffix);
  }
  for (const QString& suffix : listed) {
    CHECK_FOR(expected.count(suffix) == 1U, suffix);
  }
  // Both the install and the uninstall run the list.
  CHECK(nsh.contains(QStringLiteral("!insertmacro MitcadImportExtensions MitcadOpenWithRegister")));
  CHECK(nsh.contains(QStringLiteral("!insertmacro MitcadImportExtensions MitcadOpenWithUnregister")));
}

} // namespace

int desktopEntryTypesTests() {
  failures = 0;
  linuxTests();
  windowsTests();
  return failures;
}
MITCAD_UNIT_TEST(desktopEntryTypesTests);
