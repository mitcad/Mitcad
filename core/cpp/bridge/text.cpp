// SPDX-License-Identifier: MIT
#include "bridge/text.hpp"

#include <cstdint>
#include <string>
#include <utility>

#include "mitcad/geometry/text.hpp"
#include "mitcad_bridge/kernel/text.h"

namespace mitcad::bridge {

TextOutlines text_outlines(rust::Str family, bool bold, bool italic, rust::Str text) {
  const geometry::TextOutlines outlines =
      geometry::text_outlines(std::string(family), bold, italic, std::string(text));
  TextOutlines out{};
  out.family = rust::String(outlines.family);
  out.fallback = outlines.fallback;
  out.ascender = outlines.ascender;
  out.descender = outlines.descender;
  out.line_spacing = outlines.line_spacing;
  for (const geometry::GlyphOutline& glyph : outlines.glyphs) {
    TextGlyph g{};
    g.advance = glyph.advance;
    for (const auto& contour : glyph.contours) {
      g.contours.push_back(static_cast<std::uint32_t>(contour.size()));
      for (const auto& piece : contour) {
        g.points.push_back(static_cast<std::uint32_t>(piece.size()));
        for (const gp_Pnt2d& p : piece) {
          g.coordinates.push_back(p.X());
          g.coordinates.push_back(p.Y());
        }
      }
    }
    out.glyphs.push_back(std::move(g));
  }
  return out;
}

} // namespace mitcad::bridge
