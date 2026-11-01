// SPDX-License-Identifier: MIT
// Remembered values of command panels (mitcad#1028): a new feature's OK
// keeps the fields' values in the settings (commands/<command>/<input>),
// an edit and Cancel keep nothing; a new panel starts with the value read
// back, or with the default when none is kept or it does not evaluate in
// the design at hand.
#include "framework/RememberedValues.hpp"

#include <cstdio>

#include <QByteArray>
#include <QCoreApplication>
#include <QPair>
#include <QSettings>
#include <QString>
#include <QTemporaryDir>
#include <QVector>
#include <QtGlobal>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "remembered_values.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

const QString kFillet = QStringLiteral("solid.fillet");
const QString kChamfer = QStringLiteral("solid.chamfer");
const QString kRadius = QStringLiteral("radius");

QVector<QPair<QString, QString>> fields(const QString& input, const QString& text) {
  QVector<QPair<QString, QString>> result;
  result.append(qMakePair(input, text));
  return result;
}

// A design whose parameters are `depth` only: an expression naming another
// one does not evaluate.
bool evaluatesHere(const QString& text) { return !text.contains(QStringLiteral("width")); }

void textTests() {
  // The expression is kept, also for a bare number that got its unit; the
  // text while there is no expression; a parameter's name left out.
  CHECK(mitcad::rememberedText(QStringLiteral("3"), QStringLiteral("3 mm")) == QStringLiteral("3 mm"));
  CHECK(mitcad::rememberedText(QStringLiteral(" 3 mm "), QString()) == QStringLiteral("3 mm"));
  CHECK(mitcad::rememberedText(QStringLiteral("r = 3"), QStringLiteral("r = 3 mm")) == QStringLiteral("3 mm"));
  CHECK(mitcad::rememberedText(QStringLiteral("depth / 2"), QStringLiteral("depth / 2")) ==
        QStringLiteral("depth / 2"));
  // A comparison is no name.
  CHECK(mitcad::rememberedText(QStringLiteral("a == b"), QString()) == QStringLiteral("a == b"));
  CHECK(mitcad::rememberedText(QString(), QString()).isEmpty());
  CHECK(mitcad::rememberedValueKey(kFillet, kRadius) == QStringLiteral("commands/solid.fillet/radius"));
}

void storeTests() {
  QSettings().remove(QStringLiteral("commands"));
  // Nothing kept: the default.
  CHECK(mitcad::rememberedValue(kFillet, kRadius).isEmpty());
  mitcad::PanelStart start = mitcad::panelStartText(kFillet, kRadius, QStringLiteral("2 mm"), evaluatesHere);
  CHECK(start.text == QStringLiteral("2 mm"));
  CHECK(!start.fromMemory);
  CHECK(start.remembered.isEmpty());

  // Cancel keeps nothing, nor does an edit's OK.
  mitcad::rememberPanelValues(kFillet, fields(kRadius, QStringLiteral("3 mm")), false, false);
  CHECK(mitcad::rememberedValue(kFillet, kRadius).isEmpty());
  mitcad::rememberPanelValues(kFillet, fields(kRadius, QStringLiteral("3 mm")), true, true);
  CHECK(mitcad::rememberedValue(kFillet, kRadius).isEmpty());

  // A new feature's OK keeps it, read back by the next panel.
  mitcad::rememberPanelValues(kFillet, fields(kRadius, QStringLiteral("3 mm")), true, false);
  CHECK(mitcad::rememberedValue(kFillet, kRadius) == QStringLiteral("3 mm"));
  CHECK(QSettings().value(QStringLiteral("commands/solid.fillet/radius")).toString() == QStringLiteral("3 mm"));
  start = mitcad::panelStartText(kFillet, kRadius, QStringLiteral("2 mm"), evaluatesHere);
  CHECK(start.text == QStringLiteral("3 mm"));
  CHECK(start.fromMemory);
  // Another command's input of the same id keeps its own.
  CHECK(mitcad::panelStartText(kChamfer, kRadius, QStringLiteral("1 mm"), evaluatesHere).text ==
        QStringLiteral("1 mm"));

  // An edit and a Cancel afterwards leave it as it was.
  mitcad::rememberPanelValues(kFillet, fields(kRadius, QStringLiteral("5 mm")), true, true);
  mitcad::rememberPanelValues(kFillet, fields(kRadius, QStringLiteral("6 mm")), false, false);
  CHECK(mitcad::rememberedValue(kFillet, kRadius) == QStringLiteral("3 mm"));
  // An empty field keeps what was kept.
  mitcad::rememberPanelValues(kFillet, fields(kRadius, QString()), true, false);
  CHECK(mitcad::rememberedValue(kFillet, kRadius) == QStringLiteral("3 mm"));

  // An expression kept: used where it evaluates, the default without an
  // error where it does not (a parameter the design lacks).
  mitcad::rememberPanelValues(kFillet, fields(kRadius, QStringLiteral("width / 4")), true, false);
  start = mitcad::panelStartText(kFillet, kRadius, QStringLiteral("2 mm"), evaluatesHere);
  CHECK(start.text == QStringLiteral("2 mm"));
  CHECK(!start.fromMemory);
  CHECK(start.remembered == QStringLiteral("width / 4"));
  start = mitcad::panelStartText(kFillet, kRadius, QStringLiteral("2 mm"),
                                 [](const QString&) { return true; });
  CHECK(start.text == QStringLiteral("width / 4"));
  CHECK(start.fromMemory);

  // Several fields of one OK (Chamfer's Distance and Distance 2).
  QVector<QPair<QString, QString>> sizes = fields(QStringLiteral("distance"), QStringLiteral("2.5 mm"));
  sizes.append(qMakePair(QStringLiteral("distance2"), QStringLiteral("4 mm")));
  mitcad::rememberPanelValues(kChamfer, sizes, true, false);
  CHECK(mitcad::rememberedValue(kChamfer, QStringLiteral("distance")) == QStringLiteral("2.5 mm"));
  CHECK(mitcad::rememberedValue(kChamfer, QStringLiteral("distance2")) == QStringLiteral("4 mm"));
  CHECK(mitcad::rememberedValue(kChamfer, QStringLiteral("angle")).isEmpty());
  QSettings().remove(QStringLiteral("commands"));
}

void switchTests() {
  // On unless MITCAD_REMEMBER_PANEL_VALUES is 0 (the UI test libraries').
  const QByteArray before = qgetenv("MITCAD_REMEMBER_PANEL_VALUES");
  const bool had = qEnvironmentVariableIsSet("MITCAD_REMEMBER_PANEL_VALUES");
  qunsetenv("MITCAD_REMEMBER_PANEL_VALUES");
  CHECK(mitcad::panelValuesRemembered());
  qputenv("MITCAD_REMEMBER_PANEL_VALUES", "1");
  CHECK(mitcad::panelValuesRemembered());
  qputenv("MITCAD_REMEMBER_PANEL_VALUES", "0");
  CHECK(!mitcad::panelValuesRemembered());
  if (had) {
    qputenv("MITCAD_REMEMBER_PANEL_VALUES", before);
  } else {
    qunsetenv("MITCAD_REMEMBER_PANEL_VALUES");
  }
}

} // namespace

int rememberedValuesTests() {
  textTests();
  switchTests();
  // The settings in a folder of the test's own, never the user's.
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QSettings::Format format = QSettings::defaultFormat();
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
  QCoreApplication::setApplicationName(QStringLiteral("remembered_values"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
  CHECK(QSettings().fileName().startsWith(directory.path()));
  storeTests();
  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}
