// SPDX-License-Identifier: MIT
#pragma once

// The web browser component of Insert › Component from Web (mitcad#1579,
// app/COMMANDS.md "Component from Web"): the browser's module
// (mitcad-webbrowser) and the Chromium Embedded Framework's runtime, a
// package of its own per platform that MitCad downloads on first use, never
// part of the installers. What MitCad decides about it without the network
// or the bridge, in plain Qt Core (app/tests/browser_component.cpp): where
// components are installed, which one is in use, its interface (ABI), what
// the update manifest offers, and the messages.
//
// A package is a zip of the component's folder, with `component.json` at
// its top: {"abi": 2, "version": "2026.1110.900", "cef": "<CEF version>",
// "platform": "windows-x64", "module": "mitcad-webbrowser.dll",
// "executables": [the files to make executable after unpacking]}. It is
// unpacked into <application data>/browser/<version>/, and
// <application data>/browser/current.json names the version in use.

#include <optional>

#include <QByteArray>
#include <QList>
#include <QString>
#include <QStringList>

namespace mitcad {

// An installed component.
struct BrowserComponentInfo {
  QString folder;     // the component's folder
  QString version;    // the package's version
  QString cef;        // the CEF version inside
  QString platform;   // the platform it was built for
  QString module;     // the module's file name in the folder
  QStringList executables;
  int abi = 0;        // the module interface it was built for
  bool fromBuild = false; // MITCAD_BROWSER_COMPONENT_DIR: a build tree's, never updated
};

// Where components are installed: MITCAD_BROWSER_COMPONENTS_DIR for tests,
// else the user's application data folder's `browser`.
QString browserComponentsRoot();
// A component's description from its folder's component.json; nothing,
// with the reason, when it has none or it is not readable.
std::optional<BrowserComponentInfo> readBrowserComponent(const QString& folder, QString* problem);
// The component in use: MITCAD_BROWSER_COMPONENT_DIR (a build tree's, the
// UI tests'), else the version `root`/current.json names; nothing when
// none is installed (`problem` says why, for the log).
std::optional<BrowserComponentInfo> installedBrowserComponent(const QString& root, QString* problem);
// Makes `version`, unpacked in `root`/<version>, the one in use.
bool activateBrowserComponent(const QString& root, const QString& version, QString* error);
// Removes the versions in `root` other than the one in use and `keep` (the
// one this run loaded, which a newer installed beside it does not remove):
// how many it removed.
int removeOldBrowserComponents(const QString& root, const QString& keep);

// What the update manifest offers (the bridge's update_browser_check).
struct BrowserOffer {
  enum class Status { Available, Current, OtherInterface, None, Invalid };
  Status status = Status::Invalid;
  bool expired = false;
  QString version;
  QString cef;
  QString url;
  QString sha256;
  qint64 size = 0;
  QList<int> abis; // the interfaces published for the platform
};
BrowserOffer parseBrowserOffer(const QByteArray& json);

// Checks an unpacked package's component.json against the manifest's
// offer and this MitCad's interface: the same version and interface, a
// module file that is there; false with the reason otherwise.
bool checkUnpackedBrowserComponent(const BrowserComponentInfo& unpacked, const BrowserOffer& offer, int abi,
                                   QString* error);

// The messages of a component of another interface than this MitCad's
// (`abi`): an installed one (`installed`), and the manifest's packages
// only for other interfaces (`published`).
QString browserInterfaceMessage(int abi, int installed);
QString browserPublishedInterfaceMessage(int abi, const QList<int>& published);
// "52.3 MB" for the question before a download.
QString browserDownloadSize(qint64 bytes);

} // namespace mitcad
