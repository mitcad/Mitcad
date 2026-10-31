// SPDX-License-Identifier: MIT
#pragma once

// Names written as raw bytes of a code page other than UTF-8 in STEP files
// (mitcad#1002). STEP's own escapes (\X2\...\X0\) say what they encode, but
// many writers put a name's bytes into the file as they are, in the code
// page of the machine that wrote it (GBK on a Chinese Windows, Shift-JIS on
// a Japanese one), and OCCT reads such bytes as UTF-8.

#include <cstddef>
#include <string>

namespace mitcad::io::detail {

// What recode_step_names found in a file and did.
struct StepNameRecoding {
  // The text literals of the file that are not empty (names, ids,
  // descriptions, the header's texts).
  std::size_t names = 0;
  // Those with bytes outside ASCII written as they are.
  std::size_t not_ascii = 0;
  // Of those, the ones in valid UTF-8, kept as they are.
  std::size_t utf8 = 0;
  // The ones decoded with code_page and rewritten as \X2\ escapes.
  std::size_t decoded = 0;
  // The code page of the names that are not UTF-8: "GBK", "Shift-JIS",
  // "CP1252", or "UTF-8" when all are UTF-8 (or there are none), "none"
  // when no code page decodes any of them.
  std::string code_page = "UTF-8";
};

// Rewrites, in the text of a STEP file, each text literal whose raw bytes
// are not valid UTF-8: the first code page of GBK, Shift-JIS and CP1252
// that decodes every such literal of the file without an error, else the
// one that decodes the most, decodes them (one code page per file), and
// their characters outside ASCII are written as STEP's \X2\ escapes, which
// every code page reads alike. Literals in UTF-8, in ASCII, and those the
// chosen code page cannot decode are left as they are; so is everything
// outside the literals (comments included).
StepNameRecoding recode_step_names(std::string& text);

// The log line of a recoding: "STEP names: GBK (12 of 40 names not ASCII)",
// with the names in UTF-8 and those left undecoded after a semicolon when
// there are any.
std::string describe(const StepNameRecoding& recoding);

} // namespace mitcad::io::detail
