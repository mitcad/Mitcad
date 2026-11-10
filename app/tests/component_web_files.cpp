// SPDX-License-Identifier: MIT
// Insert › Component from Web without the browser (mitcad#1579): the
// part-number rule, the component's name, the format filter by name and by
// content, typed addresses, the sites kept in the settings, and the
// downloads' temporary folders (this run's held, an earlier run's removed).
#include "webbrowser/ComponentWebFiles.hpp"

#include <cstdio>

#include <QByteArray>
#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QSettings>
#include <QString>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "component_web_files.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

QString text(const char* value) { return QString::fromUtf8(value); }

void partNumberTests() {
  using mitcad::webPartNumber;
  // The ticket's file: the first part up to the underscore.
  CHECK(webPartNumber(text("6494K13_NO THREADS_Bearing")) == text("6494K13"));
  // Up to a space as well, whichever comes first.
  CHECK(webPartNumber(text("6494K13 Bearing_x")) == text("6494K13"));
  CHECK(webPartNumber(text("91251A540_Screw")) == text("91251A540"));
  // The whole name when it has no separator.
  CHECK(webPartNumber(text("6494K13")) == text("6494K13"));
  CHECK(webPartNumber(text("12345")) == text("12345"));
  // Leading white space does not count.
  CHECK(webPartNumber(text("  6494K13_x")) == text("6494K13"));
  // No digit: a word, not a part number.
  CHECK(webPartNumber(text("Bearing_6494K13")).isEmpty());
  CHECK(webPartNumber(text("Bracket")).isEmpty());
  // Anything but letters and digits in the first part.
  CHECK(webPartNumber(text("M6-20_bolt")).isEmpty());
  CHECK(webPartNumber(text("6494.K13_x")).isEmpty());
  CHECK(webPartNumber(text("(2)_part")).isEmpty());
  CHECK(webPartNumber(QString()).isEmpty());
  CHECK(webPartNumber(text("_6494K13")).isEmpty());
}

void nameTests() {
  using mitcad::webComponentName;
  CHECK(webComponentName(text("6494K13_NO THREADS_Bearing.step")) == text("6494K13_NO THREADS_Bearing"));
  CHECK(webComponentName(text("/tmp/d1/bracket.v2.stp")) == text("bracket.v2"));
  CHECK(webComponentName(text("plate")) == text("plate"));
  CHECK(webComponentName(text(".hidden")) == text(".hidden"));
}

void nameFilterTests() {
  using mitcad::WebFileKind;
  using mitcad::checkWebFileName;
  for (const char* name : {"a.step", "a.STP", "a.iges", "a.igs", "a.brep", "a.stl", "a.obj", "a.mitcad"}) {
    CHECK(checkWebFileName(text(name)).kind == WebFileKind::Component);
    CHECK(checkWebFileName(text(name)).note.isEmpty());
  }
  CHECK(checkWebFileName(text("parts.zip")).kind == WebFileKind::Zip);
  CHECK(checkWebFileName(text("parts.ZIP")).kind == WebFileKind::Zip);
  for (const char* name : {"a.f3d", "a.FCStd", "a.ipt", "a.iam", "a.ifc", "a.rvt", "a.dwg"}) {
    CHECK(checkWebFileName(text(name)).kind == WebFileKind::Document);
  }
  // Refused, each with a note that names its type.
  const mitcad::WebFileCheck pdf = checkWebFileName(text("datasheet.pdf"));
  CHECK(pdf.kind == WebFileKind::Refused);
  CHECK(pdf.note.contains(text("PDF")) && pdf.note.contains(text(".pdf")));
  const mitcad::WebFileCheck exe = checkWebFileName(text("setup.exe"));
  CHECK(exe.kind == WebFileKind::Refused && exe.note.contains(text(".exe")));
  const mitcad::WebFileCheck dxf = checkWebFileName(text("outline.dxf"));
  CHECK(dxf.kind == WebFileKind::Refused && dxf.note.contains(text(".dxf")));
  const mitcad::WebFileCheck printed = checkWebFileName(text("plate.3mf"));
  CHECK(printed.kind == WebFileKind::Refused && printed.note.contains(text(".3mf")));
  const mitcad::WebFileCheck bare = checkWebFileName(text("README"));
  CHECK(bare.kind == WebFileKind::Refused && !bare.note.isEmpty());
}

void contentFilterTests() {
  using mitcad::WebFileKind;
  using mitcad::checkWebFileContent;
  const QByteArray step("ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('part'),'2;1');\n");
  CHECK(checkWebFileContent(text("part.step"), step).kind == WebFileKind::Component);
  // A byte order mark and white space before the header.
  CHECK(checkWebFileContent(text("part.stp"), QByteArray("\xEF\xBB\xBF\r\n ") + step).kind ==
        WebFileKind::Component);
  // A site's sign-in page sent under the file's name.
  const QByteArray page("<!DOCTYPE html>\n<html><head><title>Sign in</title></head></html>");
  const mitcad::WebFileCheck signIn = checkWebFileContent(text("part.step"), page);
  CHECK(signIn.kind == WebFileKind::Refused && signIn.note.contains(text("part.step")));
  CHECK(checkWebFileContent(text("part.stl"), QByteArray("\n  <html lang=\"en\">")).kind == WebFileKind::Refused);
  // Not a STEP file whatever its name.
  CHECK(checkWebFileContent(text("part.step"), QByteArray("solid part\n")).kind == WebFileKind::Refused);
  CHECK(checkWebFileContent(text("part.stl"), QByteArray("%PDF-1.7\n")).kind == WebFileKind::Refused);
  CHECK(checkWebFileContent(text("part.stl"), QByteArray()).kind == WebFileKind::Refused);
  CHECK(checkWebFileContent(text("part.stl"), QByteArray("solid part\nfacet normal 0 0 1\n")).kind ==
        WebFileKind::Component);
  // A zip by its signature (an empty one too).
  CHECK(checkWebFileContent(text("parts.zip"), QByteArray("PK\x03\x04\x14\x00", 6)).kind == WebFileKind::Zip);
  CHECK(checkWebFileContent(text("parts.zip"), QByteArray("PK\x05\x06\x00\x00", 6)).kind == WebFileKind::Zip);
  CHECK(checkWebFileContent(text("parts.zip"), page).kind == WebFileKind::Refused);
  CHECK(checkWebFileContent(text("parts.zip"), QByteArray("not a zip")).kind == WebFileKind::Refused);
  // The name's refusal comes first.
  CHECK(checkWebFileContent(text("datasheet.pdf"), QByteArray("%PDF-1.7\n")).note ==
        mitcad::checkWebFileName(text("datasheet.pdf")).note);
}

void addressTests() {
  using mitcad::normalWebAddress;
  CHECK(normalWebAddress(text("  example.com/parts ")) == text("https://example.com/parts"));
  CHECK(normalWebAddress(text("http://127.0.0.1:8000/index.html")) == text("http://127.0.0.1:8000/index.html"));
  CHECK(normalWebAddress(text("https://example.com")) == text("https://example.com"));
  CHECK(normalWebAddress(text("about:blank")) == text("about:blank"));
  CHECK(normalWebAddress(text("   ")).isEmpty());
}

void sitesTests() {
  using mitcad::WebSite;
  // None are bundled.
  CHECK(mitcad::loadWebSites().isEmpty());
  const QList<WebSite> sites{{text("Supplier"), text("https://supplier.example/catalog")},
                             {text("Own server"), text("http://192.168.1.10/models")}};
  mitcad::saveWebSites(sites);
  CHECK(mitcad::loadWebSites() == sites);
  // A site without a name is shown by its address; one without an
  // address is left out.
  QSettings().setValue(text("componentBrowser/sites"),
                       QByteArray(R"([{"name":"","url":"https://a.example"},{"name":"Empty","url":""}])"));
  const QList<WebSite> read = mitcad::loadWebSites();
  CHECK(read.size() == 1 && read.first().name == text("https://a.example"));
  mitcad::saveWebSites({});
  CHECK(mitcad::loadWebSites().isEmpty());
}

void foldersTests(const QString& root) {
  qputenv("MITCAD_WEB_DOWNLOADS_DIR", root.toUtf8());
  CHECK(QDir::cleanPath(mitcad::WebDownloadFolders::root()) == QDir::cleanPath(root));
  // An earlier run's folder that no running MitCad holds (no lock at all,
  // as a run killed before it locked).
  const QString leftover = QDir(root).filePath(text("run-1-1"));
  CHECK(QDir().mkpath(QDir(leftover).filePath(text("d1"))));
  QFile file(QDir(leftover).filePath(text("d1/part.step")));
  CHECK(file.open(QIODevice::WriteOnly) && file.write("ISO-10303-21;") > 0);
  file.close();
  QString run;
  QString download;
  {
    mitcad::WebDownloadFolders folders;
    run = folders.runFolder();
    CHECK(!run.isEmpty() && QFileInfo(run).isDir() && run.startsWith(QDir::cleanPath(root)));
    download = folders.newDownloadFolder();
    const QString second = folders.newDownloadFolder();
    CHECK(QFileInfo(download).isDir() && QFileInfo(second).isDir() && download != second);
    CHECK(download.startsWith(run + QLatin1Char('/')));
    // This run's folder is held: only the earlier run's goes.
    CHECK(mitcad::WebDownloadFolders::removeLeftovers() == 1);
    CHECK(!QFileInfo::exists(leftover));
    CHECK(QFileInfo(run).isDir());
    // A download's folder goes once its import has read it; nothing outside
    // this run's folder is removed.
    folders.remove(second);
    CHECK(!QFileInfo::exists(second));
    folders.remove(root);
    CHECK(QFileInfo(root).isDir());
  }
  // The run's folder ends with the run.
  CHECK(!QFileInfo::exists(run) && !QFileInfo::exists(download));
  qunsetenv("MITCAD_WEB_DOWNLOADS_DIR");
}

} // namespace

int componentWebFilesTests() {
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QSettings::Format format = QSettings::defaultFormat();
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
  QCoreApplication::setApplicationName(QStringLiteral("component_web_files"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
  partNumberTests();
  nameTests();
  nameFilterTests();
  contentFilterTests();
  addressTests();
  sitesTests();
  foldersTests(QDir(directory.path()).filePath(QStringLiteral("downloads")));
  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}
