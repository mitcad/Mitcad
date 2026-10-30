// SPDX-License-Identifier: MIT
#include "UpdateSettings.hpp"

#include <QSettings>
#include <QtGlobal>

// The CMake option MITCAD_UPDATE_TEST_HOOKS (mitcad#921): 1 in a test
// build, which reads the tests' environment variables; never in a release.
#ifndef MITCAD_UPDATE_TEST_HOOKS
#define MITCAD_UPDATE_TEST_HOOKS 0
#endif

namespace mitcad {
namespace {

const QString kAutomatic = QStringLiteral("updates/automatic");
const QString kChannel = QStringLiteral("updates/channel");
const QString kSkipped = QStringLiteral("updates/skipped");
const QString kLastCheck = QStringLiteral("updates/lastCheck");
const QString kHintShown = QStringLiteral("updates/hintShown");

const QString kStableManifest =
    QStringLiteral("https://github.com/mitcad/Mitcad/releases/latest/download/update-manifest.json");
// Unauthenticated: one request a day stays far below GitHub's rate limit.
const QString kReleases = QStringLiteral("https://api.github.com/repos/mitcad/Mitcad/releases?per_page=20");

// The tests' variables (docs/updates.md#the-test-hooks), which only a build with
// MITCAD_UPDATE_TEST_HOOKS reads.
const char* const kTestVariables[] = {"MITCAD_UPDATE_URL", "MITCAD_UPDATE_RELEASES_URL", "MITCAD_UPDATE_TEST_KEY",
                                      "MITCAD_UPDATE_TEST_CA", kTestVersionVariable};

QString platformName() {
#if defined(_WIN32) && (defined(_M_X64) || defined(__x86_64__))
  return QStringLiteral("windows-x64");
#elif defined(__linux__) && defined(__x86_64__)
  return QStringLiteral("linux-x64");
#elif defined(__linux__) && defined(__aarch64__)
  return QStringLiteral("linux-arm64");
#elif defined(__APPLE__) && defined(__aarch64__)
  return QStringLiteral("macos-arm64");
#else
  return QString();
#endif
}

} // namespace

UpdateSettings UpdateSettings::load() {
  const QSettings settings;
  UpdateSettings updates;
  updates.automatic = settings.value(kAutomatic, updates.automatic).toBool();
  updates.channel = settings.value(kChannel).toString() == QStringLiteral("prerelease") ? Channel::Prerelease
                                                                                         : Channel::Stable;
  updates.skippedVersion = settings.value(kSkipped).toString();
  updates.lastCheck = QDateTime::fromString(settings.value(kLastCheck).toString(), Qt::ISODate);
  updates.hintShown = settings.value(kHintShown, false).toBool();
  return updates;
}

void UpdateSettings::save() const {
  QSettings settings;
  settings.setValue(kAutomatic, automatic);
  settings.setValue(kChannel, channel == Channel::Prerelease ? QStringLiteral("prerelease") : QStringLiteral("stable"));
  settings.setValue(kSkipped, skippedVersion);
  settings.setValue(kLastCheck, lastCheck.isValid() ? lastCheck.toUTC().toString(Qt::ISODate) : QString());
  settings.setValue(kHintShown, hintShown);
}

bool updatesDisabledByAdministrator() {
  const QByteArray variable = qgetenv("MITCAD_NO_UPDATE_CHECK").trimmed();
  if (!variable.isEmpty() && variable != "0") {
    return true;
  }
#ifdef _WIN32
  // Not QSettings' default format, which tests point at an INI file.
  const QSettings policy(QStringLiteral(R"(HKEY_LOCAL_MACHINE\SOFTWARE\Policies\Mitcad\Mitcad)"),
                         QSettings::NativeFormat);
  if (policy.value(QStringLiteral("DisableUpdateCheck"), 0).toInt() != 0) {
    return true;
  }
#endif
  return false;
}

bool updateTestHooksBuiltIn() { return MITCAD_UPDATE_TEST_HOOKS != 0; }

UpdateSource UpdateSource::fromEnvironment() {
  UpdateSource source;
  source.manifestUrl = kStableManifest;
  source.releasesUrl = kReleases;
  source.builtInKeys = QStringLiteral(MITCAD_UPDATE_KEY).trimmed();
  source.keys = source.builtInKeys;
  source.currentVersion = QStringLiteral(MITCAD_VERSION);
  source.platform = platformName();
  for (const char* const variable : kTestVariables) {
    if (!qEnvironmentVariableIsEmpty(variable)) {
      source.testVariables.append(QString::fromLatin1(variable));
    }
  }
#if MITCAD_UPDATE_TEST_HOOKS
  // A test build (mitcad#921): the tests' server, key, certificate and
  // version.
  source.manifestUrl = qEnvironmentVariable("MITCAD_UPDATE_URL", kStableManifest);
  source.releasesUrl = qEnvironmentVariable("MITCAD_UPDATE_RELEASES_URL", kReleases);
  const QString testKey = qEnvironmentVariable("MITCAD_UPDATE_TEST_KEY").trimmed();
  if (!testKey.isEmpty()) {
    source.keys = source.keys.isEmpty() ? testKey : source.keys + QLatin1Char(' ') + testKey;
  }
  source.testCertificates = qEnvironmentVariable("MITCAD_UPDATE_TEST_CA");
  source.currentVersion = qEnvironmentVariable(kTestVersionVariable, QStringLiteral(MITCAD_VERSION));
  source.testSource = source.manifestUrl != kStableManifest || source.releasesUrl != kReleases ||
                      !testKey.isEmpty() || !source.testCertificates.isEmpty();
#endif
  return source;
}

QStringList UpdateSource::describe() const {
  const qsizetype builtIn = builtInKeys.split(QLatin1Char(' '), Qt::SkipEmptyParts).size();
  QString verified = builtIn == 0 ? QStringLiteral("no built-in release key")
                                  : QStringLiteral("%1 built-in release key(s)").arg(builtIn);
  if (keys != builtInKeys) {
    verified += QStringLiteral(" and the test key");
  }
  QStringList lines;
  lines.append(QStringLiteral("Update source: manifest at %1, releases at %2, %3")
                   .arg(manifestUrl, releasesUrl, verified));
  if (!testVariables.isEmpty()) {
    const QString names = testVariables.join(QStringLiteral(", "));
    const QString line = updateTestHooksBuiltIn()
                             ? QStringLiteral("Update source: this test build reads %1 (MITCAD_UPDATE_TEST_HOOKS)")
                             : QStringLiteral("Update source: this build ignores %1 (MITCAD_UPDATE_TEST_HOOKS off)");
    lines.append(line.arg(names));
  }
  return lines;
}

} // namespace mitcad
