// SPDX-License-Identifier: MIT
#pragma once

#include <cstdint>
#include <memory>
#include <vector>

#include <QByteArray>
#include <QJsonObject>
#include <QObject>
#include <QString>

#include "render/RenderProtocol.hpp"
#include "render/RenderWorkerChoice.hpp"
#include "platform/MacChrome.hpp"
#ifdef __APPLE__
#include "render/MacFrameSurface.hpp"
#endif

class QProcess;

namespace mitcad::render {

class FrameMemory;
class FrameChannel;
#ifdef __APPLE__
class MacRenderClient;
#endif

// A copied frame on POSIX/CLI, or a retained private Mac surface allocation.
struct Frame {
  FrameInfo info;
  std::vector<std::uint16_t> pixels; // info.width x info.height RGBA half floats
  // The ground's catcher factors (as pixels; empty: none, mitcad#54).
  std::vector<std::uint16_t> catcher;
#ifdef __APPLE__
  std::shared_ptr<MacFrameLease> surface;
#endif
  const std::uint16_t* lightPixels() const {
#ifdef __APPLE__
    if (surface) return surface->pixels();
#endif
    return pixels.empty() ? nullptr : pixels.data();
  }
  const std::uint16_t* catcherPixels() const {
#ifdef __APPLE__
    if (surface) return info.catcher != 0 ? surface->pixels(1) : nullptr;
#endif
    return catcher.empty() ? nullptr : catcher.data();
  }
  bool empty() const { return lightPixels() == nullptr; }
};

// The application's end of the render worker (the executable mitcad-render,
// RenderProtocol.hpp): starts the process, checks its protocol version,
// owns the shared memory its frames arrive in (FrameMemory), sends
// commands and reports frames. A crash of the worker is reported, never
// passed on.
class RenderClient : public QObject {
  Q_OBJECT

public:
  explicit RenderClient(QObject* parent = nullptr);
  ~RenderClient() override;

  // The worker's executable: MITCAD_RENDER_WORKER (tests), else the one
  // picked in Preferences (mitcad#1332) while it is there and has not
  // failed in this run, else mitcad-render next to the application's
  // executable. Empty when there is none: a build or package without the
  // renderer.
  static QString workerPath() { return workerChoice().path; }
  // The same with whether it is the picked one, and why a picked one is
  // not used (logged once).
  static WorkerChoice workerChoice();

  // The render device the next start asks for (mitcad#50): "auto", "cpu"
  // or a device's id; the worker gets it as its first command.
  void setDevice(const QString& device) { m_device = device; }
  // The device the worker renders on (its "ready", then any "device").
  QString deviceLabel() const { return m_deviceLabel; }

  void start();
  // Asks the worker to stop and waits a moment for it, then ends it.
  void stop();
  bool isRunning() const;
  qint64 processId() const;
  // The last worker was of another protocol version: starting it again
  // does not help.
  bool incompatible() const { return m_incompatible; }

  // Makes sure the shared memory holds frames of this size (device
  // pixels), with room to grow; a larger one replaces it.
  void reserve(int width, int height);
  void send(const QJsonObject& command);

  // The newest frame the worker published, if it is newer than the last
  // one read.
  bool takeFrame(Frame& frame);

signals:
  void ready(const QString& renderer);
  // The worker applied a "scene" command (its "scene" event).
  void sceneApplied(const QJsonObject& event);
  // A frame is waiting (takeFrame).
  void frameAvailable();
  void finished(std::uint64_t view, int samples, double seconds);
  void message(const QString& text);
  // The CPU took over from the device that was asked for or failed.
  void deviceFallback(const QString& text);
  // The worker ended without being asked to: a crash, it could not start,
  // or it speaks another protocol version (incompatible()).
  void failed(const QString& reason);
  // The picked worker is not there, did not start or speaks another
  // protocol version: the shipped one starts instead (mitcad#1332). For
  // the status bar.
  void workerFallback(const QString& text);

private:
  void readOutput();
  void handle(const QJsonObject& event);
  // A picked worker that ended before its hello, of `reason`: the shipped
  // one starts instead; false when there is none to fall back on.
  bool fallBack(const QString& reason);

  QProcess* m_process = nullptr;
#ifdef __APPLE__
  std::unique_ptr<MacRenderClient> m_native;
#endif
  mac::ProcessActivity m_activity;
  std::unique_ptr<FrameMemory> m_memory;
#ifndef _WIN32
  std::unique_ptr<FrameChannel> m_channel; // carries the memory to the worker
#endif
  int m_memoryId = 0;
  QByteArray m_pending;
  QString m_lastError;
  QString m_device = QStringLiteral("auto");
  QString m_deviceLabel;
  bool m_stopping = false;
  bool m_frameWaiting = false;
  bool m_greeted = false; // the worker's "hello" had this protocol version
  bool m_incompatible = false;
  QString m_program; // the worker started last
  bool m_picked = false; // it is the one picked in Preferences
  bool m_restartPending = false; // the shipped worker starts after a picked one failed
};

} // namespace mitcad::render
