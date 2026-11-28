// SPDX-License-Identifier: MIT
// The active component's highlight in the Browser (mitcad#2618, GitHub
// mitcad/Mitcad#670): the preference (off without a saved choice, saved and
// read back as the next start reads it, Restore Defaults saving off, carried
// by settings export and import), the tint's one intensity (the rows' name
// colours at least 4.5:1 on it in the light and the dark theme with every
// component colour, also under the Rows background's hover tint), and a
// BrowserPanel's rows in the Docked, the Floating Card and the Floating Rows
// layout: only the active component's headers tinted (the root, or every
// occurrence of the active component, nested ones too), in its component's
// colour; the tint following activation and the new component New Component
// activates; a selected header showing the selection instead and its tint
// again once the selection moves on; nothing tinted with the preference off.
#include "UnitTests.hpp"
#include "browser/BrowserPanel.hpp"
#include "browser/BrowserPreferences.hpp"
#include "browser/BrowserRows.hpp"
#include "browser/ComponentColors.hpp"
#include "framework/ChromeStyle.hpp"
#include "framework/SettingsTransfer.hpp"
#include "framework/Theme.hpp"

#include <cstdio>
#include <utility>

#include <QApplication>
#include <QColor>
#include <QCoreApplication>
#include <QImage>
#include <QVector>
#include <QObject>
#include <QSettings>
#include <QString>
#include <QTemporaryDir>
#include <QTreeWidget>
#include <QTreeWidgetItemIterator>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "browser_active_highlight.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

const QString kHighlightKey = QStringLiteral("browser/highlightActiveComponent");

// What the settings file holds now, as the next start reads it.
QVariant savedValue(const QString& key) {
  QSettings settings;
  settings.sync();
  return settings.value(key);
}

void preferenceTests() {
  int told = 0;
  const QMetaObject::Connection counting =
      QObject::connect(&BrowserPreferencesNotifier::instance(), &BrowserPreferencesNotifier::highlightActiveChanged,
                       [&told] { ++told; });

  // The default: off, nothing saved.
  CHECK(!QSettings().contains(kHighlightKey));
  CHECK(!browserHighlightActive());

  // On saved (the next start reads it) and read back; the windows told once
  // per change.
  setBrowserHighlightActive(true);
  CHECK(savedValue(kHighlightKey).toBool());
  CHECK(browserHighlightActive());
  CHECK(told == 1);
  setBrowserHighlightActive(true); // no change
  CHECK(told == 1);

  // Off again (Restore Defaults saves off).
  setBrowserHighlightActive(false);
  CHECK(savedValue(kHighlightKey).isValid() && !savedValue(kHighlightKey).toBool());
  CHECK(!browserHighlightActive());
  CHECK(told == 2);

  // Off saved over no choice (Restore Defaults on fresh settings): the value
  // written, the windows not told (nothing changed for them).
  QSettings().remove(kHighlightKey);
  setBrowserHighlightActive(false);
  CHECK(QSettings().contains(kHighlightKey));
  CHECK(told == 2);
  QSettings().remove(kHighlightKey);

  // Settings export and import carry it, with the other preferences.
  CHECK(settingsGroupOf(kHighlightKey) == QStringLiteral("preferences"));
  QObject::disconnect(counting);
}

// `top` at `alpha` over the opaque `under`.
QColor over(const QColor& top, double alpha, const QColor& under) {
  const auto mix = [alpha](int a, int b) { return qRound(alpha * a + (1.0 - alpha) * b); };
  return QColor(mix(top.red(), under.red()), mix(top.green(), under.green()), mix(top.blue(), under.blue()));
}

void contrastTests() {
  // One intensity, translucent: the theme's surface shows through.
  CHECK(browserRows::kActiveComponentTintAlpha == 0.25);
  const QColor orange = rootComponentColor();
  const QColor tint = browserRows::activeComponentTint(orange);
  CHECK(tint.isValid() && tint.rgb() == orange.rgb());
  CHECK(qAbs(tint.alphaF() - 0.25) < 0.01);
  CHECK(!browserRows::activeComponentTint(QColor()).isValid());

  QVector<QColor> components = {rootComponentColor()};
  for (int i = 0; i < 8; ++i) {
    components.append(componentColor(i));
  }
  for (const bool dark : {false, true}) {
    const QColor surface = themeColor(ColorRole::Surface, dark);
    const QColor text = themeColor(ColorRole::Text, dark);
    const QColor hidden = themeColor(ColorRole::TextHidden, dark);
    for (const QColor& component : std::as_const(components)) {
      const QColor shown = browserRows::activeComponentTintOn(component, surface);
      // A tint, not the surface: it reads as the component's colour.
      CHECK(shown != surface);
      // The names readable on it (WCAG 2.2 AA for text).
      CHECK(contrastRatio(text, shown) >= 4.5);
      CHECK(contrastRatio(hidden, shown) >= 4.5);
      // Also under the Rows background's hover tint (the text at 8%).
      const QColor hovered = over(text, 0.08, shown);
      CHECK(contrastRatio(text, hovered) >= 4.5);
      CHECK(contrastRatio(hidden, hovered) >= 4.5);
      if (contrastRatio(hidden, hovered) < 4.5) {
        std::fprintf(stderr, "browser_active_highlight.cpp: %s theme, component %s: %.2f:1\n",
                     dark ? "dark" : "light", qPrintable(component.name()), contrastRatio(hidden, hovered));
      }
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

DocumentSnapshot::Occurrence occurrence(const QString& uid, const QString& name, const QString& path,
                                        const QString& componentUid) {
  DocumentSnapshot::Occurrence made;
  made.uid = uid;
  made.name = name;
  made.path = path;
  made.component = componentUid;
  made.visible = true;
  return made;
}

void addBody(DocumentSnapshot& snapshot, const QString& uid, const QString& name, const QString& owner) {
  DocumentSnapshot::Body body;
  body.uid = uid;
  body.name = name;
  body.component = owner;
  body.visible = true;
  snapshot.bodies.insert(body.uid, body);
}

// Root (C0) with a body; Bracket (C1) placed twice in the root (Bracket:1,
// Bracket:2), each with Plate (C2) nested in it (Plate:1); Cover (C3)
// placed once in the root. With `added`, a new component (C4, Component1)
// placed in the root, as New Component makes it. `active` is the active
// component.
DocumentSnapshot design(const QString& active, bool added = false) {
  DocumentSnapshot snapshot;
  addBody(snapshot, QStringLiteral("F1.b0"), QStringLiteral("Base"), QStringLiteral("C0"));
  addBody(snapshot, QStringLiteral("F2.b0"), QStringLiteral("Arm"), QStringLiteral("C1"));
  addBody(snapshot, QStringLiteral("F3.b0"), QStringLiteral("Sheet"), QStringLiteral("C2"));
  addBody(snapshot, QStringLiteral("F4.b0"), QStringLiteral("Lid"), QStringLiteral("C3"));
  snapshot.components.append(component(QStringLiteral("C0"), QStringLiteral("Root"), {QStringLiteral("F1.b0")}));
  snapshot.components.append(component(QStringLiteral("C1"), QStringLiteral("Bracket"), {QStringLiteral("F2.b0")}));
  snapshot.components.append(component(QStringLiteral("C2"), QStringLiteral("Plate"), {QStringLiteral("F3.b0")}));
  snapshot.components.append(component(QStringLiteral("C3"), QStringLiteral("Cover"), {QStringLiteral("F4.b0")}));
  for (int i = 1; i <= 2; ++i) {
    const QString bracketUid = QStringLiteral("O%1").arg(i);
    DocumentSnapshot::Occurrence bracket =
        occurrence(bracketUid, QStringLiteral("Bracket:%1").arg(i), bracketUid, QStringLiteral("C1"));
    const QString plateUid = QStringLiteral("O%1").arg(10 + i);
    bracket.children.append(occurrence(plateUid, QStringLiteral("Plate:1"), bracketUid + QLatin1Char('/') + plateUid,
                                       QStringLiteral("C2")));
    snapshot.occurrences.append(bracket);
  }
  snapshot.occurrences.append(
      occurrence(QStringLiteral("O3"), QStringLiteral("Cover:1"), QStringLiteral("O3"), QStringLiteral("C3")));
  if (added) {
    snapshot.components.append(component(QStringLiteral("C4"), QStringLiteral("Component1"), {}));
    snapshot.occurrences.append(occurrence(QStringLiteral("O4"), QStringLiteral("Component1:1"),
                                           QStringLiteral("O4"), QStringLiteral("C4")));
  }
  snapshot.activeComponent = active;
  return snapshot;
}

const QStringList kHeaders = {
    QStringLiteral("Root"),
    QStringLiteral("Root/Bracket:1"),
    QStringLiteral("Root/Bracket:2"),
    QStringLiteral("Root/Bracket:1/Plate:1"),
    QStringLiteral("Root/Bracket:2/Plate:1"),
    QStringLiteral("Root/Cover:1"),
};

// A row's path as the panel names it: its name after its parents' names.
QString pathOf(const QTreeWidgetItem* item) {
  QString path = item->text(0);
  for (const QTreeWidgetItem* parent = item->parent(); parent != nullptr; parent = parent->parent()) {
    path = parent->text(0) + QLatin1Char('/') + path;
  }
  return path;
}

QTreeWidgetItem* rowAt(QTreeWidget* tree, const QString& path) {
  for (QTreeWidgetItemIterator it(tree); *it != nullptr; ++it) {
    if (pathOf(*it) == path) {
      return *it;
    }
  }
  return nullptr;
}

// The rows tinted now, by their paths.
QStringList tinted(const BrowserPanel& panel, QTreeWidget* tree) {
  QStringList paths;
  for (QTreeWidgetItemIterator it(tree); *it != nullptr; ++it) {
    const QString path = pathOf(*it);
    if (panel.activeTint(path).isValid()) {
      paths << path;
    }
  }
  return paths;
}

// The colour the tree paints at a point of a row, scrolled into sight: in
// the band's place (Docked, Floating Card) near the viewport's right edge,
// with the Rows background on the row's box just after its radio button.
QRgb paintedAt(const BrowserPanel& panel, QTreeWidget* tree, const QString& path, bool rows) {
  QTreeWidgetItem* item = rowAt(tree, path);
  if (item == nullptr) {
    return 0;
  }
  tree->scrollToItem(item);
  QCoreApplication::processEvents();
  const QRect row = tree->visualItemRect(item);
  int x = tree->viewport()->width() - 12;
  if (rows) {
    const BrowserPanel::RowPlaces places = panel.rowPlaces(path);
    x = places.radio.right() + 4;
  }
  const QImage image = tree->viewport()->grab().toImage();
  const qreal ratio = image.devicePixelRatio();
  return image.pixel(qRound(x * ratio), qRound(row.center().y() * ratio));
}

void settle(QTreeWidget* tree) {
  tree->expandAll();
  QCoreApplication::processEvents();
}

void panelTests(ChromeStyle style, bool rows, const char* name) {
  const ChromeStyle was = chromeStyle();
  setChromeStyle(style);
  {
    BrowserPanel panel;
    panel.resize(360, 720);
    panel.setRowsBackground(rows);
    panel.rebuild(design(QStringLiteral("C1")), false);
    panel.show();
    auto* tree = panel.findChild<QTreeWidget*>();
    CHECK(tree != nullptr);
    if (tree == nullptr) {
      setChromeStyle(was);
      return;
    }
    settle(tree);
    for (const QString& path : kHeaders) {
      if (rowAt(tree, path) == nullptr) {
        std::fprintf(stderr, "browser_active_highlight.cpp: %s: no row %s\n", name, qPrintable(path));
        ++failures;
        setChromeStyle(was);
        return;
      }
    }
    const QVector<DocumentSnapshot::Component> components = design(QStringLiteral("C1"), true).components;
    const auto colourOf = [&components](const QString& uid) { return componentRowColor(components, uid).rgb(); };

    // Off, the default: no row tinted, an active header painted as any other.
    CHECK(!panel.highlightActive());
    CHECK(tinted(panel, tree).isEmpty());
    CHECK(paintedAt(panel, tree, QStringLiteral("Root/Bracket:1"), rows) ==
          paintedAt(panel, tree, QStringLiteral("Root/Cover:1"), rows));

    // On: both occurrences of the active Bracket, as their radio buttons, in
    // Bracket's colour; nothing else (not the root, the other components,
    // their folders or bodies).
    panel.setHighlightActive(true);
    QCoreApplication::processEvents();
    CHECK(panel.highlightActive());
    CHECK(tinted(panel, tree) == QStringList({QStringLiteral("Root/Bracket:1"), QStringLiteral("Root/Bracket:2")}));
    CHECK(panel.activeTint(QStringLiteral("Root/Bracket:1")).rgb() == colourOf(QStringLiteral("C1")));
    CHECK(qAbs(panel.activeTint(QStringLiteral("Root/Bracket:1")).alphaF() - 0.25) < 0.01);
    const QRgb plain = paintedAt(panel, tree, QStringLiteral("Root/Cover:1"), rows);
    const QRgb active = paintedAt(panel, tree, QStringLiteral("Root/Bracket:1"), rows);
    if (active == plain) {
      std::fprintf(stderr, "browser_active_highlight.cpp: %s: the active header is not tinted\n", name);
      ++failures;
    }
    CHECK(paintedAt(panel, tree, QStringLiteral("Root/Bracket:2"), rows) == active);
    CHECK(paintedAt(panel, tree, QStringLiteral("Root"), rows) == plain);
    // The tinted row is nearer Bracket's colour than the plain row.
    {
      const QColor bracket(colourOf(QStringLiteral("C1")));
      const auto distance = [&bracket](QRgb pixel) {
        const QColor seen = QColor::fromRgba(pixel);
        return qAbs(seen.red() - bracket.red()) + qAbs(seen.green() - bracket.green()) +
               qAbs(seen.blue() - bracket.blue());
      };
      CHECK(distance(active) < distance(plain));
    }

    // Activating the nested Plate: both its occurrences, in Plate's colour.
    panel.rebuild(design(QStringLiteral("C2")), false);
    settle(tree);
    CHECK(tinted(panel, tree) ==
          QStringList({QStringLiteral("Root/Bracket:1/Plate:1"), QStringLiteral("Root/Bracket:2/Plate:1")}));
    CHECK(panel.activeTint(QStringLiteral("Root/Bracket:2/Plate:1")).rgb() == colourOf(QStringLiteral("C2")));
    CHECK(paintedAt(panel, tree, QStringLiteral("Root/Bracket:1"), rows) == plain);

    // Activating the root: its own row, in the root's orange.
    panel.rebuild(design(QStringLiteral("C0")), false);
    settle(tree);
    CHECK(tinted(panel, tree) == QStringList({QStringLiteral("Root")}));
    CHECK(panel.activeTint(QStringLiteral("Root")).rgb() == colourOf(QStringLiteral("C0")));
    CHECK(paintedAt(panel, tree, QStringLiteral("Root"), rows) != plain);

    // New Component: the new component, active, has the tint.
    panel.rebuild(design(QStringLiteral("C4"), true), false);
    settle(tree);
    CHECK(tinted(panel, tree) == QStringList({QStringLiteral("Root/Component1:1")}));
    CHECK(panel.activeTint(QStringLiteral("Root/Component1:1")).rgb() == colourOf(QStringLiteral("C4")));

    // Selected, the active header shows the selection as any row; selecting
    // another row brings its tint back.
    panel.rebuild(design(QStringLiteral("C1")), false);
    settle(tree);
    tree->setCurrentItem(rowAt(tree, QStringLiteral("Root/Cover:1")));
    QCoreApplication::processEvents();
    const QRgb selected = paintedAt(panel, tree, QStringLiteral("Root/Cover:1"), rows);
    tree->setCurrentItem(rowAt(tree, QStringLiteral("Root/Bracket:1")));
    QCoreApplication::processEvents();
    CHECK(!panel.activeTint(QStringLiteral("Root/Bracket:1")).isValid());
    CHECK(panel.activeTint(QStringLiteral("Root/Bracket:2")).isValid());
    CHECK(paintedAt(panel, tree, QStringLiteral("Root/Bracket:1"), rows) == selected);
    tree->setCurrentItem(rowAt(tree, QStringLiteral("Root/Cover:1")));
    QCoreApplication::processEvents();
    CHECK(panel.activeTint(QStringLiteral("Root/Bracket:1")).isValid());
    CHECK(paintedAt(panel, tree, QStringLiteral("Root/Bracket:1"), rows) == active);
    // The selection kept through a rebuild keeps the tint hidden.
    tree->setCurrentItem(rowAt(tree, QStringLiteral("Root/Bracket:1")));
    panel.rebuild(design(QStringLiteral("C1")), false);
    settle(tree);
    CHECK(!panel.activeTint(QStringLiteral("Root/Bracket:1")).isValid());
    tree->setCurrentItem(rowAt(tree, QStringLiteral("Root/Cover:1")));
    QCoreApplication::processEvents();

    // Off again: the rows as before.
    panel.setHighlightActive(false);
    QCoreApplication::processEvents();
    CHECK(tinted(panel, tree).isEmpty());
    CHECK(paintedAt(panel, tree, QStringLiteral("Root/Bracket:1"), rows) == plain);
  }
  setChromeStyle(was);
}

} // namespace

int browserActiveHighlightTests() {
  const QSettings::Format format = QSettings::defaultFormat();
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
  QCoreApplication::setApplicationName(QStringLiteral("browser_active_highlight"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QTemporaryDir directory;
  CHECK(directory.isValid());
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());

  preferenceTests();
  contrastTests();
  panelTests(ChromeStyle::Docked, false, "Docked");
  panelTests(ChromeStyle::Floating, false, "Floating Card");
  panelTests(ChromeStyle::Floating, true, "Floating Rows");

  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}

MITCAD_UNIT_TEST(browserActiveHighlightTests);
