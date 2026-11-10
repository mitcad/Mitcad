// SPDX-License-Identifier: MIT
#pragma once

// The web browser component's checks and installation through the bridge
// (mitcad#1579): the update manifest read and verified with the release key
// (core/update, the updates' own checks), a downloaded package verified
// against it (signature, size, SHA-256), unpacked and made the one in use.
// No network here: BrowserComponentDownload fetches, and the tests
// (app/tests/browser_component_flow.cpp) hand in local files.

#include <QByteArray>
#include <QString>

#include "BrowserComponent.hpp"

namespace mitcad {

// What `manifest` offers for `platform` and the module interface `abi`,
// `installed` the installed version (or empty); Invalid with the reason in
// `error` when the manifest is not one the release key signed.
BrowserOffer checkBrowserManifest(const QByteArray& manifest, const QString& keys, const QString& platform, int abi,
                                  const QString& installed, QString* error);

// Verifies the downloaded package `zip` against the manifest, unpacks it
// into `root`/<version> (through <version>.partial, so that a broken
// unpacking never looks installed), checks its component.json against the
// offer and `abi`, makes its executables executable and makes it the one in
// use. False with the reason otherwise; nothing of a refused package stays.
bool installBrowserPackage(const QByteArray& manifest, const QString& keys, const QString& platform, int abi,
                           const BrowserOffer& offer, const QString& zip, const QString& root, QString* error);

} // namespace mitcad
