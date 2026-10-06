// SPDX-License-Identifier: MIT
#pragma once

// Automatic updates (mitcad#9, docs/updates.md): the user's settings, the
// administrator's switch, and where the releases are.

#include <QDateTime>
#include <QString>

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

// Where the releases are and what is checked with what.
struct UpdateSource {
  // The stable channel's manifest: GitHub's latest release (which skips
  // pre-releases); MITCAD_UPDATE_URL instead (HTTPS; a mirror, or a test).
  QString manifestUrl;
  // The pre-release channel: GitHub's list of releases (the REST API),
  // whose newest has the manifest; MITCAD_UPDATE_RELEASES_URL instead.
  QString releasesUrl;
  // The public keys a manifest may be signed with, in hex: the release key
  // built in (none while it is the placeholder, and then no request is
  // made; docs/updates.md), and in tests MITCAD_UPDATE_TEST_KEY.
  QString keys;
  // A PEM certificate the update requests trust besides the system's
  // (MITCAD_UPDATE_TEST_CA: a test's local HTTPS server).
  QString testCertificates;
  // The version compared with the releases: this build's, or in tests
  // MITCAD_TEST_VERSION (never passed on to the Mitcad an update starts).
  QString currentVersion;
  // The manifest's name of this build's platform: "windows-x64",
  // "linux-x64", ..., or empty (announce only).
  QString platform;

  static UpdateSource fromEnvironment();
};

// The environment variable of the tests' version, dropped for the Mitcad an
// update starts.
inline constexpr char kTestVersionVariable[] = "MITCAD_TEST_VERSION";

} // namespace mitcad
