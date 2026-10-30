// SPDX-License-Identifier: MIT
#pragma once

// Automatic updates (mitcad#9, docs/updates.md): the user's settings, the
// administrator's switch, and where the releases are.

#include <QDateTime>
#include <QString>
#include <QStringList>

namespace mitcad {

// The user's settings (Preferences, Updates): updates/automatic,
// updates/channel ("stable" or "prerelease"), updates/skipped (the version
// "Skip This Version" chose), updates/lastCheck (UTC, ISO 8601) and
// updates/hintShown (the first notice told where to turn checks off).
struct UpdateSettings {
  enum class Channel { Stable, Prerelease };

  // At start-up, at most once a day.
  bool automatic = true;
  Channel channel = Channel::Stable;
  QString skippedVersion;
  QDateTime lastCheck; // invalid: never
  bool hintShown = false;

  static UpdateSettings load();
  void save() const;
};

// An administrator turned update checks off for every user, for offline or
// managed installations: the environment variable MITCAD_NO_UPDATE_CHECK
// set (to anything but 0), or on Windows the registry value
// DisableUpdateCheck (a DWORD, not 0) under
// HKEY_LOCAL_MACHINE\SOFTWARE\Policies\Mitcad\Mitcad. Mitcad then makes no
// request at all, Help > Check for Updates neither.
bool updatesDisabledByAdministrator();

// Whether this build reads the tests' environment variables of the updates
// (MITCAD_UPDATE_URL, MITCAD_UPDATE_RELEASES_URL, MITCAD_UPDATE_TEST_KEY,
// MITCAD_UPDATE_TEST_CA, MITCAD_TEST_VERSION): only a test build configured
// with the CMake option MITCAD_UPDATE_TEST_HOOKS does (mitcad#921); a
// release ignores them (docs/updates.md#the-test-hooks).
bool updateTestHooksBuiltIn();

// Where the releases are and what is checked with what.
struct UpdateSource {
  // The stable channel's manifest: GitHub's latest release (which skips
  // pre-releases); in a test build MITCAD_UPDATE_URL instead (HTTPS).
  QString manifestUrl;
  // The pre-release channel: GitHub's list of releases (the REST API),
  // whose newest has the manifest; in a test build
  // MITCAD_UPDATE_RELEASES_URL instead.
  QString releasesUrl;
  // The public keys a manifest may be signed with, in hex: the release keys
  // built in (`builtInKeys`), and in a test build MITCAD_UPDATE_TEST_KEY.
  QString keys;
  // The release keys built in alone: none while the key file holds the
  // placeholder, and then no request is made at all, whatever test key is
  // set (docs/updates.md).
  QString builtInKeys;
  // A PEM certificate the update requests trust besides the system's (in a
  // test build MITCAD_UPDATE_TEST_CA: a test's local HTTPS server).
  QString testCertificates;
  // The version compared with the releases: this build's, or in a test
  // build MITCAD_TEST_VERSION (never passed on to the MitCad an update
  // starts).
  QString currentVersion;
  // The manifest's name of this build's platform: "windows-x64",
  // "linux-x64", ..., or empty (announce only).
  QString platform;
  // The tests' variables set (not empty) in the environment, which a test
  // build took and a release ignored (for the log).
  QStringList testVariables;
  // A test build took an address, a key or a certificate from the
  // environment: such an update never installs with administrator rights
  // it asks for (restrictToUserRights, mitcad#921).
  bool testSource = false;

  static UpdateSource fromEnvironment();
  // The log's lines of the source: "Update source: manifest at <url>,
  // releases at <url>, <keys>", and with test variables set whether this
  // build took or ignored them (the update tests read them).
  QStringList describe() const;
};

// The environment variable of the tests' version, dropped for the Mitcad an
// update starts.
inline constexpr char kTestVersionVariable[] = "MITCAD_TEST_VERSION";

} // namespace mitcad
