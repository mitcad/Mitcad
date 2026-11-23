// SPDX-License-Identifier: MIT
// The graphics check at start on Linux (mitcad#2292, GitHub mitcad/Mitcad#586).
//
// OCCT draws through EGL there (framework/DisplayPlatform.hpp), so Qt runs
// with its EGL integration on X11 too. A top-level window that holds the
// 3D view (a QOpenGLWidget) is composed by Qt with OpenGL: where Qt cannot
// make the context, or the window gets no EGL surface, nothing of the
// window is drawn, not even its panels (0.70.0 showed a black window on a
// user's X11 desktop). Before the first window, main() checks what such a
// window needs: Qt's shared context, a context of the viewport's format
// made current on a window that is never shown (on Wayland, where a window
// gets its surface only when shown, an offscreen surface), the window's
// EGL surface and the driver's GL version.
//
// - The viewport's 4.5 compatibility format first; where it fails (an EGL
//   driver that refuses the version, as Qt's GLX integration fell back to
//   a legacy context), the driver's own highest compatibility version.
//   The format that works becomes the default and the viewport's.
// - Where none works, the program starts again in this process (exec)
//   on the next platform of graphicsFallback(): X11 with GLX at the end,
//   where the window and its panels draw and the 3D view says why it
//   cannot.
//
// Each attempt and Qt's warnings during it (they name the EGL call that
// failed and its error) go to the log as "Graphics check: " lines. Qt's
// xcb_egl window makes its EGL surface without a word when that fails, and
// a context then goes current without a surface: nothing is drawn and
// nothing is logged (the reported 0.70.0 black window: NVIDIA's driver
// under XWayland, a working context, no Qt warning). For that case the
// line also says whether Qt's EGL display is the X11 one (on X11 Qt falls
// back to EGL_DEFAULT_DISPLAY, which has no X11 windows, when
// eglInitialize fails for EGL_PLATFORM_X11_KHR) and whether
// eglCreatePlatformWindowSurfaceEXT succeeds on the window when called
// here with its id as a 64-bit Window (Qt passes a 32-bit one).
#pragma once

#include "framework/DisplayPlatform.hpp"

#include <QList>
#include <QString>
#include <QStringList>
#include <QSurfaceFormat>

#include <optional>
#include <string>
#include <vector>

namespace mitcad {

struct GraphicsStart {
  // The first of the formats tried that a window draws with; none when
  // none does.
  std::optional<QSurfaceFormat> format;
  // Why none does, for the next start's note.
  QString failure;
  // "Graphics check: " lines for the log.
  QStringList log;
};

// Checks `formats` in order on the running platform, after the application
// object and before any window. `forceFailure` (MITCAD_TEST_GL_FAILURE=start,
// tests only) makes every EGL context fail the check.
GraphicsStart startGraphics(const QList<QSurfaceFormat>& formats, bool forceFailure);

// Starts this program again in this process with `arguments` (the
// command line as main() got it) on `fallback`'s platform, with `note` in
// MITCAD_GRAPHICS_FALLBACK, `restarts` + 1 in MITCAD_GRAPHICS_RESTARTS and
// `tried` (the options tried so far, this start's included) in
// MITCAD_GRAPHICS_TRIED. Returns only when that failed, with the reason.
QString startAgainWithGraphicsFallback(std::vector<std::string> arguments, const GraphicsFallback& fallback,
                                       const QString& note, int restarts, const QStringList& tried);

} // namespace mitcad
