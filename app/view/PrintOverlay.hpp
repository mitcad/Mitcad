// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <optional>
#include <utility>
#include <vector>

#include <QColor>
#include <QMargins>
#include <QObject>
#include <QString>

#include <AIS_InteractiveContext.hxx>
#include <AIS_InteractiveObject.hxx>
#include <gp_Pnt.hxx>

namespace mitcad {

class OcctViewer;
struct PrintOverlay;

// Print Analysis over the 3D view (mitcad#620, OcctViewer::setPrintOverlay):
// a marker and a label at the thinnest point and one at the smallest narrow
// gap between bodies (mitcad#1519), kept where the points are as the camera
// moves, and the legend (the analysis' name, its colours with
// the angle and the minimum wall) in the view's lower left corner; the
// triangles are drawn by the view (PrintOverlay.cpp).
//
// The marker and the legend are cards painted once into images and drawn
// by the view itself, in its overlay layers, as pictures that keep their
// size on the screen (mitcad#768): a widget over the view, however small,
// is painted again at each frame the view draws, and then the window's
// whole area under the view is composed again, which made every frame of
// an orbit cost several milliseconds more per million pixels of the view.
class PrintOverlayLabels : public QObject {
public:
  // `changed`: the view must draw again (a card changed or moved).
  PrintOverlayLabels(OcctViewer* viewer, occ::handle<AIS_InteractiveContext> context,
                     std::function<void()> changed);

  // Shows what the overlay has of it; an empty overlay hides it.
  void setOverlay(const PrintOverlay& overlay);
  // The categories' colours for another background (mitcad#1041).
  void setColors(const QColor& overhang, const QColor& thin, const QColor& gap);

private:
  // Paints the cards again when what they show, the screen's pixel ratio
  // or the theme changed, and keeps the legend clear of the cards over the
  // view (overlay insets); nothing while only the camera moves.
  void follow();
  void paintCards();
  void placeLegend();

  OcctViewer* m_viewer;
  occ::handle<AIS_InteractiveContext> m_context;
  std::function<void()> m_changed;
  occ::handle<AIS_InteractiveObject> m_marker; // the thinnest point's card
  occ::handle<AIS_InteractiveObject> m_gapMarker; // the smallest narrow gap's card
  occ::handle<AIS_InteractiveObject> m_legend; // the legend's card
  bool m_shown = false;
  std::optional<gp_Pnt> m_point; // the thinnest point, in the design
  QString m_label;
  std::optional<gp_Pnt> m_gapPoint; // the middle of the smallest narrow gap
  QString m_gapLabel;
  QColor m_gapColor;
  QString m_title;
  std::vector<std::pair<QColor, QString>> m_entries; // the legend's
  QColor m_markerColor;
  // What the cards were painted for.
  qreal m_ratio = 0.0;
  qint64 m_paletteKey = 0;
  QMargins m_insets;
};

} // namespace mitcad
