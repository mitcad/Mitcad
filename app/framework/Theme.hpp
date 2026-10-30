// SPDX-License-Identifier: MIT
#pragma once

#include <QColor>
#include <QGuiApplication>
#include <QPalette>
#include <QString>

class QWidget;

namespace mitcad {

// Whether a palette is a dark one: its window is darker than middle grey.
// Decided on the palette, which every platform's dark mode (system scheme,
// the dark palette of Qt's own style) changes, and not on QStyleHints::colorScheme(),
// which is Unknown where the platform gives none.
bool isDarkPalette(const QPalette& palette = QGuiApplication::palette());

// Colours for text on the palette's window or base: dark red, amber and blue
// on a light palette, lighter ones on a dark palette. The accent colour is
// the system's on macOS (the palette's Accent role, which the Docked chrome
// sets to the action colour), the action colour elsewhere.
QColor errorColor(const QPalette& palette = QGuiApplication::palette());
QColor warningColor(const QPalette& palette = QGuiApplication::palette());
QColor accentColor(const QPalette& palette = QGuiApplication::palette());

// The icons' action blue (mitcad#230): #5aa2ff on a dark palette, #1a6ee0
// on a light one. The Docked chrome's one accent colour (mitcad#251), for
// the selection, the focus and the timeline's marker.
QColor actionColor(const QPalette& palette = QGuiApplication::palette());

// The material of the cards that float over the 3D view (macOS): a
// translucent fill (the window colour, about 85% opaque on a light palette,
// 80% on a dark one), a hairline border and the colour of the shadow below.
QColor cardFillColor(const QPalette& palette = QGuiApplication::palette());
QColor cardBorderColor(const QPalette& palette = QGuiApplication::palette());
QColor cardShadowColor(const QPalette& palette = QGuiApplication::palette());

// Sets a style sheet of a widget whose format has the error colour (or the
// warning colour) where it says %1, e.g. "color: %1;", for the palette of
// what it sits on (its first ancestor with an opaque background, else the
// application's; mitcad#961), and keeps it in step
// with it: it is set again when the palette or the parent changes.
// A format without %1 is set as it is; an empty one clears the style sheet.
// Call it again to change the format.
// While the format colours the text ("color: %1"), the widget's dynamic
// property kSemanticTextToneProperty is "error" or "warning", for presenters
// that do not paint Qt style sheets (the AppKit dialogs).
inline constexpr char kSemanticTextToneProperty[] = "semanticTextTone";
void setErrorStyleSheet(QWidget* widget, const QString& format = QStringLiteral("color: %1;"));
void setWarningStyleSheet(QWidget* widget, const QString& format = QStringLiteral("color: %1;"));

} // namespace mitcad
