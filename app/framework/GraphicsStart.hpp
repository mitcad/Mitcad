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
//   on the next platform of graphicsFallback(): X11 with GLX after X11
//   with EGL, as 0.69 composed every window, the 3D view drawn in its own
//   EGL context (ViewEglContext below), which the check makes too; native
//   Wayland last, in a Wayland session only. Where nothing is left the
//   window and its panels draw and the 3D view says why it cannot.
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

#include <memory>
#include <optional>
#include <string>
#include <vector>

namespace mitcad {

// The 3D view's own EGL context where Qt composes the window with GLX
// (mitcad#2292): OCCT's port draws through EGL only (no Xlib), and a GLX
// context is no EGL context, so OCCT draws in this one, current without a
// window (EGL_KHR_surfaceless_context, else on a 1x1 pbuffer), into a
// framebuffer of the view's, which the view reads back into the window
// each frame (OcctViewer::paintGL). That is the 0.69 window, composed by
// Qt's GLX integration, with the 3D view of 0.70 drawn by the same EGL
// driver, but never through an X11 window's EGL surface, which the
// reported NVIDIA drivers under XWayland did not give.
//
// The display is EGL's default one, else the first EGL device (the
// graphics card), else Mesa's surfaceless platform; the context has the
// viewport's version and profile, else the driver's highest compatibility
// version. Under
// libglvnd an EGL context cannot be made current while a GLX one is:
// Qt's context is released first (OcctViewer's ViewGlCurrent).
class ViewEglContext {
public:
  // A context for `format`'s version and profile, not current; null with
  // `problem` set (each display tried and the EGL call that failed).
  static std::unique_ptr<ViewEglContext> make(const QSurfaceFormat& format, QString& problem);
  ~ViewEglContext();
  ViewEglContext(const ViewEglContext&) = delete;
  ViewEglContext& operator=(const ViewEglContext&) = delete;

  // Current on this thread, OpenGL bound as EGL's API; no other API's
  // context may be current.
  bool makeCurrent();
  void doneCurrent();
  bool isCurrent() const;
  // EGLDisplay and EGLContext, for OCCT's driver.
  void* display() const { return m_display; }
  void* context() const { return m_context; }
  // "GL 4.6.0 NVIDIA 580.95.05 on NVIDIA GeForce RTX 4070/PCIe/SSE2 through
  // EGL 1.5, NVIDIA Corporation (the default display, without a surface)".
  const QString& description() const { return m_description; }

private:
  ViewEglContext() = default;
  void* m_display = nullptr;
  void* m_context = nullptr;
  void* m_surface = nullptr;
  QString m_description;
};

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
// object and before any window; with Qt on GLX also the 3D view's own EGL
// context (ViewEglContext). Tests only: `forceFailure`
// (MITCAD_TEST_GL_FAILURE=start) makes every EGL context of Qt's fail the
// check, `forceViewFailure` (MITCAD_TEST_GL_FAILURE=start-view) the view's
// own as well.
GraphicsStart startGraphics(const QList<QSurfaceFormat>& formats, bool forceFailure, bool forceViewFailure);

// Whether MITCAD_TEST_GL_FAILURE=start-view (tests only) makes the 3D
// view's own EGL context fail, in every start of the run.
bool viewEglTestFailure();

// Starts this program again in this process with `arguments` (the
// command line as main() got it) on `fallback`'s platform, with `note` in
// MITCAD_GRAPHICS_FALLBACK, `restarts` + 1 in MITCAD_GRAPHICS_RESTARTS and
// `tried` (the options tried so far, this start's included) in
// MITCAD_GRAPHICS_TRIED. Returns only when that failed, with the reason.
QString startAgainWithGraphicsFallback(std::vector<std::string> arguments, const GraphicsFallback& fallback,
                                       const QString& note, int restarts, const QStringList& tried);

} // namespace mitcad
