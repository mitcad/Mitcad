// SPDX-License-Identifier: MIT
// Remembered values of command panels (mitcad#1028): a new feature's OK
// keeps the fields' values in the settings (commands/<command>/<input>),
// an edit and Cancel keep nothing; a new panel starts with the value read
// back, or with the default when none is kept or it does not evaluate in
// the design at hand.
#include "framework/RememberedValues.hpp"

#include <cstdio>
#include <optional>

#include <QByteArray>
#include <QChar>
#include <QCoreApplication>
#include <QPair>
#include <QSettings>
#include <QString>
#include <QStringList>
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

// A panel's settings kept by size (mitcad#1298): Thread's whole panel with
// the face's diameter and side.
const QString kThread = QStringLiteral("solid.thread");

mitcad::PanelSizeKey sizeKey(double diameter, const QString& side) {
  mitcad::PanelSizeKey key;
  key.diameter = diameter;
  key.side = side;
  return key;
}

// An M10 modelled left-hand thread 12 mm long in a Ø8.5 hole.
mitcad::PanelSettings m10Settings() {
  mitcad::PanelSettings settings;
  settings.key = sizeKey(8.5, QStringLiteral("internal"));
  settings.values.insert(QStringLiteral("length"), QStringLiteral("12 mm"));
  settings.values.insert(QStringLiteral("offset"), QStringLiteral("0 mm"));
  settings.choices.insert(QStringLiteral("standard"), QStringLiteral("iso_metric"));
  settings.choices.insert(QStringLiteral("designation"), QStringLiteral("M10x1.5"));
  settings.choices.insert(QStringLiteral("class"), QStringLiteral("6H"));
  settings.choices.insert(QStringLiteral("direction"), QStringLiteral("left"));
  settings.checks.insert(QStringLiteral("modeled"), true);
  settings.checks.insert(QStringLiteral("full_length"), false);
  return settings;
}

void sizeKeyTests() {
  CHECK(sizeKey(8.5, QStringLiteral("internal")).text() == QStringLiteral("%1%2").arg(QChar(0x00D8)).arg(QStringLiteral("8.5 internal")));
  CHECK(sizeKey(6.8000004, QStringLiteral("external")).text() ==
        QStringLiteral("%1%2").arg(QChar(0x00D8)).arg(QStringLiteral("6.8 external")));
  // Within 0.001 mm and on the same side only.
  CHECK(sizeKey(8.5, QStringLiteral("internal")).matches(sizeKey(8.5, QStringLiteral("internal"))));
  CHECK(sizeKey(8.5, QStringLiteral("internal")).matches(sizeKey(8.5009, QStringLiteral("internal"))));
  CHECK(sizeKey(8.5, QStringLiteral("internal")).matches(sizeKey(8.499, QStringLiteral("internal"))));
  CHECK(!sizeKey(8.5, QStringLiteral("internal")).matches(sizeKey(8.502, QStringLiteral("internal"))));
  CHECK(!sizeKey(8.5, QStringLiteral("internal")).matches(sizeKey(6.8, QStringLiteral("internal"))));
  CHECK(!sizeKey(8.5, QStringLiteral("internal")).matches(sizeKey(8.5, QStringLiteral("external"))));
  CHECK(!sizeKey(8.5, QString()).matches(sizeKey(8.5, QString())));
  CHECK(mitcad::panelSettingsGroup(kThread) == QStringLiteral("commands/solid.thread/last_settings"));
}

void panelSettingsTests() {
  QSettings().remove(QStringLiteral("commands"));
  CHECK(!mitcad::keptPanelSettings(kThread).has_value());

  // Cancel keeps nothing, nor does an edit's OK.
  mitcad::keepPanelSettings(kThread, m10Settings(), false, false);
  CHECK(!mitcad::keptPanelSettings(kThread).has_value());
  mitcad::keepPanelSettings(kThread, m10Settings(), true, true);
  CHECK(!mitcad::keptPanelSettings(kThread).has_value());

  // A new thread's OK keeps values, choices and checks, read back.
  mitcad::keepPanelSettings(kThread, m10Settings(), true, false);
  std::optional<mitcad::PanelSettings> kept = mitcad::keptPanelSettings(kThread);
  CHECK(kept.has_value());
  if (kept) {
    CHECK(kept->key.matches(sizeKey(8.5, QStringLiteral("internal"))));
    CHECK(kept->key.side == QStringLiteral("internal"));
    CHECK(kept->values.value(QStringLiteral("length")) == QStringLiteral("12 mm"));
    CHECK(kept->values.value(QStringLiteral("offset")) == QStringLiteral("0 mm"));
    CHECK(kept->choices.value(QStringLiteral("designation")) == QStringLiteral("M10x1.5"));
    CHECK(kept->choices.value(QStringLiteral("class")) == QStringLiteral("6H"));
    CHECK(kept->choices.value(QStringLiteral("direction")) == QStringLiteral("left"));
    CHECK(kept->checks.contains(QStringLiteral("modeled")) && kept->checks.value(QStringLiteral("modeled")));
    CHECK(kept->checks.contains(QStringLiteral("full_length")) && !kept->checks.value(QStringLiteral("full_length")));
    CHECK(mitcad::changedSettings(*kept, m10Settings()).isEmpty());
  }
  CHECK(QSettings().value(QStringLiteral("commands/solid.thread/last_settings/choices/direction")).toString() ==
        QStringLiteral("left"));
  // Beside the remembered values, not one of them.
  CHECK(mitcad::rememberedValue(kThread, QStringLiteral("length")).isEmpty());

  // An edit and a Cancel afterwards leave them as they were.
  mitcad::PanelSettings other = m10Settings();
  other.choices.insert(QStringLiteral("direction"), QStringLiteral("right"));
  mitcad::keepPanelSettings(kThread, other, true, true);
  mitcad::keepPanelSettings(kThread, other, false, false);
  kept = mitcad::keptPanelSettings(kThread);
  CHECK(kept && kept->choices.value(QStringLiteral("direction")) == QStringLiteral("left"));
  // Settings without a size keep nothing either.
  other.key = sizeKey(6.8, QString());
  mitcad::keepPanelSettings(kThread, other, true, false);
  kept = mitcad::keptPanelSettings(kThread);
  CHECK(kept && kept->key.matches(sizeKey(8.5, QStringLiteral("internal"))));

  // The next OK replaces them, whatever their size: a Ø6.8 cosmetic thread
  // without a length leaves none of the M10's.
  mitcad::PanelSettings m8;
  m8.key = sizeKey(6.8, QStringLiteral("internal"));
  m8.choices.insert(QStringLiteral("designation"), QStringLiteral("M8x1.25"));
  m8.checks.insert(QStringLiteral("modeled"), false);
  mitcad::keepPanelSettings(kThread, m8, true, false);
  kept = mitcad::keptPanelSettings(kThread);
  CHECK(kept.has_value());
  if (kept) {
    CHECK(!kept->key.matches(sizeKey(8.5, QStringLiteral("internal"))));
    CHECK(kept->key.matches(sizeKey(6.8, QStringLiteral("internal"))));
    CHECK(!kept->values.contains(QStringLiteral("length")));
    CHECK(!kept->choices.contains(QStringLiteral("direction")));
    CHECK(kept->choices.value(QStringLiteral("designation")) == QStringLiteral("M8x1.25"));
    CHECK(kept->checks.contains(QStringLiteral("modeled")) && !kept->checks.value(QStringLiteral("modeled")));
  }
  QSettings().remove(QStringLiteral("commands"));
}

void changedTests() {
  // What differs from where a panel started: a changed value, choice or
  // check, and one only one side has; sorted, each once.
  const mitcad::PanelSettings start = m10Settings();
  CHECK(mitcad::changedSettings(start, start).isEmpty());
  mitcad::PanelSettings now = start;
  now.values.insert(QStringLiteral("length"), QStringLiteral("14 mm"));
  now.choices.insert(QStringLiteral("direction"), QStringLiteral("right"));
  now.checks.insert(QStringLiteral("modeled"), false);
  now.checks.insert(QStringLiteral("chamfer"), true);
  CHECK(mitcad::changedSettings(start, now) ==
        QStringList({QStringLiteral("chamfer"), QStringLiteral("direction"), QStringLiteral("length"),
                     QStringLiteral("modeled")}));
  CHECK(mitcad::changedSettings(now, start) == mitcad::changedSettings(start, now));
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
  sizeKeyTests();
  changedTests();
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
  panelSettingsTests();
  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}
