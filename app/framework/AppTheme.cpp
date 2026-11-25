// SPDX-License-Identifier: MIT
#include "AppTheme.hpp"

#include <QApplication>
#include <QColor>
#include <QCoreApplication>
#include <QGuiApplication>
#include <QObject>
#include <QPalette>
#include <QSettings>
#include <QString>
#include <QStyleHints>
#include <QTimer>
#include <QtLogging>

#include "../platform/WindowsTheme.hpp"
#include "Theme.hpp"

namespace mitcad {
namespace {

const QString kThemeKey = QStringLiteral("appearance/theme");

// How long the platform has to change its palette after a request before
// MitCad sets its own: its theme change comes as an event of the window
// system, after the request returns.
constexpr int kSettleMs = 250;

struct ThemeState {
  ThemeChoice choice = ThemeChoice::System;
  bool ownPalette = false; // MitCad's palette set over the platform's
  bool pending = false;    // a choice waits for the platform's palette
  int generation = 0;      // the latest choice applied, for its timer
  bool watching = false;
  // The choice is a test's (applyTestColorScheme): logged as the scheme
  // in effect, and MitCad's own palette only where `testOwnPalette`.
  bool test = false;
  bool testOwnPalette = true;
};

ThemeState& state() {
  static ThemeState current;
  return current;
}

QString schemeName(bool dark) { return dark ? QStringLiteral("dark") : QStringLiteral("light"); }

// The palette now against the choice waiting; `deadline`: the platform's
// time is up, so a palette that is not as wanted is replaced by MitCad's
// own, and System's palette is the platform's answer.
void settle(bool deadline) {
  ThemeState& current = state();
  if (!current.pending) {
    return;
  }
  const bool dark = isDarkTheme();
  if (current.choice == ThemeChoice::System) {
    if (deadline) {
      current.pending = false;
      qInfo().noquote() << QStringLiteral("Theme: system (%1)").arg(schemeName(dark));
    }
    return;
  }
  const bool wanted = current.choice == ThemeChoice::Dark;
  if (dark == wanted) {
    current.pending = false;
    if (current.test) {
      qInfo().noquote() << QStringLiteral("Colour scheme in effect: %1 (the platform's palette)").arg(schemeName(wanted));
    } else {
      qInfo().noquote() << QStringLiteral("Theme: %1 (preference)").arg(schemeName(wanted));
    }
    return;
  }
  if (!deadline) {
    return;
  }
  current.pending = false;
  if (current.test && !current.testOwnPalette) {
    qInfo().noquote() << QStringLiteral("Colour scheme in effect: %1 (the platform did not switch to %2)")
                             .arg(schemeName(dark), schemeName(wanted));
    return;
  }
  const QPalette palette = themePalette(wanted);
  QApplication::setPalette(palette);
  current.ownPalette = true;
  if (current.test) {
    qInfo().noquote() << QStringLiteral("Colour scheme in effect: %1 (MitCad's own palette, window %2 over the platform's %3 one)")
                             .arg(schemeName(wanted), palette.color(QPalette::Window).name(), schemeName(dark));
    return;
  }
  qInfo().noquote() << QStringLiteral("Theme: %1 (preference; MitCad's own palette, window %2 over the platform's %3 one)")
                           .arg(schemeName(wanted), palette.color(QPalette::Window).name(), schemeName(dark));
}

// Once: a change of the platform's scheme may settle a waiting choice
// early, after the other reactions to it (Windows 10's path sets its
// palette in its own, connected before).
void watch() {
  ThemeState& current = state();
  if (current.watching) {
    return;
  }
  current.watching = true;
  QObject::connect(QGuiApplication::styleHints(), &QStyleHints::colorSchemeChanged, qApp,
                   [] { QTimer::singleShot(0, qApp, [] { settle(false); }); });
}

// The choice asked of the platform, settled by `settle` once the platform
// answers or its time is up; `test`: a test's (applyTestColorScheme).
void apply(ThemeChoice choice, bool test) {
  ThemeState& current = state();
  watch();
  current.choice = choice;
  current.test = test;
  if (current.ownPalette) {
    // The platform's palette under it again; Windows 10's path sets its
    // own for the app mode once more.
    current.ownPalette = false;
    QApplication::setPalette(QPalette());
    win::followAppColorSchemeAgain();
  }
  QStyleHints* hints = QGuiApplication::styleHints();
  if (choice == ThemeChoice::System) {
    hints->unsetColorScheme();
  } else {
    hints->setColorScheme(choice == ThemeChoice::Dark ? Qt::ColorScheme::Dark : Qt::ColorScheme::Light);
  }
  current.pending = true;
  const int generation = ++current.generation;
  settle(false);
  // The platform's time counts from when the event loop runs: at start the
  // first window is made first, and the theme change waits for the loop.
  QTimer::singleShot(0, qApp, [generation] {
    QTimer::singleShot(kSettleMs, qApp, [generation] {
      if (generation == state().generation) {
        settle(true);
      }
    });
  });
}

} // namespace

ThemeChoice themePreference() { return themeChoiceFromName(QSettings().value(kThemeKey).toString()); }

void setThemePreference(ThemeChoice choice) { QSettings().setValue(kThemeKey, themeChoiceName(choice)); }

QString themeChoiceName(ThemeChoice choice) {
  switch (choice) {
  case ThemeChoice::Light:
    return QStringLiteral("light");
  case ThemeChoice::Dark:
    return QStringLiteral("dark");
  case ThemeChoice::System:
    break;
  }
  return QStringLiteral("system");
}

ThemeChoice themeChoiceFromName(const QString& name) {
  if (name == QLatin1String("light")) {
    return ThemeChoice::Light;
  }
  if (name == QLatin1String("dark")) {
    return ThemeChoice::Dark;
  }
  return ThemeChoice::System;
}

void startThemePreference() {
  const ThemeChoice choice = themePreference();
  if (choice != ThemeChoice::System) {
    applyThemePreference(choice);
    return;
  }
  // The platform's scheme, as before the preference: nothing asked.
  state().choice = choice;
  qInfo().noquote() << QStringLiteral("Theme: system (%1)").arg(schemeName(isDarkTheme()));
}

void applyThemePreference(ThemeChoice choice) { apply(choice, false); }

void applyTestColorScheme(bool dark) {
  // Qt's Windows and macOS platform themes follow the request themselves
  // (their tests compare the platform's own dark palette); on Linux Qt's
  // GTK theme does not always (mitcad#2404: under Xvfb with the caller's
  // desktop session the palette stayed light), and its generic theme has
  // no dark palette at all.
#if defined(Q_OS_WIN) || defined(Q_OS_MACOS)
  state().testOwnPalette = false;
#else
  state().testOwnPalette = true;
#endif
  apply(dark ? ThemeChoice::Dark : ThemeChoice::Light, true);
}

} // namespace mitcad
