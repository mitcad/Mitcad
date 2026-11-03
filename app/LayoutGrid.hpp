// SPDX-License-Identifier: MIT
#pragma once

#include <AIS_InteractiveObject.hxx>
#include <Quantity_Color.hxx>

// The automatic spacing per unit and the major lines (mitcad#1210).
#include "view/GridSpacing.hpp"

namespace mitcad {

// The layout grid: lines on the XY plane of its local transformation (the
// grid plane), minor lines `step` apart and every `major`-th one a major
// line (every fifth on a metric grid, layoutGridMajor),
// in a square of half-width `extent` about the centre. Not pickable, and
// left out of the view's bounding box, so that Fit ignores it.
class LayoutGrid : public AIS_InteractiveObject {
  DEFINE_STANDARD_RTTI_INLINE(LayoutGrid, AIS_InteractiveObject)

public:
  struct Lines {
    double step = 0.0; // mm; 0: nothing drawn
    double centreX = 0.0;
    double centreY = 0.0;
    double extent = 0.0;
    bool minor = true; // false: the major lines only (minor ones too dense)
    int major = kLayoutGridMajor; // every this many lines a major one
  };

  const Lines& lines() const { return m_lines; }
  // Takes effect when the object is displayed or redisplayed.
  void setLines(const Lines& lines) { m_lines = lines; }
  void setColors(const Quantity_Color& minor, const Quantity_Color& major);

  bool AcceptDisplayMode(const int mode) const override { return mode == 0; }

protected:
  void Compute(const occ::handle<PrsMgr_PresentationManager>& manager,
               const occ::handle<Prs3d_Presentation>& presentation, const int mode) override;
  void ComputeSelection(const occ::handle<SelectMgr_Selection>&, const int) override {}

private:
  Lines m_lines;
  Quantity_Color m_minorColor = Quantity_Color(0.30, 0.31, 0.34, Quantity_TOC_RGB);
  Quantity_Color m_majorColor = Quantity_Color(0.40, 0.41, 0.44, Quantity_TOC_RGB);
};

} // namespace mitcad
