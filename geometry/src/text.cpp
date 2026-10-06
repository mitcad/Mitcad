// SPDX-License-Identifier: MIT
#include "mitcad/geometry/text.hpp"

#include <algorithm>
#include <cctype>
#include <cstddef>
#include <cstring>
#include <map>
#include <mutex>
#include <stdexcept>
#include <tuple>
#include <utility>

#include <Font_FTFont.hxx>
#include <Font_FontMgr.hxx>
#include <Font_SystemFont.hxx>
#include <NCollection_Buffer.hxx>
#include <NCollection_Sequence.hxx>
#include <TCollection_AsciiString.hxx>
#include <TCollection_HAsciiString.hxx>

#include <ft2build.h>
#include FT_FREETYPE_H
#include FT_OUTLINE_H

namespace mitcad::geometry {

// The bundled font files (geometry/cmake/embed.cmake).
namespace fonts {
extern const unsigned char droid_sans[];
extern const std::size_t droid_sans_size;
extern const unsigned char droid_sans_bold[];
extern const std::size_t droid_sans_bold_size;
} // namespace fonts

const char* const kBundledFont = "Droid Sans";

namespace {

// Fonts are loaded at 72 points and 4800 dpi: an em is 4800 pixels, and
// FreeType's 26.6 fixed point gives outline coordinates to 1/307200 em.
constexpr unsigned int kPointSize = 72;
constexpr unsigned int kResolution = 4800;
constexpr double kPixelsPerEm = kResolution;
constexpr double kUnitsPerEm = 64.0 * kResolution;

// OCCT's FreeType font with its glyph loading exposed.
class OutlineFont : public Font_FTFont {
public:
  bool load(char32_t c) { return loadGlyph(c); }
};

struct Loaded {
  occ::handle<OutlineFont> font;
  std::string family;
  bool fallback = false;
};

std::string lower(std::string text) {
  std::transform(text.begin(), text.end(), text.begin(),
                 [](unsigned char c) { return static_cast<char>(std::tolower(c)); });
  return text;
}

Font_FTFontParams params(bool italic) {
  Font_FTFontParams p(kPointSize, kResolution);
  p.ToSynthesizeItalic = italic;
  return p;
}

// The bundled font from the library's own copy.
occ::handle<OutlineFont> bundled(bool bold, bool italic) {
  const unsigned char* data = bold ? fonts::droid_sans_bold : fonts::droid_sans;
  const std::size_t size = bold ? fonts::droid_sans_bold_size : fonts::droid_sans_size;
  occ::handle<NCollection_Buffer> buffer =
      new NCollection_Buffer(NCollection_BaseAllocator::CommonBaseAllocator(), size);
  std::memcpy(buffer->ChangeData(), data, size);
  occ::handle<OutlineFont> font = new OutlineFont();
  if (!font->Init(buffer, bold ? "DroidSans-Bold.ttf" : "DroidSans.ttf", params(italic), 0)) {
    throw std::runtime_error("the bundled font could not be loaded");
  }
  return font;
}

// An installed family, or null.
occ::handle<OutlineFont> installed(const std::string& family, bool bold, bool italic, std::string& name) {
  Font_FontAspect aspect = bold ? (italic ? Font_FontAspect_BoldItalic : Font_FontAspect_Bold)
                                : (italic ? Font_FontAspect_Italic : Font_FontAspect_Regular);
  const occ::handle<Font_SystemFont> found = Font_FontMgr::GetInstance()->FindFont(
      TCollection_AsciiString(family.c_str()), Font_StrictLevel_Strict, aspect, false);
  if (found.IsNull()) {
    return occ::handle<OutlineFont>();
  }
  Font_FTFontParams p = params(false);
  int face = 0;
  const TCollection_AsciiString& path = found->FontPathAny(aspect, p.ToSynthesizeItalic, face);
  occ::handle<OutlineFont> font = new OutlineFont();
  if (path.IsEmpty() || !font->Init(path, p, face)) {
    return occ::handle<OutlineFont>();
  }
  name = found->FontName().ToCString();
  return font;
}

std::mutex& font_mutex() {
  static std::mutex mutex;
  return mutex;
}

// Loaded fonts by requested family and style (under font_mutex).
const Loaded& font_for(const std::string& family, bool bold, bool italic) {
  static std::map<std::tuple<std::string, bool, bool>, Loaded> cache;
  const auto key = std::make_tuple(lower(family), bold, italic);
  const auto found = cache.find(key);
  if (found != cache.end()) {
    return found->second;
  }
  Loaded loaded;
  const bool wants_bundled = family.empty() || lower(family) == lower(kBundledFont);
  if (!wants_bundled) {
    loaded.font = installed(family, bold, italic, loaded.family);
  }
  if (loaded.font.IsNull()) {
    loaded.font = bundled(bold, italic);
    loaded.family = kBundledFont;
    loaded.fallback = !wants_bundled;
  }
  return cache.emplace(key, loaded).first->second;
}

// Collects the contours of an outline as Bezier pieces in ems.
struct Contours {
  std::vector<std::vector<std::vector<gp_Pnt2d>>> list;
  gp_Pnt2d pen;

  static gp_Pnt2d point(const FT_Vector* v) {
    return gp_Pnt2d(static_cast<double>(v->x) / kUnitsPerEm, static_cast<double>(v->y) / kUnitsPerEm);
  }

  void add(std::vector<gp_Pnt2d> piece) {
    const bool empty = std::all_of(piece.begin() + 1, piece.end(),
                                   [&piece](const gp_Pnt2d& p) { return p.Distance(piece.front()) <= 1e-12; });
    pen = piece.back();
    if (!empty && !list.empty()) {
      list.back().push_back(std::move(piece));
    }
  }
};

int move_to(const FT_Vector* to, void* user) {
  auto& c = *static_cast<Contours*>(user);
  c.list.emplace_back();
  c.pen = Contours::point(to);
  return 0;
}

int line_to(const FT_Vector* to, void* user) {
  auto& c = *static_cast<Contours*>(user);
  c.add({c.pen, Contours::point(to)});
  return 0;
}

int conic_to(const FT_Vector* control, const FT_Vector* to, void* user) {
  auto& c = *static_cast<Contours*>(user);
  c.add({c.pen, Contours::point(control), Contours::point(to)});
  return 0;
}

int cubic_to(const FT_Vector* control1, const FT_Vector* control2, const FT_Vector* to, void* user) {
  auto& c = *static_cast<Contours*>(user);
  c.add({c.pen, Contours::point(control1), Contours::point(control2), Contours::point(to)});
  return 0;
}

// The closed contours of a glyph; open or degenerate ones are left out.
std::vector<std::vector<std::vector<gp_Pnt2d>>> contours_of(const FT_Outline* outline) {
  Contours c;
  FT_Outline_Funcs funcs{};
  funcs.move_to = move_to;
  funcs.line_to = line_to;
  funcs.conic_to = conic_to;
  funcs.cubic_to = cubic_to;
  if (FT_Outline_Decompose(const_cast<FT_Outline*>(outline), &funcs, &c) != 0) {
    return {};
  }
  std::vector<std::vector<std::vector<gp_Pnt2d>>> closed;
  for (auto& contour : c.list) {
    if (contour.size() >= 2 && contour.back().back().Distance(contour.front().front()) <= 1e-9) {
      contour.back().back() = contour.front().front();
      closed.push_back(std::move(contour));
    }
  }
  return closed;
}

// The code points of UTF-8 text.
std::u32string decode(const std::string& text) {
  const auto fail = [] { return std::invalid_argument("the text is not UTF-8 or has a NUL character"); };
  std::u32string out;
  for (std::size_t i = 0; i < text.size();) {
    const auto lead = static_cast<unsigned char>(text[i]);
    const std::size_t n = lead < 0x80            ? 1
                          : (lead >> 5) == 0x6   ? 2
                          : (lead >> 4) == 0xE   ? 3
                          : (lead >> 3) == 0x1E ? 4
                                                 : 0;
    if (n == 0 || i + n > text.size()) {
      throw fail();
    }
    char32_t c = n == 1 ? lead : static_cast<char32_t>(lead & (0xFFu >> (n + 1)));
    for (std::size_t k = 1; k < n; ++k) {
      const auto next = static_cast<unsigned char>(text[i + k]);
      if ((next & 0xC0) != 0x80) {
        throw fail();
      }
      c = (c << 6) | (next & 0x3Fu);
    }
    if (c == 0) {
      throw fail();
    }
    out.push_back(c);
    i += n;
  }
  return out;
}

} // namespace

TextOutlines text_outlines(const std::string& family, bool bold, bool italic, const std::string& text) {
  const std::u32string chars = decode(text);
  const std::lock_guard<std::mutex> lock(font_mutex());
  const Loaded& loaded = font_for(family, bold, italic);
  OutlineFont& font = *loaded.font;
  TextOutlines result;
  result.family = loaded.family;
  result.fallback = loaded.fallback;
  result.ascender = font.Ascender() / kPixelsPerEm;
  result.descender = font.Descender() / kPixelsPerEm;
  result.line_spacing = font.LineSpacing() / kPixelsPerEm;
  for (std::size_t i = 0; i < chars.size(); ++i) {
    GlyphOutline glyph;
    const char32_t c = chars[i];
    if (c != U'\n' && c != U'\r') {
      const char32_t next = i + 1 < chars.size() && chars[i + 1] != U'\n' ? chars[i + 1] : 0;
      if (const FT_Outline* outline = font.renderGlyphOutline(c)) {
        glyph.contours = contours_of(outline);
      }
      if (font.load(c)) {
        glyph.advance = font.AdvanceX(c, next) / kPixelsPerEm;
      }
    }
    result.glyphs.push_back(std::move(glyph));
  }
  return result;
}

std::vector<std::string> font_families() {
  const std::lock_guard<std::mutex> lock(font_mutex());
  NCollection_Sequence<occ::handle<TCollection_HAsciiString>> names;
  Font_FontMgr::GetInstance()->GetAvailableFontsNames(names);
  std::vector<std::string> families{kBundledFont};
  for (const occ::handle<TCollection_HAsciiString>& name : names) {
    families.emplace_back(name->ToCString());
  }
  std::sort(families.begin(), families.end(),
            [](const std::string& a, const std::string& b) { return lower(a) < lower(b); });
  families.erase(std::unique(families.begin(), families.end(),
                             [](const std::string& a, const std::string& b) { return lower(a) == lower(b); }),
                 families.end());
  return families;
}

} // namespace mitcad::geometry
