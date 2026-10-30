// SPDX-License-Identifier: MIT
// The automatic updates' test hooks (mitcad#921, docs/updates.md#the-test-hooks).
// This file and update/UpdateSettings.cpp are built twice: as a release
// (test_update_source_release, MITCAD_UPDATE_TEST_HOOKS 0) and as a test
// build (test_update_source_hooks, 1). With the tests' environment
// variables set, a release keeps GitHub's addresses, only its built-in
// release key, no extra certificate and its own version, and the core
// refuses a manifest signed with the variable's key; a test build takes
// them all and the core accepts that manifest. The key and the manifest
// are made for the run by the release tool mitcad-release.
//
// Usage: test_update_source_<release|hooks> <mitcad-release>
#include "update/UpdateSettings.hpp"

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <exception>

#include <QByteArray>
#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QIODevice>
#include <QProcess>
#include <QProcessEnvironment>
#include <QRandomGenerator>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>
#include <QtGlobal>

#include "mitcad_bridge/update.h"

#ifndef MITCAD_UPDATE_TEST_HOOKS
#error "test_update_source is built with MITCAD_UPDATE_TEST_HOOKS 0 or 1"
#endif

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "update_source.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

const QString kDefaultManifest =
    QStringLiteral("https://github.com/mitcad/Mitcad/releases/latest/download/update-manifest.json");
const QString kDefaultReleases = QStringLiteral("https://api.github.com/repos/mitcad/Mitcad/releases?per_page=20");
const QString kBuiltInKeys = QStringLiteral(MITCAD_UPDATE_KEY).trimmed();
const QString kVersion = QStringLiteral(MITCAD_VERSION);

const char* const kVariables[] = {"MITCAD_UPDATE_URL", "MITCAD_UPDATE_RELEASES_URL", "MITCAD_UPDATE_TEST_KEY",
                                  "MITCAD_UPDATE_TEST_CA", "MITCAD_TEST_VERSION"};

void clearVariables() {
  for (const char* const variable : kVariables) {
    qunsetenv(variable);
  }
}

// Runs the release tool with the signing key `seed`: true when it
// succeeded, with its standard output in `output`.
bool releaseTool(const QString& tool, const QString& seed, const QStringList& arguments, QByteArray& output) {
  QProcess process;
  QProcessEnvironment environment = QProcessEnvironment::systemEnvironment();
  environment.insert(QStringLiteral("MITCAD_RELEASE_KEY"), seed);
  process.setProcessEnvironment(environment);
  process.start(tool, arguments);
  if (!process.waitForFinished(120000) || process.exitStatus() != QProcess::NormalExit || process.exitCode() != 0) {
    std::fprintf(stderr, "%s %s failed: %s %s\n", qPrintable(tool), qPrintable(arguments.join(QLatin1Char(' '))),
                 qPrintable(process.errorString()), process.readAllStandardError().constData());
    return false;
  }
  output = process.readAllStandardOutput();
  return true;
}

// "N built-in release key(s)" or "no built-in release key", as describe()
// says.
QString builtInKeysText() {
  const qsizetype count = kBuiltInKeys.split(QLatin1Char(' '), Qt::SkipEmptyParts).size();
  return count == 0 ? QStringLiteral("no built-in release key")
                    : QStringLiteral("%1 built-in release key(s)").arg(count);
}

// Whether the core accepts the manifest with the source's keys, as the
// application's check does (UpdateClient::readManifest); the reason when
// not.
bool accepted(const QByteArray& manifest, const mitcad::UpdateSource& source, QString& reason) {
  const QByteArray keys = source.keys.toUtf8();
  const QByteArray current = source.currentVersion.toUtf8();
  const QByteArray platform = source.platform.toUtf8();
  try {
    mitcad::updates::update_check(
        rust::Slice<const std::uint8_t>(reinterpret_cast<const std::uint8_t*>(manifest.constData()),
                                        static_cast<std::size_t>(manifest.size())),
        rust::Str(keys.constData(), static_cast<std::size_t>(keys.size())),
        rust::Str(current.constData(), static_cast<std::size_t>(current.size())),
        rust::Str(platform.constData(), static_cast<std::size_t>(platform.size())), false, rust::Str(""));
  } catch (const std::exception& error) {
    reason = QString::fromUtf8(error.what());
    return false;
  }
  return true;
}

QString randomSeed() {
  QString seed;
  for (int i = 0; i < 32; ++i) {
    seed += QStringLiteral("%1").arg(QRandomGenerator::system()->bounded(256), 2, 16, QLatin1Char('0'));
  }
  return seed;
}

// Nothing set: GitHub, the built-in keys, this build's version.
void defaultsTests() {
  clearVariables();
  const mitcad::UpdateSource source = mitcad::UpdateSource::fromEnvironment();
  CHECK(source.manifestUrl == kDefaultManifest);
  CHECK(source.releasesUrl == kDefaultReleases);
  CHECK(source.keys == kBuiltInKeys);
  CHECK(source.builtInKeys == kBuiltInKeys);
  CHECK(source.testCertificates.isEmpty());
  CHECK(source.currentVersion == kVersion);
  CHECK(source.testVariables.isEmpty());
  CHECK(!source.testSource);
  const QStringList lines = source.describe();
  CHECK(lines.size() == 1);
  CHECK(lines.value(0) == QStringLiteral("Update source: manifest at %1, releases at %2, %3")
                              .arg(kDefaultManifest, kDefaultReleases, builtInKeysText()));
}

// The tests' variables set, as an attacker who can write a user's
// environment would (the ticket's case): a key of their own signs a
// manifest at an address of their own.
void variablesTests(const QString& tool, const QString& folder) {
  const QString seed = randomSeed();
  QByteArray printed;
  CHECK(releaseTool(tool, seed, QStringList(QStringLiteral("public-key")), printed));
  const QString publicKey = QString::fromUtf8(printed).trimmed();
  CHECK(publicKey.size() == 64);
  const QString manifestPath = QDir(folder).filePath(QStringLiteral("update-manifest.json"));
  CHECK(releaseTool(tool, seed,
                    {QStringLiteral("manifest"), QStringLiteral("--version"), QStringLiteral("99.0.0"),
                     QStringLiteral("--date"), QStringLiteral("2026-10-29"), QStringLiteral("--notes"),
                     QStringLiteral("https://127.0.0.1:9/notes.html"), QStringLiteral("--out"), manifestPath},
                    printed));
  QFile file(manifestPath);
  CHECK(file.open(QIODevice::ReadOnly));
  const QByteArray manifest = file.readAll();
  CHECK(!manifest.isEmpty());

  const QString url = QStringLiteral("https://127.0.0.1:9/update-manifest.json");
  const QString releases = QStringLiteral("https://127.0.0.1:9/releases.json");
  const QString certificate = QDir(folder).filePath(QStringLiteral("cert.pem"));
  clearVariables();
  qputenv("MITCAD_UPDATE_URL", url.toUtf8());
  qputenv("MITCAD_UPDATE_RELEASES_URL", releases.toUtf8());
  qputenv("MITCAD_UPDATE_TEST_KEY", publicKey.toUtf8());
  qputenv("MITCAD_UPDATE_TEST_CA", certificate.toUtf8());
  qputenv("MITCAD_TEST_VERSION", "0.0.0");
  const mitcad::UpdateSource source = mitcad::UpdateSource::fromEnvironment();
  CHECK(source.testVariables == QStringList({QStringLiteral("MITCAD_UPDATE_URL"),
                                             QStringLiteral("MITCAD_UPDATE_RELEASES_URL"),
                                             QStringLiteral("MITCAD_UPDATE_TEST_KEY"),
                                             QStringLiteral("MITCAD_UPDATE_TEST_CA"),
                                             QStringLiteral("MITCAD_TEST_VERSION")}));
  // The gate of a build without a release key reads the built-in keys
  // alone: a test key never makes such a build send a request.
  CHECK(source.builtInKeys == kBuiltInKeys);
  QString reason;
  const QStringList lines = source.describe();
  CHECK(lines.size() == 2);
#if MITCAD_UPDATE_TEST_HOOKS
  CHECK(mitcad::updateTestHooksBuiltIn());
  CHECK(source.manifestUrl == url);
  CHECK(source.releasesUrl == releases);
  CHECK(source.keys == (kBuiltInKeys.isEmpty() ? publicKey : kBuiltInKeys + QLatin1Char(' ') + publicKey));
  CHECK(source.testCertificates == certificate);
  CHECK(source.currentVersion == QStringLiteral("0.0.0"));
  CHECK(source.testSource);
  CHECK(accepted(manifest, source, reason));
  CHECK(lines.value(0) == QStringLiteral("Update source: manifest at %1, releases at %2, %3 and the test key")
                              .arg(url, releases, builtInKeysText()));
  CHECK(lines.value(1) == QStringLiteral("Update source: this test build reads MITCAD_UPDATE_URL, "
                                         "MITCAD_UPDATE_RELEASES_URL, MITCAD_UPDATE_TEST_KEY, MITCAD_UPDATE_TEST_CA, "
                                         "MITCAD_TEST_VERSION (MITCAD_UPDATE_TEST_HOOKS)"));
#else
  // A release: the check goes to GitHub's address and the manifest signed
  // with the variable's key is refused.
  CHECK(!mitcad::updateTestHooksBuiltIn());
  CHECK(source.manifestUrl == kDefaultManifest);
  CHECK(source.releasesUrl == kDefaultReleases);
  CHECK(source.keys == kBuiltInKeys);
  CHECK(!source.keys.contains(publicKey));
  CHECK(source.testCertificates.isEmpty());
  CHECK(source.currentVersion == kVersion);
  CHECK(!source.testSource);
  CHECK(!accepted(manifest, source, reason));
  CHECK(!reason.isEmpty());
  std::printf("ok   a release refuses the manifest signed with MITCAD_UPDATE_TEST_KEY: %s\n", qPrintable(reason));
  CHECK(lines.value(0) == QStringLiteral("Update source: manifest at %1, releases at %2, %3")
                              .arg(kDefaultManifest, kDefaultReleases, builtInKeysText()));
  CHECK(lines.value(1) == QStringLiteral("Update source: this build ignores MITCAD_UPDATE_URL, "
                                         "MITCAD_UPDATE_RELEASES_URL, MITCAD_UPDATE_TEST_KEY, MITCAD_UPDATE_TEST_CA, "
                                         "MITCAD_TEST_VERSION (MITCAD_UPDATE_TEST_HOOKS off)"));
#endif

  // What makes a test's source (the elevation rule, mitcad#921): an
  // address, a key or a certificate; the version alone does not.
  clearVariables();
  qputenv("MITCAD_TEST_VERSION", "0.0.0");
  CHECK(!mitcad::UpdateSource::fromEnvironment().testSource);
  clearVariables();
  qputenv("MITCAD_UPDATE_TEST_CA", certificate.toUtf8());
  CHECK(mitcad::UpdateSource::fromEnvironment().testSource == mitcad::updateTestHooksBuiltIn());
  clearVariables();
  qputenv("MITCAD_UPDATE_RELEASES_URL", releases.toUtf8());
  CHECK(mitcad::UpdateSource::fromEnvironment().testSource == mitcad::updateTestHooksBuiltIn());
  clearVariables();
  qputenv("MITCAD_UPDATE_TEST_KEY", publicKey.toUtf8());
  const mitcad::UpdateSource keyOnly = mitcad::UpdateSource::fromEnvironment();
  CHECK(keyOnly.testSource == mitcad::updateTestHooksBuiltIn());
  CHECK(keyOnly.builtInKeys == kBuiltInKeys);
  // Set but empty is not set.
  clearVariables();
  qputenv("MITCAD_UPDATE_TEST_KEY", "");
  CHECK(mitcad::UpdateSource::fromEnvironment().testVariables.isEmpty());
  clearVariables();
}

} // namespace

int main(int argc, char** argv) {
  QCoreApplication application(argc, argv);
  if (argc != 2) {
    std::fprintf(stderr, "usage: %s <mitcad-release>\n", argv[0]);
    return 2;
  }
  const QString tool = QString::fromLocal8Bit(argv[1]);
  QTemporaryDir folder;
  CHECK(folder.isValid());
  defaultsTests();
  variablesTests(tool, folder.path());
  if (failures > 0) {
    std::fprintf(stderr, "%d check(s) failed\n", failures);
    return 1;
  }
  std::printf("update source (%s): all checks passed\n",
              mitcad::updateTestHooksBuiltIn() ? "test build" : "release build");
  return 0;
}
