// SPDX-License-Identifier: MIT
// The web browser component without the network or the bridge (mitcad#1579):
// plain Qt Core, see BrowserComponent.hpp.
#include "BrowserComponent.hpp"

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLocale>
#include <QSaveFile>
#include <QStandardPaths>
#include <QtGlobal>

namespace mitcad {
namespace {

struct Text {
  Q_DECLARE_TR_FUNCTIONS(BrowserComponent)
};

const QString kComponentFile = QStringLiteral("component.json");
const QString kCurrentFile = QStringLiteral("current.json");

// A version as a folder name: what a semantic version allows, nothing that
// leaves the folder.
bool safeVersion(const QString& version) {
  if (version.isEmpty() || version.startsWith(QLatin1Char('.'))) {
    return false;
  }
  for (const QChar c : version) {
    if (!(c.isLetterOrNumber() && c.unicode() < 128) && c != QLatin1Char('.') && c != QLatin1Char('-') &&
        c != QLatin1Char('+')) {
      return false;
    }
  }
  return true;
}

} // namespace

QString browserComponentsRoot() {
  const QString given = qEnvironmentVariable("MITCAD_BROWSER_COMPONENTS_DIR");
  if (!given.isEmpty()) {
    return QDir::cleanPath(given);
  }
  return QDir(QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation))
      .filePath(QStringLiteral("browser"));
}

std::optional<BrowserComponentInfo> readBrowserComponent(const QString& folder, QString* problem) {
  QFile file(QDir(folder).filePath(kComponentFile));
  if (!file.open(QIODevice::ReadOnly)) {
    *problem = QStringLiteral("%1 has no %2").arg(QDir::toNativeSeparators(folder), kComponentFile);
    return std::nullopt;
  }
  const QJsonObject json = QJsonDocument::fromJson(file.readAll()).object();
  BrowserComponentInfo info;
  info.folder = QDir::cleanPath(QFileInfo(folder).absoluteFilePath());
  info.abi = json.value(QStringLiteral("abi")).toInt();
  info.version = json.value(QStringLiteral("version")).toString();
  info.cef = json.value(QStringLiteral("cef")).toString();
  info.platform = json.value(QStringLiteral("platform")).toString();
  info.module = json.value(QStringLiteral("module")).toString();
  for (const QJsonValue& value : json.value(QStringLiteral("executables")).toArray()) {
    info.executables << value.toString();
  }
  if (info.abi <= 0 || info.version.isEmpty() || info.module.isEmpty() || info.module.contains(QLatin1Char('/')) ||
      info.module.contains(QLatin1Char('\\'))) {
    *problem = QStringLiteral("%1 in %2 is not a component's").arg(kComponentFile, QDir::toNativeSeparators(folder));
    return std::nullopt;
  }
  return info;
}

std::optional<BrowserComponentInfo> installedBrowserComponent(const QString& root, QString* problem) {
  const QString build = qEnvironmentVariable("MITCAD_BROWSER_COMPONENT_DIR");
  if (!build.isEmpty()) {
    std::optional<BrowserComponentInfo> info = readBrowserComponent(build, problem);
    if (info) {
      info->fromBuild = true;
    }
    return info;
  }
  QFile current(QDir(root).filePath(kCurrentFile));
  if (!current.open(QIODevice::ReadOnly)) {
    *problem = QStringLiteral("no web browser component is installed in %1").arg(QDir::toNativeSeparators(root));
    return std::nullopt;
  }
  const QString version =
      QJsonDocument::fromJson(current.readAll()).object().value(QStringLiteral("version")).toString();
  if (!safeVersion(version)) {
    *problem = QStringLiteral("%1 names no version").arg(QDir::toNativeSeparators(current.fileName()));
    return std::nullopt;
  }
  std::optional<BrowserComponentInfo> info = readBrowserComponent(QDir(root).filePath(version), problem);
  if (info && info->version != version) {
    *problem = QStringLiteral("the component in %1 is version %2").arg(QDir::toNativeSeparators(info->folder),
                                                                        info->version);
    return std::nullopt;
  }
  return info;
}

bool activateBrowserComponent(const QString& root, const QString& version, QString* error) {
  if (!safeVersion(version) || !QFileInfo(QDir(root).filePath(version)).isDir()) {
    *error = QStringLiteral("no component %1 in %2").arg(version, QDir::toNativeSeparators(root));
    return false;
  }
  QSaveFile current(QDir(root).filePath(kCurrentFile));
  if (!current.open(QIODevice::WriteOnly)) {
    *error = current.errorString();
    return false;
  }
  current.write(QJsonDocument(QJsonObject{{QStringLiteral("version"), version}}).toJson(QJsonDocument::Compact));
  if (!current.commit()) {
    *error = current.errorString();
    return false;
  }
  return true;
}

int removeOldBrowserComponents(const QString& root, const QString& keep) {
  QString problem;
  const std::optional<BrowserComponentInfo> inUse = qEnvironmentVariableIsEmpty("MITCAD_BROWSER_COMPONENT_DIR")
                                                        ? installedBrowserComponent(root, &problem)
                                                        : std::nullopt;
  if (!inUse) {
    return 0; // nothing in use known: nothing is old
  }
  int removed = 0;
  const QDir folder(root);
  for (const QString& name : folder.entryList(QDir::Dirs | QDir::NoDotAndDotDot)) {
    if (name == inUse->version || name == keep || !safeVersion(name)) {
      continue;
    }
    // An unpacking's leftover (`<version>.partial`) too.
    if (QDir(folder.filePath(name)).removeRecursively()) {
      ++removed;
    }
  }
  // A download's leftover.
  for (const QString& name : folder.entryList({QStringLiteral("*.download.zip")}, QDir::Files)) {
    if (QFile::remove(folder.filePath(name))) {
      ++removed;
    }
  }
  return removed;
}

BrowserOffer parseBrowserOffer(const QByteArray& json) {
  BrowserOffer offer;
  const QJsonObject object = QJsonDocument::fromJson(json).object();
  const QString status = object.value(QStringLiteral("status")).toString();
  offer.status = status == QStringLiteral("available") ? BrowserOffer::Status::Available
                 : status == QStringLiteral("current") ? BrowserOffer::Status::Current
                 : status == QStringLiteral("abi")     ? BrowserOffer::Status::OtherInterface
                 : status == QStringLiteral("none")    ? BrowserOffer::Status::None
                                                       : BrowserOffer::Status::Invalid;
  offer.expired = object.value(QStringLiteral("expired")).toBool();
  const QJsonObject package = object.value(QStringLiteral("package")).toObject();
  offer.version = package.value(QStringLiteral("version")).toString();
  offer.cef = package.value(QStringLiteral("cef")).toString();
  offer.url = package.value(QStringLiteral("url")).toString();
  offer.sha256 = package.value(QStringLiteral("sha256")).toString();
  offer.size = static_cast<qint64>(package.value(QStringLiteral("size")).toDouble());
  for (const QJsonValue& value : object.value(QStringLiteral("abis")).toArray()) {
    offer.abis << value.toInt();
  }
  if ((offer.status == BrowserOffer::Status::Available || offer.status == BrowserOffer::Status::Current) &&
      (!safeVersion(offer.version) || offer.size <= 0 || !offer.url.startsWith(QStringLiteral("https://")))) {
    offer.status = BrowserOffer::Status::Invalid;
  }
  return offer;
}

bool checkUnpackedBrowserComponent(const BrowserComponentInfo& unpacked, const BrowserOffer& offer, int abi,
                                   QString* error) {
  if (unpacked.abi != abi) {
    *error = browserInterfaceMessage(abi, unpacked.abi);
    return false;
  }
  if (unpacked.version != offer.version) {
    *error = Text::tr("The downloaded web browser component is version %1, not %2.").arg(unpacked.version,
                                                                                          offer.version);
    return false;
  }
  if (!QFileInfo::exists(QDir(unpacked.folder).filePath(unpacked.module))) {
    *error = Text::tr("The downloaded web browser component has no %1.").arg(unpacked.module);
    return false;
  }
  return true;
}

QString browserInterfaceMessage(int abi, int installed) {
  return Text::tr("The installed web browser component is for another version of MitCad (interface %1; this "
                  "MitCad needs %2). MitCad downloads the matching one.")
      .arg(installed)
      .arg(abi);
}

QString browserPublishedInterfaceMessage(int abi, const QList<int>& published) {
  QStringList numbers;
  for (const int number : published) {
    numbers << QString::number(number);
  }
  return Text::tr("No web browser component for this version of MitCad is published (it needs interface %1, "
                  "the published ones are for %2). Update MitCad to use Component from Web.")
      .arg(abi)
      .arg(numbers.join(QStringLiteral(", ")));
}

QString browserDownloadSize(qint64 bytes) { return QLocale().formattedDataSize(bytes, 1); }

} // namespace mitcad
