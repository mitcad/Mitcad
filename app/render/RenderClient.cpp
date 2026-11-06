// SPDX-License-Identifier: MIT
#include "render/RenderClient.hpp"

#include <algorithm>
#include <cstring>
#include <new>

#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QProcess>
#include <QtLogging>

#include "render/FrameMemory.hpp"
#ifdef __APPLE__
#include "render/MacRenderClient.hpp"
#endif
#include "render/RenderBatch.hpp"
#include "render/RenderDevice.hpp"
#include "render/RenderWorkerFailure.hpp"
#include "framework/Diagnostics.hpp"

namespace mitcad::render {

RenderClient::RenderClient(QObject* parent) : QObject(parent) {}

RenderClient::~RenderClient() { stop(); }

WorkerChoice RenderClient::workerChoice() {
  const WorkerChoice choice = chooseWorker(qEnvironmentVariable("MITCAD_RENDER_WORKER"), pickedWorker(),
                                           failedPickedWorker(), shippedWorkerExecutable());
  // A picked worker that is not there: said once per run.
  static QString logged;
  if (!choice.fallback.isEmpty() && choice.fallback != logged) {
    logged = choice.fallback;
    qWarning().noquote() << QStringLiteral("Render worker fallback: %1").arg(choice.fallback);
  }
  return choice;
}

bool RenderClient::fallBack(const QString& reason) {
  if (!m_picked || m_greeted || m_stopping) {
    return false;
  }
  m_picked = false;
  const QString text = pickedWorkerFailed(m_program, reason);
  if (workerChoice().path.isEmpty()) {
    return false; // reported as the picked worker's failure
  }
  emit workerFallback(text);
  m_restartPending = true;
  QMetaObject::invokeMethod(
      this,
      [this] {
        if (m_restartPending) {
          m_restartPending = false;
          start();
        }
      },
      Qt::QueuedConnection);
  return true;
}

void RenderClient::start() {
  if (isRunning()) {
    return;
  }
  m_stopping = false;
  m_greeted = false;
  m_incompatible = false;
  m_lastError.clear();
  m_pending.clear();
  m_memory.reset();
  m_frameWaiting = false;
  m_restartPending = false;
  const WorkerChoice choice = workerChoice();
  m_picked = choice.picked;
  m_program = choice.path;
  if (!choice.fallback.isEmpty()) {
    // A picked worker that is not there (mitcad#1332): the status bar says
    // so once per run.
    static QString reported;
    if (choice.fallback != reported) {
      reported = choice.fallback;
      const QString text = choice.fallback;
      QMetaObject::invokeMethod(this, [this, text] { emit workerFallback(text); }, Qt::QueuedConnection);
    }
  }
#ifdef __APPLE__
  m_native.reset();
  // Explicit worker overrides and a picked worker keep their existing POSIX
  // process semantics.
  if (qEnvironmentVariableIsEmpty("MITCAD_RENDER_WORKER") && !m_picked) {
    m_native = std::make_unique<MacRenderClient>(this);
    m_activity.start("Rendering the view");
    if (!m_native->start([this](const QJsonObject& event) { handle(event); }, [this](const QString& reason) {
          m_activity.stop();
          qWarning().noquote() << QStringLiteral("Render worker stopped: %1").arg(reason);
          if (!m_stopping) emit failed(reason);
        })) {
      m_activity.stop();
      QMetaObject::invokeMethod(this, [this] { emit failed(tr("the render service could not start")); }, Qt::QueuedConnection);
    }
    return;
  }
#endif
  const QString program = m_program;
  QStringList arguments;
#ifndef _WIN32
  // The socket the frame memory's file descriptors go through.
  std::string channelError;
  auto channel = std::make_unique<FrameChannel>();
  if (!channel->open(channelError)) {
    m_lastError = tr("the render process could not start: %1").arg(QString::fromStdString(channelError));
  }
  m_channel = std::move(channel);
  arguments << QStringLiteral("--frame-channel") << QString::number(m_channel->workerEnd());
#endif
  m_process = new QProcess(this);
  // The worker's stderr (Cycles' own messages) goes to the application's log.
  m_process->setProcessChannelMode(QProcess::SeparateChannels);
  connect(m_process, &QProcess::readyReadStandardOutput, this, &RenderClient::readOutput);
  connect(m_process, &QProcess::readyReadStandardError, this, [this] {
    const QByteArray text = m_process->readAllStandardError();
    for (const QByteArray& line : text.split('\n')) {
      if (!line.trimmed().isEmpty()) {
        qDebug().noquote() << QStringLiteral("Render worker: %1").arg(QString::fromUtf8(line.trimmed()));
      }
    }
  });
  QProcess* process = m_process;
  connect(m_process, &QProcess::finished, this, [this, process](int exitCode, QProcess::ExitStatus status) {
    if (process != m_process) {
      return;
    }
    readOutput();
    const bool asked = m_stopping;
    m_process->deleteLater();
    m_process = nullptr;
    m_activity.stop();
    m_memory.reset();
#ifndef _WIN32
    m_channel.reset();
#endif
    if (asked) {
      qInfo().noquote() << QStringLiteral("Render worker ended");
      return;
    }
    const QString reason = workerExitReason(m_lastError, exitCode, status == QProcess::CrashExit, m_incompatible);
    qWarning().noquote() << QStringLiteral("Render worker stopped: %1").arg(reason);
    if (fallBack(reason)) {
      return;
    }
    emit failed(reason);
  });
  connect(m_process, &QProcess::errorOccurred, this, [this, process](QProcess::ProcessError error) {
    if (process != m_process || error != QProcess::FailedToStart) {
      return;
    }
    // No "finished" follows.
    m_lastError = tr("the render process could not start: %1").arg(m_process->errorString());
    m_process->deleteLater();
    m_process = nullptr;
    m_activity.stop();
    m_memory.reset();
#ifndef _WIN32
    m_channel.reset();
#endif
    qWarning().noquote() << QStringLiteral("Render worker stopped: %1").arg(m_lastError);
    if (fallBack(m_lastError)) {
      return;
    }
    emit failed(m_lastError);
  });
#ifndef _WIN32
  // Only the worker inherits its end of the socket (both close on exec).
  const int workerEnd = m_channel->workerEnd();
  m_process->setChildProcessModifier([workerEnd] { FrameChannel::makeInheritable(workerEnd); });
#endif
  if (program.isEmpty()) {
    m_lastError = tr("there is no %1 next to MitCad").arg(QString::fromLatin1(kWorkerName));
  }
  if (!m_lastError.isEmpty()) {
    // Reported like a worker that ended at once.
    const QString reason = m_lastError;
    m_process->deleteLater();
    m_process = nullptr;
    qWarning().noquote() << QStringLiteral("Render worker stopped: %1").arg(reason);
    QMetaObject::invokeMethod(this, [this, reason] { emit failed(reason); }, Qt::QueuedConnection);
    return;
  }
  m_activity.start("Rendering the view");
  m_process->start(program, arguments);
#ifndef _WIN32
  if (m_channel) { // gone when the process could not start
    m_channel->closeWorkerEnd();
  }
#endif
  if (m_process != nullptr) {
    qInfo().noquote()
        << QStringLiteral("Render worker started (pid %1): %2").arg(m_process->processId()).arg(program);
  }
}

void RenderClient::stop() {
  m_restartPending = false;
  if (!isRunning()) {
    return;
  }
  m_stopping = true;
#ifdef __APPLE__
  if (m_native) { m_native->stop(); m_activity.stop(); qInfo().noquote() << QStringLiteral("Render worker ended"); return; }
#endif
  m_process->write("{\"cmd\":\"stop\"}\n");
  m_process->closeWriteChannel();
  // Cycles stops between samples; a frame of a large view can take a while.
  if (!m_process->waitForFinished(3000)) {
    m_process->kill();
    m_process->waitForFinished(1000);
  }
}

bool RenderClient::isRunning() const {
#ifdef __APPLE__
  if (m_native) return m_native->isRunning();
#endif
  return m_process != nullptr && m_process->state() != QProcess::NotRunning;
}

qint64 RenderClient::processId() const {
#ifdef __APPLE__
  if (m_native) return m_native->processId();
#endif
  return m_process != nullptr ? m_process->processId() : 0;
}

void RenderClient::reserve(int width, int height) {
#ifdef __APPLE__
  if (m_native) { m_native->reserve(width, height); return; }
#endif
  if (!isRunning() || width <= 0 || height <= 0) {
    return;
  }
  if (m_memory) {
    const auto* header = static_cast<const FrameHeader*>(m_memory->data());
    if (static_cast<std::uint32_t>(width) <= header->capacityWidth &&
        static_cast<std::uint32_t>(height) <= header->capacityHeight) {
      return;
    }
  }
  // Room for the view to grow a little before the memory is replaced.
  const auto capacityWidth = static_cast<std::uint32_t>(width + width / 4);
  const auto capacityHeight = static_cast<std::uint32_t>(height + height / 4);
  auto memory = std::make_unique<FrameMemory>();
  std::string error;
  if (!memory->create(segmentBytes(capacityWidth, capacityHeight), error)) {
    qWarning().noquote() << QStringLiteral("Render frame memory of %1 x %2 not created: %3")
                                .arg(capacityWidth)
                                .arg(capacityHeight)
                                .arg(QString::fromStdString(error));
    return;
  }
  auto* header = new (memory->data()) FrameHeader();
  header->capacityWidth = capacityWidth;
  header->capacityHeight = capacityHeight;
  ++m_memoryId;
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("memory")}, {QStringLiteral("id"), m_memoryId}};
#ifdef _WIN32
  command.insert(QStringLiteral("key"), QString::fromStdString(memory->key()));
#else
  // The descriptor first: the worker takes it from the socket when the
  // command comes.
  if (!m_channel || !m_channel->send(memory->descriptor(), error)) {
    qWarning().noquote() << QStringLiteral("Render frame memory not passed on: %1").arg(QString::fromStdString(error));
    return;
  }
#endif
  // The worker detaches from the old memory when it attaches to the new one.
  m_memory = std::move(memory);
  m_frameWaiting = false;
  send(command);
  qDebug().noquote()
      << QStringLiteral("Render frame memory %1: %2 x %3").arg(m_memoryId).arg(capacityWidth).arg(capacityHeight);
}

void RenderClient::send(const QJsonObject& command) {
#ifdef __APPLE__
  if (m_native) { m_native->send(command); return; }
#endif
  if (!isRunning()) {
    return;
  }
  m_process->write(QJsonDocument(command).toJson(QJsonDocument::Compact) + '\n');
}

bool RenderClient::takeFrame(Frame& frame) {
#ifdef __APPLE__
  if (m_native) { m_frameWaiting = false; return m_native->takeFrame(frame); }
#endif
  if (!m_memory || !m_frameWaiting) {
    return false;
  }
  m_frameWaiting = false;
  auto* header = static_cast<FrameHeader*>(m_memory->data());
  // Mark the published slot as being read, then check that it still is the
  // published one: the worker never writes into a slot marked so.
  std::uint32_t slot = kNoSlot;
  for (int attempt = 0; attempt < 8; ++attempt) {
    slot = header->published.load();
    if (slot == kNoSlot) {
      return false;
    }
    header->reading.store(slot);
    if (header->published.load() == slot) {
      break;
    }
    slot = kNoSlot;
  }
  if (slot == kNoSlot || slot >= kFrameSlots) {
    header->reading.store(kNoSlot);
    return false;
  }
  frame.info = header->frames[slot];
#ifdef __APPLE__
  frame.surface.reset();
#endif
  const std::size_t count = std::size_t(frame.info.width) * frame.info.height * 4;
  QString detail;
  if (timingLogged()) {
    detail = QStringLiteral("view %1 samples %2 %3x%4 denoised %5 planes %6 bytes %7")
                 .arg(frame.info.view).arg(frame.info.samples).arg(frame.info.width).arg(frame.info.height)
                 .arg(frame.info.denoised).arg(frame.info.catcher != 0 ? 2 : 1)
                 .arg(static_cast<qulonglong>(count * sizeof(std::uint16_t) * (frame.info.catcher != 0 ? 2 : 1)));
  }
  ScopedTiming allocation("render allocate");
  frame.pixels.resize(count);
  frame.catcher.clear();
  if (frame.info.catcher != 0) {
    frame.catcher.resize(count);
  }
  allocation.setDetail(detail);
  allocation.finish();
  ScopedTiming copy("render copy");
  std::memcpy(frame.pixels.data(), slotPixels(header, slot), count * sizeof(std::uint16_t));
  if (frame.info.catcher != 0) {
    std::memcpy(frame.catcher.data(), slotPixels(header, slot, 1), count * sizeof(std::uint16_t));
  }
  header->reading.store(kNoSlot);
  copy.setDetail(detail);
  return frame.info.width > 0 && frame.info.height > 0;
}

void RenderClient::readOutput() {
  if (m_process == nullptr) {
    return;
  }
  m_pending += m_process->readAllStandardOutput();
  qsizetype end = 0;
  while ((end = m_pending.indexOf('\n')) >= 0) {
    const QByteArray line = m_pending.left(end).trimmed();
    m_pending.remove(0, end + 1);
    if (line.isEmpty()) {
      continue;
    }
    const QJsonDocument document = QJsonDocument::fromJson(line);
    if (!document.isObject()) {
      qDebug().noquote() << QStringLiteral("Render worker: %1").arg(QString::fromUtf8(line));
      continue;
    }
    handle(document.object());
  }
}

void RenderClient::handle(const QJsonObject& event) {
  if (m_incompatible) {
    return;
  }
  const QString name = event.value(QStringLiteral("event")).toString();
  // The first event says which protocol the worker speaks; nothing goes to
  // a worker of another version.
  if (!m_greeted) {
    const int version = event.value(QStringLiteral("protocol")).toInt(0);
    if (name == QLatin1String("hello") && version == static_cast<int>(kProtocolVersion)) {
      m_greeted = true;
#ifdef __APPLE__
      if (m_native) qInfo().noquote() << QStringLiteral("Render worker started (pid %1): embedded XPC renderer").arg(m_native->processId());
#endif
      // The first command chooses the device the renderer starts on.
      send({{QStringLiteral("cmd"), QStringLiteral("device")}, {QStringLiteral("id"), m_device}});
      return;
    }
    if (name == QLatin1String("error") || name == QLatin1String("status")) {
      // Reported below (a worker that cannot start at all).
    } else {
      m_incompatible = true;
      m_lastError = tr("%1 does not match this MitCad (its protocol version is %2, MitCad's is %3); "
                       "install them together")
                        .arg(QString::fromLatin1(kWorkerName))
                        .arg(version)
                        .arg(kProtocolVersion);
      qWarning().noquote() << QStringLiteral("Render worker error: %1").arg(m_lastError);
#ifdef __APPLE__
      if (m_native) m_native->stop();
      else
#endif
      m_process->kill();
      return;
    }
  }
  if (name == QLatin1String("transport")) {
    if (event.value(QStringLiteral("kind")).toString() == QLatin1String("metal-buffer"))
      qInfo().noquote() << QStringLiteral("Render frame transport: Metal buffer interop");
    else if (event.value(QStringLiteral("kind")).toString() == QLatin1String("native-auxiliary"))
      qInfo().noquote() << QStringLiteral("Render frame transport: native auxiliary %1, composite %2")
          .arg(event.value(QStringLiteral("graphics")).toBool() ? QStringLiteral("GPU") : QStringLiteral("CPU"))
          .arg(event.value(QStringLiteral("composite")).toBool() ? 1 : 0);
  } else if (name == QLatin1String("frame")) {
#ifdef __APPLE__
    if (m_native) {
      if (!m_frameWaiting) { m_frameWaiting = true; emit frameAvailable(); }
      return;
    }
#endif
    // Frames published in a memory the application has replaced are gone.
    if (event.value(QStringLiteral("memory")).toInt(m_memoryId) == m_memoryId && !m_frameWaiting) {
      m_frameWaiting = true;
      emit frameAvailable();
    }
  } else if (name == QLatin1String("scene")) {
    emit sceneApplied(event);
  } else if (name == QLatin1String("ready")) {
    const RenderDevice device = RenderDevice::fromJson(event.value(QStringLiteral("device")).toObject());
    m_deviceLabel = device.label();
    QStringList devices;
    for (const QJsonValue& value : event.value(QStringLiteral("devices")).toArray()) {
      devices << RenderDevice::fromJson(value.toObject()).label();
    }
    qInfo().noquote() << QStringLiteral("Render device: %1 (asked for %2); devices: %3")
                             .arg(m_deviceLabel, m_device, devices.join(QStringLiteral(", ")));
    qInfo().noquote() << QStringLiteral("Render diagnostics: %1").arg(device.diagnostics());
    emit ready(event.value(QStringLiteral("renderer")).toString());
  } else if (name == QLatin1String("denoiser")) {
    const RenderDevice device = RenderDevice::fromJson(event.value(QStringLiteral("device")).toObject());
    qInfo().noquote() << QStringLiteral("Render denoising diagnostics: %1").arg(device.diagnostics());
  } else if (name == QLatin1String("device")) {
    // The CPU took over (mitcad#50): from a device that is not there or
    // failed.
    const RenderDevice device = RenderDevice::fromJson(event.value(QStringLiteral("device")).toObject());
    m_deviceLabel = device.label();
    qInfo().noquote() << QStringLiteral("Render diagnostics after fallback: %1").arg(device.diagnostics());
    const QString text = event.value(QStringLiteral("message")).toString();
    qWarning().noquote() << QStringLiteral("Render device fallback: %1").arg(text);
    emit deviceFallback(text);
  } else if (name == QLatin1String("done")) {
    emit finished(static_cast<std::uint64_t>(event.value(QStringLiteral("view")).toInteger()),
                  event.value(QStringLiteral("samples")).toInt(), event.value(QStringLiteral("seconds")).toDouble());
  } else if (name == QLatin1String("error")) {
    m_lastError = event.value(QStringLiteral("message")).toString();
    qWarning().noquote() << QStringLiteral("Render worker error: %1").arg(m_lastError);
    emit message(m_lastError);
  } else if (name == QLatin1String("warning")) {
    const QString text = event.value(QStringLiteral("message")).toString();
    qWarning().noquote() << QStringLiteral("Render worker warning: %1").arg(text);
    emit message(text);
  } else if (name == QLatin1String("status")) {
    qDebug().noquote() << QStringLiteral("Render worker: %1").arg(event.value(QStringLiteral("text")).toString());
  }
}

} // namespace mitcad::render
