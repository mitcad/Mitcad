// SPDX-License-Identifier: MIT
#pragma once

// Automatic updates (mitcad#9): what a release manifest offers the running
// version, and how the update notice tells it (mitcad#1390). Plain Qt
// Core, so that the unit tests read the notice's text without a window.

#include <QByteArray>
#include <QString>
#include <QtGlobal>

namespace mitcad {

// What a release manifest offers the running version (core/update: check).
struct UpdateOffer {
  QString status; // "available", "skipped" or "current"
  QString version;
  QString date;
  // The manifest's last day, or empty (mitcad#938).
  QString expires;
  // A newer version whose manifest has expired: status "current"; a check
  // the user asked for announces it, without Install.
  bool expired = false;
  QString notes; // the release's page
  bool prerelease = false;
  // The download for this platform; without one the release is announced
  // only, and only to a check the user asked for (mitcad#1390).
  bool hasDownload = false;
  QString url;
  qint64 size = 0;
  // The download's SHA-256 (64 lower-case hex digits) from the verified
  // manifest; the Windows updater checks the installer against it again
  // right before it runs it (mitcad#603).
  QString sha256;
  // The manifest as received, verified again with the download.
  QByteArray manifest;
};

// The platform of a manifest's assets in the user's terms: "Windows",
// "Linux" or "macOS" for "windows-x64", "linux-x64", "macos-arm64" and
// the other architectures; empty for a platform it does not know.
QString platformName(const QString& platform);

// How the update notice tells an offer (mitcad#1390).
struct OfferNotice {
  // False: the notice is not shown. An automatic check says nothing of a
  // release without a download for this platform (the log says why), so
  // that the notice comes once the platform's download is out.
  bool show = true;
  // Rich text; every value from the network escaped (mitcad#938).
  QString html;
  bool install = false;
  // Skip This Version: not for a release without a download here, as
  // skipping it would also hide the download that comes out later under
  // the same version.
  bool skip = true;
};

// The notice of `offer` to MitCad `current` on `platform` (the manifest's
// name, e.g. "linux-x64"). `canInstall`: this installation can install
// updates (else it announces only); `manual`: the user asked (Check for
// Updates); `hint`: the notice also tells where checks are turned off.
OfferNotice offerNotice(const UpdateOffer& offer, const QString& current, const QString& platform, bool canInstall,
                        bool manual, bool hint);

} // namespace mitcad
