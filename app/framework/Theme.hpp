// SPDX-License-Identifier: MIT
#pragma once

#include <vector>

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

// The color roles (mitcad#1041, docs/ui-conventions.md "Colors"): one table
// of what each color means, with a light and a dark value, read by the Qt
// chrome, the 3D view and the AppKit chrome (through dynamic colors that
// follow the appearance, app/platform/macos/NativeColors.hpp). Every color
// the user sees comes from a role or is a recorded exception of the check
// `repo.ui-colors` (tools/ui-colors-check).
//
// A chrome role takes its value by the palette it is drawn on (light or
// dark window); a role drawn in the 3D view by the view's background
// (viewColor), never by the widget palette.
enum class ColorRole {
  // Status: the levels of NoticeLevel.hpp, and nothing else.
  ErrorText,     // text and icons on a window
  ErrorBorder,   // rings, outlines and lines (at least 3:1)
  ErrorFill,     // the highlight box behind an error, under the window text
  ErrorBadge,    // the error badge's circle
  BadgeMark,     // the white mark on the error, information and success badges
  WarningText,   // the warning's text: yellow-brown on light, yellow on dark
  WarningBorder, // rings, outlines and lines
  WarningFill,   // the highlight box behind a warning
  WarningBadge,  // the yellow triangle
  WarningMark,   // the dark "!" on it
  SuccessText,
  SuccessBadge,
  InfoText, // information, hints and progress: the secondary text
  InfoBadge,
  // Interaction.
  Action,      // the chrome's accent where the system gives none (Docked, Linux, Windows)
  Select,      // the 3D view's selection: one fixed blue on every platform
  SelectLine,  // a feature's input lines in the 3D view, lighter on a dark background
  Hover,       // what the pointer is over, in the 3D view, sketch, drawings, pickers, timeline
  HoverFill,   // the chrome's hovered row or cell
  SearchMatch, // a match of a search (the timeline's ring); not a status hue
  DropAllowed, // a drop that will be taken
  DropRefused, // a drop that will not
  // Neutral: the chrome's surfaces and texts (Docked and Floating).
  Window,
  Surface,
  Raised,
  Text,
  TextSecondary,
  TextHidden, // a hidden item's name (with its eye icon)
  TitleText,
  Line,
  AccentText, // text on the accent
  TimelineMarker,
  GroupBand, // over an open timeline group's features
  ViewInk,   // text and outlines in the light boxes and pills over the 3D view
  // Object states (with their opacity, kStateDimOpacity).
  BoundText, // a value bound to a formula or a configuration (fx)
  BoundFill,
  // Command feedback in the 3D view.
  Input1, // the command panel's selection inputs, in order (with their swatches)
  Input2,
  Input3,
  Input4,
  PreviewAdd,      // material a feature adds
  PreviewRemove,   // material a cut removes (a recorded exception: red by convention)
  PreviewFailed,   // a feature that fails: a dashed outline, never the cut fill
  PreviewTarget,   // what an inspection points at (BUILD's auto-length target)
  ExtendedTool,    // a split's tool as the feature extends it (the second input's)
  Interference,    // overlapping volume an inspection found (a problem)
  Handle,          // manipulators, the timeline marker's handle, snaps, drawing picks
  Provisional,     // a mesh still being made
  ProvisionalEdge,
  FeatureFaces,      // a selected Fillet's or Chamfer's own faces, apart from its input lines
  FeatureFacesHover, // the same, hovered
  HandleActive,      // an axis-colored handle (Free Move's) hovered or dragged: none of the axis colors (mitcad#1707)
  // The 3D view's own objects.
  Body,
  Edge,
  HiddenEdge,
  Thread,
  BuildVolume,
  PrintBed,
  SectionCap, // the root component's section caps
  DatumPlane,
  DatumAxis, // datum axes, points and plane outlines
  ProfileFill,
  // Sketch, the same in the 3D view and the overlay.
  SketchFree,
  SketchFixed,
  SketchCenterline,
  SketchConstruction,
  SketchProjected,
  SketchInactive, // inactive constraints and dimensions
  SketchInactiveFill,
  SketchDriven,
  SketchDimension,
  SketchAxis,   // the sketch's axes and origin in the overlay
  SketchPicked, // a reference or trim pick while a tool runs
  SketchPreview,
  SketchGuide,
  SketchTrim, // what a trim removes (a remove preview)
  SketchHalo, // the outline under fixed lines, in the background's tone
  // Domain palettes (with legends).
  Component0, // the components' colors: timeline bands, section caps, Component Colors
  Component1,
  Component2,
  Component3,
  Component4,
  Component5,
  Component6,
  Component7,
  AxisX, // the X, Y and Z axes: the view cube, the joint triad, Free Move's handles (a recorded exception)
  AxisY,
  AxisZ,
  JointA,
  JointB,
  JointOrigin,
  PrintOverhang, // Print Analysis' categories: no status hues
  PrintThin,
  PrintGap, // narrow gaps between bodies printed in place (mitcad#1519)
  RuleFrame, // configuration rule blocks (#808, #851): category hues with white text
  RuleName,
  RuleLiteral,
  RuleFormula,
  RuleLogic,
  RuleAction,
  RuleText,
  // Simulate's stock (mitcad#1013): the stock, the material left from thin
  // to thick, and a cut into the part, with the legend in its window.
  CamStock,
  CamLeftThin,
  CamLeftThick,
  CamGouge,
};

// How a role is used, for the contrast test: text (at least 4.5:1 on a
// window), an indicator (at least 3:1), a fill under the window text, a
// badge or category fill under its own mark or text, or a color of the 3D
// view or a domain palette with a legend (no chrome contrast).
enum class ColorUse { Text, Indicator, Fill, Badge, View };

// Every role, in the enum's order; its name ("ErrorText"); its use.
const std::vector<ColorRole>& allColorRoles();
QString colorRoleName(ColorRole role);
ColorUse colorRoleUse(ColorRole role);
// What a Fill role is read under (its text) and what a Badge role carries
// (its mark or text); the role itself for the others.
ColorRole colorRolePartner(ColorRole role);

// The role's value on `palette` (light or dark by isDarkPalette).
QColor themeColor(ColorRole role, const QPalette& palette = QGuiApplication::palette());
// The role's value on a light or a dark background.
QColor themeColor(ColorRole role, bool dark);
// A whole palette of the neutral roles of a light or a dark theme, for Qt's
// own style (QFusionStyle), which draws every control from the palette:
// the window and the dialogs in Surface, fields and lists in Window,
// buttons and tooltips in Raised, the texts in Text and TextSecondary, the
// selection in Action. Every role of every group is set, so that nothing
// resolves from the platform's palette under it. Windows 10's dark app
// mode (platform/WindowsTheme) and the Theme preference where the platform
// keeps its own palette (framework/AppTheme) set it.
QPalette themePalette(bool dark);
// The role's value in the 3D view, by the view's background.
QColor viewColor(ColorRole role, bool darkBackground);
// The component palette's color of a component's index (componentColorIndex).
QColor componentPaletteColor(int index);
// The command panel's selection inputs' colors in order (Input1 to Input4,
// then again), shared by the Qt and the AppKit panel.
QColor commandInputColor(int index);

// How much of an object shows in a state that dims it (suppressed, rolled
// back), on both frontends.
inline constexpr double kStateDimOpacity = 0.35;

// The contrast ratio of two opaque colors (WCAG 2.2): 1 to 21.
double contrastRatio(const QColor& a, const QColor& b);

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
// The same with a second role's colour where the format says %2, kept in
// step with the palette too (mitcad#1152: a message box's fill, e.g.
// "color: %1; background: %2;" with ColorRole::ErrorFill, kept the old
// theme's fill under the new theme's text).
void setErrorStyleSheet(QWidget* widget, const QString& format, ColorRole fill);
void setWarningStyleSheet(QWidget* widget, const QString& format, ColorRole fill);
// The same with any role's color (mitcad#1041), e.g. the bound state's text.
void setRoleStyleSheet(QWidget* widget, ColorRole role, const QString& format = QStringLiteral("color: %1;"));

} // namespace mitcad
