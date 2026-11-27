// SPDX-License-Identifier: MIT
// The Browser rows' spacing (mitcad#2513, GitHub mitcad/Mitcad#675): the
// preference (Standard without a saved choice, Compact and Comfortable saved
// and read back as the next start reads them, Restore Defaults saving
// Standard, an unknown value read as Standard, carried by settings export and
// import), the row height each spacing gives (Standard the height each
// layout always had, Compact less but never below the icon and the name,
// Comfortable more), and a BrowserPanel's rows in the Floating and the Docked
// layout laid out again with each spacing: the selection, the current row,
// the row at the top of a scrolled view and the light bulb's and the radio
// button's places on the row kept.
#include "UnitTests.hpp"
#include "browser/BrowserPanel.hpp"
#include "browser/BrowserPreferences.hpp"
#include "framework/ChromeStyle.hpp"
#include "framework/SettingsTransfer.hpp"
#include "framework/WindowStyle.hpp"

#include <cstdio>

#include <QApplication>
#include <QCoreApplication>
#include <QObject>
#include <QScrollBar>
#include <QSettings>
#include <QString>
#include <QTemporaryDir>
#include <QTreeWidget>
#include <QTreeWidgetItemIterator>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "browser_row_spacing.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

const QString kSpacingKey = QStringLiteral("browser/rowSpacing");

// What the settings file holds now, as the next start reads it.
QString savedValue(const QString& key) {
  QSettings settings;
  settings.sync();
  return settings.value(key).toString();
}

void preferenceTests() {
  int told = 0;
  const QMetaObject::Connection counting = QObject::connect(
      &BrowserPreferencesNotifier::instance(), &BrowserPreferencesNotifier::rowSpacingChanged, [&told] { ++told; });

  // The default: Standard, nothing saved.
  CHECK(!QSettings().contains(kSpacingKey));
  CHECK(browserRowSpacing() == BrowserRowSpacing::Standard);

  // The values' words, and back; an unknown one is Standard.
  CHECK(browserRowSpacingId(BrowserRowSpacing::Compact) == QStringLiteral("compact"));
  CHECK(browserRowSpacingId(BrowserRowSpacing::Standard) == QStringLiteral("standard"));
  CHECK(browserRowSpacingId(BrowserRowSpacing::Comfortable) == QStringLiteral("comfortable"));
  CHECK(browserRowSpacingFromId(QStringLiteral("compact")) == BrowserRowSpacing::Compact);
  CHECK(browserRowSpacingFromId(QStringLiteral("standard")) == BrowserRowSpacing::Standard);
  CHECK(browserRowSpacingFromId(QStringLiteral("comfortable")) == BrowserRowSpacing::Comfortable);
  CHECK(browserRowSpacingFromId(QStringLiteral("dense")) == BrowserRowSpacing::Standard);
  CHECK(browserRowSpacingFromId(QString()) == BrowserRowSpacing::Standard);

  // Compact saved (the next start reads it) and read back; the windows told
  // once per change.
  setBrowserRowSpacing(BrowserRowSpacing::Compact);
  CHECK(savedValue(kSpacingKey) == QStringLiteral("compact"));
  CHECK(browserRowSpacing() == BrowserRowSpacing::Compact);
  CHECK(told == 1);
  setBrowserRowSpacing(BrowserRowSpacing::Compact); // no change
  CHECK(told == 1);
  setBrowserRowSpacing(BrowserRowSpacing::Comfortable);
  CHECK(savedValue(kSpacingKey) == QStringLiteral("comfortable"));
  CHECK(browserRowSpacing() == BrowserRowSpacing::Comfortable);
  CHECK(told == 2);

  // Restore Defaults saves Standard.
  setBrowserRowSpacing(BrowserRowSpacing::Standard);
  CHECK(savedValue(kSpacingKey) == QStringLiteral("standard"));
  CHECK(browserRowSpacing() == BrowserRowSpacing::Standard);
  CHECK(told == 3);

  // Standard saved over no choice (Restore Defaults on fresh settings): the
  // value written, the windows not told (nothing changed for them).
  QSettings().remove(kSpacingKey);
  setBrowserRowSpacing(BrowserRowSpacing::Standard);
  CHECK(savedValue(kSpacingKey) == QStringLiteral("standard"));
  CHECK(told == 3);

  // An unknown saved value is Standard.
  QSettings().setValue(kSpacingKey, QStringLiteral("tight"));
  CHECK(browserRowSpacing() == BrowserRowSpacing::Standard);
  QSettings().remove(kSpacingKey);

  // Settings export and import carry it, with the other preferences.
  CHECK(settingsGroupOf(kSpacingKey) == QStringLiteral("preferences"));
  QObject::disconnect(counting);
}

void heightTests() {
  // Standard: the layout's least height, or the style's when it is larger
  // (a larger text or icon size), as it always was.
  CHECK(spacedBrowserRowHeight(BrowserRowSpacing::Standard, 26, 20, 22) == 26);
  CHECK(spacedBrowserRowHeight(BrowserRowSpacing::Standard, panel::kRowHeight, 20, 22) == panel::kRowHeight);
  CHECK(spacedBrowserRowHeight(BrowserRowSpacing::Standard, 26, 31, 33) == 31);
  // Compact: a fifth less, not below what the icon and the name need.
  CHECK(spacedBrowserRowHeight(BrowserRowSpacing::Compact, 30, 18, 20) == 24);
  CHECK(spacedBrowserRowHeight(BrowserRowSpacing::Compact, 26, 20, 22) == 22);
  CHECK(spacedBrowserRowHeight(BrowserRowSpacing::Compact, 26, 20, 18) == 21);
  CHECK(spacedBrowserRowHeight(BrowserRowSpacing::Compact, 26, 31, 33) == 33);
  // Comfortable: a quarter more, also on top of a larger style's height.
  CHECK(spacedBrowserRowHeight(BrowserRowSpacing::Comfortable, 26, 20, 22) == 32);
  CHECK(spacedBrowserRowHeight(BrowserRowSpacing::Comfortable, panel::kRowHeight, 20, 22) == 35);
  CHECK(spacedBrowserRowHeight(BrowserRowSpacing::Comfortable, 26, 32, 34) == 40);
  // Every spacing keeps the row whole.
  for (const BrowserRowSpacing spacing :
       {BrowserRowSpacing::Compact, BrowserRowSpacing::Standard, BrowserRowSpacing::Comfortable}) {
    for (const int content : {14, 20, 26, 40}) {
      CHECK(spacedBrowserRowHeight(spacing, 26, content, content + 2) >= content);
    }
  }
}

DocumentSnapshot::Component component(const QString& uid, const QString& name, const QStringList& bodies) {
  DocumentSnapshot::Component made;
  made.uid = uid;
  made.name = name;
  made.bodies = bodies;
  return made;
}

// Root (C0) with twelve bodies (F1.b0 … F12.b0, Body 1 … Body 12); Bracket
// (C1) placed once in the root (O1, Bracket:1), with the body Block: more
// rows than a short panel shows.
DocumentSnapshot design() {
  DocumentSnapshot snapshot;
  QStringList rootBodies;
  for (int i = 1; i <= 12; ++i) {
    DocumentSnapshot::Body body;
    body.uid = QStringLiteral("F%1.b0").arg(i);
    body.name = QStringLiteral("Body %1").arg(i);
    body.component = QStringLiteral("C0");
    body.visible = true;
    snapshot.bodies.insert(body.uid, body);
    rootBodies << body.uid;
  }
  DocumentSnapshot::Body block;
  block.uid = QStringLiteral("F20.b0");
  block.name = QStringLiteral("Block");
  block.component = QStringLiteral("C1");
  block.visible = true;
  snapshot.bodies.insert(block.uid, block);
  snapshot.components.append(component(QStringLiteral("C0"), QStringLiteral("Root"), rootBodies));
  snapshot.components.append(component(QStringLiteral("C1"), QStringLiteral("Bracket"), {block.uid}));
  DocumentSnapshot::Occurrence bracket;
  bracket.uid = QStringLiteral("O1");
  bracket.name = QStringLiteral("Bracket:1");
  bracket.path = QStringLiteral("O1");
  bracket.component = QStringLiteral("C1");
  bracket.visible = true;
  snapshot.occurrences.append(bracket);
  snapshot.activeComponent = QStringLiteral("C0");
  return snapshot;
}

QTreeWidgetItem* rowNamed(QTreeWidget* tree, const QString& name) {
  for (QTreeWidgetItemIterator it(tree); *it != nullptr; ++it) {
    if ((*it)->text(0) == name) {
      return *it;
    }
  }
  return nullptr;
}

// The row's height as the tree lays it out now.
int heightOf(QTreeWidget* tree, QTreeWidgetItem* item) { return tree->visualItemRect(item).height(); }

// A part of a row (the light bulb, the radio button) whole within its row.
bool within(const QRect& part, const QRect& row) {
  return !part.isEmpty() && part.top() >= row.top() && part.bottom() <= row.bottom();
}

void panelTests(ChromeStyle style, int standard) {
  const ChromeStyle was = chromeStyle();
  setChromeStyle(style);
  {
    BrowserPanel panel;
    panel.resize(320, 160); // shorter than the rows
    panel.rebuild(design(), false);
    panel.show();
    QCoreApplication::processEvents();
    auto* tree = panel.findChild<QTreeWidget*>();
    CHECK(tree != nullptr);
    if (tree == nullptr) {
      setChromeStyle(was);
      return;
    }
    tree->expandAll();
    QCoreApplication::processEvents();
    QTreeWidgetItem* root = rowNamed(tree, QStringLiteral("Root"));
    QTreeWidgetItem* bracket = rowNamed(tree, QStringLiteral("Bracket:1"));
    QTreeWidgetItem* picked = rowNamed(tree, QStringLiteral("Body 7"));
    QTreeWidgetItem* other = rowNamed(tree, QStringLiteral("Body 9"));
    QTreeWidgetItem* top = rowNamed(tree, QStringLiteral("Body 4"));
    CHECK(root != nullptr && bracket != nullptr && picked != nullptr && other != nullptr && top != nullptr);
    if (root == nullptr || bracket == nullptr || picked == nullptr || other == nullptr || top == nullptr) {
      setChromeStyle(was);
      return;
    }

    // Standard, the default: the height the layout always had.
    CHECK(panel.rowSpacing() == BrowserRowSpacing::Standard);
    const int standardHeight = heightOf(tree, picked);
    CHECK(standardHeight >= standard);

    // Two rows selected, one current, and the view scrolled to a row.
    tree->setCurrentItem(picked);
    other->setSelected(true);
    tree->scrollToItem(top, QAbstractItemView::PositionAtTop);
    QCoreApplication::processEvents();
    CHECK(tree->verticalScrollBar()->value() > 0);
    CHECK(tree->itemAt(1, 1) == top);

    const auto kept = [&](const char* what) {
      CHECK(tree->currentItem() == picked);
      CHECK(picked->isSelected() && other->isSelected());
      CHECK(tree->selectedItems().size() == 2);
      // The row at the top of the view is still at the top.
      if (tree->itemAt(1, 1) != top) {
        std::fprintf(stderr, "browser_row_spacing.cpp: %s: another row at the top\n", what);
        ++failures;
      }
      // Every row the same height, the controls whole on their rows.
      CHECK(heightOf(tree, other) == heightOf(tree, picked));
      const BrowserPanel::RowPlaces body = panel.rowPlaces(QStringLiteral("Root/Bodies/Body 7"));
      CHECK(within(body.eye, tree->visualItemRect(picked)));
      CHECK(body.eye.right() < body.name.left());
      CHECK(!body.name.isEmpty() && !body.elided);
    };

    // Compact: lower rows, at least the icon and the name with some air.
    panel.setRowSpacing(BrowserRowSpacing::Compact);
    QCoreApplication::processEvents();
    CHECK(panel.rowSpacing() == BrowserRowSpacing::Compact);
    const int compactHeight = heightOf(tree, picked);
    CHECK(compactHeight < standardHeight);
    CHECK(compactHeight >= tree->iconSize().height() + 4);
    CHECK(compactHeight >= tree->fontMetrics().height() + 4);
    kept("compact");

    // Comfortable: higher rows.
    panel.setRowSpacing(BrowserRowSpacing::Comfortable);
    QCoreApplication::processEvents();
    const int comfortableHeight = heightOf(tree, picked);
    CHECK(comfortableHeight > standardHeight);
    kept("comfortable");

    // The Activate radio button whole on its row with every spacing: on the
    // row of the occurrence, scrolled into sight.
    tree->scrollToItem(bracket, QAbstractItemView::PositionAtTop);
    QCoreApplication::processEvents();
    for (const BrowserRowSpacing spacing :
         {BrowserRowSpacing::Compact, BrowserRowSpacing::Standard, BrowserRowSpacing::Comfortable}) {
      panel.setRowSpacing(spacing);
      QCoreApplication::processEvents();
      const BrowserPanel::RowPlaces occurrence = panel.rowPlaces(QStringLiteral("Root/Bracket:1"));
      CHECK(within(occurrence.radio, tree->visualItemRect(bracket)));
      CHECK(occurrence.radio.left() > occurrence.name.right());
    }

    // Standard again: as at the start.
    tree->scrollToItem(top, QAbstractItemView::PositionAtTop);
    QCoreApplication::processEvents();
    panel.setRowSpacing(BrowserRowSpacing::Comfortable);
    QCoreApplication::processEvents();
    panel.setRowSpacing(BrowserRowSpacing::Standard);
    QCoreApplication::processEvents();
    CHECK(heightOf(tree, picked) == standardHeight);
    kept("standard");
  }
  setChromeStyle(was);
}

} // namespace

int browserRowSpacingTests() {
  const QSettings::Format format = QSettings::defaultFormat();
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
  QCoreApplication::setApplicationName(QStringLiteral("browser_row_spacing"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QTemporaryDir directory;
  CHECK(directory.isValid());
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());

  preferenceTests();
  heightTests();
  panelTests(ChromeStyle::Floating, 26);
  panelTests(ChromeStyle::Docked, panel::kRowHeight);

  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}

MITCAD_UNIT_TEST(browserRowSpacingTests);
