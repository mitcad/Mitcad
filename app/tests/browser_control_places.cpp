// SPDX-License-Identifier: MIT
// The Browser rows' Activate radio button and Show eye places (mitcad#2223):
// the two preferences (Classic without a saved choice, Right saved and read
// back, Restore Defaults saving Classic, an unknown value read as Classic,
// both carried by settings export and import), and a BrowserPanel's rows
// laid out with each: Classic as before (the radio button right after the
// name, the eye before the icon); Right puts the radio button and the eye
// in one column at the visible right edge, the same x on every row, the eye
// left of the radio button, a long name elided before them.
#include "UnitTests.hpp"
#include "browser/BrowserPanel.hpp"
#include "browser/BrowserPreferences.hpp"
#include "framework/SettingsTransfer.hpp"

#include <cstdio>
#include <cstdlib>
#include <tuple>

#include <QApplication>
#include <QCoreApplication>
#include <QObject>
#include <QSettings>
#include <QString>
#include <QTemporaryDir>
#include <QTreeWidget>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "browser_control_places.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

const QString kActivateKey = QStringLiteral("browser/activateButtonPlace");
const QString kEyeKey = QStringLiteral("browser/showEyePlace");

// What the settings file holds now, as the next start reads it.
QString savedValue(const QString& key) {
  QSettings settings;
  settings.sync();
  return settings.value(key).toString();
}

void preferenceTests() {
  int told = 0;
  const QMetaObject::Connection counting = QObject::connect(
      &BrowserPreferencesNotifier::instance(), &BrowserPreferencesNotifier::controlPlacesChanged, [&told] { ++told; });

  // The defaults: Classic, nothing saved.
  CHECK(!QSettings().contains(kActivateKey));
  CHECK(!QSettings().contains(kEyeKey));
  CHECK(browserActivatePlace() == BrowserControlPlace::Classic);
  CHECK(browserEyePlace() == BrowserControlPlace::Classic);

  // The values' words, and back; an unknown one is Classic.
  CHECK(browserControlPlaceId(BrowserControlPlace::Classic) == QStringLiteral("classic"));
  CHECK(browserControlPlaceId(BrowserControlPlace::Right) == QStringLiteral("right"));
  CHECK(browserControlPlaceFromId(QStringLiteral("right")) == BrowserControlPlace::Right);
  CHECK(browserControlPlaceFromId(QStringLiteral("classic")) == BrowserControlPlace::Classic);
  CHECK(browserControlPlaceFromId(QStringLiteral("left")) == BrowserControlPlace::Classic);

  // Right saved and read back; the windows told once per change.
  setBrowserActivatePlace(BrowserControlPlace::Right);
  CHECK(savedValue(kActivateKey) == QStringLiteral("right"));
  CHECK(browserActivatePlace() == BrowserControlPlace::Right);
  CHECK(browserEyePlace() == BrowserControlPlace::Classic);
  CHECK(told == 1);
  setBrowserActivatePlace(BrowserControlPlace::Right); // no change
  CHECK(told == 1);
  setBrowserEyePlace(BrowserControlPlace::Right);
  CHECK(savedValue(kEyeKey) == QStringLiteral("right"));
  CHECK(browserEyePlace() == BrowserControlPlace::Right);
  CHECK(told == 2);

  // Restore Defaults saves Classic for both.
  setBrowserActivatePlace(BrowserControlPlace::Classic);
  setBrowserEyePlace(BrowserControlPlace::Classic);
  CHECK(savedValue(kActivateKey) == QStringLiteral("classic"));
  CHECK(savedValue(kEyeKey) == QStringLiteral("classic"));
  CHECK(browserActivatePlace() == BrowserControlPlace::Classic);
  CHECK(browserEyePlace() == BrowserControlPlace::Classic);
  CHECK(told == 4);

  // Classic saved over no choice (Restore Defaults on fresh settings): the
  // value written, the windows not told (nothing changed for them).
  QSettings().remove(kEyeKey);
  setBrowserEyePlace(BrowserControlPlace::Classic);
  CHECK(savedValue(kEyeKey) == QStringLiteral("classic"));
  CHECK(told == 4);

  // An unknown saved value is Classic.
  QSettings().setValue(kActivateKey, QStringLiteral("middle"));
  CHECK(browserActivatePlace() == BrowserControlPlace::Classic);
  QSettings().remove(kActivateKey);
  QSettings().remove(kEyeKey);

  // Settings export and import carry both, with the other preferences.
  CHECK(settingsGroupOf(kActivateKey) == QStringLiteral("preferences"));
  CHECK(settingsGroupOf(kEyeKey) == QStringLiteral("preferences"));
  QObject::disconnect(counting);
}

DocumentSnapshot::Component component(const QString& uid, const QString& name, const QString& bodyUid) {
  DocumentSnapshot::Component made;
  made.uid = uid;
  made.name = name;
  if (!bodyUid.isEmpty()) {
    made.bodies << bodyUid;
  }
  return made;
}

// Root (C0) with the body Glass (F1.b0); Bracket placed once in the root
// (O1, of C1), named `occurrence`, shown, with the body Block (F2.b0).
DocumentSnapshot design(const QString& occurrence) {
  DocumentSnapshot snapshot;
  snapshot.components.append(component(QStringLiteral("C0"), QStringLiteral("Root"), QStringLiteral("F1.b0")));
  snapshot.components.append(component(QStringLiteral("C1"), QStringLiteral("Bracket"), QStringLiteral("F2.b0")));
  for (const auto& [uid, name, owner] : {std::tuple{"F1.b0", "Glass", "C0"}, std::tuple{"F2.b0", "Block", "C1"}}) {
    DocumentSnapshot::Body body;
    body.uid = QString::fromLatin1(uid);
    body.name = QString::fromLatin1(name);
    body.component = QString::fromLatin1(owner);
    body.visible = true;
    snapshot.bodies.insert(body.uid, body);
  }
  DocumentSnapshot::Occurrence bracket;
  bracket.uid = QStringLiteral("O1");
  bracket.name = occurrence;
  bracket.path = QStringLiteral("O1");
  bracket.component = QStringLiteral("C1");
  bracket.visible = true;
  snapshot.occurrences.append(bracket);
  snapshot.activeComponent = QStringLiteral("C0");
  return snapshot;
}

// The horizontal middle of a part.
int middle(const QRect& rect) { return rect.left() + rect.width() / 2; }

void layoutTests() {
  BrowserPanel panel;
  panel.resize(320, 480);
  panel.rebuild(design(QStringLiteral("Bracket:1")), false);
  panel.show();
  QCoreApplication::processEvents();
  auto* tree = panel.findChild<QTreeWidget*>();
  CHECK(tree != nullptr);
  if (tree == nullptr) {
    return;
  }
  tree->expandAll();
  QCoreApplication::processEvents();
  const QString root = QStringLiteral("Root");
  const QString bracket = QStringLiteral("Root/Bracket:1");
  const QString glass = QStringLiteral("Root/Bodies/Glass");
  const QString block = QStringLiteral("Root/Bracket:1/Bodies/Block");

  // Classic (the default): the radio button right after each name, so at
  // another x on rows of another depth or name; the eye before the name.
  CHECK(panel.activatePlace() == BrowserControlPlace::Classic);
  CHECK(panel.eyePlace() == BrowserControlPlace::Classic);
  BrowserPanel::RowPlaces rootRow = panel.rowPlaces(root);
  BrowserPanel::RowPlaces bracketRow = panel.rowPlaces(bracket);
  BrowserPanel::RowPlaces glassRow = panel.rowPlaces(glass);
  CHECK(!rootRow.radio.isEmpty() && !bracketRow.radio.isEmpty());
  CHECK(rootRow.radio.left() > rootRow.name.right());
  CHECK(bracketRow.radio.left() > bracketRow.name.right());
  CHECK(rootRow.radio.left() - rootRow.name.right() == bracketRow.radio.left() - bracketRow.name.right());
  CHECK(middle(rootRow.radio) != middle(bracketRow.radio));
  CHECK(!glassRow.eye.isEmpty());
  CHECK(glassRow.eye.right() < glassRow.name.left());

  // Activate on the right: one column at the visible right edge (the
  // viewport's, less the box padding), the same x on every row.
  const int edge = tree->viewport()->width() - 1;
  panel.setControlPlaces(BrowserControlPlace::Right, BrowserControlPlace::Classic);
  rootRow = panel.rowPlaces(root);
  bracketRow = panel.rowPlaces(bracket);
  glassRow = panel.rowPlaces(glass);
  CHECK(!rootRow.radio.isEmpty() && !bracketRow.radio.isEmpty());
  CHECK(std::abs(middle(rootRow.radio) - middle(bracketRow.radio)) <= 1);
  CHECK(rootRow.radio.right() <= edge && rootRow.radio.right() >= edge - 16);
  CHECK(rootRow.name.right() < rootRow.radio.left());
  CHECK(bracketRow.name.right() < bracketRow.radio.left());
  // The eye stays before the name.
  CHECK(!glassRow.eye.isEmpty());
  CHECK(glassRow.eye.right() < glassRow.name.left());

  // Both on the right: the eyes in one column left of the radio buttons,
  // also on rows without a radio button.
  panel.setControlPlaces(BrowserControlPlace::Right, BrowserControlPlace::Right);
  rootRow = panel.rowPlaces(root);
  bracketRow = panel.rowPlaces(bracket);
  glassRow = panel.rowPlaces(glass);
  const BrowserPanel::RowPlaces blockRow = panel.rowPlaces(block);
  CHECK(!bracketRow.eye.isEmpty() && !glassRow.eye.isEmpty() && !blockRow.eye.isEmpty());
  CHECK(std::abs(middle(bracketRow.eye) - middle(glassRow.eye)) <= 1);
  CHECK(std::abs(middle(bracketRow.eye) - middle(blockRow.eye)) <= 1);
  CHECK(bracketRow.eye.right() < bracketRow.radio.left());
  CHECK(std::abs(middle(rootRow.radio) - middle(bracketRow.radio)) <= 1);
  CHECK(glassRow.name.right() < glassRow.eye.left());
  CHECK(bracketRow.name.right() < bracketRow.eye.left());

  // A long name: elided before the column, which stays where it was.
  const QString longName = QStringLiteral("Bracket-for-the-left-side-motor-unit-with-the-mounting-plate:1");
  panel.rebuild(design(longName), false);
  tree->expandAll();
  QCoreApplication::processEvents();
  const BrowserPanel::RowPlaces longRow = panel.rowPlaces(QStringLiteral("Root/") + longName);
  rootRow = panel.rowPlaces(root);
  CHECK(longRow.elided);
  CHECK(!longRow.eye.isEmpty() && !longRow.radio.isEmpty());
  CHECK(longRow.name.right() < longRow.eye.left());
  CHECK(std::abs(middle(longRow.radio) - middle(rootRow.radio)) <= 1);

  // Classic again: as at the start.
  panel.setControlPlaces(BrowserControlPlace::Classic, BrowserControlPlace::Classic);
  glassRow = panel.rowPlaces(glass);
  CHECK(glassRow.eye.right() < glassRow.name.left());
  rootRow = panel.rowPlaces(root);
  CHECK(rootRow.radio.left() > rootRow.name.right());
  CHECK(rootRow.radio.right() < edge - 16);
}

} // namespace

int browserControlPlacesTests() {
  const QSettings::Format format = QSettings::defaultFormat();
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
  QCoreApplication::setApplicationName(QStringLiteral("browser_control_places"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QTemporaryDir directory;
  CHECK(directory.isValid());
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());

  preferenceTests();
  layoutTests();

  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}

MITCAD_UNIT_TEST(browserControlPlacesTests);
