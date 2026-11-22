// SPDX-License-Identifier: MIT
// Qt's platform plugin on Linux (mitcad#2031, framework/DisplayPlatform.hpp):
// Wayland in a Wayland session with X11 as Qt's fallback, X11 elsewhere,
// the user's QT_QPA_PLATFORM first; Qt's EGL integration on X11 when OCCT
// draws through EGL, never over the user's own; an OCCT built for GLX
// keeps X11 with Qt's default integration. The start-up line says which
// platform runs and why, a fallback included. Applying the choice sets
// only what it names; the environment is put back afterwards.
#include "UnitTests.hpp"
#include "framework/DisplayPlatform.hpp"

#include <cstdio>

#include <QByteArray>
#include <QString>
#include <QtGlobal>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "display_platform.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

// An environment variable as it was, put back when the test ends.
class KeptVariable {
public:
  explicit KeptVariable(const char* name)
      : m_name(name), m_set(qEnvironmentVariableIsSet(name)), m_value(qgetenv(name)) {}
  ~KeptVariable() {
    if (m_set) {
      qputenv(m_name, m_value);
    } else {
      qunsetenv(m_name);
    }
  }
  KeptVariable(const KeptVariable&) = delete;
  KeptVariable& operator=(const KeptVariable&) = delete;

private:
  const char* m_name;
  bool m_set;
  QByteArray m_value;
};

mitcad::DisplayEnvironment environment(const QString& platform, const QString& integration, const QString& wayland,
                                      bool optIn = true) {
  mitcad::DisplayEnvironment result;
  result.qtPlatform = platform;
  result.xcbGlIntegration = integration;
  result.waylandDisplay = wayland;
  result.waylandOptIn = optIn;
  return result;
}

void eglTests() {
  // A Wayland session without MITCAD_WAYLAND=1 (the default until mitcad#2031's
  // toolbar panel shows on Wayland): X11 through XWayland, with the EGL integration.
  {
    mitcad::DisplayPlatformChoice x11 =
        mitcad::chooseDisplayPlatform(environment(QString(), QString(), QStringLiteral("wayland-0"), false), true);
    CHECK(x11.platform == QStringLiteral("xcb"));
    CHECK(x11.xcbGlIntegration == QStringLiteral("xcb_egl"));
    CHECK(!x11.waylandFirst);
  }
  // A Wayland session with MITCAD_WAYLAND=1: Wayland first, X11 (XWayland) as Qt's fallback,
  // which needs the EGL integration.
  mitcad::DisplayPlatformChoice choice =
      mitcad::chooseDisplayPlatform(environment(QString(), QString(), QStringLiteral("wayland-0")), true);
  CHECK(choice.platform == QStringLiteral("wayland;xcb"));
  CHECK(choice.xcbGlIntegration == QStringLiteral("xcb_egl"));
  CHECK(choice.waylandFirst);
  CHECK(mitcad::displayPlatformDescription(choice, QStringLiteral("wayland")) ==
        QStringLiteral("wayland (a Wayland session: WAYLAND_DISPLAY wayland-0)"));
  // The Wayland platform did not start (no plugin, no compositor): the
  // line says Qt fell back, and why it tried.
  CHECK(mitcad::displayPlatformDescription(choice, QStringLiteral("xcb")) ==
        QStringLiteral("xcb (the Wayland platform did not start; Qt fell back to X11 (a Wayland session: "
                       "WAYLAND_DISPLAY wayland-0); OpenGL through xcb_egl)"));

  // An X11 session, the UI tests' Xvfb (ui-test-lib.sh unsets
  // WAYLAND_DISPLAY): X11 with the EGL integration.
  choice = mitcad::chooseDisplayPlatform(environment(QString(), QString(), QString()), true);
  CHECK(choice.platform == QStringLiteral("xcb"));
  CHECK(choice.xcbGlIntegration == QStringLiteral("xcb_egl"));
  CHECK(!choice.waylandFirst);
  CHECK(mitcad::displayPlatformDescription(choice, QStringLiteral("xcb")) ==
        QStringLiteral("xcb (no Wayland session: WAYLAND_DISPLAY is not set; OpenGL through xcb_egl)"));

  // The user's platform wins, also in a Wayland session; on X11 still
  // the EGL integration.
  choice = mitcad::chooseDisplayPlatform(
      environment(QStringLiteral("xcb"), QString(), QStringLiteral("wayland-0")), true);
  CHECK(choice.platform.isEmpty());
  CHECK(choice.xcbGlIntegration == QStringLiteral("xcb_egl"));
  CHECK(mitcad::displayPlatformDescription(choice, QStringLiteral("xcb")) ==
        QStringLiteral("xcb (QT_QPA_PLATFORM xcb; OpenGL through xcb_egl)"));
  // With arguments, and in a list.
  choice = mitcad::chooseDisplayPlatform(
      environment(QStringLiteral("wayland;xcb:nograb"), QString(), QString()), true);
  CHECK(choice.platform.isEmpty());
  CHECK(choice.xcbGlIntegration == QStringLiteral("xcb_egl"));
  // No X11 in it: no integration of X11's.
  choice = mitcad::chooseDisplayPlatform(
      environment(QStringLiteral("wayland"), QString(), QStringLiteral("wayland-1")), true);
  CHECK(choice.platform.isEmpty());
  CHECK(choice.xcbGlIntegration.isEmpty());
  CHECK(mitcad::displayPlatformDescription(choice, QStringLiteral("wayland")) ==
        QStringLiteral("wayland (QT_QPA_PLATFORM wayland)"));
  // Headless runs (offscreen) are left alone.
  choice = mitcad::chooseDisplayPlatform(environment(QStringLiteral("offscreen"), QString(), QString()), true);
  CHECK(choice.platform.isEmpty());
  CHECK(choice.xcbGlIntegration.isEmpty());
  // The user's own integration wins too.
  choice = mitcad::chooseDisplayPlatform(environment(QString(), QStringLiteral("xcb_glx"), QString()), true);
  CHECK(choice.platform == QStringLiteral("xcb"));
  CHECK(choice.xcbGlIntegration.isEmpty());
}

void glxTests() {
  // An OCCT built for GLX: X11 also in a Wayland session, Qt's default
  // integration.
  mitcad::DisplayPlatformChoice choice =
      mitcad::chooseDisplayPlatform(environment(QString(), QString(), QStringLiteral("wayland-0")), false);
  CHECK(choice.platform == QStringLiteral("xcb"));
  CHECK(choice.xcbGlIntegration.isEmpty());
  CHECK(!choice.waylandFirst);
  CHECK(mitcad::displayPlatformDescription(choice, QStringLiteral("xcb")) ==
        QStringLiteral("xcb (OCCT draws through GLX, which needs X11)"));
  choice = mitcad::chooseDisplayPlatform(environment(QStringLiteral("offscreen"), QString(), QString()), false);
  CHECK(choice.platform.isEmpty());
  CHECK(choice.xcbGlIntegration.isEmpty());
}

void environmentTests() {
  const KeptVariable platform("QT_QPA_PLATFORM");
  const KeptVariable integration("QT_XCB_GL_INTEGRATION");
  const KeptVariable wayland("WAYLAND_DISPLAY");
  const KeptVariable optIn("MITCAD_WAYLAND");

  // A Wayland session without the opt-in runs on X11 (XWayland).
  qunsetenv("QT_QPA_PLATFORM");
  qunsetenv("QT_XCB_GL_INTEGRATION");
  qunsetenv("MITCAD_WAYLAND");
  qputenv("WAYLAND_DISPLAY", QByteArray("wayland-0"));
  mitcad::DisplayEnvironment read = mitcad::displayEnvironment();
  CHECK(!read.waylandOptIn);
  mitcad::applyDisplayPlatform(mitcad::chooseDisplayPlatform(read, true));
  CHECK(qgetenv("QT_QPA_PLATFORM") == QByteArray("xcb"));
  CHECK(qgetenv("QT_XCB_GL_INTEGRATION") == QByteArray("xcb_egl"));

  // MITCAD_WAYLAND=1 opts in to native Wayland.
  qunsetenv("QT_QPA_PLATFORM");
  qunsetenv("QT_XCB_GL_INTEGRATION");
  qputenv("MITCAD_WAYLAND", QByteArray("1"));
  read = mitcad::displayEnvironment();
  CHECK(read.qtPlatform.isEmpty());
  CHECK(read.xcbGlIntegration.isEmpty());
  CHECK(read.waylandDisplay == QStringLiteral("wayland-0"));
  CHECK(read.waylandOptIn);
  mitcad::applyDisplayPlatform(mitcad::chooseDisplayPlatform(read, true));
  CHECK(qgetenv("QT_QPA_PLATFORM") == QByteArray("wayland;xcb"));
  CHECK(qgetenv("QT_XCB_GL_INTEGRATION") == QByteArray("xcb_egl"));

  // A choice that names nothing changes nothing.
  qputenv("QT_QPA_PLATFORM", QByteArray("offscreen"));
  qunsetenv("QT_XCB_GL_INTEGRATION");
  mitcad::applyDisplayPlatform(mitcad::chooseDisplayPlatform(mitcad::displayEnvironment(), true));
  CHECK(qgetenv("QT_QPA_PLATFORM") == QByteArray("offscreen"));
  CHECK(!qEnvironmentVariableIsSet("QT_XCB_GL_INTEGRATION"));
}

} // namespace

int displayPlatformTests() {
  eglTests();
  glxTests();
  environmentTests();
  return failures;
}
MITCAD_UNIT_TEST(displayPlatformTests);
