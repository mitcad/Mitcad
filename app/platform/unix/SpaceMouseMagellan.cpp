// SPDX-License-Identifier: MIT
#include "SpaceMouseMagellan.hpp"

#include "../SpaceMouseDriverReports.hpp"

#include <array>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <memory>

#include <QAbstractNativeEventFilter>
#include <QByteArray>
#include <QCoreApplication>
#include <QGuiApplication>
#include <QTimer>
#include <QWindow>

#include <xcb/xcb.h>

namespace mitcad {
namespace {

// The 3dxsrv Magellan protocol uses X11 ClientMessage events and the
// root window's CommandEvent property. It does not expose device names.
// The driver is looked for on the screens' root windows alone: MitCad's own
// window is asked for only once a driver's "Magellan Window" is there, and
// then only if it already exists. Asking for it earlier made the design's
// window native before it sat in its frame, and the Floating cards'
// translucent margins were painted black on every Linux desktop without a
// 3D-mouse daemon (mitcad#2059).
constexpr std::uint16_t kRegisterApplicationWindow = 27695;

template <class Reply> using XReply = std::unique_ptr<Reply, decltype(&std::free)>;

xcb_atom_t serverAtom(xcb_connection_t* x11, const QByteArray& name) {
  const auto query = xcb_intern_atom(x11, 1, static_cast<std::uint16_t>(name.size()), name.constData());
  const XReply<xcb_intern_atom_reply_t> reply(xcb_intern_atom_reply(x11, query, nullptr), &std::free);
  return reply ? reply->atom : xcb_atom_t{XCB_ATOM_NONE};
}

class MagellanBackend final : public SpaceMouseBackend, private QAbstractNativeEventFilter {
public:
  QString name() const override { return QStringLiteral("3DxWare/Magellan X11"); }

  bool start(QString& device) override {
    auto* x11 = qGuiApp->nativeInterface<QNativeInterface::QX11Application>();
    if (!x11 || !nativeWindow) {
      return false;
    }
    m_x11 = x11->connection();
    if (!m_x11) {
      return false;
    }

    QObject::connect(qGuiApp, &QGuiApplication::focusWindowChanged, &m_poll, [this] { focusChanged(); });
    QObject::connect(qGuiApp, &QGuiApplication::applicationStateChanged, &m_poll, [this] { focusChanged(); });
    QObject::connect(&m_poll, &QTimer::timeout, &m_poll, [this] { refresh(true); });
    m_poll.start(5000);
    m_motionTimeout.setSingleShot(true);
    QObject::connect(&m_motionTimeout, &QTimer::timeout, &m_motionTimeout, [this] { stopMotion(); });
    if (refresh(false)) {
      device = name();
      return true;
    }
    return false;
  }

private:
  bool active() const {
    return QGuiApplication::applicationState() == Qt::ApplicationActive &&
           (!acceptsInput || acceptsInput());
  }

  void stopMotion() {
    m_motionTimeout.stop();
    if (m_moving && motion) {
      motion({});
    }
    m_moving = false;
  }

  void focusChanged() {
    if (!active()) {
      stopMotion();
      m_recipient = XCB_WINDOW_NONE;
      return;
    }
    refresh(true);
  }

  xcb_window_t discoverDaemon() {
    if (m_command == XCB_ATOM_NONE) {
      m_command = serverAtom(m_x11, "CommandEvent");
      m_press = serverAtom(m_x11, "ButtonPressEvent");
      m_release = serverAtom(m_x11, "ButtonReleaseEvent");
      m_move = serverAtom(m_x11, "MotionEvent");
      if (m_press == XCB_ATOM_NONE || m_release == XCB_ATOM_NONE || m_move == XCB_ATOM_NONE) {
        m_command = XCB_ATOM_NONE;
      }
    }
    if (m_command == XCB_ATOM_NONE) {
      return XCB_WINDOW_NONE;
    }
    for (auto screens = xcb_setup_roots_iterator(xcb_get_setup(m_x11)); screens.rem > 0; xcb_screen_next(&screens)) {
      const auto daemon = magellanWindow(screens.data->root);
      if (daemon != XCB_WINDOW_NONE) {
        return daemon;
      }
    }
    return XCB_WINDOW_NONE;
  }

  // The driver's window that the root's CommandEvent property names, when
  // it is titled "Magellan Window".
  xcb_window_t magellanWindow(xcb_window_t root) {
    const auto query = xcb_get_property(m_x11, 0, root, m_command, XCB_GET_PROPERTY_TYPE_ANY, 0, 1);
    const XReply<xcb_get_property_reply_t> property(xcb_get_property_reply(m_x11, query, nullptr), &std::free);
    if (!property || property->format != 32 ||
        xcb_get_property_value_length(property.get()) != static_cast<int>(sizeof(xcb_window_t))) {
      return XCB_WINDOW_NONE;
    }
    xcb_window_t daemon = XCB_WINDOW_NONE;
    std::memcpy(&daemon, xcb_get_property_value(property.get()), sizeof(daemon));
    if (daemon == XCB_WINDOW_NONE) {
      return XCB_WINDOW_NONE;
    }
    const auto getName = xcb_get_property(m_x11, 0, daemon, XCB_ATOM_WM_NAME, XCB_GET_PROPERTY_TYPE_ANY, 0, 8);
    const XReply<xcb_get_property_reply_t> wmName(xcb_get_property_reply(m_x11, getName, nullptr), &std::free);
    if (!wmName || wmName->format != 8) {
      return XCB_WINDOW_NONE;
    }
    const auto* rawName = static_cast<const char*>(xcb_get_property_value(wmName.get()));
    QByteArray title(rawName, xcb_get_property_value_length(wmName.get()));
    const auto terminator = title.indexOf('\0');
    if (terminator >= 0) {
      title.truncate(terminator);
    }
    return title == "Magellan Window" ? daemon : xcb_window_t{XCB_WINDOW_NONE};
  }

  bool registerWindow(xcb_window_t daemon, xcb_window_t recipient) {
    xcb_client_message_event_t registration{};
    registration.response_type = XCB_CLIENT_MESSAGE;
    registration.format = 16;
    registration.window = daemon;
    registration.type = m_command;
    registration.data.data16[0] = static_cast<std::uint16_t>(recipient >> 16);
    registration.data.data16[1] = static_cast<std::uint16_t>(recipient & 0xffffU);
    registration.data.data16[2] = kRegisterApplicationWindow;
    const auto cookie = xcb_send_event_checked(m_x11, 0, daemon, 0, reinterpret_cast<const char*>(&registration));
    const XReply<xcb_generic_error_t> error(xcb_request_check(m_x11, cookie), &std::free);
    return !error;
  }

  bool refresh(bool notify) {
    const auto previous = m_daemon;
    m_daemon = discoverDaemon();
    if (m_daemon != XCB_WINDOW_NONE && active()) {
      // A window that does not exist yet is 0 (never created here); the
      // next poll or focus change registers it.
      const auto recipient = static_cast<xcb_window_t>(nativeWindow());
      if (recipient != XCB_WINDOW_NONE && (m_daemon != previous || recipient != m_recipient)) {
        if (registerWindow(m_daemon, recipient)) {
          m_recipient = recipient;
          if (!m_filtering) {
            QCoreApplication::instance()->installNativeEventFilter(this);
            m_filtering = true;
          }
        } else {
          m_daemon = XCB_WINDOW_NONE;
        }
      }
    }
    const bool present = m_daemon != XCB_WINDOW_NONE;
    if (!present) {
      m_recipient = XCB_WINDOW_NONE;
      stopMotion();
    }
    if (notify && connection && present != (previous != XCB_WINDOW_NONE)) {
      connection(present, name());
    }
    return present;
  }

  bool nativeEventFilter(const QByteArray& eventType, void* message, qintptr*) override {
    if (m_recipient == XCB_WINDOW_NONE || eventType != "xcb_generic_event_t") {
      return false;
    }
    const auto* event = static_cast<const xcb_generic_event_t*>(message);
    if ((event->response_type & 0x7fU) != XCB_CLIENT_MESSAGE) {
      return false;
    }
    const auto* client = static_cast<const xcb_client_message_event_t*>(message);
    if (client->window != m_recipient || client->format != 16 || !active()) {
      return false;
    }
    if (client->type == m_move && motion) {
      std::array<std::uint16_t, 10> words{};
      std::memcpy(words.data(), client->data.data16, sizeof(words));
      const auto mapped = spaceMouseFromSpnav(magellanMotion(words));
      m_moving = mapped != SpaceMouseAxes{};
      if (m_moving) {
        // Magellan has no device-removal event. Clear a held rate if
        // the stream disappears before the cap sends its zero report.
        m_motionTimeout.start(250);
      } else {
        m_motionTimeout.stop();
      }
      motion(mapped);
      return true;
    }
    if ((client->type == m_press || client->type == m_release) && button) {
      const auto number = client->data.data16[2];
      if (number < 32) {
        button(number, client->type == m_press);
      }
      return true;
    }
    return false;
  }

  xcb_connection_t* m_x11 = nullptr;
  xcb_window_t m_daemon = XCB_WINDOW_NONE;
  xcb_window_t m_recipient = XCB_WINDOW_NONE;
  xcb_atom_t m_command = XCB_ATOM_NONE;
  xcb_atom_t m_press = XCB_ATOM_NONE;
  xcb_atom_t m_release = XCB_ATOM_NONE;
  xcb_atom_t m_move = XCB_ATOM_NONE;
  QTimer m_poll;
  QTimer m_motionTimeout;
  bool m_moving = false;
  bool m_filtering = false;
};

} // namespace

std::unique_ptr<SpaceMouseBackend> createMagellanSpaceMouseBackend() {
  return std::make_unique<MagellanBackend>();
}

} // namespace mitcad
