// SPDX-License-Identifier: MIT
// The web browser component's checks and installation through the bridge
// (mitcad#1579): see BrowserComponentInstall.hpp.
#include "BrowserComponentInstall.hpp"

#include <cstddef>
#include <cstdint>
#include <exception>
#include <optional>

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QtLogging>

#include "framework/Json.hpp"
#include "mitcad_bridge/update.h"
#include "mitcad_bridge/lib.h"

namespace mitcad {
namespace {

struct Text {
  Q_DECLARE_TR_FUNCTIONS(BrowserComponent)
};

rust::Slice<const std::uint8_t> bytes(const QByteArray& data) {
  return rust::Slice<const std::uint8_t>(reinterpret_cast<const std::uint8_t*>(data.constData()),
                                         static_cast<std::size_t>(data.size()));
}

// Unpacks every file of `zip` into `folder` (the bridge's reader, which
// leaves out names that would leave the folder).
bool unpack(const QString& zip, const QString& folder, QString* error) {
  try {
    const QByteArray zipPath = zip.toUtf8();
    const QJsonArray entries = parseJson(zip_list(rustStr(zipPath))).array();
    if (entries.isEmpty()) {
      *error = Text::tr("The web browser component's package is empty.");
      return false;
    }
    for (const QJsonValue& value : entries) {
      const QByteArray name = value.toObject().value(QStringLiteral("name")).toString().toUtf8();
      const QByteArray target = QDir(folder).filePath(QString::fromUtf8(name)).toUtf8();
      zip_extract(rustStr(zipPath), rustStr(name), rustStr(target));
    }
  } catch (const std::exception& e) {
    *error = Text::tr("The web browser component's package could not be unpacked: %1").arg(errorText(e));
    return false;
  }
  return true;
}

} // namespace

BrowserOffer checkBrowserManifest(const QByteArray& manifest, const QString& keys, const QString& platform, int abi,
                                  const QString& installed, QString* error) {
  try {
    const QByteArray keyText = keys.toUtf8();
    const QByteArray platformText = platform.toUtf8();
    const QByteArray installedText = installed.toUtf8();
    const rust::String json =
        updates::update_browser_check(bytes(manifest), rustStr(keyText), rustStr(platformText),
                                      static_cast<std::uint32_t>(abi), rustStr(installedText));
    return parseBrowserOffer(QByteArray(json.data(), static_cast<qsizetype>(json.size())));
  } catch (const std::exception& e) {
    *error = errorText(e);
    return BrowserOffer();
  }
}

bool installBrowserPackage(const QByteArray& manifest, const QString& keys, const QString& platform, int abi,
                           const BrowserOffer& offer, const QString& zip, const QString& root, QString* error) {
  try {
    const QByteArray keyText = keys.toUtf8();
    const QByteArray platformText = platform.toUtf8();
    const QByteArray versionText = offer.version.toUtf8();
    const QByteArray zipText = zip.toUtf8();
    updates::update_verify_browser_download(bytes(manifest), rustStr(keyText), rustStr(platformText),
                                            static_cast<std::uint32_t>(abi), rustStr(versionText), rustStr(zipText));
  } catch (const std::exception& e) {
    *error = Text::tr("The downloaded web browser component is not MitCad's: %1").arg(errorText(e));
    return false;
  }
  if (!QDir().mkpath(root)) {
    *error = Text::tr("MitCad could not make the folder %1.").arg(QDir::toNativeSeparators(root));
    return false;
  }
  const QString partial = QDir(root).filePath(offer.version + QStringLiteral(".partial"));
  QDir(partial).removeRecursively();
  if (!unpack(zip, partial, error)) {
    QDir(partial).removeRecursively();
    return false;
  }
  QString problem;
  std::optional<BrowserComponentInfo> unpacked = readBrowserComponent(partial, &problem);
  if (!unpacked) {
    *error = Text::tr("The downloaded web browser component is incomplete: %1").arg(problem);
    QDir(partial).removeRecursively();
    return false;
  }
  if (!checkUnpackedBrowserComponent(*unpacked, offer, abi, error)) {
    QDir(partial).removeRecursively();
    return false;
  }
  // The module's subprocess helper and, on macOS, the helper apps' programs:
  // zip files keep no permissions.
  for (const QString& executable : unpacked->executables) {
    const QString path = QDir::cleanPath(QDir(partial).filePath(executable));
    if (!path.startsWith(QDir::cleanPath(partial) + QLatin1Char('/')) || !QFileInfo::exists(path)) {
      continue;
    }
    QFile::setPermissions(path, QFile::permissions(path) | QFileDevice::ExeOwner | QFileDevice::ExeGroup |
                                    QFileDevice::ExeOther | QFileDevice::ExeUser);
  }
  const QString folder = QDir(root).filePath(offer.version);
  if (QFileInfo::exists(folder) && !QDir(folder).removeRecursively()) {
    *error = Text::tr("MitCad could not replace %1.").arg(QDir::toNativeSeparators(folder));
    QDir(partial).removeRecursively();
    return false;
  }
  if (!QDir().rename(partial, folder)) {
    *error = Text::tr("MitCad could not install the web browser component in %1.").arg(QDir::toNativeSeparators(folder));
    QDir(partial).removeRecursively();
    return false;
  }
  if (!activateBrowserComponent(root, offer.version, error)) {
    return false;
  }
  qInfo().noquote() << QStringLiteral("Component from Web: installed the web browser component %1 (CEF %2) in %3")
                           .arg(offer.version, unpacked->cef, QDir::toNativeSeparators(folder));
  return true;
}

} // namespace mitcad
