// SPDX-License-Identifier: MIT
#pragma once

#include <algorithm>

#include <QMargins>
#include <QRectF>
#include <QSizeF>

namespace mitcad {

// Where the pill of a running command's view hint goes in the 3D view
// (Inspection::viewHint, mitcad#394; ManipulatorOverlay), in the view's
// coordinates (mitcad#2190): its top kViewHintTop below the part of the view
// that the cards floating over it cover (OcctViewer::overlayInsets: the
// Floating layout's title row and toolbar panel; none in the Docked layout,
// where the pill keeps its place 16 px from the view's top), and below the
// view's banner (ModeBanner) when one shows (`banner` not empty, the
// banner's rectangle in the view), kViewHintGap under it; centered between
// the left and the right insets and at most as wide as the room there
// (12 px from each side, as the banner; at least 120 px). `pill` is the
// size the pill wants: its text with the padding.
constexpr double kViewHintTop = 16.0;
constexpr double kViewHintGap = 6.0;
constexpr double kViewHintSide = 12.0;

inline QRectF viewHintRect(const QSizeF& view, const QMargins& insets, const QRectF& banner, const QSizeF& pill) {
  const double left = insets.left() + kViewHintSide;
  const double room = std::max(120.0, view.width() - insets.left() - insets.right() - 2.0 * kViewHintSide);
  const double width = std::min(room, pill.width());
  double top = insets.top() + kViewHintTop;
  if (!banner.isEmpty()) {
    top = std::max(top, banner.bottom() + kViewHintGap);
  }
  return {left + (room - width) / 2.0, top, width, pill.height()};
}

} // namespace mitcad
