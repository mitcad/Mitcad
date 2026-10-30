// SPDX-License-Identifier: MIT
// 3D mice on Linux and the BSDs (mitcad#951): the spacenavd daemon's own
// socket (/var/run/spnav.sock, or SPNAV_SOCKET), read in the daemon's
// original protocol, which it speaks to every client that asks for no
// other, so that no library is needed (SpaceMouseDevice.hpp parses the
// events). Without the daemon nothing fails: the backend tries again every
// 5 s, and finds a daemon started or restarted later. The daemon sends its
// events to every client: they count only while Mitcad is the active
// application.

#include "../SpaceMouseDevice.hpp"

#include <array>
#include <cerrno>
#include <cstdint>
#include <cstring>
#include <memory>

#include <QByteArray>
#include <QDebug>
#include <QFile>
#include <QGuiApplication>
#include <QSocketNotifier>
#include <QTimer>
#include <QtGlobal>

#include <fcntl.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

namespace mitcad {
namespace {

QString socketPath() {
  const QByteArray configured = qgetenv("SPNAV_SOCKET");
  return configured.isEmpty() ? QStringLiteral("/var/run/spnav.sock") : QString::fromLocal8Bit(configured);
}

class SpnavBackend final : public SpaceMouseBackend {
public:
  SpnavBackend() {
    m_retry.setInterval(5000);
    QObject::connect(&m_retry, &QTimer::timeout, &m_retry, [this] {
      if (connectDaemon() && connection) {
        connection(true, m_path);
      }
    });
  }
  ~SpnavBackend() override {
    m_notifier.reset();
    if (m_fd >= 0) {
      ::close(m_fd);
    }
  }

  QString name() const override { return QStringLiteral("spacenavd"); }

  bool start(QString& device) override {
    if (connectDaemon()) {
      device = m_path;
      return true;
    }
    m_retry.start();
    return false;
  }

private:
  bool connectDaemon() {
    const QByteArray path = QFile::encodeName(socketPath());
    sockaddr_un address{};
    address.sun_family = AF_UNIX;
    if (path.isEmpty() || static_cast<std::size_t>(path.size()) >= sizeof(address.sun_path)) {
      return false;
    }
    std::memcpy(address.sun_path, path.constData(), static_cast<std::size_t>(path.size()));
    const int fd = ::socket(AF_UNIX, SOCK_STREAM, 0);
    if (fd < 0) {
      return false;
    }
    if (::connect(fd, reinterpret_cast<const sockaddr*>(&address), sizeof(address)) != 0) {
      ::close(fd);
      return false;
    }
    ::fcntl(fd, F_SETFD, FD_CLOEXEC);
    ::fcntl(fd, F_SETFL, ::fcntl(fd, F_GETFL) | O_NONBLOCK);
    m_fd = fd;
    m_path = QString::fromLocal8Bit(path);
    m_pending.clear();
    m_notifier = std::make_unique<QSocketNotifier>(static_cast<qintptr>(fd), QSocketNotifier::Read);
    QObject::connect(m_notifier.get(), &QSocketNotifier::activated, m_notifier.get(), [this] { readEvents(); });
    m_retry.stop();
    return true;
  }

  void readEvents() {
    char buffer[512];
    for (;;) {
      const ssize_t got = ::read(m_fd, buffer, sizeof(buffer));
      if (got > 0) {
        m_pending.append(buffer, static_cast<int>(got));
        continue;
      }
      if (got < 0 && errno == EINTR) {
        continue;
      }
      if (got < 0 && (errno == EAGAIN || errno == EWOULDBLOCK)) {
        break;
      }
      lost(); // closed by the daemon, or broken
      return;
    }
    constexpr int kEventBytes = 8 * 4;
    while (m_pending.size() >= kEventBytes) {
      std::array<std::int32_t, 8> data{};
      std::memcpy(data.data(), m_pending.constData(), kEventBytes);
      m_pending.remove(0, kEventBytes);
      handle(parseSpnavEvent(data));
    }
  }

  void handle(const SpnavEvent& event) {
    const bool active = QGuiApplication::applicationState() == Qt::ApplicationActive;
    switch (event.kind) {
    case SpnavEvent::Kind::Motion:
      if (!motion) {
        break;
      }
      if (active) {
        const SpaceMouseAxes axes = spaceMouseFromSpnav(event.motion);
        m_moving = axes != SpaceMouseAxes{};
        motion(axes);
      } else if (m_moving) {
        m_moving = false;
        motion(SpaceMouseAxes{});
      }
      break;
    case SpnavEvent::Kind::Press:
    case SpnavEvent::Kind::Release:
      if (button && event.button >= 0 && (active || event.kind == SpnavEvent::Kind::Release)) {
        button(event.button, event.kind == SpnavEvent::Kind::Press);
      }
      break;
    case SpnavEvent::Kind::Other:
      break;
    }
  }

  void lost() {
    if (m_notifier) {
      // Deleted later: this runs in its signal.
      m_notifier->setEnabled(false);
      m_notifier.release()->deleteLater();
    }
    if (m_fd >= 0) {
      ::close(m_fd);
      m_fd = -1;
    }
    m_pending.clear();
    if (m_moving && motion) {
      m_moving = false;
      motion(SpaceMouseAxes{});
    }
    if (connection) {
      connection(false, m_path);
    }
    m_retry.start();
  }

  int m_fd = -1;
  QString m_path;
  QByteArray m_pending;
  std::unique_ptr<QSocketNotifier> m_notifier;
  QTimer m_retry;
  bool m_moving = false;
};

} // namespace

std::unique_ptr<SpaceMouseBackend> createSpaceMouseBackend() {
  return std::make_unique<SpnavBackend>();
}

} // namespace mitcad
