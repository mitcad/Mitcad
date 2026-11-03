// SPDX-License-Identifier: MIT
//
// Keeps the user-facing texts to the UI conventions (docs/ui-conventions.md,
// "The check"; mitcad#852, mitcad#1045): no "..." for the ellipsis, the
// program's name MitCad and not Mitcad, "›" and not " > " between the
// levels of a menu path, none of the retired terms and no British spelling;
// short prepositions, articles and conjunctions in lower case inside Title
// Case ("Save as Mesh", not "Save As Mesh"); "×" in sizes and "°" in angles
// of the app's texts; no user-facing AppKit literal outside the system menu
// items; and no '{x}' quoting in the Rust messages.
//
//   mitcad-check-ui-strings DIR|FILE.md...
//                                  scans the .cpp, .hpp, .mm and .rs files
//                                  under each DIR (folders named tests, bin,
//                                  model-tests, model-testkit and fuzz left
//                                  out) and each Markdown file; prints every
//                                  text that breaks a rule as file:line and
//                                  fails if any
//   mitcad-check-ui-strings --self-test
//                                  checks the scan on planted sources
//
// The scan reads the source text, not the compiled program. In C++ and
// Objective-C++ it tokenises each file (comments and preprocessor lines left
// out, adjacent string literals joined) and takes the texts of tr("..."),
// QT_TR_NOOP("..."), translate("context", "...") and
// QT_TRANSLATE_NOOP("context", "..."), and the Objective-C literals @"..."
// except identifiers (a lower-case word without spaces, such as
// @"native.ribbon" or an SF Symbol's name). An Objective-C literal that reads
// as text (words with a space, or a capitalised word) is itself a finding
// (rule 10: the AppKit frontend takes its texts through tr()), unless it is
// one of kObjcAllowed: the system menu items and the titles code compares
// with. In Markdown it reads the prose: fenced code blocks, code spans, link
// targets, URLs and HTML tags left out. Texts built at run time and
// QStringLiteral texts are not seen. In Rust it reads the string literals of
// the sources (the item each #[cfg(test)] guards left out: a test module, a
// test helper, a field; the scan goes on after it) and finds a value quoted
// as '{x}' (rule 6: a name stands unquoted, a value that could be misread
// gets curly quotes).
//
// A text that must keep a word the rules retire (a string of Qt's own
// translation context, a file format's own term) goes into kAllowed with
// its reason; a Rust string whose quotes belong to a syntax goes into
// kRustAllowed.

#include <algorithm>
#include <cctype>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <set>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

namespace {

// Texts that keep a retired form, and why: the rule's name and a fragment of
// the text.
struct Allowed {
  const char* rule;
  const char* fragment;
  const char* reason;
};
const Allowed kAllowed[] = {
    {"\"...\"", "Show Details...", "QMessageBox's own source text, looked up in Qt's translations"},
    {"\"...\"", "Hide Details...", "QMessageBox's own source text, looked up in Qt's translations"},
    {"deg", "The value to key (30 deg, 12 mm)", "an example of the expression typed in the field"},
};

// Objective-C literals that read as text but are not the frontend's own
// texts, matched whole.
struct ObjcAllowed {
  const char* text;
  const char* reason;
};
const ObjcAllowed kObjcAllowed[] = {
    {"Main Menu", "the main menu's title, which the system does not show"},
    {"Services", "a system menu item"},
    {"Hide Others", "a system menu item"},
    {"Show All", "a system menu item"},
    {"Hide MitCad", "a system menu item"},
    {"Quit MitCad", "a system menu item"},
    {"Minimize", "a system menu item"},
    {"Zoom", "a system menu item"},
    {"Show Tab Bar", "a system window-tab menu item"},
    {"Show All Tabs", "a system window-tab menu item"},
    {"Merge All Windows", "a system window-tab menu item"},
    {"Bring All to Front", "a system menu item"},
    {"Cut", "a system menu item"},
    {"Copy", "a system menu item"},
    {"Paste", "a system menu item"},
    {"Select All", "a system menu item"},
    {"Edit", "the Edit menu's title, compared with to find its items"},
    {"Create", "a menu's title, compared with by the native test input"},
    {"Sketch", "a menu's title, compared with by the native test input"},
    {"Ag", "a sample of a font's letters, measured and not shown"},
};

// Rust strings whose quotes belong to a syntax, not to a message: the end
// of the file's path and a fragment of the string.
struct RustAllowed {
  const char* file;
  const char* fragment;
  const char* reason;
};
const RustAllowed kRustAllowed[] = {
    {"expr/src/format.rs", "'{}'", "an expression's text literal"},
    {"ifc/src/step.rs", "'{}'", "a STEP file's string"},
    {"print-keys/src/lib.rs", "'{text}'", "a slicer key's text value as its file writes it"},
    {"import-ifc/src/report.rs", "'{}'", "the import report quotes the names as the IFC file stores them"},
    {"import-ifc/src/lib.rs", "#{id} {} '{}'", "a comparison line of the IFC import's log"},
};

// Short words that Title Case keeps in lower case inside a label.
const std::set<std::string> kSmallWords = {
    "a",  "an",   "the",  "and", "but", "or",   "nor",  "as",  "at",  "by",  "for",
    "from", "in", "into", "of",  "on",  "onto", "per",  "to",  "via", "with",
};

// Retired terms, matched as they are written (docs/ui-conventions.md).
const char* const kRetired[] = {
    "Library Parts",     "Part Picker",      "Part picker",     "part picker",    "Linear Part",
    "Derived Part",      "Parts List",       "Parts list",      "parts list",     "History marker",
    "history marker",    "Fully defined",    "fully defined",   "over-constrained", "datum plane",
    "Floating chrome",   "Search Commands",  "Change Parameters", "Save As DXF",
};

// Retired words, matched as whole words in any case: British spellings and
// "ribbon" (the toolbar).
const std::set<std::string> kRetiredWords = {
    "colour",     "colours",     "coloured",   "colouring",  "centre",     "centres",    "centred",
    "centring",   "metre",       "metres",     "millimetre", "millimetres", "centimetre", "centimetres",
    "kilometre",  "kilometres",  "licence",    "licences",   "cancelled",  "cancelling", "modelling",
    "modelled",   "favourite",   "favourites", "mitre",      "mitres",     "mitred",     "grey",
    "greys",      "greyed",      "behaviour",  "behaviours", "neighbour",  "neighbours", "catalogue",
    "catalogues", "aluminium",   "ribbon",
};

bool identStart(char c) {
  return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') || c == '_';
}
bool identChar(char c) {
  return identStart(c) || (c >= '0' && c <= '9');
}
bool letter(char c) {
  return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z');
}

bool digit(char c) {
  return c >= '0' && c <= '9';
}
bool upperCase(char c) {
  return c >= 'A' && c <= 'Z';
}

// A Title Case label with a short word capitalised between two capitalised
// words: "Save As Mesh", "Pitch From Walls" (rule 1). A short word after
// another one ("With In Place") is a name's start, not a label's middle.
bool titleCaseSmallWord(const std::string& text) {
  struct Word {
    std::size_t start;
    std::size_t end;
  };
  std::vector<Word> words;
  for (std::size_t i = 0; i < text.size();) {
    if (!letter(text[i])) {
      ++i;
      continue;
    }
    std::size_t end = i;
    while (end < text.size() && letter(text[end])) {
      ++end;
    }
    words.push_back({i, end});
    i = end;
  }
  const auto lowered = [&text](const Word& w) {
    std::string out;
    for (std::size_t i = w.start; i < w.end; ++i) {
      out += static_cast<char>(std::tolower(static_cast<unsigned char>(text[i])));
    }
    return out;
  };
  for (std::size_t k = 1; k + 1 < words.size(); ++k) {
    const Word& previous = words[k - 1];
    const Word& middle = words[k];
    const Word& next = words[k + 1];
    if (middle.start != previous.end + 1 || text[previous.end] != ' ' || next.start != middle.end + 1 ||
        text[middle.end] != ' ') {
      continue;
    }
    if (!upperCase(text[middle.start]) || !kSmallWords.count(lowered(middle))) {
      continue;
    }
    bool rest = true; // "As", not "AS"
    for (std::size_t i = middle.start + 1; i < middle.end; ++i) {
      rest = rest && !upperCase(text[i]);
    }
    // Single letters around it are keys or letters ("W A S D"), not words.
    const bool words2 = previous.end - previous.start >= 2 && next.end - next.start >= 2;
    if (rest && words2 && upperCase(text[previous.start]) && !kSmallWords.count(lowered(previous)) &&
        upperCase(text[next.start])) {
      return true;
    }
  }
  return false;
}

// "deg" after a number: "between 0 and 90 deg" (rule 7: "°" for angles
// shown to the user; "deg" only in expressions).
bool degrees(const std::string& text) {
  for (std::size_t at = text.find("deg"); at != std::string::npos; at = text.find("deg", at + 1)) {
    const bool number = at > 0 && (digit(text[at - 1]) || (at > 1 && text[at - 1] == ' ' && digit(text[at - 2])));
    if (number && (at + 3 == text.size() || !letter(text[at + 3]))) {
      return true;
    }
  }
  return false;
}

// " x " between numbers or placeholders: "%1 x %2 mm" (rule 7: "×" in
// sizes). A designation without spaces ("M10x1.5") is not a size.
bool timesInSize(const std::string& text) {
  for (std::size_t at = text.find(" x "); at != std::string::npos; at = text.find(" x ", at + 1)) {
    if (at > 0 && digit(text[at - 1]) && at + 3 < text.size() && (digit(text[at + 3]) || text[at + 3] == '%')) {
      return true;
    }
  }
  return false;
}

// The rules a text breaks, each by its name ("\"...\"", "Mitcad", ...);
// `source` for a text of the app's sources, not of a document.
std::vector<std::string> broken(const std::string& text, bool source) {
  std::vector<std::string> rules;
  const auto add = [&rules, &text](const std::string& rule) {
    for (const Allowed& allowed : kAllowed) {
      if (rule == allowed.rule && text.find(allowed.fragment) != std::string::npos) {
        return;
      }
    }
    if (std::find(rules.begin(), rules.end(), rule) == rules.end()) {
      rules.push_back(rule);
    }
  };
  for (std::size_t at = text.find("..."); at != std::string::npos; at = text.find("...", at + 1)) {
    const bool before = at > 0 && text[at - 1] == '.';
    const bool after = at + 3 < text.size() && text[at + 3] == '.';
    if (!before && !after) {
      add("\"...\"");
      break;
    }
  }
  for (std::size_t at = text.find("Mitcad"); at != std::string::npos; at = text.find("Mitcad", at + 1)) {
    const char before = at > 0 ? text[at - 1] : ' ';
    const char after = at + 6 < text.size() ? text[at + 6] : ' ';
    if (!identChar(before) && before != '/' && before != '.' && before != '-' && !identChar(after) &&
        after != '/' && after != '-') {
      add("Mitcad");
      break;
    }
  }
  for (std::size_t at = text.find(" > "); at != std::string::npos; at = text.find(" > ", at + 1)) {
    if (at > 0 && identChar(text[at - 1]) && at + 3 < text.size() && text[at + 3] >= 'A' && text[at + 3] <= 'Z') {
      add("\" > \"");
      break;
    }
  }
  for (const char* term : kRetired) {
    if (text.find(term) != std::string::npos) {
      add(term);
    }
  }
  std::string word;
  for (std::size_t i = 0; i <= text.size(); ++i) {
    const char c = i < text.size() ? text[i] : ' ';
    if (letter(c)) {
      word += static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
      continue;
    }
    if (kRetiredWords.count(word)) {
      add(word);
    }
    word.clear();
  }
  if (titleCaseSmallWord(text)) {
    add("Title Case");
  }
  if (degrees(text)) {
    add("deg");
  }
  if (source && timesInSize(text)) {
    add("x in a size");
  }
  return rules;
}

// An Objective-C literal that reads as text: words with a space between
// them, or one capitalised word ("Browser"), not an identifier in camel case
// ("NSObject", "MitCad"), in capitals ("MODIFY") or with dots or slashes.
bool objcText(const std::string& text) {
  std::size_t run = 0;
  std::size_t longest = 0;
  for (const char c : text) {
    run = letter(c) ? run + 1 : 0;
    longest = std::max(longest, run);
  }
  if (longest < 2) {
    return false; // "%@ %@", "\r", a single letter
  }
  if (text.find(' ') != std::string::npos) {
    return true;
  }
  if (!(text[0] >= 'A' && text[0] <= 'Z')) {
    return false;
  }
  for (std::size_t i = 1; i < text.size(); ++i) {
    if (!(text[i] >= 'a' && text[i] <= 'z')) {
      return false;
    }
  }
  return true;
}

bool objcAllowed(const std::string& text) {
  for (const ObjcAllowed& allowed : kObjcAllowed) {
    if (text == allowed.text) {
      return true;
    }
  }
  return false;
}

enum class Kind { Ident, String, Number, Punct };

struct Token {
  Kind kind;
  std::string text; // a string literal's content, escapes kept as written
  int line;
  bool objc; // an Objective-C literal @"..."
};

class Lexer {
public:
  explicit Lexer(const std::string& source) : m_s(source) {}

  std::vector<Token> run() {
    std::vector<Token> tokens;
    bool lineStart = true;
    bool objc = false;
    while (m_i < m_s.size()) {
      const char c = m_s[m_i];
      if (c == '\n') {
        ++m_line;
        ++m_i;
        lineStart = true;
        continue;
      }
      if (c == ' ' || c == '\t' || c == '\r' || c == '\f' || c == '\v') {
        ++m_i;
        continue;
      }
      if (c == '/' && peek(1) == '/') {
        while (m_i < m_s.size() && m_s[m_i] != '\n') {
          ++m_i;
        }
        continue;
      }
      if (c == '/' && peek(1) == '*') {
        m_i += 2;
        while (m_i < m_s.size() && !(m_s[m_i] == '*' && peek(1) == '/')) {
          if (m_s[m_i] == '\n') {
            ++m_line;
          }
          ++m_i;
        }
        m_i += 2;
        continue;
      }
      if (c == '#' && lineStart) {
        skipDirective();
        continue;
      }
      lineStart = false;
      if (c == '@' && peek(1) == '"') {
        objc = true;
        ++m_i;
        continue;
      }
      if (identStart(c)) {
        const std::size_t start = m_i;
        while (m_i < m_s.size() && identChar(m_s[m_i])) {
          ++m_i;
        }
        const std::string word = m_s.substr(start, m_i - start);
        if (m_i < m_s.size() && m_s[m_i] == '"') {
          if (word == "R" || word == "u8R" || word == "uR" || word == "UR" || word == "LR") {
            tokens.push_back(rawString());
            continue;
          }
          if (word == "u8" || word == "u" || word == "U" || word == "L") {
            tokens.push_back(string(false));
            continue;
          }
        }
        if (m_i < m_s.size() && m_s[m_i] == '\'' && (word == "u8" || word == "u" || word == "U" || word == "L")) {
          character();
          tokens.push_back({Kind::Number, "'c'", m_line, false});
          continue;
        }
        tokens.push_back({Kind::Ident, word, m_line, false});
        continue;
      }
      if (std::isdigit(static_cast<unsigned char>(c)) ||
          (c == '.' && std::isdigit(static_cast<unsigned char>(peek(1))))) {
        const std::size_t start = m_i;
        while (m_i < m_s.size()) {
          const char d = m_s[m_i];
          if ((d == '+' || d == '-') && m_i > start &&
              (m_s[m_i - 1] == 'e' || m_s[m_i - 1] == 'E' || m_s[m_i - 1] == 'p' || m_s[m_i - 1] == 'P')) {
            ++m_i;
          } else if (identChar(d) || d == '.' || (d == '\'' && m_i + 1 < m_s.size() && identChar(m_s[m_i + 1]))) {
            ++m_i;
          } else {
            break;
          }
        }
        tokens.push_back({Kind::Number, m_s.substr(start, m_i - start), m_line, false});
        continue;
      }
      if (c == '"') {
        tokens.push_back(string(objc));
        objc = false;
        continue;
      }
      if (c == '\'') {
        character();
        tokens.push_back({Kind::Number, "'c'", m_line, false});
        continue;
      }
      tokens.push_back({Kind::Punct, std::string(1, c), m_line, false});
      ++m_i;
    }
    return tokens;
  }

private:
  char peek(std::size_t ahead) const {
    return m_i + ahead < m_s.size() ? m_s[m_i + ahead] : '\0';
  }

  void skipDirective() {
    while (m_i < m_s.size() && m_s[m_i] != '\n') {
      if (m_s[m_i] == '\\' && peek(1) == '\n') {
        ++m_line;
        m_i += 2;
        continue;
      }
      if (m_s[m_i] == '\\' && peek(1) == '\r' && peek(2) == '\n') {
        ++m_line;
        m_i += 3;
        continue;
      }
      ++m_i;
    }
  }

  Token string(bool objc) {
    const int line = m_line;
    std::string text;
    ++m_i; // the opening quote
    while (m_i < m_s.size() && m_s[m_i] != '"' && m_s[m_i] != '\n') {
      if (m_s[m_i] == '\\' && m_i + 1 < m_s.size()) {
        text += m_s[m_i++];
        // A backslash-newline continues the literal on the next line.
        if (m_s[m_i] == '\r' && peek(1) == '\n') {
          text += m_s[m_i++];
        }
        if (m_s[m_i] == '\n') {
          ++m_line;
        }
      }
      text += m_s[m_i++];
    }
    ++m_i; // the closing quote
    return {Kind::String, text, line, objc};
  }

  Token rawString() {
    const int line = m_line;
    ++m_i; // the quote
    const std::size_t open = m_s.find('(', m_i);
    if (open == std::string::npos) {
      m_i = m_s.size();
      return {Kind::String, "", line, false};
    }
    const std::string close = ")" + m_s.substr(m_i, open - m_i) + "\"";
    const std::size_t end = m_s.find(close, open + 1);
    const std::size_t stop = end == std::string::npos ? m_s.size() : end;
    const std::string text = m_s.substr(open + 1, stop - open - 1);
    m_line += static_cast<int>(std::count(text.begin(), text.end(), '\n'));
    m_i = end == std::string::npos ? m_s.size() : end + close.size();
    return {Kind::String, text, line, false};
  }

  void character() {
    ++m_i;
    while (m_i < m_s.size() && m_s[m_i] != '\'' && m_s[m_i] != '\n') {
      m_i += m_s[m_i] == '\\' ? 2 : 1;
    }
    ++m_i;
  }

  const std::string& m_s;
  std::size_t m_i = 0;
  int m_line = 1;
};

struct Violation {
  std::string file;
  int line;
  std::vector<std::string> rules;
  std::string text;
};

bool punct(const Token& t, const char* text) {
  return t.kind == Kind::Punct && t.text == text;
}

// An Objective-C literal that is an identifier, not a text: a lower-case
// word without spaces ("native.ribbon", "sidebar.left").
bool identifier(const std::string& text) {
  if (text.empty() || !(text[0] >= 'a' && text[0] <= 'z')) {
    return text.empty();
  }
  return std::all_of(text.begin(), text.end(),
                     [](char c) { return identChar(c) || c == '.' || c == '-' || c == ':'; });
}

// `source` for a text of the app's sources, `objc` for an Objective-C
// literal (rule 10).
void checkText(const std::string& file, int line, const std::string& text, bool source, bool objc,
               std::vector<Violation>& out) {
  std::vector<std::string> rules = broken(text, source);
  if (objc && objcText(text) && !objcAllowed(text)) {
    rules.push_back("@\"text\"");
  }
  if (!rules.empty()) {
    out.push_back({file, line, rules, text});
  }
}

// The texts of a C++ or Objective-C++ source.
void scanSource(const std::string& file, const std::string& source, std::vector<Violation>& out) {
  const std::vector<Token> t = Lexer(source).run();
  // The run of adjacent literals from `at`, joined; `end` after it.
  const auto literals = [&t](std::size_t at, std::size_t& end) {
    std::string text;
    end = at;
    while (end < t.size() && t[end].kind == Kind::String) {
      text += t[end].text;
      ++end;
    }
    return text;
  };
  for (std::size_t i = 0; i + 2 < t.size(); ++i) {
    if (t[i].kind != Kind::Ident || !punct(t[i + 1], "(")) {
      continue;
    }
    const std::string& name = t[i].text;
    std::size_t at = i + 2;
    if (name == "translate" || name == "QT_TRANSLATE_NOOP") {
      std::size_t end = at;
      literals(at, end);
      if (end == at || end >= t.size() || !punct(t[end], ",")) {
        continue;
      }
      at = end + 1;
    } else if (name != "tr" && name != "QT_TR_NOOP") {
      continue;
    }
    if (at < t.size() && t[at].kind == Kind::String) {
      std::size_t end = at;
      const std::string text = literals(at, end);
      checkText(file, t[at].line, text, true, false, out);
    }
  }
  for (const Token& token : t) {
    if (token.kind == Kind::String && token.objc && !identifier(token.text)) {
      checkText(file, token.line, token.text, true, true, out);
    }
  }
}

// Markdown prose without its code, link targets, URLs and HTML tags.
std::string prose(const std::string& line) {
  std::string out;
  std::size_t i = 0;
  while (i < line.size()) {
    if (line[i] == '`') {
      std::size_t run = 0;
      while (i + run < line.size() && line[i + run] == '`') {
        ++run;
      }
      const std::size_t close = line.find(std::string(run, '`'), i + run);
      if (close == std::string::npos) {
        out += line.substr(i, run);
        i += run;
        continue;
      }
      out += ' ';
      i = close + run;
      continue;
    }
    if (line[i] == ']' && i + 1 < line.size() && line[i + 1] == '(') {
      const std::size_t close = line.find(')', i + 2);
      if (close != std::string::npos) {
        out += "] ";
        i = close + 1;
        continue;
      }
    }
    if (line.compare(i, 7, "http://") == 0 || line.compare(i, 8, "https://") == 0) {
      while (i < line.size() && line[i] != ' ' && line[i] != '\t') {
        ++i;
      }
      continue;
    }
    if (line[i] == '<') {
      const std::size_t close = line.find('>', i + 1);
      if (close != std::string::npos) {
        out += ' ';
        i = close + 1;
        continue;
      }
    }
    out += line[i++];
  }
  return out;
}

void scanMarkdown(const std::string& file, const std::string& source, std::vector<Violation>& out) {
  std::istringstream in(source);
  std::string line;
  int number = 0;
  bool fence = false;
  while (std::getline(in, line)) {
    ++number;
    const std::size_t first = line.find_first_not_of(" \t");
    if (first != std::string::npos && (line.compare(first, 3, "```") == 0 || line.compare(first, 3, "~~~") == 0)) {
      fence = !fence;
      continue;
    }
    if (!fence) {
      checkText(file, number, prose(line), false, false, out);
    }
  }
}

// The words that start a Rust item or statement whose own commas (a where
// clause, a closure's parameters) do not end it, unlike a field's, a
// variant's or a match arm's.
const std::set<std::string> kRustItemWords = {
    "fn",  "impl", "struct", "enum", "union",  "trait", "type",       "const",
    "static", "let", "use",  "mod",  "extern", "unsafe", "macro_rules",
};

// The item a #[cfg(test)] guards, skipped up to its end: its `;` or its
// closing brace at its own level, its `,` as a field, variant, field value
// or match arm, or the end of the braces around it. `depth` counts the
// brackets, parentheses and braces open in the item, `angle` the generic
// arguments open at its own level ("HashMap<K, V>,").
struct RustSkip {
  bool on = false;
  int depth = 0;
  int angle = 0;
  bool item = false; // started with one of kRustItemWords
};

// The string literals of a Rust source with their lines: plain, byte and
// raw strings; comments, characters, lifetimes and the items #[cfg(test)]
// guards (a file's test module, a test helper, a field) left out.
std::vector<std::pair<int, std::string>> rustStrings(const std::string& s) {
  std::vector<std::pair<int, std::string>> strings;
  int line = 1;
  std::size_t i = 0;
  RustSkip skip;
  const auto peek = [&s](std::size_t at) { return at < s.size() ? s[at] : '\0'; };
  // A punctuation character at `at`: the brackets and the item's end while
  // an item is skipped.
  const auto punctuation = [&s, &skip, &peek](std::size_t at) {
    const char c = s[at];
    if (!skip.on) {
      return;
    }
    if (c == '(' || c == '[' || c == '{') {
      ++skip.depth;
    } else if (c == ')' || c == ']' || c == '}') {
      --skip.depth;
      // Its balanced braces end the item; a closing bracket below its level
      // closes what holds it ("S { #[cfg(test)] a: u8 }").
      skip.on = skip.depth > 0 || (skip.depth == 0 && c != '}');
    } else if (skip.depth == 0 && c == ';') {
      skip.on = false;
    } else if (skip.depth == 0 && c == ',' && skip.angle == 0 && !skip.item) {
      skip.on = false;
    } else if (skip.depth == 0 && c == '<' && peek(at + 1) != '=' && peek(at + 1) != '<' &&
               (at == 0 || s[at - 1] != '<')) {
      ++skip.angle;
    } else if (skip.depth == 0 && c == '>' && skip.angle > 0 && at > 0 && s[at - 1] != '-' && s[at - 1] != '=' &&
               peek(at + 1) != '=') {
      --skip.angle;
    }
  };
  while (i < s.size()) {
    const char c = s[i];
    if (c == '\n') {
      ++line;
      ++i;
      continue;
    }
    if (c == '/' && peek(i + 1) == '/') {
      while (i < s.size() && s[i] != '\n') {
        ++i;
      }
      continue;
    }
    if (c == '/' && peek(i + 1) == '*') {
      int depth = 1;
      i += 2;
      while (i < s.size() && depth > 0) {
        if (s[i] == '/' && peek(i + 1) == '*') {
          ++depth;
          i += 2;
        } else if (s[i] == '*' && peek(i + 1) == '/') {
          --depth;
          i += 2;
        } else {
          line += s[i] == '\n' ? 1 : 0;
          ++i;
        }
      }
      continue;
    }
    if (!skip.on && c == '#' && s.compare(i, 12, "#[cfg(test)]") == 0) {
      skip = RustSkip();
      skip.on = true;
      i += 12;
      continue;
    }
    if (identStart(c)) {
      const std::size_t start = i;
      while (i < s.size() && identChar(s[i])) {
        ++i;
      }
      const std::string word = s.substr(start, i - start);
      if (skip.on && skip.depth == 0 && kRustItemWords.count(word)) {
        skip.item = true;
      }
      if ((word == "r" || word == "br" || word == "cr") && (peek(i) == '"' || peek(i) == '#')) {
        std::size_t hashes = 0;
        while (peek(i + hashes) == '#') {
          ++hashes;
        }
        if (peek(i + hashes) != '"') {
          continue;
        }
        const std::string close = "\"" + std::string(hashes, '#');
        const std::size_t from = i + hashes + 1;
        const std::size_t end = s.find(close, from);
        const std::size_t stop = end == std::string::npos ? s.size() : end;
        const std::string text = s.substr(from, stop - from);
        if (!skip.on) {
          strings.emplace_back(line, text);
        }
        line += static_cast<int>(std::count(text.begin(), text.end(), '\n'));
        i = end == std::string::npos ? s.size() : end + close.size();
      }
      continue;
    }
    if (c == '"') {
      const int first = line;
      std::string text;
      ++i;
      while (i < s.size() && s[i] != '"') {
        if (s[i] == '\\' && i + 1 < s.size()) {
          text += s[i++];
        }
        line += s[i] == '\n' ? 1 : 0;
        text += s[i++];
      }
      ++i;
      if (!skip.on) {
        strings.emplace_back(first, text);
      }
      continue;
    }
    if (c == '\'') {
      // A character ('a', '\'', '\u{2026}', 'ä') or a lifetime ('a).
      if (peek(i + 1) == '\\') {
        std::size_t end = i + 3;
        while (end < s.size() && s[end] != '\'' && s[end] != '\n') {
          ++end;
        }
        i = end + 1;
        continue;
      }
      std::size_t width = 1;
      const unsigned char lead = static_cast<unsigned char>(peek(i + 1));
      if (lead >= 0xF0) {
        width = 4;
      } else if (lead >= 0xE0) {
        width = 3;
      } else if (lead >= 0xC0) {
        width = 2;
      }
      i += peek(i + 1 + width) == '\'' ? width + 2 : 1;
      continue;
    }
    punctuation(i);
    ++i;
  }
  return strings;
}

// A value quoted as '{x}' in a Rust string (rule 6).
bool rustQuoted(const std::string& text) {
  for (std::size_t at = text.find("'{"); at != std::string::npos; at = text.find("'{", at + 1)) {
    const std::size_t close = text.find('}', at + 2);
    if (close == std::string::npos) {
      return false;
    }
    const std::string inner = text.substr(at + 2, close - at - 2);
    if (inner.find_first_of("{'") == std::string::npos && close + 1 < text.size() && text[close + 1] == '\'') {
      return true;
    }
  }
  return false;
}

void scanRust(const std::string& file, const std::string& source, std::vector<Violation>& out) {
  for (const auto& [line, text] : rustStrings(source)) {
    if (!rustQuoted(text)) {
      continue;
    }
    bool allowed = false;
    for (const RustAllowed& rule : kRustAllowed) {
      const std::string end = rule.file;
      allowed = allowed || (file.size() >= end.size() && file.compare(file.size() - end.size(), end.size(), end) == 0 &&
                            text.find(rule.fragment) != std::string::npos);
    }
    if (!allowed) {
      out.push_back({file, line, {"'{x}'"}, text});
    }
  }
}

std::string readFile(const std::filesystem::path& path) {
  std::ifstream in(path, std::ios::binary);
  std::stringstream text;
  text << in.rdbuf();
  return text.str();
}

std::vector<Violation> scan(const std::vector<std::string>& paths) {
  namespace fs = std::filesystem;
  std::vector<Violation> out;
  for (const std::string& given : paths) {
    const fs::path path = fs::path(given).lexically_normal();
    const fs::path base = path.has_parent_path() ? path.parent_path() : fs::path();
    if (fs::is_regular_file(path)) {
      scanMarkdown(path.lexically_relative(base).generic_string(), readFile(path), out);
      continue;
    }
    // Tests, developer tools and test support are not the user's texts.
    static const std::set<std::string> skipped = {"tests", "bin", "model-tests", "model-testkit", "fuzz"};
    std::vector<fs::path> files;
    for (auto it = fs::recursive_directory_iterator(path); it != fs::recursive_directory_iterator(); ++it) {
      if (it->is_directory() && skipped.count(it->path().filename().string())) {
        it.disable_recursion_pending();
        continue;
      }
      const std::string ext = it->path().extension().string();
      const std::string stem = it->path().stem().string();
      const bool test = stem == "tests" || stem == "testdata" ||
                        (stem.size() > 6 && stem.compare(stem.size() - 6, 6, "_tests") == 0);
      if (it->is_regular_file() && (ext == ".cpp" || ext == ".hpp" || ext == ".mm" || (ext == ".rs" && !test))) {
        files.push_back(it->path());
      }
    }
    std::sort(files.begin(), files.end());
    for (const fs::path& file : files) {
      const std::string name = file.lexically_relative(base).generic_string();
      if (file.extension() == ".rs") {
        scanRust(name, readFile(file), out);
      } else {
        scanSource(name, readFile(file), out);
      }
    }
  }
  return out;
}

std::string joined(const std::vector<std::string>& rules) {
  std::string text;
  for (const std::string& rule : rules) {
    text += (text.empty() ? "" : ", ") + rule;
  }
  return text;
}

// Planted sources: what the scan must find, and what it must not.
int selfTest() {
  const std::string source =
      "// SPDX-License-Identifier: MIT\n"                                      // 1
      "void f() {\n"                                                           // 2
      "  a->setText(tr(\"Open...\"));\n"                                       // 3: "..."
      "  b->setText(tr(\"Open\xE2\x80\xA6\"));\n"                              // 4: fine (an ellipsis)
      "  c->setText(QObject::tr(\"Mitcad is \"\n"                              // 5: Mitcad, joined
      "                         \"open\"));\n"                                 // 6
      "  d->setText(tr(\"MitCad's mitcad-cli and Mitcad/Mitcad\"));\n"         // 7: fine
      "  e->setText(tr(\"In Tools > Preferences\"));\n"                        // 8: " > "
      "  f->setText(tr(\"x > 0 and a > b\"));\n"                               // 9: fine
      "  g->setText(tr(\"The Parts List and the colour\"));\n"                 // 10: two rules
      "  h->setText(QStringLiteral(\"Parts List colour...\"));\n"              // 11: not a tr text
      "  // tr(\"Mitcad colour\")\n"                                           // 12: a comment
      "  i = QCoreApplication::translate(\"ctx colour\", \"the centre\");\n"   // 13: the text only
      "  j.title = @\"Show Inspector\";\n"                                     // 14: fine
      "  k.identifier = @\"native.ribbon\";\n"                                 // 15: an identifier
      "  l.label = @\"Scroll the ribbon\";\n"                                  // 16: ribbon
      "  m = QMessageBox::tr(\"Show Details...\");\n"                          // 17: allowed
      "  n = tr(\"Recolouring and centrelines\");\n"                           // 18: not whole words
      "  o = tr(\"Grey\");\n"                                                   // 19: any case
      "  p = tr(\"Save As Mesh and Save as Copy\");\n"                          // 20: Title Case
      "  q = tr(\"%1 x %2 mm, M10x1.5, at 30 deg; 30 degrees\");\n"             // 21: x, deg
      "  r = tr(\"With In Place, Zoom In\");\n"                                 // 22: fine
      "  s.title = @\"Hide Others\";\n"                                         // 23: allowed
      "  t = NSClassFromString(@\"MitcadNativeTile\");\n"                       // 24: an identifier
      "  u = @\"%@ %@\";\n"                                                     // 25: a format
      "  v = tr(\"Open the \\\n"                                                // 26: continued
      "and save\");\n"                                                          // 27
      "  w = tr(\"Mitcad again\");\n"                                           // 28: Mitcad
      "}\n";
  const std::string markdown =
      "# Title\n"                                                              // 1
      "Open `Mitcad...` and [the guide](a/Mitcad.md).\n"                       // 2: fine
      "Choose Tools > Preferences...\n"                                        // 3: two rules
      "```\n"                                                                  // 4
      "Mitcad colour...\n"                                                     // 5: code
      "```\n"                                                                  // 6
      "See https://example.invalid/colour and <a href=\"centre\">x</a>.\n"     // 7: fine
      "The licence of Mitcad.\n"                                               // 8: two rules
      "Use **Save As Mesh** at 30 deg on 2 x 3 mm.\n";                         // 9: no x rule here
  std::vector<Violation> found;
  const std::string rust =
      "// SPDX-License-Identifier: MIT\n"                                      // 1
      "#[cfg(test)]\n"                                                         // 2: tests in
      "mod tests;\n"                                                           // 3: their own file
      "fn f<'a>(x: &'a str) -> String {\n"                                     // 4: lifetimes
      "    let (c, d, e) = ('\\'', '\"', '\xC3\xA4');\n"                         // 5: characters
      "    format!(\"unknown parameter '{x}'\")\n"                              // 6: '{x}'
      "}\n"                                                                    // 7
      "// format!(\"'{x}'\")\n"                                                // 8: a comment
      "const R: &str = r#\"a \"raw\" '{y}' string\"#;\n"                       // 9: '{x}'
      "const S: &str = \"the {name} and \xE2\x80\x9C{text}\xE2\x80\x9D\";\n"   // 10: fine
      "const T: &str = \"'{{' and '{}'\";\n"                                   // 11: '{x}'
      "#[cfg(test)]\n"                                                         // 12
      "mod tests { const U: &str = \"'{z}'\"; }\n"                             // 13: a test
      "#[cfg(test)]\n"                                                         // 14: a helper
      "fn h<A, B>(a: A) -> String where A: Into<String>, B: Copy "              // 15
      "{ format!(\"'{z}' {}\", '}') }\n"                                       //
      "const W: &str = \"'{w}'\";\n"                                           // 16: '{x}'
      "fn g() -> E { E { #[cfg(test)] hook: \"'{z}'\".into(), "                // 17: a field
      "name: \"'{v}'\".into() } }\n"                                           //     then '{x}'
      "#[cfg(test)]\n"                                                         // 18
      "const X: [&str; 2] = [\"'{z}'\", \"b\"];\n"                             // 19: a test value
      "fn k() -> E { E { #[cfg(test)] hook: M::<String, u8>::new(), "          // 20: a field
      "name: \"the '{t}'\" } }\n"                                              //     then '{x}'
      "const Y: &str = \"the value '{u}'\";\n";                                // 21: '{x}'
  scanSource("planted.cpp", source, found);
  scanMarkdown("planted.md", markdown, found);
  scanRust("core/planted/src/lib.rs", rust, found);
  scanRust("core/expr/src/format.rs", "fn q(t: &str) -> String { format!(\"'{}'\", t) }\n", found);
  const std::set<std::string> expected = {
      "planted.cpp:3 \"...\"",          "planted.cpp:5 Mitcad",
      "planted.cpp:8 \" > \"",          "planted.cpp:10 Parts List, colour",
      "planted.cpp:13 centre",
      "planted.cpp:19 grey",            "planted.md:3 \"...\", \" > \"",
      "planted.md:8 Mitcad, licence",
      "planted.cpp:14 @\"text\"",       "planted.cpp:16 ribbon, @\"text\"",
      "planted.cpp:20 Title Case",      "planted.cpp:21 deg, x in a size",
      "planted.md:9 Title Case, deg",   "planted.cpp:28 Mitcad",
      "core/planted/src/lib.rs:6 '{x}'",  "core/planted/src/lib.rs:9 '{x}'",
      "core/planted/src/lib.rs:11 '{x}'", "core/planted/src/lib.rs:16 '{x}'",
      "core/planted/src/lib.rs:17 '{x}'", "core/planted/src/lib.rs:20 '{x}'",
      "core/planted/src/lib.rs:21 '{x}'",
  };
  std::set<std::string> got;
  for (const Violation& v : found) {
    got.insert(v.file + ":" + std::to_string(v.line) + " " + joined(v.rules));
  }
  int failures = 0;
  for (const std::string& e : expected) {
    if (!got.count(e)) {
      std::cout << "self-test: not found: " << e << "\n";
      ++failures;
    }
  }
  for (const std::string& g : got) {
    if (!expected.count(g)) {
      std::cout << "self-test: found, not planted: " << g << "\n";
      ++failures;
    }
  }
  std::cout << "self-test: " << got.size() << " planted texts found, " << failures << " failures\n";
  return failures == 0 ? 0 : 1;
}

} // namespace

int main(int argc, char** argv) {
  if (argc == 2 && std::string(argv[1]) == "--self-test") {
    return selfTest();
  }
  if (argc < 2) {
    std::cerr << "usage: mitcad-check-ui-strings DIR|FILE.md... | --self-test\n";
    return 2;
  }
  std::vector<Violation> found;
  try {
    found = scan(std::vector<std::string>(argv + 1, argv + argc));
  } catch (const std::exception& error) {
    std::cerr << "mitcad-check-ui-strings: " << error.what() << "\n";
    return 2;
  }
  for (const Violation& v : found) {
    std::string shown = v.text.substr(0, 70);
    while (!shown.empty() && (static_cast<unsigned char>(shown.back()) & 0xC0) == 0x80) {
      shown.pop_back(); // not in the middle of a character
    }
    if (!shown.empty() && (static_cast<unsigned char>(shown.back()) & 0x80)) {
      shown.pop_back();
    }
    std::cout << v.file << ":" << v.line << ": " << joined(v.rules) << ": \"" << shown << "\"\n";
  }
  std::cout << found.size() << " texts break the UI conventions (docs/ui-conventions.md, \"The check\")\n";
  return found.empty() ? 0 : 1;
}
