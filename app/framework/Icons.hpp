// SPDX-License-Identifier: MIT
#pragma once

#include <optional>

#include <QByteArray>
#include <QColor>
#include <QIcon>
#include <QString>

#include "Theme.hpp"

namespace mitcad {

// What an icon is drawn on, which decides its colours: the widgets' palette
// (Palette: the light colours on a light palette, the dark ones on a dark
// palette), or a surface of its own, like the 3D view, whose background does
// not follow the palette.
enum class IconBackdrop { Palette, Light, Dark };

// The colour roles of the icons (mitcad#230). All the icons are one family:
// every #rrggbb of their sources stands for one role, whatever the file, and
// each role has one colour and opacity per theme (iconRoleColour). Status
// colours (errors, done, warnings) take the role table's; the others
// (a projection, the swatches of Appearance) keep their colour.
enum class IconRole {
  Outline,          // the line work
  TopFace,          // a solid's lit top face
  SideFace,         // a solid's side face
  FarSide,          // a solid's shaded far side
  PaleSurface,      // sheets, plates and other pale surfaces
  Hollow,           // white insides: paper, a hole, a frame
  ConstructionNear, // planes, the ruler and section faces in front
  ConstructionFar,  // planes behind
  ConstructionLine, // construction lines and points, plane outlines
  Action,           // what the command does: arrows, edges, the joint dot
  Sketch,           // sketch geometry
  Pencil,           // the sketch pencil
  Muted,            // secondary lines: grids, axes, hidden things
  Status,           // a colour with a meaning of its own, kept
};

// The role of a colour of the icons' sources; none for a colour that is not
// one of the family's (an icon drawn with it would be left as drawn).
std::optional<IconRole> iconRole(const QColor& source);

// The colour of a role on a light or a dark theme, with its opacity in the
// colour's alpha; fills and strokes differ for the construction roles and the
// pencil. A status colour of an error, a success or a warning is drawn in its
// role of the role table (iconStatusRole, mitcad#1041); any other stays
// itself, only made brighter on a dark theme where it has less than 3:1
// contrast with the dark surface.
QColor iconRoleColour(IconRole role, bool dark, bool stroke, const QColor& source = QColor());

// The role of the role table a status source colour is drawn in: the
// error's ring red, the success's green, the warning's yellow; none for the
// status colours that keep their own (a projection, Appearance's swatches).
std::optional<ColorRole> iconStatusRole(const QColor& source);

// An icon's SVG source with the colours of a theme: each fill, stroke and
// gradient stop gets its role's colour, and its opacity as fill-opacity,
// stroke-opacity or stop-opacity (the SVG renderer has no rgba()). Disabled:
// every role in the outline colour (white stays white); the whole icon is
// then painted at kDisabledIconOpacity.
QByteArray themedIconSvg(const QByteArray& source, bool dark, bool disabled = false);

// The opacity of a disabled icon, painted over the disabled colours.
constexpr qreal kDisabledIconOpacity = 0.34;

// An icon of app/icons (<name>.svg), rendered here so that no image plugin
// is needed; a null icon when there is no such file. The icon renders the
// drawing at the size and the device pixel ratio it is asked for (sharp on
// HiDPI screens) and when it is asked, so that it follows the palette. The
// disabled mode is the disabled colours at kDisabledIconOpacity on every
// platform; the selected mode is the style's.
QIcon themeIcon(const QString& name, IconBackdrop backdrop = IconBackdrop::Palette);

// A small square of a colour, for the selection inputs of command panels.
QIcon swatchIcon(const QColor& color);

} // namespace mitcad
