// SPDX-License-Identifier: MIT
#include "../WindowsTheme.hpp"

#include <memory>

#include <QApplication>
#include <QColor>
#include <QGuiApplication>
#include <QMetaObject>
#include <QObject>
#include <QOperatingSystemVersion>
#include <QPalette>
#include <QString>
#include <QStyle>
#include <QStyleFactory>
#include <QStyleHints>
#include <QtLogging>

#include "../../framework/Theme.hpp"

namespace mitcad::win {
namespace {

constexpr int kWindows11 = 22000;

// What the path switched: the style it goes back to when the app mode is
// light, and whether the dark style and palette are in force.
struct AppModeState {
  QString defaultStyle;
  bool dark = false;
};

AppModeState& state() {
  static AppModeState current;
  return current;
}

// The key of Qt's own style, found by its class (QFusionStyle) among the
// styles Qt offers; empty if there is none.
QString paletteStyleKey() {
  static const QString key = [] {
    for (const QString& candidate : QStyleFactory::keys()) {
      const std::unique_ptr<QStyle> style(QStyleFactory::create(candidate));
      if (style != nullptr && qstrcmp(style->metaObject()->className(), "QFusionStyle") == 0) {
        return candidate;
      }
    }
    return QString();
  }();
  return key;
}

// The app mode now: the dark style and palette, or the default style with
// its own palette (the platform theme's, as before any switch). `again`:
// set them even when the app mode is the one last followed (another part
// replaced the application's palette meanwhile).
void follow(bool again = false) {
  AppModeState& current = state();
  const bool dark = QGuiApplication::styleHints()->colorScheme() == Qt::ColorScheme::Dark;
  if (dark == current.dark && !again) {
    return;
  }
  current.dark = dark;
  if (dark) {
    QApplication::setStyle(paletteStyleKey());
    // The theme's neutral roles of a dark theme (mitcad#1041), as the Docked
    // and Floating chrome draw them on Windows 11 and macOS.
    const QPalette palette = themePalette(true);
    QApplication::setPalette(palette);
    qInfo().noquote() << QStringLiteral("Windows 10 app mode: dark, the style %1 with a dark palette (window %2)")
                             .arg(QApplication::style()->name(), palette.color(QPalette::Window).name());
  } else {
    QApplication::setStyle(current.defaultStyle);
    // A palette that sets no role: the application's resolves from the
    // style and the platform theme again.
    QApplication::setPalette(QPalette());
    qInfo().noquote() << QStringLiteral("Windows 10 app mode: light, the style %1 with its own palette")
                             .arg(QApplication::style()->name());
  }
}

} // namespace

void followAppColorScheme() {
  // The build number, as for the window frame (WindowsFrame.cpp).
  const int build = QOperatingSystemVersion::current().microVersion();
  const bool forced = qEnvironmentVariableIntValue("MITCAD_TEST_WINDOWS10") == 1;
  if (build >= kWindows11 && !forced) {
    qInfo().noquote() << QStringLiteral("Windows 10 app mode: not followed (Windows build %1)").arg(build);
    return;
  }
  const QString style = QApplication::style()->name();
  if (!forced && style.compare(QLatin1String("windowsvista"), Qt::CaseInsensitive) != 0) {
    qInfo().noquote() << QStringLiteral("Windows 10 app mode: not followed (the style %1)").arg(style);
    return;
  }
  if (paletteStyleKey().isEmpty()) {
    qInfo().noquote() << QStringLiteral("Windows 10 app mode: not followed (Qt's own style is not available)");
    return;
  }
  state().defaultStyle = style;
  qInfo().noquote() << QStringLiteral("Windows 10 app mode: followed (Windows build %1%2, the style %3)")
                           .arg(QString::number(build),
                                forced ? QStringLiteral(", MITCAD_TEST_WINDOWS10=1") : QString(), style);
  // Before any window: its widgets connect to the same signal later, so
  // this switch comes first when the app mode changes.
  QObject::connect(QGuiApplication::styleHints(), &QStyleHints::colorSchemeChanged, qApp, [] { follow(); });
  follow();
}

void followAppColorSchemeAgain() {
  // Only where the path is taken (followAppColorScheme set the default style).
  if (!state().defaultStyle.isEmpty()) {
    follow(true);
  }
}

} // namespace mitcad::win
