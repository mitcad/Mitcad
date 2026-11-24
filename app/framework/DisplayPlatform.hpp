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
#include <QStringList>

#include <optional>

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

// The start when the window cannot draw with OpenGL (mitcad#2292, GitHub
// mitcad/Mitcad#586). A top-level window that holds the 3D view is composed
// with OpenGL by Qt; where Qt's context or the window's surface fails, the
// whole window stays black. framework/GraphicsStart.hpp checks that at
// start, before any window, and starts the program again in its own
// process on the first option of this order that this run has not tried:
//
// - X11 with Qt's EGL integration (the default since 0.70);
// - X11 with GLX: Qt composes the window with GLX, as every window up to
//   0.69 was, and the 3D view, which OCCT draws through EGL only, draws
//   in an EGL context of its own without a window and is read back into
//   the window each frame (GraphicsStart.hpp, ViewEglContext). The
//   reported black or missing windows (0.70.0, NVIDIA's driver, XWayland
//   in a Wayland session) had working EGL contexts but no EGL surface for
//   their X11 window, while 0.69's GLX window drew;
// - in a Wayland session only, native Wayland last: it is opt-in
//   (MITCAD_WAYLAND=1) until the toolbar panel shows there (mitcad#2031).
//
// Nothing after an X11 integration the user chose other than these two.
struct GraphicsFallback {
  QString key;              // "wayland", "xcb_egl" or "xcb_glx"
  QString platform;         // QT_QPA_PLATFORM of the next start
  QString xcbGlIntegration; // QT_XCB_GL_INTEGRATION of the next start; empty: left
  QString description;      // for the log
};

// The option `platformName` (QGuiApplication::platformName()) with
// `xcbGlIntegration` (QT_XCB_GL_INTEGRATION) is: "wayland", "xcb_egl"
// (also when none is named), "xcb_glx", or empty for any other.
QString graphicsOption(const QString& platformName, const QString& xcbGlIntegration);

// The next option after the running one, given whether this is a Wayland
// session (WAYLAND_DISPLAY set) and the options earlier starts of this run
// tried (MITCAD_GRAPHICS_TRIED), or none.
std::optional<GraphicsFallback> graphicsFallback(const QString& platformName, const QString& xcbGlIntegration,
                                                 bool waylandSession, const QStringList& tried);

// What the next start is told in MITCAD_GRAPHICS_FALLBACK: `earlier` (an
// earlier start's note), then why this start could not draw and where the
// next one runs. Read with graphicsStartFailure() in the next start.
QString graphicsFallbackNote(const QString& earlier, const QString& failure, const GraphicsFallback& fallback);

// The variables a fallback start reads: the note, how many starts came
// before it (at most two, so that a platform the command line forces
// cannot start it again and again), and the options they tried
// (comma-separated keys).
inline constexpr char kGraphicsFallbackVariable[] = "MITCAD_GRAPHICS_FALLBACK";
inline constexpr char kGraphicsRestartsVariable[] = "MITCAD_GRAPHICS_RESTARTS";
inline constexpr char kGraphicsTriedVariable[] = "MITCAD_GRAPHICS_TRIED";
inline constexpr int kGraphicsRestartLimit = 2;

// Why an earlier start of this run could not draw (the note), empty when
// none: the 3D view's message says the view could not start instead of
// that it lost its connection.
QString graphicsStartFailure();

} // namespace mitcad
