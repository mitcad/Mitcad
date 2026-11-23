// SPDX-License-Identifier: MIT
// Qt's platform plugin on Linux (mitcad#2031): see DisplayPlatform.hpp.
#include "framework/DisplayPlatform.hpp"

#include <QList>
#include <QStringList>
#include <QtGlobal>

#include <utility>

namespace mitcad {

namespace {

const QString kXcb = QStringLiteral("xcb");
const QString kXcbEgl = QStringLiteral("xcb_egl");

// Whether Qt may start the xcb platform for `platforms`, Qt's list of
// platforms to try ("wayland;xcb"), each with its arguments after a colon.
bool mayUseXcb(const QString& platforms) {
  const QStringList entries = platforms.split(QLatin1Char(';'), Qt::SkipEmptyParts);
  for (const QString& entry : entries) {
    if (entry.section(QLatin1Char(':'), 0, 0).trimmed() == kXcb) {
      return true;
    }
  }
  return false;
}

} // namespace

DisplayEnvironment displayEnvironment() {
  DisplayEnvironment environment;
  environment.qtPlatform = qEnvironmentVariable("QT_QPA_PLATFORM");
  environment.xcbGlIntegration = qEnvironmentVariable("QT_XCB_GL_INTEGRATION");
  environment.waylandDisplay = qEnvironmentVariable("WAYLAND_DISPLAY");
  environment.waylandOptIn = qEnvironmentVariable("MITCAD_WAYLAND") == QLatin1String("1");
  return environment;
}

DisplayPlatformChoice chooseDisplayPlatform(const DisplayEnvironment& environment, bool occtUsesEgl) {
  DisplayPlatformChoice choice;
  if (!occtUsesEgl) {
    // OCCT's OpenGL driver uses GLX: Qt must use X11 also on Wayland
    // desktops and WSLg, with Qt's default (GLX) integration.
    if (environment.qtPlatform.isEmpty()) {
      choice.platform = kXcb;
      choice.reason = QStringLiteral("OCCT draws through GLX, which needs X11");
    } else {
      choice.reason = QStringLiteral("QT_QPA_PLATFORM %1").arg(environment.qtPlatform);
    }
    return choice;
  }
  QString platforms = environment.qtPlatform;
  if (!platforms.isEmpty()) {
    choice.reason = QStringLiteral("QT_QPA_PLATFORM %1").arg(platforms);
  } else if (!environment.waylandDisplay.isEmpty() && !environment.waylandOptIn) {
    // Native Wayland is opt-in (MITCAD_WAYLAND=1) until the toolbar panel
    // shows there (mitcad#2031): X11 through XWayland.
    platforms = kXcb;
    choice.platform = platforms;
    choice.reason = QStringLiteral("a Wayland session, run on X11 (XWayland; MITCAD_WAYLAND=1 for native Wayland)");
  } else if (!environment.waylandDisplay.isEmpty()) {
    platforms = QStringLiteral("wayland;xcb");
    choice.platform = platforms;
    choice.waylandFirst = true;
    choice.reason = QStringLiteral("a Wayland session: WAYLAND_DISPLAY %1").arg(environment.waylandDisplay);
  } else {
    platforms = kXcb;
    choice.platform = platforms;
    choice.reason = QStringLiteral("no Wayland session: WAYLAND_DISPLAY is not set");
  }
  // OCCT built for EGL cannot use a GLX context: on X11 Qt's EGL
  // integration, unless the user chose one.
  if (mayUseXcb(platforms) && environment.xcbGlIntegration.isEmpty()) {
    choice.xcbGlIntegration = kXcbEgl;
  }
  return choice;
}

void applyDisplayPlatform(const DisplayPlatformChoice& choice) {
  if (!choice.platform.isEmpty()) {
    qputenv("QT_QPA_PLATFORM", choice.platform.toLocal8Bit());
  }
  if (!choice.xcbGlIntegration.isEmpty()) {
    qputenv("QT_XCB_GL_INTEGRATION", choice.xcbGlIntegration.toLocal8Bit());
  }
}

QString displayPlatformDescription(const DisplayPlatformChoice& choice, const QString& platformName) {
  QString reason = choice.reason;
  if (choice.waylandFirst && platformName == kXcb) {
    reason = QStringLiteral("the Wayland platform did not start; Qt fell back to X11 (%1)").arg(choice.reason);
  }
  if (platformName == kXcb && !choice.xcbGlIntegration.isEmpty()) {
    reason += QStringLiteral("; OpenGL through %1").arg(choice.xcbGlIntegration);
  }
  return QStringLiteral("%1 (%2)").arg(platformName, reason);
}

QString graphicsOption(const QString& platformName, const QString& xcbGlIntegration) {
  if (platformName == QLatin1String("wayland")) {
    return QStringLiteral("wayland");
  }
  if (platformName == kXcb && (xcbGlIntegration.isEmpty() || xcbGlIntegration == kXcbEgl)) {
    return kXcbEgl;
  }
  if (platformName == kXcb && xcbGlIntegration == QLatin1String("xcb_glx")) {
    return QStringLiteral("xcb_glx");
  }
  return QString();
}

std::optional<GraphicsFallback> graphicsFallback(const QString& platformName, const QString& xcbGlIntegration,
                                                 bool waylandSession, const QStringList& tried) {
  const QString current = graphicsOption(platformName, xcbGlIntegration);
  if (current.isEmpty()) {
    return std::nullopt;
  }
  // "wayland;xcb": Qt takes X11 where the Wayland platform does not start
  // (the check then goes on from there) rather than ending.
  const GraphicsFallback wayland{QStringLiteral("wayland"), QStringLiteral("wayland;xcb"), QString(),
                                 QStringLiteral("native Wayland")};
  const GraphicsFallback egl{kXcbEgl, kXcb, kXcbEgl, QStringLiteral("X11 (XWayland) with Qt's EGL integration")};
  const GraphicsFallback glx{QStringLiteral("xcb_glx"), kXcb, QStringLiteral("xcb_glx"),
                             QStringLiteral("X11 with GLX: the window draws, the 3D view (EGL) cannot")};
  QList<GraphicsFallback> order;
  if (waylandSession) {
    order << wayland;
  }
  order << egl << glx;
  for (const GraphicsFallback& option : std::as_const(order)) {
    if (option.key != current && !tried.contains(option.key)) {
      return option;
    }
  }
  return std::nullopt;
}

QString graphicsFallbackNote(const QString& earlier, const QString& failure, const GraphicsFallback& fallback) {
  const QString note = QStringLiteral("%1; started again on %2").arg(failure, fallback.description);
  return earlier.isEmpty() ? note : QStringLiteral("%1; then %2").arg(earlier, note);
}

QString graphicsStartFailure() {
  return qEnvironmentVariable(kGraphicsFallbackVariable);
}

} // namespace mitcad
