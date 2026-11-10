// SPDX-License-Identifier: MIT
// The web browser component's first use with a local manifest (mitcad#1579):
// a package (a zip of a component's folder) and an update manifest listing
// it, signed by mitcad-release with a key of the test's own; the manifest's
// offer read through the bridge, the package verified (signature, size,
// SHA-256), unpacked, checked and made the one in use, its helper made
// executable; a tampered package, a manifest signed with another key and
// another interface refused, nothing of them left. No network: the
// application's download (BrowserComponentDownload) is what fetches these
// files from the release.
//
// Usage: test_browser_component_flow <mitcad-release>

#include <cstdint>
#include <cstdio>
#include <optional>

#include <QByteArray>
#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QList>
#include <QPair>
#include <QProcess>
#include <QProcessEnvironment>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>

#include "webbrowser/BrowserComponent.hpp"
#include "webbrowser/BrowserComponentInstall.hpp"
#include "webbrowser/ComponentBrowserHost.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "browser_component_flow.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

const QString kPlatform = QStringLiteral("linux-x64");
const QString kVersion = QStringLiteral("2026.1110.900");
const QString kCef = QStringLiteral("156.0.27+ga10dae3+chromium-156.0.8078.88");
// Test keys (Ed25519 seeds), never a release's.
const QString kKey = QStringLiteral("1111111111111111111111111111111111111111111111111111111111111111");
const QString kOtherKey = QStringLiteral("2222222222222222222222222222222222222222222222222222222222222222");

std::uint32_t crc32(const QByteArray& data) {
  std::uint32_t crc = 0xFFFFFFFFu;
  for (const char byte : data) {
    crc ^= static_cast<std::uint8_t>(byte);
    for (int bit = 0; bit < 8; ++bit) {
      crc = (crc >> 1) ^ (0xEDB88320u & (0u - (crc & 1u)));
    }
  }
  return ~crc;
}

void put16(QByteArray& out, std::uint32_t value) {
  out.append(static_cast<char>(value & 0xFF));
  out.append(static_cast<char>((value >> 8) & 0xFF));
}

void put32(QByteArray& out, std::uint32_t value) {
  put16(out, value & 0xFFFF);
  put16(out, value >> 16);
}

// A zip of stored files, as a package's.
QByteArray zipOf(const QList<QPair<QString, QByteArray>>& files) {
  QByteArray zip;
  QByteArray directory;
  for (const auto& [name, data] : files) {
    const QByteArray path = name.toUtf8();
    const auto offset = static_cast<std::uint32_t>(zip.size());
    const std::uint32_t crc = crc32(data);
    const auto size = static_cast<std::uint32_t>(data.size());
    put32(zip, 0x04034b50);
    put16(zip, 20);
    put16(zip, 0x0800); // UTF-8 names
    put16(zip, 0);      // stored
    put16(zip, 0);
    put16(zip, 0x21);
    put32(zip, crc);
    put32(zip, size);
    put32(zip, size);
    put16(zip, static_cast<std::uint32_t>(path.size()));
    put16(zip, 0);
    zip.append(path);
    zip.append(data);
    put32(directory, 0x02014b50);
    put16(directory, 20);
    put16(directory, 20);
    put16(directory, 0x0800);
    put16(directory, 0);
    put16(directory, 0);
    put16(directory, 0x21);
    put32(directory, crc);
    put32(directory, size);
    put32(directory, size);
    put16(directory, static_cast<std::uint32_t>(path.size()));
    put16(directory, 0);
    put16(directory, 0);
    put16(directory, 0);
    put16(directory, 0);
    put32(directory, 0);
    put32(directory, offset);
    directory.append(path);
  }
  const auto directoryOffset = static_cast<std::uint32_t>(zip.size());
  zip.append(directory);
  put32(zip, 0x06054b50);
  put16(zip, 0);
  put16(zip, 0);
  put16(zip, static_cast<std::uint32_t>(files.size()));
  put16(zip, static_cast<std::uint32_t>(files.size()));
  put32(zip, static_cast<std::uint32_t>(directory.size()));
  put32(zip, directoryOffset);
  put16(zip, 0);
  return zip;
}

QByteArray componentJson(int abi, const QString& version) {
  return QJsonDocument(QJsonObject{{QStringLiteral("abi"), abi},
                                   {QStringLiteral("version"), version},
                                   {QStringLiteral("cef"), kCef},
                                   {QStringLiteral("platform"), kPlatform},
                                   {QStringLiteral("module"), QStringLiteral("mitcad-webbrowser.so")},
                                   {QStringLiteral("executables"),
                                    QJsonArray{QStringLiteral("mitcad-webbrowser-helper")}}})
      .toJson();
}

bool writeFile(const QString& path, const QByteArray& content) {
  QFile file(path);
  return file.open(QIODevice::WriteOnly) && file.write(content) == content.size();
}

QByteArray readFile(const QString& path) {
  QFile file(path);
  return file.open(QIODevice::ReadOnly) ? file.readAll() : QByteArray();
}

// mitcad-release with the key `key` in its environment: its output, or
// empty when it failed.
QByteArray release(const QString& tool, const QString& key, const QStringList& arguments) {
  QProcess process;
  QProcessEnvironment environment = QProcessEnvironment::systemEnvironment();
  environment.insert(QStringLiteral("MITCAD_RELEASE_KEY"), key);
  environment.remove(QStringLiteral("MITCAD_RELEASE_KEY_FILE"));
  process.setProcessEnvironment(environment);
  process.start(tool, arguments);
  if (!process.waitForFinished(60000) || process.exitStatus() != QProcess::NormalExit || process.exitCode() != 0) {
    std::fprintf(stderr, "mitcad-release %s failed: %s\n", qPrintable(arguments.join(QLatin1Char(' '))),
                 process.readAllStandardError().constData());
    return QByteArray();
  }
  return process.readAllStandardOutput();
}

// A manifest listing `zip` as the package `version` of interface `abi`,
// signed with `key`.
QByteArray manifestFor(const QString& tool, const QString& key, const QString& zip, int abi, const QString& version,
                       const QString& out) {
  const QString fingerprint =
      QString::fromLatin1(QCryptographicHash::hash("module and CEF", QCryptographicHash::Sha256).toHex());
  const QByteArray output = release(
      tool, key,
      {QStringLiteral("manifest"), QStringLiteral("--version"), QStringLiteral("0.60.0"), QStringLiteral("--notes"),
       QStringLiteral("https://example.invalid/releases/tag/v0.60.0"), QStringLiteral("--browser"), kPlatform,
       QString::number(abi), version, kCef, fingerprint, zip,
       QStringLiteral("https://example.invalid/mitcad-browser-%1-linux-x64.zip").arg(version), QStringLiteral("--out"),
       out});
  return output.isEmpty() ? QByteArray() : readFile(out);
}

} // namespace

int main(int argc, char* argv[]) {
  QCoreApplication app(argc, argv);
  if (argc != 2) {
    std::fprintf(stderr, "usage: test_browser_component_flow <mitcad-release>\n");
    return 2;
  }
  const QString tool = QString::fromLocal8Bit(argv[1]);
  qunsetenv("MITCAD_BROWSER_COMPONENT_DIR");
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QDir work(directory.path());
  const int abi = mitcad::kComponentBrowserAbi;
  const QString keys = QString::fromLatin1(release(tool, kKey, {QStringLiteral("public-key")})).trimmed();
  CHECK(keys.size() == 64);

  // The package: a component's folder as the build stages it.
  const QByteArray package = zipOf({{QStringLiteral("component.json"), componentJson(abi, kVersion)},
                                    {QStringLiteral("mitcad-webbrowser.so"), QByteArray("the module")},
                                    {QStringLiteral("mitcad-webbrowser-helper"), QByteArray("the helper")},
                                    {QStringLiteral("locales/en-US.pak"), QByteArray("a locale")}});
  const QString zip = work.filePath(QStringLiteral("package.zip"));
  CHECK(writeFile(zip, package));
  const QByteArray manifest = manifestFor(tool, kKey, zip, abi, kVersion, work.filePath(QStringLiteral("m.json")));
  CHECK(!manifest.isEmpty());

  // The first use: nothing installed, the offer read, the package installed.
  const QString root = work.filePath(QStringLiteral("components"));
  QString problem;
  CHECK(!mitcad::installedBrowserComponent(root, &problem));
  QString error;
  const mitcad::BrowserOffer offer = mitcad::checkBrowserManifest(manifest, keys, kPlatform, abi, QString(), &error);
  CHECK(offer.status == mitcad::BrowserOffer::Status::Available);
  CHECK(offer.version == kVersion && offer.cef == kCef && offer.size == package.size());
  CHECK(mitcad::installBrowserPackage(manifest, keys, kPlatform, abi, offer, zip, root, &error));
  const std::optional<mitcad::BrowserComponentInfo> installed = mitcad::installedBrowserComponent(root, &problem);
  CHECK(installed && installed->version == kVersion && installed->abi == abi);
  CHECK(installed && readFile(QDir(installed->folder).filePath(QStringLiteral("mitcad-webbrowser.so"))) ==
                         QByteArray("the module"));
  CHECK(installed && QFileInfo::exists(QDir(installed->folder).filePath(QStringLiteral("locales/en-US.pak"))));
#ifndef Q_OS_WIN
  CHECK(installed && QFileInfo(QDir(installed->folder).filePath(QStringLiteral("mitcad-webbrowser-helper")))
                         .isExecutable());
#endif
  CHECK(!QFileInfo::exists(QDir(root).filePath(kVersion + QStringLiteral(".partial"))));
  // Installed: nothing newer offered.
  CHECK(mitcad::checkBrowserManifest(manifest, keys, kPlatform, abi, kVersion, &error).status ==
        mitcad::BrowserOffer::Status::Current);

  // Another interface: refused before any download, with the published ones.
  const mitcad::BrowserOffer other = mitcad::checkBrowserManifest(manifest, keys, kPlatform, abi + 1, QString(), &error);
  CHECK(other.status == mitcad::BrowserOffer::Status::OtherInterface && other.abis == QList<int>{abi});
  // No package for the platform.
  CHECK(mitcad::checkBrowserManifest(manifest, keys, QStringLiteral("windows-x64"), abi, QString(), &error).status ==
        mitcad::BrowserOffer::Status::None);

  // A package changed after signing: refused, nothing installed of it.
  const QString freshRoot = work.filePath(QStringLiteral("fresh"));
  QByteArray tampered = package;
  tampered[tampered.size() / 2] = static_cast<char>(tampered[tampered.size() / 2] ^ 0x5A);
  const QString tamperedZip = work.filePath(QStringLiteral("tampered.zip"));
  CHECK(writeFile(tamperedZip, tampered));
  error.clear();
  CHECK(!mitcad::installBrowserPackage(manifest, keys, kPlatform, abi, offer, tamperedZip, freshRoot, &error));
  CHECK(!error.isEmpty());
  CHECK(!mitcad::installedBrowserComponent(freshRoot, &problem));
  CHECK(!QFileInfo::exists(QDir(freshRoot).filePath(kVersion)));

  // A manifest signed with another key than the trusted one: no offer.
  const QByteArray forged =
      manifestFor(tool, kOtherKey, zip, abi, kVersion, work.filePath(QStringLiteral("forged.json")));
  CHECK(!forged.isEmpty());
  error.clear();
  CHECK(mitcad::checkBrowserManifest(forged, keys, kPlatform, abi, QString(), &error).status ==
        mitcad::BrowserOffer::Status::Invalid);
  CHECK(!error.isEmpty());
  CHECK(!mitcad::installBrowserPackage(forged, keys, kPlatform, abi, offer, zip, freshRoot, &error));

  // A package whose component.json is of another interface than its entry.
  const QByteArray mismatched = zipOf({{QStringLiteral("component.json"), componentJson(abi + 1, kVersion)},
                                       {QStringLiteral("mitcad-webbrowser.so"), QByteArray("the module")}});
  const QString mismatchedZip = work.filePath(QStringLiteral("mismatched.zip"));
  CHECK(writeFile(mismatchedZip, mismatched));
  const QByteArray mismatchedManifest =
      manifestFor(tool, kKey, mismatchedZip, abi, kVersion, work.filePath(QStringLiteral("mismatched.json")));
  const mitcad::BrowserOffer mismatchedOffer =
      mitcad::checkBrowserManifest(mismatchedManifest, keys, kPlatform, abi, QString(), &error);
  CHECK(!mitcad::installBrowserPackage(mismatchedManifest, keys, kPlatform, abi, mismatchedOffer, mismatchedZip,
                                       freshRoot, &error));
  CHECK(!QFileInfo::exists(QDir(freshRoot).filePath(kVersion)));

  if (failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", failures);
    return 1;
  }
  std::printf("browser component flow tests passed\n");
  return 0;
}
