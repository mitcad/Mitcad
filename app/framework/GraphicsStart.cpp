// SPDX-License-Identifier: MIT
// The graphics check at start on Linux (mitcad#2292): see GraphicsStart.hpp.
#include "framework/GraphicsStart.hpp"

// EGL's headers without Xlib's, whose macros (None, Bool, Status) clash
// with Qt's names (as in OcctViewer.cpp).
#ifndef EGL_NO_X11
#define EGL_NO_X11
#endif
#include <EGL/egl.h>
#include <EGL/eglext.h>

#include <QByteArray>
#include <QGuiApplication>
#include <QtGui/qguiapplication_platform.h>
#include <QOffscreenSurface>
#include <QOpenGLContext>
#include <QOpenGLFunctions>
#include <QWindow>
#include <QtGlobal>
#include <QtGui/qopenglcontext_platform.h>
#include <QtLogging>

#include <cerrno>
#include <cstdio>
#include <cstring>
#include <memory>
#include <unistd.h>

namespace mitcad {

namespace {

// Qt's warnings while a format is checked: QEGLPlatformContext's "Failed
// to create context: 3009" and "eglMakeCurrent failed: 3009" name the EGL
// call that failed and its error, which Qt has read already. Passed on to
// the handler before.
QStringList* g_captured = nullptr;
QtMessageHandler g_previous = nullptr;

void captureMessage(QtMsgType type, const QMessageLogContext& context, const QString& message) {
  if (g_captured != nullptr && type != QtDebugMsg && type != QtInfoMsg) {
    g_captured->append(message.trimmed());
  }
  if (g_previous != nullptr) {
    g_previous(type, context, message);
  }
}

class CapturedMessages {
public:
  CapturedMessages() {
    g_captured = &m_messages;
    g_previous = qInstallMessageHandler(captureMessage);
  }
  ~CapturedMessages() {
    qInstallMessageHandler(g_previous);
    g_captured = nullptr;
    g_previous = nullptr;
  }
  CapturedMessages(const CapturedMessages&) = delete;
  CapturedMessages& operator=(const CapturedMessages&) = delete;

  // `problem` with Qt's warnings and EGL's pending error, if any.
  QString describe(const QString& problem) const {
    QStringList details = m_messages;
    const EGLint error = eglGetError();
    if (error != EGL_SUCCESS) {
      details << QStringLiteral("eglGetError 0x%1").arg(static_cast<unsigned>(error), 0, 16);
    }
    return details.isEmpty() ? problem : QStringLiteral("%1 (%2)").arg(problem, details.join(QStringLiteral("; ")));
  }

private:
  QStringList m_messages;
};

QString formatText(const QSurfaceFormat& format) {
  QString profile = QStringLiteral("without a profile");
  if (format.profile() == QSurfaceFormat::CompatibilityProfile) {
    profile = QStringLiteral("compatibility");
  } else if (format.profile() == QSurfaceFormat::CoreProfile) {
    profile = QStringLiteral("core");
  }
  return QStringLiteral("OpenGL %1.%2 %3").arg(format.majorVersion()).arg(format.minorVersion()).arg(profile);
}

QString eglErrorText(EGLint error) {
  return QStringLiteral("0x%1").arg(static_cast<unsigned>(error), 0, 16);
}

// Why an X11 window got no EGL surface from Qt's xcb_egl integration, as
// far as EGL tells: whether the client offers EGL_EXT_platform_x11, whether
// Qt's display (`egl`) is the X11 platform's display or Qt fell back to
// EGL_DEFAULT_DISPLAY because eglInitialize failed for X11, and, on the X11
// display, whether eglCreatePlatformWindowSurfaceEXT succeeds on `window`
// with its id passed as a 64-bit Window (Qt passes its 32-bit window id).
QString x11SurfaceDiagnosis(QNativeInterface::QEGLContext* egl, QWindow* window) {
  QStringList facts;
  const char* client = eglQueryString(EGL_NO_DISPLAY, EGL_EXTENSIONS);
  const bool platformX11 = client != nullptr && std::strstr(client, "EGL_EXT_platform_x11") != nullptr;
  if (!platformX11) {
    facts << QStringLiteral("the EGL client offers no EGL_EXT_platform_x11");
  }
  const char* vendor = eglQueryString(egl->display(), EGL_VENDOR);
  facts << QStringLiteral("Qt's EGL display: %1")
               .arg(vendor != nullptr ? QString::fromLatin1(vendor) : QStringLiteral("no vendor"));
  auto* x11 = qGuiApp != nullptr ? qGuiApp->nativeInterface<QNativeInterface::QX11Application>() : nullptr;
  const auto getPlatformDisplay =
      reinterpret_cast<PFNEGLGETPLATFORMDISPLAYEXTPROC>(eglGetProcAddress("eglGetPlatformDisplayEXT"));
  if (!platformX11 || x11 == nullptr || x11->display() == nullptr || getPlatformDisplay == nullptr) {
    return facts.join(QStringLiteral("; "));
  }
  const EGLDisplay display = getPlatformDisplay(EGL_PLATFORM_X11_KHR, x11->display(), nullptr);
  if (display == EGL_NO_DISPLAY) {
    facts << QStringLiteral("eglGetPlatformDisplayEXT(EGL_PLATFORM_X11_KHR) gave no display (%1)")
                 .arg(eglErrorText(eglGetError()));
    return facts.join(QStringLiteral("; "));
  }
  if (display != egl->display()) {
    // The same native display gives the same EGLDisplay: Qt's is another
    // one, EGL_DEFAULT_DISPLAY after a failed eglInitialize on this one.
    EGLint major = 0;
    EGLint minor = 0;
    if (eglInitialize(display, &major, &minor) == EGL_TRUE) {
      facts << QStringLiteral("Qt's EGL display is not the X11 one, which initializes here (EGL %1.%2)")
                   .arg(major)
                   .arg(minor);
    } else {
      facts << QStringLiteral("Qt's EGL display is not the X11 one: eglInitialize on EGL_PLATFORM_X11_KHR "
                              "failed (%1), and EGL_DEFAULT_DISPLAY has no X11 windows")
                   .arg(eglErrorText(eglGetError()));
    }
    return facts.join(QStringLiteral("; "));
  }
  facts << QStringLiteral("Qt's EGL display is the X11 one");
  const auto createSurface = reinterpret_cast<PFNEGLCREATEPLATFORMWINDOWSURFACEEXTPROC>(
      eglGetProcAddress("eglCreatePlatformWindowSurfaceEXT"));
  if (createSurface != nullptr && window != nullptr) {
    // EGL_PLATFORM_X11_KHR takes a pointer to an Xlib Window (unsigned long).
    unsigned long xid = static_cast<unsigned long>(window->winId());
    const EGLSurface surface = createSurface(egl->display(), egl->config(), &xid, nullptr);
    if (surface == EGL_NO_SURFACE) {
      facts << QStringLiteral("eglCreatePlatformWindowSurfaceEXT on the window fails here too (%1)")
                   .arg(eglErrorText(eglGetError()));
    } else {
      eglDestroySurface(egl->display(), surface);
      facts << QStringLiteral("eglCreatePlatformWindowSurfaceEXT on the window succeeds here with its id as a "
                              "64-bit Window");
    }
  }
  return facts.join(QStringLiteral("; "));
}

QString glText(QOpenGLFunctions* functions, unsigned name) {
  const auto* text = reinterpret_cast<const char*>(functions->glGetString(name));
  return text != nullptr ? QString::fromLatin1(text) : QString();
}

// Why a window cannot draw with `format`, empty when it can; then `drawn`
// says on what.
QString checkFormat(const QSurfaceFormat& format, bool forceFailure, QString& drawn) {
  CapturedMessages captured;
  QOpenGLContext* share = QOpenGLContext::globalShareContext();
  if (share != nullptr && !share->isValid()) {
    return captured.describe(QStringLiteral("Qt's shared context was not made (eglCreateContext)"));
  }
  QOpenGLContext context;
  context.setFormat(format);
  context.setShareContext(share);
  if (!context.create()) {
    return captured.describe(QStringLiteral("the context was not made (eglCreateContext)"));
  }
  auto* egl = context.nativeInterface<QNativeInterface::QEGLContext>();
  if (forceFailure && egl != nullptr) {
    return QStringLiteral("forced by MITCAD_TEST_GL_FAILURE=start");
  }
  // On X11 a window of the format, as the main window is, but never shown:
  // its EGL surface is the one Qt composes the window into. On Wayland a
  // window gets its surface only when shown: an offscreen surface there.
  const bool onX11 = QGuiApplication::platformName() == QLatin1String("xcb");
  std::unique_ptr<QWindow> window;
  std::unique_ptr<QOffscreenSurface> offscreen;
  QSurface* surface = nullptr;
  if (onX11) {
    window = std::make_unique<QWindow>();
    window->setSurfaceType(QSurface::OpenGLSurface);
    window->setFormat(format);
    window->setTitle(QStringLiteral("MitCad graphics check"));
    window->resize(16, 16);
    window->create();
    surface = window.get();
  } else {
    offscreen = std::make_unique<QOffscreenSurface>();
    offscreen->setFormat(format);
    offscreen->create();
    surface = offscreen.get();
  }
  if (!context.makeCurrent(surface)) {
    return captured.describe(onX11 ? QStringLiteral("the context could not be made current on a window (eglMakeCurrent)")
                                   : QStringLiteral("the context could not be made current (eglMakeCurrent)"));
  }
  // Qt's xcb_egl window makes its surface with eglCreatePlatformWindowSurfaceEXT
  // (or eglCreateWindowSurface) without a word when that fails; a context
  // then goes current without a surface where the driver allows it, and
  // the window shows nothing.
  if (egl != nullptr && onX11 && eglGetCurrentSurface(EGL_DRAW) == EGL_NO_SURFACE) {
    context.doneCurrent();
    const QString problem = captured.describe(
        QStringLiteral("the window got no EGL surface (eglCreatePlatformWindowSurfaceEXT or eglCreateWindowSurface)"));
    return QStringLiteral("%1; %2").arg(problem, x11SurfaceDiagnosis(egl, window.get()));
  }
  QOpenGLFunctions* functions = context.functions();
  const QString version = glText(functions, GL_VERSION);
  const QString renderer = glText(functions, GL_RENDERER);
  if (version.isEmpty()) {
    context.doneCurrent();
    return captured.describe(QStringLiteral("the driver answers no GL version"));
  }
  QString through = QStringLiteral("GLX");
  if (egl != nullptr) {
    const char* eglVersion = eglQueryString(egl->display(), EGL_VERSION);
    const char* eglVendor = eglQueryString(egl->display(), EGL_VENDOR);
    through = QStringLiteral("EGL %1, %2")
                  .arg(eglVersion != nullptr ? QString::fromLatin1(eglVersion) : QStringLiteral("?"),
                       eglVendor != nullptr ? QString::fromLatin1(eglVendor) : QStringLiteral("?"));
  }
  drawn = QStringLiteral("GL %1 on %2 through %3").arg(version, renderer, through);
  context.doneCurrent();
  return QString();
}

void useFormat(const QSurfaceFormat& format) {
  QSurfaceFormat::setDefaultFormat(format);
  // QGuiApplication made the shared context (Qt::AA_ShareOpenGLContexts)
  // with the default format of then; made again with this one.
  if (QOpenGLContext* share = QOpenGLContext::globalShareContext()) {
    share->setFormat(format);
    share->create();
  }
}

} // namespace

GraphicsStart startGraphics(const QList<QSurfaceFormat>& formats, bool forceFailure) {
  GraphicsStart start;
  QStringList failures;
  for (qsizetype index = 0; index < formats.size(); ++index) {
    const QSurfaceFormat& format = formats.at(index);
    if (index > 0) {
      useFormat(format);
    }
    QString drawn;
    const QString problem = checkFormat(format, forceFailure, drawn);
    if (problem.isEmpty()) {
      start.format = format;
      start.log << QStringLiteral("Graphics check: the window draws with %1: %2").arg(formatText(format), drawn);
      return start;
    }
    start.log << QStringLiteral("Graphics check: %1 on %2 failed: %3")
                     .arg(formatText(format), QGuiApplication::platformName(), problem);
    failures << QStringLiteral("%1: %2").arg(formatText(format), problem);
  }
  // As before the check, for a start that cannot go elsewhere.
  if (formats.size() > 1) {
    useFormat(formats.constFirst());
  }
  start.failure = QStringLiteral("no OpenGL for the window on %1 (%2)")
                      .arg(QGuiApplication::platformName(), failures.join(QStringLiteral("; ")));
  return start;
}

QString startAgainWithGraphicsFallback(std::vector<std::string> arguments, const GraphicsFallback& fallback,
                                       const QString& note, int restarts, const QStringList& tried) {
  if (arguments.empty()) {
    return QStringLiteral("no command line");
  }
  qputenv("QT_QPA_PLATFORM", fallback.platform.toLocal8Bit());
  // Native Wayland leaves Qt's X11 integration as it was, for a later
  // start on X11.
  if (!fallback.xcbGlIntegration.isEmpty()) {
    qputenv("QT_XCB_GL_INTEGRATION", fallback.xcbGlIntegration.toLocal8Bit());
  }
  qputenv(kGraphicsFallbackVariable, note.toUtf8());
  qputenv(kGraphicsRestartsVariable, QByteArray::number(restarts + 1));
  qputenv(kGraphicsTriedVariable, tried.join(QLatin1Char(',')).toUtf8());
  std::vector<char*> argv;
  argv.reserve(arguments.size() + 1);
  for (std::string& argument : arguments) {
    argv.push_back(argument.data());
  }
  argv.push_back(nullptr);
  std::fflush(nullptr);
  // The same process (an AppImage's runtime waits for it, so its mount
  // stays), no window shown yet, nothing of the user's opened.
  ::execv("/proc/self/exe", argv.data());
  return QString::fromLocal8Bit(std::strerror(errno));
}

} // namespace mitcad
