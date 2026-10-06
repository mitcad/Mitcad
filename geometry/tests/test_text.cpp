// SPDX-License-Identifier: MIT
// Glyph outlines of sketch text: the bundled font, fallback, styles.

#include <cmath>

#include "check.hpp"
#include "mitcad/geometry/text.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

// The signed area of a contour of Bezier pieces (Green's theorem on dense
// samples), in ems squared.
double contour_area(const std::vector<std::vector<gp_Pnt2d>>& contour) {
  double area = 0.0;
  for (const auto& piece : contour) {
    gp_Pnt2d previous = piece.front();
    for (int k = 1; k <= 64; ++k) {
      const double t = k / 64.0;
      // De Casteljau.
      std::vector<gp_Pnt2d> p = piece;
      for (std::size_t n = p.size(); n > 1; --n) {
        for (std::size_t i = 0; i + 1 < n; ++i) {
          p[i] = gp_Pnt2d((1 - t) * p[i].X() + t * p[i + 1].X(), (1 - t) * p[i].Y() + t * p[i + 1].Y());
        }
      }
      area += previous.X() * p[0].Y() - p[0].X() * previous.Y();
      previous = p[0];
    }
  }
  return area / 2;
}

void test_bundled_font_outlines() {
  const TextOutlines text = text_outlines("", false, false, "AO l\n1");
  CHECK(text.family == kBundledFont);
  CHECK(!text.fallback);
  CHECK(text.glyphs.size() == 6);
  CHECK(text.ascender > 0.7 && text.ascender < 1.2);
  CHECK(text.descender < 0.0 && text.descender > -0.4);
  CHECK(text.line_spacing > text.ascender - text.descender - 1e-9);
  // A and O have a counter; the space and the newline have none.
  CHECK(text.glyphs[0].contours.size() == 2);
  CHECK(text.glyphs[1].contours.size() == 2);
  CHECK(text.glyphs[2].contours.empty() && text.glyphs[2].advance > 0.1);
  CHECK(text.glyphs[4].contours.empty() && text.glyphs[4].advance == 0.0);
  CHECK(text.glyphs[5].contours.size() == 1);
  // Closed contours of lines and quadratics, outer and inner turning
  // opposite ways.
  for (const auto& glyph : text.glyphs) {
    for (const auto& contour : glyph.contours) {
      CHECK(contour.front().front().IsEqual(contour.back().back(), 0.0));
      for (std::size_t i = 1; i < contour.size(); ++i) {
        CHECK(contour[i].front().IsEqual(contour[i - 1].back(), 1e-12));
      }
      for (const auto& piece : contour) {
        CHECK(piece.size() == 2 || piece.size() == 3);
      }
    }
  }
  const double outer = contour_area(text.glyphs[1].contours[0]);
  const double inner = contour_area(text.glyphs[1].contours[1]);
  CHECK(outer * inner < 0.0);
  CHECK(std::abs(outer) > std::abs(inner) && std::abs(outer) > 0.1);
  // The same request gives the same outlines.
  const TextOutlines again = text_outlines("Droid Sans", false, false, "AO l\n1");
  const auto& a = again.glyphs[1].contours[1];
  const auto& b = text.glyphs[1].contours[1];
  bool same = a.size() == b.size();
  for (std::size_t i = 0; same && i < a.size(); ++i) {
    same = a[i].size() == b[i].size();
    for (std::size_t k = 0; same && k < a[i].size(); ++k) {
      same = a[i][k].IsEqual(b[i][k], 0.0);
    }
  }
  CHECK(same);
}

void test_fallback_and_styles() {
  const TextOutlines missing = text_outlines("No Such Font Family", false, false, "O");
  CHECK(missing.fallback);
  CHECK(missing.family == kBundledFont);
  const TextOutlines regular = text_outlines("", false, false, "l");
  const TextOutlines bold = text_outlines("", true, false, "l");
  const TextOutlines italic = text_outlines("", false, true, "l");
  // Bold strokes are wider; italic leans right at the top.
  const double regular_area = std::abs(contour_area(regular.glyphs[0].contours[0]));
  CHECK(std::abs(contour_area(bold.glyphs[0].contours[0])) > 1.2 * regular_area);
  // The rightmost point above half an em.
  const auto top = [](const TextOutlines& text) {
    double x = -1e9;
    for (const auto& piece : text.glyphs[0].contours[0]) {
      if (piece.front().Y() > 0.5) {
        x = std::max(x, piece.front().X());
      }
    }
    return x;
  };
  CHECK(top(italic) > top(regular) + 0.05);
  CHECK(!font_families().empty());
  CHECK(throws_with([] { text_outlines("", false, false, std::string("a\xff", 2)); }, "UTF-8"));
}

} // namespace

void text_tests() {
  guarded("bundled font", test_bundled_font_outlines);
  guarded("fallback and styles", test_fallback_and_styles);
}

} // namespace test
