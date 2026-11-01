// SPDX-License-Identifier: MIT
#include "../WindowsTestSync.hpp"

#include <windows.h>

namespace mitcad::win {

bool isSyncKey(const void* message, bool* released) {
  const auto* msg = static_cast<const MSG*>(message);
  const bool down = msg->message == WM_KEYDOWN || msg->message == WM_SYSKEYDOWN;
  const bool up = msg->message == WM_KEYUP || msg->message == WM_SYSKEYUP;
  if ((!down && !up) || msg->wParam != VK_PAUSE) {
    return false;
  }
  *released = up;
  return true;
}

bool nativeActivation(quintptr* active, quintptr* focus, quintptr* foreground) {
  *active = reinterpret_cast<quintptr>(GetActiveWindow());
  *focus = reinterpret_cast<quintptr>(GetFocus());
  *foreground = reinterpret_cast<quintptr>(GetForegroundWindow());
  return true;
}

bool activationMessage(const void* message, QString* what, quintptr* window, quintptr* other) {
  const auto* msg = static_cast<const MSG*>(message);
  *window = reinterpret_cast<quintptr>(msg->hwnd);
  *other = 0;
  const QString on = msg->wParam != 0 ? QStringLiteral("on") : QStringLiteral("off");
  switch (msg->message) {
  case WM_ACTIVATE: {
    const int state = LOWORD(msg->wParam);
    const QString name = state == WA_INACTIVE ? QStringLiteral("inactive")
                         : state == WA_ACTIVE ? QStringLiteral("active")
                                              : QStringLiteral("click-active");
    *what = QStringLiteral("WM_ACTIVATE ") + name +
            (HIWORD(msg->wParam) != 0 ? QStringLiteral(" minimized") : QString());
    *other = static_cast<quintptr>(msg->lParam);
    return true;
  }
  case WM_ACTIVATEAPP:
    *what = QStringLiteral("WM_ACTIVATEAPP ") + on;
    return true;
  case WM_NCACTIVATE:
    *what = QStringLiteral("WM_NCACTIVATE ") + on;
    return true;
  case WM_ENABLE:
    *what = QStringLiteral("WM_ENABLE ") + on;
    return true;
  case WM_SETFOCUS:
    *what = QStringLiteral("WM_SETFOCUS");
    *other = static_cast<quintptr>(msg->wParam);
    return true;
  case WM_KILLFOCUS:
    *what = QStringLiteral("WM_KILLFOCUS");
    *other = static_cast<quintptr>(msg->wParam);
    return true;
  case WM_SIZE: {
    // A top-level window's only: every resize sizes the child windows too.
    if ((GetWindowLongPtrW(msg->hwnd, GWL_STYLE) & WS_CHILD) != 0) {
      return false;
    }
    const QString state = msg->wParam == SIZE_MAXIMIZED   ? QStringLiteral("maximized")
                          : msg->wParam == SIZE_MINIMIZED ? QStringLiteral("minimized")
                          : msg->wParam == SIZE_RESTORED  ? QStringLiteral("restored")
                                                          : QString::number(static_cast<qulonglong>(msg->wParam));
    *what = QStringLiteral("WM_SIZE %1 %2x%3")
                .arg(state)
                .arg(static_cast<int>(LOWORD(msg->lParam)))
                .arg(static_cast<int>(HIWORD(msg->lParam)));
    return true;
  }
  default:
    return false;
  }
}

} // namespace mitcad::win
