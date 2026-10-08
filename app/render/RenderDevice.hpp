// SPDX-License-Identifier: MIT
#pragma once

// The render device as the application and mitcad-cli see it (mitcad#50,
// docs/rendering.md "Devices"): the per-machine choice in the user's
// settings (Preferences, Display: render/device) and the devices the
// render worker finds (mitcad-render --list-devices). Qt Core only.

#include <QJsonObject>
#include <QList>
#include <QString>

namespace mitcad::render {

struct RenderDevice {
  QString id;   // "CPU", or the renderer's id of a GPU
  QString type; // CPU, CUDA, OPTIX, HIP, METAL, ONEAPI
  QString name;
  bool denoisesOnDevice = false;

  static RenderDevice fromJson(const QJsonObject& device);
  // "NVIDIA GeForce ... (CUDA)", or the CPU's name.
  QString label() const;
};

// The choice in the settings: "auto" (the default: the best GPU, else the
// CPU), "cpu" or a device's id.
QString renderDeviceChoice();
void setRenderDeviceChoice(const QString& choice);

// The devices of the worker (its --list-devices, at most once per run of
// the application: they do not change while it runs); empty, with
// `error`, when it cannot tell.
QList<RenderDevice> renderDevices(const QString& worker, QString& error);

} // namespace mitcad::render
