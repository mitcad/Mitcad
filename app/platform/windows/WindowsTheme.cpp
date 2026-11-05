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

// The dark palette of Qt's own style: the theme's neutral roles of a dark
// theme (mitcad#1041), as the Docked and Floating chrome draw them on
// Windows 11 and macOS. Every role of every group is set, so that nothing
// resolves from the light palette of the platform's theme under it.
QPalette darkPalette() {
  const auto role = [](ColorRole r) { return themeColor(r, true); };
  const QColor window = role(ColorRole::Surface); // the panels' and the dialogs' colour
  const QColor base = role(ColorRole::Window);    // fields and lists, recessed below it
  const QColor raised = role(ColorRole::Raised);  // buttons and tooltips
  const QColor text = role(ColorRole::Text);
  const QColor secondary = role(ColorRole::TextSecondary);
  const QColor disabledText = secondary.darker(150);
  const QColor action = role(ColorRole::Action);
  const QColor accentText = role(ColorRole::AccentText);

  QPalette palette;
  const auto set = [&palette](QPalette::ColorRole which, const QColor& enabled, const QColor& disabled) {
    palette.setColor(QPalette::Active, which, enabled);
    palette.setColor(QPalette::Inactive, which, enabled);
    palette.setColor(QPalette::Disabled, which, disabled);
  };
  set(QPalette::Window, window, window);
  set(QPalette::WindowText, text, disabledText);
  set(QPalette::Base, base, window);
  set(QPalette::AlternateBase, window, window);
  set(QPalette::ToolTipBase, raised, raised);
  set(QPalette::ToolTipText, text, text);
  set(QPalette::PlaceholderText, secondary, disabledText);
  set(QPalette::Text, text, disabledText);
  set(QPalette::Button, raised, window);
  set(QPalette::ButtonText, text, disabledText);
  set(QPalette::BrightText, QColor(Qt::white), QColor(Qt::white));
  set(QPalette::Light, raised.lighter(150), raised.lighter(150));
  set(QPalette::Midlight, raised.lighter(125), raised.lighter(125));
  set(QPalette::Mid, raised.darker(130), raised.darker(130));
  set(QPalette::Dark, raised.darker(200), raised.darker(200));
  set(QPalette::Shadow, QColor(Qt::black), QColor(Qt::black));
  set(QPalette::Highlight, action, raised);
  set(QPalette::HighlightedText, accentText, secondary);
  set(QPalette::Link, action, secondary);
  set(QPalette::LinkVisited, action, secondary);
  set(QPalette::Accent, action, raised);
  return palette;
}

// The app mode now: the dark style and palette, or the default style with
// its own palette (the platform theme's, as before any switch).
void follow() {
  AppModeState& current = state();
  const bool dark = QGuiApplication::styleHints()->colorScheme() == Qt::ColorScheme::Dark;
  if (dark == current.dark) {
    return;
  }
  current.dark = dark;
  if (dark) {
    QApplication::setStyle(paletteStyleKey());
    const QPalette palette = darkPalette();
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

} // namespace mitcad::win
