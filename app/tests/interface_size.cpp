// SPDX-License-Identifier: MIT
// Preferences › Display › Interface size (mitcad#1495,
// framework/InterfaceSize.hpp): 100 % by default, a kept choice read back,
// anything else 100 %; Qt's scale factor over the user's own
// QT_SCALE_FACTOR; what applyInterfaceSize leaves in the environment for
// itself and for a process started from it (a restart), which must not
// multiply the factor again. The environment is put back afterwards.
#include "framework/InterfaceSize.hpp"

#include <cmath>
#include <cstdio>

#include <QByteArray>
#include <QSettings>
#include <QTemporaryDir>
#include <QtGlobal>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "interface_size.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool nearly(double a, double b) { return std::abs(a - b) < 1e-9; }

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

void settingTests() {
  QTemporaryDir directory;
  CHECK(directory.isValid());
  QSettings settings(directory.filePath(QStringLiteral("settings.ini")), QSettings::IniFormat);
  // None kept: 100 %.
  CHECK(mitcad::savedInterfaceSize(settings) == 100);
  // Kept as the percent and read back.
  CHECK(mitcad::saveInterfaceSize(settings, 150));
  CHECK(settings.value(QStringLiteral("appearance/interfaceSize")).toInt() == 150);
  CHECK(mitcad::savedInterfaceSize(settings) == 150);
  // The same again changes nothing; a size that is not a choice is not kept.
  CHECK(!mitcad::saveInterfaceSize(settings, 150));
  CHECK(!mitcad::saveInterfaceSize(settings, 130));
  CHECK(mitcad::savedInterfaceSize(settings) == 150);
  CHECK(mitcad::saveInterfaceSize(settings, 100));
  CHECK(mitcad::savedInterfaceSize(settings) == 100);
  // Every choice round trips; anything else is 100 %.
  for (const int percent : mitcad::kInterfaceSizes) {
    CHECK(mitcad::interfaceSizeFromValue(percent) == percent);
  }
  CHECK(mitcad::interfaceSizeFromValue(QVariant()) == 100);
  CHECK(mitcad::interfaceSizeFromValue(QStringLiteral("175")) == 175);
  CHECK(mitcad::interfaceSizeFromValue(QStringLiteral("big")) == 100);
  CHECK(mitcad::interfaceSizeFromValue(90) == 100);
  CHECK(mitcad::interfaceSizeFromValue(300) == 100);
}

void factorTests() {
  // Over no user factor, or one that is not a positive number.
  CHECK(nearly(mitcad::interfaceScaleFactor(100, QByteArray()), 1.0));
  CHECK(nearly(mitcad::interfaceScaleFactor(125, QByteArray()), 1.25));
  CHECK(nearly(mitcad::interfaceScaleFactor(150, QByteArray("abc")), 1.5));
  CHECK(nearly(mitcad::interfaceScaleFactor(200, QByteArray("-2")), 2.0));
  // Multiplied with the user's own.
  CHECK(nearly(mitcad::interfaceScaleFactor(150, QByteArray("2")), 3.0));
  CHECK(nearly(mitcad::interfaceScaleFactor(175, QByteArray(" 1.2 ")), 2.1));
  // Not a choice: 100 %.
  CHECK(nearly(mitcad::interfaceScaleFactor(130, QByteArray("2")), 2.0));
}

void environmentTests() {
  const KeptVariable qtScale("QT_SCALE_FACTOR");
  const KeptVariable userScale("MITCAD_USER_QT_SCALE_FACTOR");

  // A first start without the user's factor at 150 %: Qt gets 1.5, the
  // user's none is kept for a restart.
  qunsetenv("QT_SCALE_FACTOR");
  qunsetenv("MITCAD_USER_QT_SCALE_FACTOR");
  mitcad::InterfaceScale scale = mitcad::applyInterfaceSize(150);
  CHECK(scale.percent == 150);
  CHECK(scale.userValue.isEmpty());
  CHECK(nearly(scale.factor, 1.5));
  CHECK(qgetenv("QT_SCALE_FACTOR") == QByteArray("1.5"));
  CHECK(qgetenv("MITCAD_USER_QT_SCALE_FACTOR") == QByteArray("none"));
  CHECK(mitcad::interfaceScaleDescription(scale) ==
        QStringLiteral("150% (Qt scale factor 1.5; the user's QT_SCALE_FACTOR none)"));

  // The restart inherits both: still 1.5 at 150 %, not 2.25.
  scale = mitcad::applyInterfaceSize(150);
  CHECK(nearly(scale.factor, 1.5));
  CHECK(qgetenv("QT_SCALE_FACTOR") == QByteArray("1.5"));
  // The restart after 100 % was chosen: no factor of its own.
  scale = mitcad::applyInterfaceSize(100);
  CHECK(nearly(scale.factor, 1.0));
  CHECK(!qEnvironmentVariableIsSet("QT_SCALE_FACTOR"));

  // The user's own factor 2: multiplied, and put back at 100 %.
  qputenv("QT_SCALE_FACTOR", QByteArray("2"));
  qunsetenv("MITCAD_USER_QT_SCALE_FACTOR");
  scale = mitcad::applyInterfaceSize(125);
  CHECK(scale.userValue == QByteArray("2"));
  CHECK(nearly(scale.factor, 2.5));
  CHECK(qgetenv("QT_SCALE_FACTOR") == QByteArray("2.5"));
  CHECK(qgetenv("MITCAD_USER_QT_SCALE_FACTOR") == QByteArray("2"));
  scale = mitcad::applyInterfaceSize(100);
  CHECK(nearly(scale.factor, 2.0));
  CHECK(qgetenv("QT_SCALE_FACTOR") == QByteArray("2"));
  CHECK(mitcad::interfaceScaleDescription(scale) ==
        QStringLiteral("100% (Qt scale factor 2; the user's QT_SCALE_FACTOR 2)"));
}

} // namespace

int interfaceSizeTests() {
  settingTests();
  factorTests();
  environmentTests();
  return failures;
}
