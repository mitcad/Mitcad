// SPDX-License-Identifier: MIT
#include "LayoutGrid.hpp"

#include <cmath>
#include <vector>

#include <Graphic3d_ArrayOfSegments.hxx>
#include <Graphic3d_AspectLine3d.hxx>
#include <Graphic3d_Group.hxx>
#include <Prs3d_Presentation.hxx>

namespace mitcad {
namespace {

// The densest lines on screen (pixels apart); the next coarser step is
// chosen before lines get further apart than this.
constexpr double kMaxPixels = 35.0;
// More lines than this on each side are not drawn (a fixed spacing far out).
constexpr long kMaxLines = 2000;

} // namespace

double layoutGridStep(double pixelsPerMm) {
  if (!(pixelsPerMm > 0.0) || !std::isfinite(pixelsPerMm)) {
    return 10.0;
  }
  // 1 or 5 times a power of ten: no 2, so the major lines (five steps) fall
  // on the next step's lines.
  const double limit = kMaxPixels / pixelsPerMm;
  const double power = std::pow(10.0, std::floor(std::log10(limit)));
  return 5.0 * power <= limit ? 5.0 * power : power;
}

void LayoutGrid::setColors(const Quantity_Color& minor, const Quantity_Color& major) {
  m_minorColor = minor;
  m_majorColor = major;
}

void LayoutGrid::Compute(const occ::handle<PrsMgr_PresentationManager>&,
                         const occ::handle<Prs3d_Presentation>& presentation, const int mode) {
  presentation->SetInfiniteState(true); // not in the bounding box
  const double step = m_lines.step;
  if (mode != 0 || !(step > 0.0) || !(m_lines.extent > 0.0)) {
    return;
  }
  const long count = std::lround(m_lines.extent / step);
  const long firstX = std::lround(m_lines.centreX / step) - count;
  const long firstY = std::lround(m_lines.centreY / step) - count;
  const double length = 2.0 * static_cast<double>(count) * step;
  // Each line is minor or major by its index from the origin.
  const auto collect = [&](long first, std::vector<double>& minorAt, std::vector<double>& majorAt) {
    for (long i = 0; i <= 2 * count; ++i) {
      const long index = first + i;
      const double at = static_cast<double>(index) * step;
      if (index % kLayoutGridMajor == 0) {
        majorAt.push_back(at);
      } else if (m_lines.minor && count <= kMaxLines) {
        minorAt.push_back(at);
      }
    }
  };
  std::vector<double> minorX;
  std::vector<double> majorX;
  std::vector<double> minorY;
  std::vector<double> majorY;
  if (count / kLayoutGridMajor > kMaxLines) {
    return;
  }
  collect(firstX, minorX, majorX);
  collect(firstY, minorY, majorY);

  const double x0 = static_cast<double>(firstX) * step;
  const double y0 = static_cast<double>(firstY) * step;
  const auto draw = [&](const std::vector<double>& xs, const std::vector<double>& ys,
                        const Quantity_Color& color) {
    if (xs.empty() && ys.empty()) {
      return;
    }
    const occ::handle<Graphic3d_ArrayOfSegments> segments =
        new Graphic3d_ArrayOfSegments(static_cast<int>(2 * (xs.size() + ys.size())));
    for (const double x : xs) {
      segments->AddVertex(x, y0, 0.0);
      segments->AddVertex(x, y0 + length, 0.0);
    }
    for (const double y : ys) {
      segments->AddVertex(x0, y, 0.0);
      segments->AddVertex(x0 + length, y, 0.0);
    }
    const occ::handle<Graphic3d_Group> group = presentation->NewGroup();
    group->SetGroupPrimitivesAspect(new Graphic3d_AspectLine3d(color, Aspect_TOL_SOLID, 1.0));
    group->AddPrimitiveArray(segments);
  };
  // The major lines over the minor ones.
  draw(minorX, minorY, m_minorColor);
  draw(majorX, majorY, m_majorColor);
}

} // namespace mitcad
