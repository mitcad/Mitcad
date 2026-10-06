// SPDX-License-Identifier: MIT
#pragma once

// Glyph outlines for sketch text (P3): the characters of a text in a font,
// through OCCT's font manager (Font_FontMgr) and its FreeType font
// (Font_FTFont). The bundled font (Droid Sans, third_party/fonts/) is
// embedded in the library: it is the default and the fallback for a family
// that is not installed, so that outlines do not depend on the machine.

#include <string>
#include <vector>

#include <gp_Pnt2d.hxx>

namespace mitcad::geometry {

// The bundled font's family name.
extern const char* const kBundledFont;

// One character: closed contours of Bezier pieces (2 control points for a
// line, 3 for a quadratic and 4 for a cubic curve), each piece starting
// where the one before ends and the last ending where the first starts.
// Units are ems (the font size is 1), the pen at the origin on the
// baseline, y up.
struct GlyphOutline {
  // To the next character's pen, kerning with it included.
  double advance = 0.0;
  std::vector<std::vector<std::vector<gp_Pnt2d>>> contours;
};

struct TextOutlines {
  std::string family; // the family the outlines come from
  bool fallback = false; // the requested family was not found
  double ascender = 0.0;
  double descender = 0.0; // negative below the baseline
  double line_spacing = 0.0;
  std::vector<GlyphOutline> glyphs; // one per character (code point) of the text
};

// The outlines of a UTF-8 text's characters in a font family (empty for the
// bundled font); a newline is a glyph without contours or advance. Italic
// is synthesized for a family without it; bold falls back to the regular
// face. Throws std::invalid_argument for text that is not UTF-8.
TextOutlines text_outlines(const std::string& family, bool bold, bool italic, const std::string& text);

// The families of the installed fonts and the bundled one, sorted.
std::vector<std::string> font_families();

} // namespace mitcad::geometry
