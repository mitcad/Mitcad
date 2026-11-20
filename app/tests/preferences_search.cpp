// SPDX-License-Identifier: MIT
// Preferences' search (mitcad#2074, GitHub mitcad/Mitcad#450) on a test
// page built as the real ones are: a form row's label, a check box, a
// match only in a field's tooltip, a combo box's item, a table's row and a
// sub-tab are each found and painted; a page's keyword matches the page as
// a whole; two words match in any order; the counts per page are right;
// the page list counts and greys; no match greys every page; Esc clears and
// an empty field leaves Return to the dialog.
#include "UnitTests.hpp"
#include "framework/PreferencesSearch.hpp"
#include "framework/Theme.hpp"

#include <cstdio>
#include <initializer_list>
#include <vector>

#include <QCheckBox>
#include <QComboBox>
#include <QDoubleSpinBox>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QKeyEvent>
#include <QLabel>
#include <QListWidget>
#include <QTabBar>
#include <QTabWidget>
#include <QTableWidget>
#include <QVBoxLayout>
#include <QWidget>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "preferences_search.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

// The pages: General (a label row with a combo box, a label row whose
// field's tooltip says more, a theme row, a check box, a hidden check box),
// Navigation (a table on its first sub-tab, a check box on its second) and
// Display (nothing but its keywords).
struct TestPages {
  QWidget general;
  QWidget navigation;
  QWidget display;
  QLabel* temperatureLabel = nullptr;
  QComboBox* temperature = nullptr;
  QDoubleSpinBox* tolerance = nullptr;
  QComboBox* theme = nullptr;
  QCheckBox* openInCurrent = nullptr;
  QTabWidget* tabs = nullptr;
  QTableWidget* table = nullptr;
  QCheckBox* lockHorizon = nullptr;

  TestPages() {
    auto* form = new QFormLayout(&general);
    temperature = new QComboBox;
    temperature->setObjectName(QStringLiteral("temperatureUnit"));
    temperature->addItems({QStringLiteral("°C"), QStringLiteral("°F")});
    temperature->setToolTip(QStringLiteral("The unit materials' temperatures are shown in"));
    form->addRow(QStringLiteral("Temperature unit:"), temperature);
    temperatureLabel = qobject_cast<QLabel*>(form->labelForField(temperature));
    tolerance = new QDoubleSpinBox;
    tolerance->setToolTip(QStringLiteral("How far replayed geometry of an imported design may be from the file's"));
    form->addRow(QStringLiteral("Import tolerance:"), tolerance);
    auto* units = new QComboBox;
    units->addItems({QStringLiteral("mm"), QStringLiteral("in")});
    form->addRow(QStringLiteral("Default &units for new designs:"), units);
    // The theme and, in the same row, another setting with a label of its
    // own, as Display has them.
    theme = new QComboBox;
    theme->addItems({QStringLiteral("System"), QStringLiteral("Light"), QStringLiteral("Dark")});
    auto* size = new QComboBox;
    size->addItems({QStringLiteral("100%"), QStringLiteral("125%")});
    auto* sizeLabel = new QLabel(QStringLiteral("&Interface size:"));
    sizeLabel->setBuddy(size);
    auto* themeRow = new QHBoxLayout;
    themeRow->addWidget(theme);
    themeRow->addWidget(sizeLabel);
    themeRow->addWidget(size);
    auto* themeLabel = new QLabel(QStringLiteral("&Theme:"));
    themeLabel->setBuddy(theme);
    form->addRow(themeLabel, themeRow);
    openInCurrent = new QCheckBox(QStringLiteral("Open in the current window"));
    form->addRow(openInCurrent);
    auto* hidden = new QCheckBox(QStringLiteral("Hidden option"));
    form->addRow(hidden);
    hidden->hide();

    auto* layout = new QVBoxLayout(&navigation);
    tabs = new QTabWidget;
    layout->addWidget(tabs);
    auto* mouse = new QWidget;
    auto* mouseLayout = new QVBoxLayout(mouse);
    table = new QTableWidget(2, 2);
    table->setHorizontalHeaderLabels({QStringLiteral("Input"), QStringLiteral("Action")});
    const QStringList inputs{QStringLiteral("Middle drag"), QStringLiteral("Right drag")};
    const QStringList actions{QStringLiteral("Orbit"), QStringLiteral("Pan")};
    for (int row = 0; row < 2; ++row) {
      table->setItem(row, 0, new QTableWidgetItem(inputs.at(row)));
      auto* action = new QComboBox;
      action->addItems({QStringLiteral("None"), QStringLiteral("Orbit"), QStringLiteral("Pan"), QStringLiteral("Zoom")});
      action->setCurrentText(actions.at(row));
      table->setCellWidget(row, 1, action);
    }
    mouseLayout->addWidget(table);
    tabs->addTab(mouse, QStringLiteral("Mouse and Trackpad"));
    auto* space = new QWidget;
    auto* spaceLayout = new QVBoxLayout(space);
    lockHorizon = new QCheckBox(QStringLiteral("Lock horizon"));
    spaceLayout->addWidget(lockHorizon);
    tabs->addTab(space, QStringLiteral("3D Mouse"));
  }
};

bool painted(const QWidget* widget) {
  return widget->palette().color(QPalette::Active, QPalette::WindowText) == themeColor(ColorRole::SearchMatch);
}

bool flagged(const QWidget* widget) { return widget->property(PreferencesSearch::kMatchProperty).toBool(); }

std::vector<int> counts(std::initializer_list<int> values) { return std::vector<int>(values); }

void searchTests() {
  TestPages pages;
  PreferencesSearch search;
  search.addPage(&pages.general, QStringLiteral("General"), {QStringLiteral("language"), QStringLiteral("celsius")});
  search.addPage(&pages.navigation, QStringLiteral("Navigation"), {QStringLiteral("mouse"), QStringLiteral("orbit")});
  search.addPage(&pages.display, QStringLiteral("Display"),
                 {QStringLiteral("colour"), QStringLiteral("color"), QStringLiteral("quick access toolbar")});

  // A form row's label: painted, its control marked, the other pages none.
  CHECK(search.apply(QStringLiteral("temperature")) == counts({1, 0, 0}));
  CHECK(search.matchTexts(0) == QStringList{QStringLiteral("Temperature unit")});
  CHECK(pages.temperatureLabel != nullptr && painted(pages.temperatureLabel));
  CHECK(flagged(pages.temperature));
  CHECK(search.revealFirst(0) == pages.temperature);
  CHECK(search.total() == 1);

  // A combo box's item, case-insensitive; the earlier match put back.
  CHECK(search.apply(QStringLiteral("DARK")) == counts({1, 0, 0}));
  CHECK(search.matchTexts(0) == QStringList{QStringLiteral("Theme")});
  CHECK(flagged(pages.theme));
  CHECK(!painted(pages.temperatureLabel));
  CHECK(!pages.temperatureLabel->testAttribute(Qt::WA_SetPalette));
  CHECK(!flagged(pages.temperature));

  // A control with a label of its own in another label's row is that
  // label's only.
  CHECK(search.apply(QStringLiteral("125%")) == counts({1, 0, 0}));
  CHECK(search.matchTexts(0) == QStringList{QStringLiteral("Interface size")});

  // A match only in a field's tooltip.
  CHECK(search.apply(QStringLiteral("replayed")) == counts({1, 0, 0}));
  CHECK(search.matchTexts(0) == QStringList{QStringLiteral("Import tolerance")});
  CHECK(search.revealFirst(0) == pages.tolerance);

  // A check box, by two words in any order; a hidden one is not found.
  CHECK(search.apply(QStringLiteral("window open")) == counts({1, 0, 0}));
  CHECK(search.matchTexts(0) == QStringList{QStringLiteral("Open in the current window")});
  CHECK(painted(pages.openInCurrent));
  CHECK(search.apply(QStringLiteral("hidden option")) == counts({0, 0, 0}));

  // A table's row by a cell and its action's choice (not every item of the
  // combo box: "zoom" is no row's).
  CHECK(search.apply(QStringLiteral("middle orbit")) == counts({0, 1, 0}));
  CHECK(search.matchTexts(1) == QStringList{QStringLiteral("Middle drag / Orbit")});
  CHECK(flagged(pages.table));
  CHECK(pages.table->item(0, 0)->foreground().color() == themeColor(ColorRole::SearchMatch));
  CHECK(pages.table->item(1, 0)->foreground().color() != themeColor(ColorRole::SearchMatch));
  // "zoom" is no row's choice: only the page's own words could match, and
  // they do not hold it.
  CHECK(search.apply(QStringLiteral("zoom")) == counts({0, 0, 0}));
  // The table's header.
  CHECK(search.apply(QStringLiteral("action")) == counts({0, 1, 0}));

  // A sub-tab by its name: marked and opened.
  CHECK(pages.tabs->currentIndex() == 0);
  CHECK(search.apply(QStringLiteral("3d mouse")) == counts({0, 1, 0}));
  CHECK(search.matchTexts(1) == QStringList{QStringLiteral("3D Mouse")});
  CHECK(pages.tabs->currentIndex() == 1);
  CHECK(pages.tabs->tabBar()->tabTextColor(1) == themeColor(ColorRole::SearchMatch));
  // A setting on another tab than the one shown opens its tab.
  pages.tabs->setCurrentIndex(0);
  CHECK(search.apply(QStringLiteral("horizon")) == counts({0, 1, 0}));
  CHECK(pages.tabs->currentIndex() == 1);
  CHECK(painted(pages.lockHorizon));
  CHECK(search.revealFirst(1) == pages.lockHorizon);
  // The tab shown keeps its place when it has a match itself.
  pages.tabs->setCurrentIndex(0);
  CHECK(search.apply(QStringLiteral("drag")) == counts({0, 2, 0}));
  CHECK(pages.tabs->currentIndex() == 0);

  // A page's keyword matches the page as a whole, with no setting painted;
  // its settings count when they match too.
  CHECK(search.apply(QStringLiteral("colour")) == counts({0, 0, 1}));
  CHECK(search.matchTexts(2).isEmpty());
  CHECK(search.revealFirst(2) == nullptr);
  CHECK(search.apply(QStringLiteral("access toolbar")) == counts({0, 0, 1}));
  CHECK(search.apply(QStringLiteral("orbit")) == counts({0, 1, 0}));

  // Counts per page over several pages.
  CHECK(search.apply(QStringLiteral("unit")) == counts({2, 0, 0}));
  CHECK(search.apply(QStringLiteral("o")).size() == 3);
  CHECK(search.counts()[0] >= 3 && search.counts()[1] >= 3);

  // No match anywhere.
  CHECK(search.apply(QStringLiteral("xyzzy")) == counts({0, 0, 0}));
  CHECK(search.total() == 0);
  // An empty search: nothing counted, nothing painted.
  CHECK(search.apply(QString()) == counts({0, 0, 0}));
  CHECK(!painted(pages.lockHorizon));
  CHECK(!pages.tabs->tabBar()->tabTextColor(1).isValid());
}

void fieldTests() {
  TestPages pages;
  QListWidget list;
  const QStringList names{QStringLiteral("General"), QStringLiteral("Navigation"), QStringLiteral("Display")};
  list.addItems(names);
  list.setCurrentRow(2);
  int shown = -1;
  PreferencesSearchField field(&list, [&list, &shown](int row) {
    shown = row;
    list.setCurrentRow(row);
  });
  field.addPage(&pages.general, names.at(0), {QStringLiteral("language")});
  field.addPage(&pages.navigation, names.at(1), {QStringLiteral("mouse")});
  field.addPage(&pages.display, names.at(2), {QStringLiteral("colour")});
  CHECK(field.objectName() == QStringLiteral("preferencesSearch"));
  CHECK(field.placeholderText() == QStringLiteral("Search settings"));
  auto* noMatch = field.findChild<QLabel*>(QStringLiteral("preferencesSearchNoMatch"));
  CHECK(noMatch != nullptr && noMatch->isHidden());

  // The counts in the list, the pages without a match greyed; Display shown
  // had none, so General (the first with one) is shown.
  field.setText(QStringLiteral("temperature"));
  CHECK(list.item(0)->text() == QStringLiteral("General (1)"));
  CHECK(list.item(1)->text() == QStringLiteral("Navigation"));
  CHECK(!list.item(0)->data(Qt::ForegroundRole).isValid());
  CHECK(list.item(1)->data(Qt::ForegroundRole).isValid());
  CHECK(list.item(2)->data(Qt::ForegroundRole).isValid());
  CHECK(shown == 0 && list.currentRow() == 0);
  // A greyed page can be shown; the text stays.
  list.setCurrentRow(2);
  CHECK(field.text() == QStringLiteral("temperature"));
  CHECK(list.item(2)->flags().testFlag(Qt::ItemIsEnabled));

  // Return with text is the field's, never the dialog's OK.
  QKeyEvent enter(QEvent::KeyPress, Qt::Key_Return, Qt::NoModifier);
  enter.ignore();
  QCoreApplication::sendEvent(&field, &enter);
  CHECK(enter.isAccepted());
  CHECK(list.currentRow() == 0);

  // No match: every page greyed and the field says so.
  field.setText(QStringLiteral("xyzzy"));
  for (int row = 0; row < list.count(); ++row) {
    CHECK(list.item(row)->data(Qt::ForegroundRole).isValid());
    CHECK(list.item(row)->text() == names.at(row));
  }
  CHECK(noMatch != nullptr && !noMatch->isHidden());

  // Esc clears; the list shows the plain names again.
  QKeyEvent escape(QEvent::KeyPress, Qt::Key_Escape, Qt::NoModifier);
  escape.ignore();
  QCoreApplication::sendEvent(&field, &escape);
  CHECK(escape.isAccepted());
  CHECK(field.text().isEmpty());
  for (int row = 0; row < list.count(); ++row) {
    CHECK(!list.item(row)->data(Qt::ForegroundRole).isValid());
    CHECK(list.item(row)->text() == names.at(row));
  }
  CHECK(noMatch != nullptr && noMatch->isHidden());
  // An empty field leaves Esc and Return to the dialog.
  QKeyEvent closing(QEvent::KeyPress, Qt::Key_Escape, Qt::NoModifier);
  closing.ignore();
  QCoreApplication::sendEvent(&field, &closing);
  CHECK(!closing.isAccepted());
  QKeyEvent ok(QEvent::KeyPress, Qt::Key_Return, Qt::NoModifier);
  ok.ignore();
  QCoreApplication::sendEvent(&field, &ok);
  CHECK(!ok.isAccepted());
}

void queryTests() {
  CHECK(preferencesSearchWords(QStringLiteral("  3d   mouse ")) ==
        (QStringList{QStringLiteral("3d"), QStringLiteral("mouse")}));
  CHECK(preferencesSearchMatches(QStringLiteral("Open in the current window"),
                                 {QStringLiteral("WINDOW"), QStringLiteral("open")}));
  CHECK(!preferencesSearchMatches(QStringLiteral("Open in the current window"), {}));
  CHECK(preferencesPlainText(QStringLiteral("Default &units && more:")) == QStringLiteral("Default units & more:"));
  // The command search's words that are not the command's name.
  const QString general = QStringLiteral("Preferences: General");
  CHECK(preferencesSearchQueryFor(QStringLiteral("temperature"), general) == QStringLiteral("temperature"));
  CHECK(preferencesSearchQueryFor(QStringLiteral("pref temperature"), general) == QStringLiteral("temperature"));
  CHECK(preferencesSearchQueryFor(QStringLiteral("Preferences: General"), general).isEmpty());
}

} // namespace

int preferencesSearchTests() {
  failures = 0;
  queryTests();
  searchTests();
  fieldTests();
  return failures;
}
MITCAD_UNIT_TEST(preferencesSearchTests);
