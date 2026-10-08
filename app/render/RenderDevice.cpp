// SPDX-License-Identifier: MIT
#include "render/RenderDevice.hpp"

#include <QCoreApplication>
#include <QHash>
#include <QJsonArray>
#include <QJsonDocument>
#include <QProcess>
#include <QSettings>

#include "render/RenderProtocol.hpp"

namespace mitcad::render {

namespace {
const char* const kDeviceKey = "render/device";
} // namespace

RenderDevice RenderDevice::fromJson(const QJsonObject& device) {
  RenderDevice entry;
  entry.id = device.value(QStringLiteral("id")).toString();
  entry.type = device.value(QStringLiteral("type")).toString();
  entry.name = device.value(QStringLiteral("name")).toString();
  entry.denoisesOnDevice = device.value(QStringLiteral("denoiser")).toString() == QLatin1String("device");
  return entry;
}

QString RenderDevice::label() const {
  if (type == QLatin1String("CPU")) {
    return name.isEmpty() ? QStringLiteral("CPU") : name;
  }
  return QStringLiteral("%1 (%2)").arg(name, type);
}

QString renderDeviceChoice() {
  const QSettings settings;
  const QString choice = settings.value(QLatin1String(kDeviceKey)).toString();
  return choice.isEmpty() ? QStringLiteral("auto") : choice;
}

void setRenderDeviceChoice(const QString& choice) {
  QSettings settings;
  settings.setValue(QLatin1String(kDeviceKey), choice);
}

QList<RenderDevice> renderDevices(const QString& worker, QString& error) {
  // Per worker: the list is asked for once.
  static QHash<QString, QList<RenderDevice>> known;
  if (const auto found = known.constFind(worker); found != known.constEnd()) {
    return *found;
  }
  if (worker.isEmpty()) {
    error = QCoreApplication::translate("RenderDevice", "there is no render worker");
    return {};
  }
  QProcess process;
  process.start(worker, {QStringLiteral("--list-devices")});
  // Finding the GPUs loads their drivers, which can take a moment.
  if (!process.waitForFinished(20000) || process.exitStatus() != QProcess::NormalExit ||
      process.exitCode() != 0) {
    process.kill();
    error = QCoreApplication::translate("RenderDevice", "the render worker did not list its devices");
    return {};
  }
  const QJsonObject answer = QJsonDocument::fromJson(process.readAllStandardOutput().trimmed()).object();
  if (answer.value(QStringLiteral("protocol")).toInt() != static_cast<int>(kProtocolVersion)) {
    error = QCoreApplication::translate("RenderDevice", "the render worker does not match this Mitcad");
    return {};
  }
  QList<RenderDevice> devices;
  for (const QJsonValue& device : answer.value(QStringLiteral("devices")).toArray()) {
    devices.append(RenderDevice::fromJson(device.toObject()));
  }
  known.insert(worker, devices);
  return devices;
}

} // namespace mitcad::render
