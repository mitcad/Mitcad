// SPDX-License-Identifier: MIT
#pragma once

// The Start page's backdrop (mitcad#1439, app/COMMANDS.md "The Start tab";
// the page is files/StartPage): the surface behind the welcome, the quick
// actions and the designs' browser is a rounded rectangle of its own, inset
// from the 3D view's edges as the floating cards are (FloatingLayout's
// kMargin) and rounded with their radius (GlassCard::cornerRadius), on
// every platform and in both layouts. Around it the page paints the view's
// own background (its vertical gradient, OcctViewer::backgroundColors), so
// that what shows outside the corners is what shows around the floating
// cards, and never what the view drew last under the page (a design behind
// it, or nothing at all where the view renders elsewhere). Plain Qt Gui,
// painted into images in app.unit (tests/start_backdrop.cpp).

#include <QColor>
#include <QMargins>
#include <QRectF>
#include <QSize>

class QPainter;

namespace mitcad::start {

// What the page paints.
struct Backdrop {
  QColor fill;        // the backdrop's surface (the palette's window colour)
  QColor border;      // a hairline round it
  QColor viewTop;     // the view's background at its top
  QColor viewBottom;  // and at its bottom (the same for one colour)
  qreal radius = 0;   // the corners' (the floating cards')
};

// The backdrop's distances from the page's edges (the page is the whole
// view): `inset` from its left and right edges; at its top `inset` below
// `top`, the room the rows over the view's top keep (the workspace tabs
// where they float over it; 0 where nothing is over the view, as with the
// header above it on macOS and in the Docked layout); at its bottom `inset`
// from the edge, or `gap` above a band that floats over the view's bottom
// and keeps `below` of it (0: none).
QMargins backdropMargins(int inset, int top, int below, int gap);
// The backdrop's rectangle in a page of `size`; empty when nothing is left.
QRectF backdropRect(const QSize& size, int inset, int top, int below, int gap);

// Paints a page of `size`: the view's background everywhere, then the
// backdrop over it at `rect`, antialiased, its corners rounded with the
// backdrop's radius (at most half its shorter side).
void paintBackdrop(QPainter& painter, const QSize& size, const QRectF& rect, const Backdrop& backdrop);

} // namespace mitcad::start
