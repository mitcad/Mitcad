// SPDX-License-Identifier: MIT
// Insert › Component from Web without the browser (mitcad#1579): the
// part-number rule, the component's name, the format filter by name and by
// content, typed addresses, the sites kept in the settings, the downloads'
// temporary folders (this run's held, an earlier run's removed) and the
// browser's download reports (a repeated download told once each).
#include "UnitTests.hpp"
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
#include <QUrl>

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
  // 3MF files (mitcad#2156) as STL files.
  for (const char* name :
       {"a.step", "a.STP", "a.iges", "a.igs", "a.brep", "a.stl", "a.obj", "a.mitcad", "a.serp", "a.3mf", "a.3MF"}) {
    CHECK(checkWebFileName(text(name)).kind == WebFileKind::Component);
    CHECK(checkWebFileName(text(name)).note.isEmpty());
  }
  CHECK(checkWebFileName(text("parts.zip")).kind == WebFileKind::Zip);
  CHECK(checkWebFileName(text("parts.ZIP")).kind == WebFileKind::Zip);
  for (const char* name : {"a.f3d", "a.FCStd", "a.ipt", "a.iam", "a.ifc", "a.rvt", "a.dwg", "a.3dm"}) {
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
  // A 3MF file is a zip package (mitcad#2156).
  CHECK(checkWebFileContent(text("plate.3mf"), QByteArray("PK\x03\x04\x14\x00", 6)).kind == WebFileKind::Component);
  const mitcad::WebFileCheck notZip = checkWebFileContent(text("plate.3mf"), QByteArray("solid plate\n"));
  CHECK(notZip.kind == WebFileKind::Refused && notZip.note.contains(text("3MF")) &&
        notZip.note.contains(text("plate.3mf")));
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

// Sites › Add This Page… (mitcad#1844): the loaded page; on the start page
// (a data: address) the typed address, never the start page's own.
void siteToAddTests() {
  using mitcad::webSiteToAdd;
  using mitcad::WebSite;
  const QUrl start(text("data:text/html;charset=utf-8,%3Ch1%3EComponents%3C/h1%3E"));
  // A typed address, Enter not pressed: the row holds the typed address.
  CHECK((webSiteToAdd(start, text("Components from the web"), text("parts.example.com/catalog")) ==
         WebSite{text("parts.example.com"), text("https://parts.example.com/catalog")}));
  CHECK((webSiteToAdd(QUrl(), QString(), text("http://127.0.0.1:8000/")) ==
         WebSite{text("127.0.0.1"), text("http://127.0.0.1:8000/")}));
  CHECK((webSiteToAdd(QUrl(text("about:blank")), QString(), text("example.org")) ==
         WebSite{text("example.org"), text("https://example.org")}));
  // Nothing typed on the start page: nothing to add (the entry disabled).
  CHECK(webSiteToAdd(start, text("Components from the web"), QString()).url.isEmpty());
  CHECK(webSiteToAdd(start, QString(), text("   ")).url.isEmpty());
  CHECK(webSiteToAdd(start, QString(), text("about:blank")).url.isEmpty());
  // A loaded page: its address and title, whatever the field holds.
  CHECK((webSiteToAdd(QUrl(text("https://parts.example.com/catalog")), text(" Parts catalog "),
                      text("other.example")) ==
         WebSite{text("Parts catalog"), text("https://parts.example.com/catalog")}));
  CHECK((webSiteToAdd(QUrl(text("https://parts.example.com/catalog")), QString(), QString()) ==
         WebSite{text("parts.example.com"), text("https://parts.example.com/catalog")}));
}

void downloadFailureTests() {
  using mitcad::webDownloadFailure;
  // Why a download stopped, in words (mitcad#1716): the connection, the
  // file, the server, the user; never the browser's code.
  CHECK(webDownloadFailure(22) == text("the connection was lost"));
  CHECK(webDownloadFailure(20) == text("the connection was lost"));
  CHECK(webDownloadFailure(21) == text("the connection timed out"));
  CHECK(webDownloadFailure(1) == text("the file could not be written"));
  CHECK(webDownloadFailure(3) == text("the disk is full"));
  CHECK(webDownloadFailure(36) == text("the server refused it"));
  CHECK(webDownloadFailure(34) == text("the server refused it: the site may want you to sign in first"));
  CHECK(webDownloadFailure(38) == text("the server sent the file incomplete"));
  CHECK(webDownloadFailure(40) == text("it was canceled"));
  CHECK(webDownloadFailure(0) == text("it stopped before the end"));
  CHECK(webDownloadFailure(99) == text("it stopped before the end"));
  for (int code = 0; code <= 60; ++code) {
    const QString words = webDownloadFailure(code);
    bool digit = false;
    for (const QChar c : words) {
      digit = digit || c.isDigit();
    }
    CHECK(!words.isEmpty() && !digit && !words.endsWith(QLatin1Char('.')));
  }
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

// A finished download's file, as the browser writes it into the folder the
// component browser gave it.
QString writeDownload(mitcad::WebDownloadFolders& folders, const char* name) {
  const QString folder = folders.newDownloadFolder();
  const QString path = QDir(folder).filePath(text(name));
  QFile file(path);
  CHECK(file.open(QIODevice::WriteOnly) && file.write("ISO-10303-21;\nHEADER;\n") > 0);
  file.close();
  return path;
}

// The browser's download reports (mitcad#2281, mitcad#2289): the same file
// downloaded again and again, each finished download told once, never a
// report after its end or of a download this window never started, which
// had the import read a file that was gone ("... is empty.").
void repeatedDownloadTests(const QString& root) {
  using mitcad::WebDownloadReports;
  using mitcad::WebDownloadState;
  using Ignored = WebDownloadReports::Ignored;
  qputenv("MITCAD_WEB_DOWNLOADS_DIR", root.toUtf8());
  mitcad::WebDownloadFolders folders;
  WebDownloadReports reports;

  // The browser reports a download before it asks where it goes: not
  // told, it has no file yet.
  CHECK(reports.report(1, WebDownloadState::InProgress, QString()).ignored == Ignored::NotStarted);
  CHECK(reports.pathOf(1).isEmpty());
  const QString first = writeDownload(folders, "part.step");
  reports.started(1, first);
  CHECK(reports.pathOf(1) == first);
  CHECK(reports.report(1, WebDownloadState::InProgress, first).told());
  const WebDownloadReports::Verdict done = reports.report(1, WebDownloadState::Complete, first);
  CHECK(done.told() && done.path == first);
  CHECK(reports.pathOf(1).isEmpty());
  // The import read it; its folder goes.
  folders.remove(QFileInfo(first).absolutePath());
  CHECK(!QFileInfo::exists(first));
  // The browser finds its file gone and reports the finished download
  // again: not told, so nothing reads the file that is gone.
  CHECK(reports.report(1, WebDownloadState::Complete, first).ignored == Ignored::AfterEnd);
  CHECK(reports.report(1, WebDownloadState::InProgress, first).ignored == Ignored::AfterEnd);

  // The same file downloaded again: a download of its own, in a folder of
  // its own, told once with its own file, which is there.
  CHECK(reports.report(2, WebDownloadState::InProgress, QString()).ignored == Ignored::NotStarted);
  const QString again = writeDownload(folders, "part.step");
  CHECK(again != first && QFileInfo::exists(again));
  reports.started(2, again);
  const WebDownloadReports::Verdict second = reports.report(2, WebDownloadState::Complete, again);
  CHECK(second.told() && second.path == again && QFileInfo::exists(second.path));
  folders.remove(QFileInfo(again).absolutePath());
  CHECK(reports.report(2, WebDownloadState::Complete, again).ignored == Ignored::AfterEnd);
  // And a third time while the second's report comes again in between.
  const QString third = writeDownload(folders, "part.step");
  reports.started(3, third);
  CHECK(reports.report(2, WebDownloadState::Complete, again).ignored == Ignored::AfterEnd);
  CHECK(reports.report(3, WebDownloadState::Complete, third).path == third);
  folders.remove(QFileInfo(third).absolutePath());

  // A finished download this window never started (one of an earlier run
  // that the browser's history brings back, its file long gone): not told.
  const QString earlier = QDir(root).filePath(text("run-1-1/d1/part.step"));
  CHECK(reports.report(40, WebDownloadState::Complete, earlier).ignored == Ignored::NotStarted);
  CHECK(reports.report(41, WebDownloadState::Failed, earlier).ignored == Ignored::NotStarted);

  // Interrupted: told once (its folder goes), a report again is not; when
  // the browser resumes it by itself it asks again where it goes, and runs
  // again. An item interrupted before it wrote anything reports no path:
  // the one it was given names it (mitcad#1716).
  const QString broken = writeDownload(folders, "broken.step");
  reports.started(5, broken);
  const WebDownloadReports::Verdict failed = reports.report(5, WebDownloadState::Failed, QString());
  CHECK(failed.told() && failed.path == broken);
  folders.remove(QFileInfo(broken).absolutePath());
  CHECK(reports.report(5, WebDownloadState::Failed, broken).ignored == Ignored::AfterInterruption);
  CHECK(reports.pathOf(5).isEmpty());
  const QString resumed = writeDownload(folders, "broken.step");
  reports.started(5, resumed);
  CHECK(reports.report(5, WebDownloadState::InProgress, resumed).told());
  CHECK(reports.report(5, WebDownloadState::Complete, resumed).path == resumed);
  CHECK(reports.report(5, WebDownloadState::Complete, resumed).ignored == Ignored::AfterEnd);
  folders.remove(QFileInfo(resumed).absolutePath());

  // Canceled is an end too; a refused download (no path) is never told.
  const QString canceled = writeDownload(folders, "big.step");
  reports.started(6, canceled);
  CHECK(reports.report(6, WebDownloadState::Canceled, canceled).told());
  CHECK(reports.report(6, WebDownloadState::Canceled, canceled).ignored == Ignored::AfterEnd);
  folders.remove(QFileInfo(canceled).absolutePath());
  reports.started(7, QString());
  CHECK(reports.refused(7) && !reports.refused(6));
  CHECK(reports.report(7, WebDownloadState::InProgress, QString()).ignored == Ignored::Refused);
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
  siteToAddTests();
  downloadFailureTests();
  sitesTests();
  foldersTests(QDir(directory.path()).filePath(QStringLiteral("downloads")));
  repeatedDownloadTests(QDir(directory.path()).filePath(QStringLiteral("repeated")));
  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}
MITCAD_UNIT_TEST(componentWebFilesTests);
