// SPDX-License-Identifier: MIT
#pragma once

// The part picker's stars (PartPicker.hpp): a favourite part's or group's
// filled star and the empty one that stars it, drawn by the grid's tiles,
// the groups' tree and the picker's buttons.

#include <QColor>
#include <QPainterPath>
#include <QPalette>
#include <QRectF>

class QPainter;

namespace mitcad {

// The filled star's colour, the same on every theme (the native macOS
// grid's starColour in NativeCollection.mm matches it).
QColor partStarColour();

// A five-pointed star in `box`, its points on the box's inscribed circle.
QPainterPath partStarPath(const QRectF& box);

// A filled star when on, else its outline in `outline`, one pixel inside
// `box`.
void paintPartStar(QPainter* painter, const QRectF& box, bool on, const QColor& outline);

// A tile's star at the top right corner of its picture (mitcad#1014): an
// empty star gets a disc of the palette's base under it and its outline in
// the palette's text colour, neither depending on the tile's selection, so
// that it stays visible over any picture and on the selected tile; a filled
// star is drawn as paintPartStar draws it. Nothing is drawn outside `box`.
void paintTileStar(QPainter* painter, const QRectF& box, bool on, const QPalette& palette);

} // namespace mitcad
