// SPDX-License-Identifier: MIT
// Qt's platform plugin on Linux (mitcad#2031, GitHub mitcad/Mitcad#434),
// chosen before the application object exists. OCCT draws through EGL
// there (third_party/vcpkg-ports/opencascade: no Xlib, so no GLX), so Qt
// may run natively on Wayland, where a scaled desktop stays sharp, and on
// X11 needs Qt's EGL integration (xcb_egl) instead of GLX:
//
// - QT_QPA_PLATFORM set by the user wins; when it may be xcb, its OpenGL
//   integration is xcb_egl unless the user set QT_XCB_GL_INTEGRATION too;
// - a Wayland session (WAYLAND_DISPLAY set): "wayland;xcb", Qt's list,
//   so that Qt falls back to X11 (XWayland) when the Wayland platform does
//   not start, with xcb_egl for that fallback;
// - otherwise (an X11 session, the UI tests' Xvfb): xcb with xcb_egl.
//
// An OCCT built with GLX (an older port) keeps the earlier choice: xcb
// everywhere, with Qt's default GLX integration. The start-up's
// "Display platform:" line says which platform runs and why.
// Plain Qt Core, so that app.unit tests it on every platform.
#pragma once

#include <QString>

namespace mitcad {

// The variables the choice reads; empty when not set.
struct DisplayEnvironment {
  QString qtPlatform;       // QT_QPA_PLATFORM
  QString xcbGlIntegration; // QT_XCB_GL_INTEGRATION
  QString waylandDisplay;   // WAYLAND_DISPLAY
  // MITCAD_WAYLAND=1: the native Wayland platform in a Wayland session. Off by
  // default until the toolbar panel shows on Wayland (mitcad#2031): a Wayland
  // session runs MitCad on X11 (XWayland) with Qt's EGL integration.
  bool waylandOptIn = false;
};

struct DisplayPlatformChoice {
  // The value to set QT_QPA_PLATFORM to; empty: leave it as it is.
  QString platform;
  // The value to set QT_XCB_GL_INTEGRATION to; empty: leave it.
  QString xcbGlIntegration;
  // Why, for the start-up's log line.
  QString reason;
  // Wayland was asked for first (a Wayland session): the platform that
  // runs being xcb then means Qt fell back.
  bool waylandFirst = false;
};

// The current process's variables.
DisplayEnvironment displayEnvironment();

// The choice for `environment`; `occtUsesEgl` is whether OCCT's OpenGL
// driver was built for EGL (MITCAD_OCCT_EGL).
DisplayPlatformChoice chooseDisplayPlatform(const DisplayEnvironment& environment, bool occtUsesEgl);

// Sets the variables the choice names.
void applyDisplayPlatform(const DisplayPlatformChoice& choice);

// The start-up's line after "Display platform: ", once Qt has started
// `platformName` (QGuiApplication::platformName()), e.g.
// "wayland (a Wayland session: WAYLAND_DISPLAY wayland-0)" or
// "xcb (the Wayland platform did not start; Qt fell back to X11)".
QString displayPlatformDescription(const DisplayPlatformChoice& choice, const QString& platformName);

} // namespace mitcad
