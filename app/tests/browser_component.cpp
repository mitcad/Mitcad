// SPDX-License-Identifier: MIT
// The web browser component without the network or the bridge (mitcad#1579):
// a component's component.json read and refused, the one in use found by
// current.json or a build's folder (MITCAD_BROWSER_COMPONENT_DIR), made the
// one in use, older ones removed (never the one in use or the one a run
// loaded), the update manifest's offers read, an unpacked package checked
// against the offer and this MitCad's interface, and the messages.
#include "webbrowser/BrowserComponent.hpp"
#include "webbrowser/ComponentBrowserHost.hpp"

#include <cstdio>

#include <QByteArray>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QString>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "browser_component.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

QString text(const char* value) { return QString::fromUtf8(value); }

void writeFile(const QString& path, const QByteArray& content) {
  QDir().mkpath(QFileInfo(path).absolutePath());
  QFile file(path);
  CHECK(file.open(QIODevice::WriteOnly));
  file.write(content);
}

// A component's folder as a package unpacks it.
void makeComponent(const QString& folder, int abi, const QString& version) {
  const QJsonObject json{{QStringLiteral("abi"), abi},
                         {QStringLiteral("version"), version},
                         {QStringLiteral("cef"), QStringLiteral("156.0.27+ga10dae3+chromium-156.0.8078.88")},
                         {QStringLiteral("platform"), QStringLiteral("linux-x64")},
                         {QStringLiteral("module"), QStringLiteral("mitcad-webbrowser.so")},
                         {QStringLiteral("executables"), QJsonArray{QStringLiteral("mitcad-webbrowser-helper")}}};
  writeFile(QDir(folder).filePath(QStringLiteral("component.json")), QJsonDocument(json).toJson());
  writeFile(QDir(folder).filePath(QStringLiteral("mitcad-webbrowser.so")), QByteArray("module"));
}

void readTests(const QString& root) {
  QString problem;
  CHECK(!mitcad::readBrowserComponent(QDir(root).filePath(text("nothing")), &problem));
  CHECK(!problem.isEmpty());
  const QString folder = QDir(root).filePath(text("read"));
  makeComponent(folder, 2, text("2026.1110.900"));
  const std::optional<mitcad::BrowserComponentInfo> info = mitcad::readBrowserComponent(folder, &problem);
  CHECK(info && info->abi == 2 && info->version == text("2026.1110.900") && info->module == text("mitcad-webbrowser.so"));
  CHECK(info && info->executables == QStringList{text("mitcad-webbrowser-helper")});
  // A module's name that leaves the folder, no interface: not a component.
  writeFile(QDir(folder).filePath(text("component.json")),
            QByteArray(R"({"abi": 2, "version": "1.0.0", "module": "../evil.so"})"));
  CHECK(!mitcad::readBrowserComponent(folder, &problem));
  writeFile(QDir(folder).filePath(text("component.json")),
            QByteArray(R"({"version": "1.0.0", "module": "m.so"})"));
  CHECK(!mitcad::readBrowserComponent(folder, &problem));
}

void installedTests(const QString& root) {
  QString problem;
  // Nothing installed.
  CHECK(!mitcad::installedBrowserComponent(root, &problem));
  makeComponent(QDir(root).filePath(text("2026.1110.900")), 2, text("2026.1110.900"));
  makeComponent(QDir(root).filePath(text("2026.1112.1530")), 2, text("2026.1112.1530"));
  QString error;
  CHECK(mitcad::activateBrowserComponent(root, text("2026.1110.900"), &error));
  std::optional<mitcad::BrowserComponentInfo> info = mitcad::installedBrowserComponent(root, &problem);
  CHECK(info && info->version == text("2026.1110.900") && !info->fromBuild);
  CHECK(mitcad::activateBrowserComponent(root, text("2026.1112.1530"), &error));
  info = mitcad::installedBrowserComponent(root, &problem);
  CHECK(info && info->version == text("2026.1112.1530"));
  // A version that is not there, or one that would leave the folder.
  CHECK(!mitcad::activateBrowserComponent(root, text("9.9.9"), &error));
  CHECK(!mitcad::activateBrowserComponent(root, text("../x"), &error));
  // current.json naming a folder whose component is another version.
  writeFile(QDir(root).filePath(text("current.json")), QByteArray(R"({"version": "2026.1110.900"})"));
  makeComponent(QDir(root).filePath(text("2026.1110.900")), 2, text("2026.1101.1"));
  CHECK(!mitcad::installedBrowserComponent(root, &problem));
  makeComponent(QDir(root).filePath(text("2026.1110.900")), 2, text("2026.1110.900"));

  // Old ones go, never the one in use or the one this run loaded; a
  // download's leftovers too.
  CHECK(mitcad::activateBrowserComponent(root, text("2026.1112.1530"), &error));
  writeFile(QDir(root).filePath(text("2026.1113.100.partial/x")), QByteArray("x"));
  writeFile(QDir(root).filePath(text("2026.1113.100.download.zip")), QByteArray("x"));
  CHECK(mitcad::removeOldBrowserComponents(root, text("2026.1110.900")) == 2);
  CHECK(QFileInfo(QDir(root).filePath(text("2026.1110.900"))).isDir());
  CHECK(mitcad::removeOldBrowserComponents(root, QString()) == 1);
  CHECK(!QFileInfo::exists(QDir(root).filePath(text("2026.1110.900"))));
  CHECK(QFileInfo(QDir(root).filePath(text("2026.1112.1530"))).isDir());

  // A build's component (the UI tests'): used whatever is installed.
  const QString build = QDir(root).filePath(text("build-tree"));
  makeComponent(build, 2, text("0.0.0-build"));
  qputenv("MITCAD_BROWSER_COMPONENT_DIR", build.toUtf8());
  info = mitcad::installedBrowserComponent(root, &problem);
  CHECK(info && info->fromBuild && info->version == text("0.0.0-build"));
  // Nothing is removed while a build's component is in use.
  CHECK(mitcad::removeOldBrowserComponents(root, QString()) == 0);
  qunsetenv("MITCAD_BROWSER_COMPONENT_DIR");
}

void offerTests(const QString& root) {
  using Status = mitcad::BrowserOffer::Status;
  const QByteArray available(
      R"({"status": "available", "expired": false, "abis": [1, 2], "package": {"platform": "linux-x64", "abi": 2,
          "version": "2026.1112.1530", "cef": "156.0.27", "fingerprint": "f", "url": "https://example.invalid/b.zip",
          "size": 52345678, "sha256": "s", "signature": "x"}})");
  const mitcad::BrowserOffer offer = mitcad::parseBrowserOffer(available);
  CHECK(offer.status == Status::Available && offer.version == text("2026.1112.1530"));
  CHECK(offer.size == 52345678 && offer.cef == text("156.0.27") && offer.abis == (QList<int>{1, 2}));
  CHECK(mitcad::parseBrowserOffer(R"({"status": "current", "package": {"version": "1.0.0", "size": 1,
      "url": "https://example.invalid/b.zip"}})").status == Status::Current);
  const mitcad::BrowserOffer other = mitcad::parseBrowserOffer(R"({"status": "abi", "package": null, "abis": [3]})");
  CHECK(other.status == Status::OtherInterface && other.abis == QList<int>{3});
  CHECK(mitcad::parseBrowserOffer(R"({"status": "none", "package": null, "abis": []})").status == Status::None);
  // Not HTTPS, no size, a version that would leave the folder: invalid.
  CHECK(mitcad::parseBrowserOffer(R"({"status": "available", "package": {"version": "1.0.0", "size": 1,
      "url": "http://example.invalid/b.zip"}})").status == Status::Invalid);
  CHECK(mitcad::parseBrowserOffer(R"({"status": "available", "package": {"version": "1.0.0", "size": 0,
      "url": "https://example.invalid/b.zip"}})").status == Status::Invalid);
  CHECK(mitcad::parseBrowserOffer(R"({"status": "available", "package": {"version": "../1", "size": 1,
      "url": "https://example.invalid/b.zip"}})").status == Status::Invalid);
  CHECK(mitcad::parseBrowserOffer("not json").status == Status::Invalid);

  // An unpacked package against the offer and this MitCad's interface.
  const QString folder = QDir(root).filePath(text("unpacked"));
  makeComponent(folder, mitcad::kComponentBrowserAbi, text("2026.1112.1530"));
  QString problem;
  const std::optional<mitcad::BrowserComponentInfo> unpacked = mitcad::readBrowserComponent(folder, &problem);
  QString error;
  CHECK(unpacked && mitcad::checkUnpackedBrowserComponent(*unpacked, offer, mitcad::kComponentBrowserAbi, &error));
  CHECK(unpacked && !mitcad::checkUnpackedBrowserComponent(*unpacked, offer, mitcad::kComponentBrowserAbi + 1, &error));
  CHECK(error.contains(QString::number(mitcad::kComponentBrowserAbi + 1)));
  mitcad::BrowserOffer older = offer;
  older.version = text("2026.1101.1");
  CHECK(unpacked && !mitcad::checkUnpackedBrowserComponent(*unpacked, older, mitcad::kComponentBrowserAbi, &error));
  QFile::remove(QDir(folder).filePath(text("mitcad-webbrowser.so")));
  CHECK(unpacked && !mitcad::checkUnpackedBrowserComponent(*unpacked, offer, mitcad::kComponentBrowserAbi, &error));

  CHECK(mitcad::browserInterfaceMessage(2, 1).contains(text("1")));
  CHECK(mitcad::browserPublishedInterfaceMessage(2, {1, 3}).contains(text("1, 3")));
  CHECK(!mitcad::browserDownloadSize(52345678).isEmpty());
}

} // namespace

int browserComponentTests() {
  QTemporaryDir directory;
  CHECK(directory.isValid());
  qunsetenv("MITCAD_BROWSER_COMPONENT_DIR");
  readTests(QDir(directory.path()).filePath(QStringLiteral("read")));
  installedTests(QDir(directory.path()).filePath(QStringLiteral("installed")));
  offerTests(QDir(directory.path()).filePath(QStringLiteral("offers")));
  return failures;
}
