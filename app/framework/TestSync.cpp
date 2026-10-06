// SPDX-License-Identifier: MIT
#include "framework/TestSync.hpp"

#include <QCoreApplication>
#include <QGuiApplication>
#include <QList>
#include <QPointer>
#include <QtLogging>

#include <algorithm>

#ifdef Q_OS_LINUX
#include <xcb/xcb.h>

#include <cstdint>
#include <cstdlib>
#endif

namespace mitcad {
namespace {

// The sync answers this long after its key at the earliest, and waits for
// watched timers this long at the most (one that keeps restarting).
constexpr qint64 kSettleMs = 20;
constexpr qint64 kLongestMs = 5000;
constexpr int kPollMs = 10;

TestSync* g_sync = nullptr;

QList<QPointer<QTimer>>& watchedTimers() {
  static QList<QPointer<QTimer>> timers;
  return timers;
}

#ifdef Q_OS_LINUX
constexpr xcb_keysym_t kPauseKeysym = 0xff13; // XK_Pause

// The keycode the X server's keyboard mapping gives a keysym, 0 if none.
unsigned keycodeOf(xcb_connection_t* connection, xcb_keysym_t keysym) {
  const xcb_setup_t* setup = xcb_get_setup(connection);
  const int first = setup->min_keycode;
  const int count = setup->max_keycode - first + 1;
  xcb_get_keyboard_mapping_reply_t* reply = xcb_get_keyboard_mapping_reply(
      connection,
      xcb_get_keyboard_mapping(connection, static_cast<xcb_keycode_t>(first), static_cast<std::uint8_t>(count)),
      nullptr);
  if (reply == nullptr) {
    return 0;
  }
  const xcb_keysym_t* keysyms = xcb_get_keyboard_mapping_keysyms(reply);
  const int length = xcb_get_keyboard_mapping_keysyms_length(reply);
  const int perKeycode = std::max<int>(reply->keysyms_per_keycode, 1);
  unsigned keycode = 0;
  for (int i = 0; i < length; ++i) {
    if (keysyms[i] == keysym) {
      keycode = static_cast<unsigned>(first + i / perKeycode);
      break;
    }
  }
  std::free(reply);
  return keycode;
}
#endif

} // namespace

void TestSync::installIfRequested() {
  if (g_sync != nullptr || qEnvironmentVariableIntValue("MITCAD_TEST_SYNC") == 0) {
    return;
  }
#ifdef Q_OS_LINUX
  auto* x11 = qGuiApp->nativeInterface<QNativeInterface::QX11Application>();
  const unsigned keycode = x11 != nullptr ? keycodeOf(x11->connection(), kPauseKeysym) : 0;
  if (keycode == 0) {
    qWarning() << "MITCAD_TEST_SYNC: no X11 connection with a Pause key; no sync";
    return;
  }
  g_sync = new TestSync(keycode);
  QCoreApplication::instance()->installNativeEventFilter(g_sync);
#else
  qWarning() << "MITCAD_TEST_SYNC works on X11 only; no sync";
#endif
}

void TestSync::watch(QTimer* timer) {
  if (g_sync == nullptr) {
    return;
  }
  QList<QPointer<QTimer>>& timers = watchedTimers();
  timers.removeIf([](const QPointer<QTimer>& watched) { return watched.isNull(); });
  timers.append(timer);
}

TestSync::TestSync(unsigned keycode) : QObject(QCoreApplication::instance()), m_keycode(keycode) {
  m_poll.setInterval(kPollMs);
  connect(&m_poll, &QTimer::timeout, this, &TestSync::poll);
}

bool TestSync::nativeEventFilter(const QByteArray& eventType, void* message, qintptr* result) {
  Q_UNUSED(result);
#ifdef Q_OS_LINUX
  if (eventType != "xcb_generic_event_t") {
    return false;
  }
  const auto* event = static_cast<const xcb_generic_event_t*>(message);
  const int type = event->response_type & 0x7f; // without the "sent" bit
  if ((type != XCB_KEY_PRESS && type != XCB_KEY_RELEASE) ||
      static_cast<const xcb_key_press_event_t*>(message)->detail != m_keycode) {
    return false;
  }
  if (type == XCB_KEY_RELEASE) {
    request();
  }
  return true; // the key goes no further
#else
  Q_UNUSED(eventType);
  Q_UNUSED(message);
  return false;
#endif
}

void TestSync::request() {
  ++m_count;
  if (!m_since.isValid()) {
    m_since.start();
  }
  m_last.start();
  if (!m_poll.isActive()) {
    m_poll.start();
  }
}

void TestSync::poll() {
  const QList<QPointer<QTimer>>& timers = watchedTimers();
  const bool running = std::any_of(timers.cbegin(), timers.cend(),
                                   [](const QPointer<QTimer>& timer) { return timer && timer->isActive(); });
  if (m_last.elapsed() < kSettleMs || (running && m_since.elapsed() < kLongestMs)) {
    return;
  }
  while (m_answered < m_count) {
    qDebug().noquote() << QStringLiteral("Sync %1").arg(++m_answered);
  }
  m_since.invalidate();
  m_poll.stop();
}

} // namespace mitcad
