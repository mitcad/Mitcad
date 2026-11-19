// SPDX-License-Identifier: MIT
// Preferences › Display › Toolbar labels (mitcad#1936, GitHub
// mitcad/Mitcad#356): Icons only without a saved choice (or with an unknown
// value), the choice saved and told to the ribbons once per change; a
// button's label is its command's short label or its name without the
// mnemonic and the ellipsis; the labels' font follows Icon size and is
// elided past two and a half icons.
#include "UnitTests.hpp"
#include "framework/RibbonLabels.hpp"

#include <cmath>
#include <cstdio>

#include <QChar>
#include <QCoreApplication>
#include <QList>
#include <QObject>
#include <QSettings>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "ribbon_labels.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::RibbonLabels;

void idTests() {
  CHECK(mitcad::ribbonLabelsId(RibbonLabels::IconsOnly) == QStringLiteral("icons"));
  CHECK(mitcad::ribbonLabelsId(RibbonLabels::UnderIcons) == QStringLiteral("labels"));
  CHECK(mitcad::ribbonLabelsFromId(QStringLiteral("icons")) == RibbonLabels::IconsOnly);
  CHECK(mitcad::ribbonLabelsFromId(QStringLiteral("labels")) == RibbonLabels::UnderIcons);
  CHECK(!mitcad::ribbonLabelsFromId(QStringLiteral("text")).has_value());
}

void settingsTests() {
  const QString key = mitcad::ribbonLabelsKey();
  CHECK(key == QStringLiteral("appearance/ribbonLabels"));
  QSettings().remove(key);
  CHECK(mitcad::savedRibbonLabels() == RibbonLabels::IconsOnly);
  QSettings().setValue(key, QStringLiteral("large"));
  CHECK(mitcad::savedRibbonLabels() == RibbonLabels::IconsOnly);
  QSettings().remove(key);

  QList<bool> told;
  const auto connection = QObject::connect(&mitcad::RibbonLabelsNotifier::instance(),
                                           &mitcad::RibbonLabelsNotifier::labelsChanged,
                                           [&told](bool underIcons) { told.append(underIcons); });
  // Icons only is the default: nothing changes, nothing is told.
  CHECK(!mitcad::saveRibbonLabels(RibbonLabels::IconsOnly));
  CHECK(mitcad::saveRibbonLabels(RibbonLabels::UnderIcons));
  CHECK(mitcad::savedRibbonLabels() == RibbonLabels::UnderIcons);
  CHECK(QSettings().value(key).toString() == QStringLiteral("labels"));
  CHECK(!mitcad::saveRibbonLabels(RibbonLabels::UnderIcons));
  CHECK(mitcad::saveRibbonLabels(RibbonLabels::IconsOnly));
  CHECK(mitcad::savedRibbonLabels() == RibbonLabels::IconsOnly);
  CHECK((told == QList<bool>{true, false}));
  QObject::disconnect(connection);
  QSettings().remove(key);

  // A run's layout may make labels the default (mitcad#1990): without a
  // saved choice they show, and Icons only is then saved as a choice.
  CHECK(mitcad::defaultRibbonLabels() == RibbonLabels::IconsOnly);
  mitcad::setDefaultRibbonLabels(RibbonLabels::UnderIcons);
  CHECK(mitcad::savedRibbonLabels() == RibbonLabels::UnderIcons);
  CHECK(!mitcad::saveRibbonLabels(RibbonLabels::UnderIcons));
  CHECK(mitcad::saveRibbonLabels(RibbonLabels::IconsOnly));
  CHECK(QSettings().value(key).toString() == QStringLiteral("icons"));
  CHECK(mitcad::savedRibbonLabels() == RibbonLabels::IconsOnly);
  QSettings().remove(key);
  mitcad::setDefaultRibbonLabels(RibbonLabels::IconsOnly);
}

void labelTests() {
  CHECK(mitcad::ribbonButtonLabel(QStringLiteral("Extrude")) == QStringLiteral("Extrude"));
  CHECK(mitcad::ribbonButtonLabel(QStringLiteral("Create Sketch"), QStringLiteral("Sketch")) ==
        QStringLiteral("Sketch"));
  CHECK(mitcad::ribbonButtonLabel(QStringLiteral("&Hole")) == QStringLiteral("Hole"));
  CHECK(mitcad::ribbonButtonLabel(QStringLiteral("Nuts && Bolts")) == QStringLiteral("Nuts & Bolts"));
  CHECK(mitcad::ribbonButtonLabel(QStringLiteral("Printers") + QChar(0x2026)) == QStringLiteral("Printers"));
  CHECK(mitcad::ribbonButtonLabel(QStringLiteral("Edit Parameters...")) == QStringLiteral("Edit Parameters"));
  // One line.
  CHECK(mitcad::ribbonButtonLabel(QStringLiteral("Floor\n/ Roof")) == QStringLiteral("Floor / Roof"));

  CHECK(std::abs(mitcad::ribbonLabelFontScale(100, 100) - 0.85) < 1e-9);
  CHECK(std::abs(mitcad::ribbonLabelFontScale(150, 100) - 1.275) < 1e-9);
  CHECK(std::abs(mitcad::ribbonLabelFontScale(100, 125) - 0.68) < 1e-9);
  CHECK(mitcad::ribbonLabelWidthLimit(28) == 70);
  CHECK(mitcad::ribbonLabelWidthLimit(26) == 65);

  // The short labels of long names (mitcad#1975): each id once, each label
  // at most ten letters, none for a command without one.
  QStringList ids = mitcad::shortLabelIds();
  CHECK(ids.size() > 30);
  CHECK(ids.removeDuplicates() == 0);
  for (const QString& id : ids) {
    const QString label = mitcad::defaultShortLabel(id);
    CHECK(!label.isEmpty() && label.size() <= 10);
  }
  CHECK(mitcad::defaultShortLabel(QStringLiteral("construct.offset_plane")) == QStringLiteral("Offset"));
  CHECK(mitcad::defaultShortLabel(QStringLiteral("solid.extrude")).isEmpty());
}

} // namespace

int ribbonLabelsTests() {
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  const QSettings::Format format = QSettings::defaultFormat();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadRibbonLabelsTest"));
  QCoreApplication::setApplicationName(QStringLiteral("MitcadRibbonLabelsTest"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
  QSettings().clear();

  idTests();
  settingsTests();
  labelTests();

  QSettings().clear();
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  QSettings::setDefaultFormat(format);
  return failures;
}
MITCAD_UNIT_TEST(ribbonLabelsTests);
