// SPDX-License-Identifier: MIT
#include "LocalEndpoint.hpp"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QRegularExpression>
#include <QSaveFile>
#include <QStandardPaths>

#ifndef Q_OS_WIN
#include <cerrno>

#include <signal.h>
#include <sys/types.h>
#include <unistd.h>
#else
#include "WindowsEndpointSecurity.hpp"
#endif

namespace mitcad::mcp {
namespace {

bool fail(QString* error, const QString& message) {
  if (error) {
    *error = message;
  }
  return false;
}

bool privatePath(const QFileInfo& info) {
  if (!info.exists() || info.isSymLink()) {
    return false;
  }
#ifndef Q_OS_WIN
  const auto publicAccess = QFile::ReadGroup | QFile::WriteGroup | QFile::ExeGroup | QFile::ReadOther |
                            QFile::WriteOther | QFile::ExeOther;
  return info.ownerId() == static_cast<uint>(getuid()) && !(info.permissions() & publicAccess);
#else
  return privateWindowsEndpointPath(info.absoluteFilePath());
#endif
}

QString endpointPath(const Endpoint& endpoint) {
  return QDir(discoveryDirectory()).filePath(endpoint.instance + QStringLiteral(".json"));
}

bool hex(const QString& value, int length) {
  static const QRegularExpression expression(QStringLiteral("^[0-9a-f]+$"));
  return value.size() == length && expression.match(value).hasMatch();
}

} // namespace

QString discoveryDirectory() {
  const QString override = qEnvironmentVariable("MITCAD_MCP_DISCOVERY_DIR");
  if (!override.isEmpty()) {
    return QDir::cleanPath(override);
  }
  return QDir(QStandardPaths::writableLocation(QStandardPaths::GenericCacheLocation))
      .filePath(QStringLiteral("Mitcad/mcp"));
}

bool prepareDiscoveryDirectory(QString* error) {
  const QString path = discoveryDirectory();
  if (!QDir::isAbsolutePath(path)) {
    return fail(error, QStringLiteral("MCP discovery directory must be absolute"));
  }
  const QFileInfo existing(path);
  if (existing.exists() || existing.isSymLink()) {
    if (!existing.isDir() || !privatePath(existing)) {
      return fail(error, QStringLiteral("MCP discovery directory must be a private directory owned by this user"));
    }
    return true;
  }
  if (!QDir().mkpath(path)) {
    return fail(error, QStringLiteral("Could not create the MCP discovery directory"));
  }
#ifdef Q_OS_WIN
  const bool secured = secureWindowsEndpointPath(path, true);
#else
  const bool secured = QFile::setPermissions(path, QFile::ReadOwner | QFile::WriteOwner | QFile::ExeOwner);
#endif
  if (!secured || !privatePath(QFileInfo(path))) {
    return fail(error, QStringLiteral("Could not create a private MCP discovery directory"));
  }
  return true;
}

bool publishEndpoint(const Endpoint& endpoint, QString* error) {
  if (!prepareDiscoveryDirectory(error)) {
    return false;
  }
  QSaveFile file(endpointPath(endpoint));
  file.setDirectWriteFallback(false);
  if (!file.open(QIODevice::WriteOnly) || !file.setPermissions(QFile::ReadOwner | QFile::WriteOwner)) {
    return fail(error, QStringLiteral("Could not write the private MCP discovery record"));
  }
  const QByteArray bytes = QJsonDocument(QJsonObject{{QStringLiteral("protocol"), 1},
                                                  {QStringLiteral("host"), QStringLiteral("127.0.0.1")},
                                                  {QStringLiteral("instance"), endpoint.instance},
                                                  {QStringLiteral("pid"), endpoint.pid},
                                                  {QStringLiteral("port"), endpoint.port},
                                                  {QStringLiteral("token"), endpoint.token}})
                               .toJson(QJsonDocument::Compact);
  if (file.write(bytes) != bytes.size() || !file.commit()) {
    return fail(error, QStringLiteral("Could not publish the MCP discovery record"));
  }
#ifdef Q_OS_WIN
  if (!secureWindowsEndpointPath(endpointPath(endpoint), false)) {
    QFile::remove(endpointPath(endpoint));
    return fail(error, QStringLiteral("Could not secure the MCP discovery record"));
  }
#endif
  if (!privatePath(QFileInfo(endpointPath(endpoint)))) {
    QFile::remove(endpointPath(endpoint));
    return fail(error, QStringLiteral("The MCP discovery record is not private to this user"));
  }
  return true;
}

void removeEndpoint(const Endpoint& endpoint) {
  // Do not remove another process's replacement record.
  QFile file(endpointPath(endpoint));
  if (!privatePath(QFileInfo(file)) || !file.open(QIODevice::ReadOnly) || file.size() > 16384) {
    return;
  }
  const auto object = QJsonDocument::fromJson(file.readAll()).object();
  file.close();
  if (object.value(QStringLiteral("token")).toString() == endpoint.token) {
    file.remove();
  }
}

QList<Endpoint> discover(QString* error) {
  QList<Endpoint> endpoints;
  const QString directory = discoveryDirectory();
  if (!QDir::isAbsolutePath(directory)) {
    fail(error, QStringLiteral("MCP discovery directory must be absolute"));
    return endpoints;
  }
  const QFileInfo info(directory);
  if (!info.exists() && !info.isSymLink()) {
    return endpoints;
  }
  if (!info.isDir() || !privatePath(info)) {
    fail(error, QStringLiteral("MCP discovery directory is not private to this user"));
    return endpoints;
  }
  const auto entries = QDir(directory).entryInfoList({QStringLiteral("*.json")}, QDir::Files, QDir::Name);
  for (const auto& entry : entries) {
    if (!privatePath(entry) || entry.size() > 16384) {
      continue;
    }
    QFile file(entry.filePath());
    if (!file.open(QIODevice::ReadOnly)) {
      continue;
    }
    const auto object = QJsonDocument::fromJson(file.readAll()).object();
    Endpoint endpoint;
    endpoint.instance = object.value(QStringLiteral("instance")).toString();
    endpoint.token = object.value(QStringLiteral("token")).toString();
    endpoint.pid = object.value(QStringLiteral("pid")).toInteger();
    const int port = object.value(QStringLiteral("port")).toInt();
    if (object.value(QStringLiteral("protocol")).toInt() != 1 ||
        object.value(QStringLiteral("host")).toString() != QStringLiteral("127.0.0.1") ||
        !hex(endpoint.instance, 32) || !hex(endpoint.token, 64) || endpoint.pid <= 0 || port <= 0 || port > 65535 ||
        entry.fileName() != endpoint.instance + QStringLiteral(".json")) {
      continue;
    }
    endpoint.port = static_cast<quint16>(port);
    if (!processRunning(endpoint.pid)) {
      // Left by a killed app: probing its port would only time out (a
      // closed loopback port is retried, not refused, on Windows).
      file.close();
      removeEndpoint(endpoint);
      continue;
    }
    endpoints.append(endpoint);
  }
  return endpoints;
}

QByteArray encodeAuth(const Endpoint& endpoint, const QString& chat) {
  QJsonObject authentication{{QStringLiteral("mitcadAuth"), endpoint.token}};
  if (!chat.isEmpty()) {
    // The chat panel's own bridge connection (mitcad#947).
    authentication.insert(QStringLiteral("mitcadChat"), chat);
  }
  return QJsonDocument(authentication).toJson(QJsonDocument::Compact) + '\n';
}

bool processRunning(qint64 pid) {
#ifdef Q_OS_WIN
  return windowsProcessRunning(pid);
#else
  if (pid <= 0 || static_cast<qint64>(static_cast<pid_t>(pid)) != pid) {
    return false;
  }
  // Signal 0 only checks: EPERM is another user's running process.
  return kill(static_cast<pid_t>(pid), 0) == 0 || errno != ESRCH;
#endif
}

} // namespace mitcad::mcp
