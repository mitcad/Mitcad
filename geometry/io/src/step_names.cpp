// SPDX-License-Identifier: MIT
#include "step_names.hpp"

#include <array>
#include <cstdint>
#include <optional>
#include <string_view>
#include <utility>
#include <vector>

#include <Resource_FormatType.hxx>
#include <Resource_Unicode.hxx>
#include <TCollection_ExtendedString.hxx>

namespace mitcad::io::detail {

namespace {

// The bytes between a text literal's apostrophes.
struct Literal {
  std::size_t begin = 0;
  std::size_t end = 0;
};

bool is_space(char c) {
  return c == ' ' || c == '\n' || c == '\r';
}

// The text literals of a STEP file, outside comments. A literal ends, as
// OCCT's scanner has it, at an apostrophe followed (after spaces and line
// breaks) by a comma or a closing parenthesis; any other apostrophe (a
// doubled one among them) belongs to the text.
std::vector<Literal> text_literals(const std::string& text) {
  std::vector<Literal> literals;
  const std::size_t size = text.size();
  std::size_t at = 0;
  while (at < size) {
    at = text.find_first_of("'/", at);
    if (at == std::string::npos) {
      break;
    }
    if (text[at] == '/') {
      if (at + 1 < size && text[at + 1] == '*') {
        const std::size_t close = text.find("*/", at + 2);
        at = close == std::string::npos ? size : close + 2;
      } else {
        ++at;
      }
      continue;
    }
    const std::size_t begin = at + 1;
    std::size_t quote = begin;
    std::size_t end = std::string::npos;
    while ((quote = text.find('\'', quote)) != std::string::npos) {
      std::size_t next = quote + 1;
      while (next < size && is_space(text[next])) {
        ++next;
      }
      if (next < size && (text[next] == ',' || text[next] == ')')) {
        end = quote;
        break;
      }
      ++quote;
    }
    if (end == std::string::npos) {
      break; // not closed: the scanner refuses the file anyway
    }
    literals.push_back(Literal{begin, end});
    at = end + 1;
  }
  return literals;
}

bool has_byte_outside_ascii(std::string_view bytes) {
  for (const char c : bytes) {
    if (static_cast<unsigned char>(c) >= 0x80) {
      return true;
    }
  }
  return false;
}

// Strict UTF-8: no overlong forms, no surrogates, nothing past U+10FFFF.
bool valid_utf8(std::string_view bytes) {
  const std::size_t size = bytes.size();
  std::size_t at = 0;
  while (at < size) {
    const unsigned char lead = static_cast<unsigned char>(bytes[at]);
    if (lead < 0x80) {
      ++at;
      continue;
    }
    std::size_t count = 0;
    unsigned char low = 0x80;
    unsigned char high = 0xBF;
    if (lead >= 0xC2 && lead <= 0xDF) {
      count = 1;
    } else if (lead >= 0xE0 && lead <= 0xEF) {
      count = 2;
      if (lead == 0xE0) {
        low = 0xA0;
      } else if (lead == 0xED) {
        high = 0x9F;
      }
    } else if (lead >= 0xF0 && lead <= 0xF4) {
      count = 3;
      if (lead == 0xF0) {
        low = 0x90;
      } else if (lead == 0xF4) {
        high = 0x8F;
      }
    } else {
      return false;
    }
    if (count > size - at - 1) {
      return false; // cut off at the end
    }
    for (std::size_t i = 1; i <= count; ++i) {
      const unsigned char next = static_cast<unsigned char>(bytes[at + i]);
      // Only the second byte has the narrower ranges.
      const unsigned char lowest = i == 1 ? low : static_cast<unsigned char>(0x80);
      const unsigned char highest = i == 1 ? high : static_cast<unsigned char>(0xBF);
      if (next < lowest || next > highest) {
        return false;
      }
    }
    at += count + 1;
  }
  return true;
}

enum class CodePage { Gbk, ShiftJis, Cp1252 };

constexpr std::array<CodePage, 3> code_pages{{CodePage::Gbk, CodePage::ShiftJis, CodePage::Cp1252}};

const char* code_page_name(CodePage page) {
  switch (page) {
  case CodePage::Gbk:
    return "GBK";
  case CodePage::ShiftJis:
    return "Shift-JIS";
  case CodePage::Cp1252:
    return "CP1252";
  }
  return "";
}

// A character OCCT decoded from one character's bytes: exactly one, and
// neither a null, a C1 control (what OCCT gives for CP1252's undefined
// bytes) nor a replacement character.
std::optional<char16_t> one_character(const TCollection_ExtendedString& decoded) {
  if (decoded.Length() != 1) {
    return std::nullopt;
  }
  const char16_t c = decoded.Value(1);
  if (c == 0 || (c >= 0x80 && c <= 0x9F) || c == 0xFFFD) {
    return std::nullopt;
  }
  return c;
}

std::optional<char16_t> decode_pair(CodePage page, unsigned char lead, unsigned char trail) {
  const char bytes[3] = {static_cast<char>(lead), static_cast<char>(trail), '\0'};
  TCollection_ExtendedString decoded;
  if (page == CodePage::Gbk) {
    if (!Resource_Unicode::ConvertGBKToUnicode(bytes, decoded)) {
      return std::nullopt;
    }
  } else {
    Resource_Unicode::ConvertSJISToUnicode(bytes, decoded);
  }
  return one_character(decoded);
}

// GBK (CP936): ASCII, 0x80 for the euro sign, and pairs of a lead byte
// 0x81-0xFE and a trail byte 0x40-0x7E or 0x80-0xFE. GB18030's four-byte
// sequences are no GBK.
bool decode_gbk(std::string_view bytes, std::u16string& out) {
  const std::size_t size = bytes.size();
  for (std::size_t at = 0; at < size; ++at) {
    const unsigned char lead = static_cast<unsigned char>(bytes[at]);
    if (lead < 0x80) {
      out.push_back(static_cast<char16_t>(lead));
      continue;
    }
    if (lead == 0x80) {
      out.push_back(u'\x20AC');
      continue;
    }
    if (lead == 0xFF || at + 1 >= size) {
      return false;
    }
    const unsigned char trail = static_cast<unsigned char>(bytes[at + 1]);
    if (trail < 0x40 || trail == 0x7F || trail == 0xFF) {
      return false;
    }
    const std::optional<char16_t> c = decode_pair(CodePage::Gbk, lead, trail);
    if (!c) {
      return false;
    }
    out.push_back(*c);
    ++at;
  }
  return true;
}

// Shift-JIS: ASCII, half-width katakana 0xA1-0xDF, and JIS X 0208 pairs of
// a lead byte 0x81-0x9F or 0xE0-0xEF (OCCT's table) and a trail byte
// 0x40-0x7E or 0x80-0xFC.
bool decode_shift_jis(std::string_view bytes, std::u16string& out) {
  const std::size_t size = bytes.size();
  for (std::size_t at = 0; at < size; ++at) {
    const unsigned char lead = static_cast<unsigned char>(bytes[at]);
    if (lead < 0x80) {
      out.push_back(static_cast<char16_t>(lead));
      continue;
    }
    if (lead >= 0xA1 && lead <= 0xDF) {
      out.push_back(static_cast<char16_t>(0xFF61 + (lead - 0xA1)));
      continue;
    }
    const bool lead_byte = (lead >= 0x81 && lead <= 0x9F) || (lead >= 0xE0 && lead <= 0xEF);
    if (!lead_byte || at + 1 >= size) {
      return false;
    }
    const unsigned char trail = static_cast<unsigned char>(bytes[at + 1]);
    if (trail < 0x40 || trail == 0x7F || trail > 0xFC) {
      return false;
    }
    const std::optional<char16_t> c = decode_pair(CodePage::ShiftJis, lead, trail);
    if (!c) {
      return false;
    }
    out.push_back(*c);
    ++at;
  }
  return true;
}

// CP1252: every byte one character, but for the five it leaves undefined
// (0x81, 0x8D, 0x8F, 0x90, 0x9D).
bool decode_cp1252(std::string_view bytes, std::u16string& out) {
  for (const char byte : bytes) {
    const unsigned char b = static_cast<unsigned char>(byte);
    if (b < 0x80) {
      out.push_back(static_cast<char16_t>(b));
      continue;
    }
    const char single[2] = {byte, '\0'};
    TCollection_ExtendedString decoded;
    Resource_Unicode::ConvertFormatToUnicode(Resource_FormatType_CP1252, single, decoded);
    const std::optional<char16_t> c = one_character(decoded);
    if (!c) {
      return false;
    }
    out.push_back(*c);
  }
  return true;
}

std::optional<std::u16string> decode(CodePage page, std::string_view bytes) {
  std::u16string out;
  out.reserve(bytes.size());
  bool decoded = false;
  switch (page) {
  case CodePage::Gbk:
    decoded = decode_gbk(bytes, out);
    break;
  case CodePage::ShiftJis:
    decoded = decode_shift_jis(bytes, out);
    break;
  case CodePage::Cp1252:
    decoded = decode_cp1252(bytes, out);
    break;
  }
  if (!decoded) {
    return std::nullopt;
  }
  return out;
}

// The text in ASCII: characters outside it as \X2\ escapes (UTF-16, four
// hexadecimal digits each), one escape per run of them.
std::string escaped(const std::u16string& text) {
  static const char digits[] = "0123456789ABCDEF";
  std::string out;
  out.reserve(text.size() * 2);
  bool in_escape = false;
  for (const char16_t c : text) {
    if (c < 0x80) {
      if (in_escape) {
        out += "\\X0\\";
        in_escape = false;
      }
      out.push_back(static_cast<char>(c));
      continue;
    }
    if (!in_escape) {
      out += "\\X2\\";
      in_escape = true;
    }
    const std::uint32_t value = c;
    for (int shift = 12; shift >= 0; shift -= 4) {
      out.push_back(digits[(value >> shift) & 0xFU]);
    }
  }
  if (in_escape) {
    out += "\\X0\\";
  }
  return out;
}

} // namespace

StepNameRecoding recode_step_names(std::string& text) {
  StepNameRecoding recoding;
  const std::vector<Literal> literals = text_literals(text);
  // The literals neither ASCII nor UTF-8, by their index.
  std::vector<std::size_t> others;
  for (std::size_t i = 0; i < literals.size(); ++i) {
    const Literal& literal = literals[i];
    const std::string_view bytes(text.data() + literal.begin, literal.end - literal.begin);
    if (bytes.empty()) {
      continue;
    }
    ++recoding.names;
    if (!has_byte_outside_ascii(bytes)) {
      continue;
    }
    ++recoding.not_ascii;
    if (valid_utf8(bytes)) {
      ++recoding.utf8;
    } else {
      others.push_back(i);
    }
  }
  if (others.empty()) {
    return recoding;
  }

  // One code page for the file: the first that decodes all, else the one
  // that decodes the most (the earlier on a tie).
  std::vector<std::optional<std::u16string>> best;
  std::size_t best_count = 0;
  for (const CodePage page : code_pages) {
    std::vector<std::optional<std::u16string>> results;
    results.reserve(others.size());
    std::size_t count = 0;
    for (const std::size_t index : others) {
      const Literal& literal = literals[index];
      const std::string_view bytes(text.data() + literal.begin, literal.end - literal.begin);
      results.push_back(decode(page, bytes));
      if (results.back()) {
        ++count;
      }
    }
    if (count > best_count) {
      best_count = count;
      best = std::move(results);
      recoding.code_page = code_page_name(page);
    }
    if (count == others.size()) {
      break;
    }
  }
  if (best_count == 0) {
    recoding.code_page = "none";
    return recoding;
  }
  recoding.decoded = best_count;

  std::string recoded;
  recoded.reserve(text.size() + text.size() / 16);
  std::size_t copied = 0;
  for (std::size_t k = 0; k < others.size(); ++k) {
    if (!best[k]) {
      continue;
    }
    const Literal& literal = literals[others[k]];
    recoded.append(text, copied, literal.begin - copied);
    recoded += escaped(*best[k]);
    copied = literal.end;
  }
  recoded.append(text, copied, std::string::npos);
  text.swap(recoded);
  return recoding;
}

std::string describe(const StepNameRecoding& recoding) {
  std::string line = "STEP names: " + recoding.code_page + " (" + std::to_string(recoding.not_ascii) + " of " +
                     std::to_string(recoding.names) + " names not ASCII";
  const std::size_t left = recoding.not_ascii - recoding.utf8 - recoding.decoded;
  const bool mixed = recoding.code_page != "UTF-8" && recoding.utf8 > 0;
  if (mixed || left > 0) {
    line += ";";
    if (mixed) {
      line += " " + std::to_string(recoding.utf8) + " in UTF-8";
    }
    if (left > 0) {
      line += std::string(mixed ? "," : "") + " " + std::to_string(left) + " not decoded, kept as read";
    }
  }
  line += ")";
  return line;
}

} // namespace mitcad::io::detail
