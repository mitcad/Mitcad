// SPDX-License-Identifier: MIT
//
// The interface languages' catalogues (mitcad#1460, docs/translations.md):
// Qt Linguist files app/translations/mitcad_<code>.ts, one per language of
// app/translations/languages.txt.
//
//   mitcad-translations check LANGUAGES.TXT
//       the ctest repo.translations: every language listed has its .ts file
//       beside the list and every mitcad_*.ts there is listed; no obsolete
//       or vanished entries; each finished translation keeps its source's
//       placeholders (%1, %L2, %n) and its accelerator (&); and the count of
//       unfinished entries is the language's baseline in the list, which
//       only goes down as the language is completed (a lower count asks for
//       the lower baseline, a higher one fails)
//   mitcad-translations baseline LANGUAGES.TXT
//       writes each language's count of unfinished entries into the list
//       (after lupdate brought new texts: tools/translations-update.cmake)
//   mitcad-translations prepare IN.ts OUT.ts
//       the catalogue the build compiles: IN with a context-free copy of
//       every translated message (the context MitCad.AnyContext), which the
//       application looks a text up in when its context at run time is not
//       the one lupdate saw (app/framework/Language.cpp)
//   mitcad-translations pseudo IN.ts OUT.ts
//       a pseudo-language for tests (mitcad_xx): every message of IN
//       translated as its source with accented letters, a third longer and in
//       brackets, placeholders, accelerators, markup and a trailing ellipsis
//       or colon kept, to show untranslated and cut texts
//   mitcad-translations --self-test
//       checks the parser, the checks and the pseudo-language on planted
//       catalogues
//
// The .ts files are lupdate's XML; this reads them with a small scanner of
// its own (the elements context, name, message, source, comment,
// translation and numerusform, the five named entities, numeric character
// references and lupdate's <byte value="..."/>), plain C++17.

#include <algorithm>
#include <cstdio>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <map>
#include <set>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

namespace fs = std::filesystem;

namespace {

const char* const kAnyContext = "MitCad.AnyContext";

struct Message {
  std::string raw;           // the element as written, <message ...> to </message>
  std::string source;        // decoded
  std::string comment;       // decoded disambiguation
  std::string rawSource;     // <source>...</source> as written
  std::string rawComment;    // <comment>...</comment> as written, or empty
  std::string type;          // the translation's type: "", "unfinished", "vanished", "obsolete"
  bool numerus = false;
  std::vector<std::string> translations; // decoded: one, or the numerus forms
};

struct Context {
  std::string name;
  std::vector<Message> messages;
};

struct Catalogue {
  std::string header; // everything before the first <context>
  std::vector<Context> contexts;
  std::string language;
};

std::string readFile(const fs::path& path, bool& ok) {
  std::ifstream in(path, std::ios::binary);
  ok = static_cast<bool>(in);
  std::ostringstream text;
  text << in.rdbuf();
  return text.str();
}

bool writeFile(const fs::path& path, const std::string& text) {
  std::ofstream out(path, std::ios::binary);
  out << text;
  return static_cast<bool>(out);
}

void appendUtf8(std::string& out, unsigned long code) {
  if (code < 0x80) {
    out += static_cast<char>(code);
  } else if (code < 0x800) {
    out += static_cast<char>(0xc0 | (code >> 6));
    out += static_cast<char>(0x80 | (code & 0x3f));
  } else if (code < 0x10000) {
    out += static_cast<char>(0xe0 | (code >> 12));
    out += static_cast<char>(0x80 | ((code >> 6) & 0x3f));
    out += static_cast<char>(0x80 | (code & 0x3f));
  } else {
    out += static_cast<char>(0xf0 | (code >> 18));
    out += static_cast<char>(0x80 | ((code >> 12) & 0x3f));
    out += static_cast<char>(0x80 | ((code >> 6) & 0x3f));
    out += static_cast<char>(0x80 | (code & 0x3f));
  }
}

unsigned long parseNumber(const std::string& digits, int base) {
  unsigned long value = 0;
  for (const char c : digits) {
    int digit = -1;
    if (c >= '0' && c <= '9') digit = c - '0';
    else if (base == 16 && c >= 'a' && c <= 'f') digit = c - 'a' + 10;
    else if (base == 16 && c >= 'A' && c <= 'F') digit = c - 'A' + 10;
    if (digit < 0) return 0;
    value = value * static_cast<unsigned long>(base) + static_cast<unsigned long>(digit);
  }
  return value;
}

// XML text (with lupdate's <byte value="xHH"/>) to UTF-8.
std::string decode(const std::string& text) {
  std::string out;
  for (std::size_t i = 0; i < text.size();) {
    if (text.compare(i, 13, "<byte value=\"") == 0) {
      const std::size_t end = text.find("\"/>", i + 13);
      if (end != std::string::npos) {
        std::string value = text.substr(i + 13, end - i - 13);
        const bool hex = !value.empty() && value[0] == 'x';
        appendUtf8(out, parseNumber(hex ? value.substr(1) : value, hex ? 16 : 10));
        i = end + 3;
        continue;
      }
    }
    if (text[i] == '&') {
      const std::size_t end = text.find(';', i);
      if (end != std::string::npos && end - i <= 10) {
        const std::string name = text.substr(i + 1, end - i - 1);
        bool known = true;
        if (name == "lt") out += '<';
        else if (name == "gt") out += '>';
        else if (name == "amp") out += '&';
        else if (name == "apos") out += '\'';
        else if (name == "quot") out += '"';
        else if (name.size() > 2 && name[0] == '#' && (name[1] == 'x' || name[1] == 'X'))
          appendUtf8(out, parseNumber(name.substr(2), 16));
        else if (name.size() > 1 && name[0] == '#')
          appendUtf8(out, parseNumber(name.substr(1), 10));
        else
          known = false;
        if (known) {
          i = end + 1;
          continue;
        }
      }
    }
    out += text[i++];
  }
  return out;
}

// UTF-8 to XML text as lupdate writes it.
std::string encode(const std::string& text) {
  std::string out;
  for (const char c : text) {
    const auto byte = static_cast<unsigned char>(c);
    if (c == '&') out += "&amp;";
    else if (c == '<') out += "&lt;";
    else if (c == '>') out += "&gt;";
    else if (c == '\'') out += "&apos;";
    else if (c == '"') out += "&quot;";
    else if (byte < 0x20 && c != '\n') {
      char buffer[24];
      std::snprintf(buffer, sizeof buffer, "<byte value=\"x%x\"/>", static_cast<unsigned>(byte));
      out += buffer;
    } else {
      out += c;
    }
  }
  return out;
}

// The text of the first <tag ...>...</tag> in `text` from `from`, its
// attributes, and where the element starts and ends; false when there is none.
struct Element {
  std::size_t begin = 0, end = 0; // the whole element
  std::string attributes;
  std::string inner;
  bool found = false;
};

Element findElement(const std::string& text, const std::string& tag, std::size_t from, std::size_t limit) {
  Element element;
  std::size_t at = from;
  while (true) {
    at = text.find("<" + tag, at);
    if (at == std::string::npos || at >= limit) return element;
    if (at + tag.size() + 1 >= text.size()) return element;
    const char next = text[at + tag.size() + 1];
    if (next == '>' || next == ' ' || next == '/') break;
    ++at;
  }
  const std::size_t close = text.find('>', at);
  if (close == std::string::npos) return element;
  element.begin = at;
  element.found = true;
  element.attributes = text.substr(at + tag.size() + 1, close - at - tag.size() - 1);
  if (!element.attributes.empty() && element.attributes.back() == '/') {
    element.attributes.pop_back();
    element.end = close + 1;
    return element;
  }
  const std::string ending = "</" + tag + ">";
  const std::size_t stop = text.find(ending, close + 1);
  if (stop == std::string::npos) {
    element.found = false;
    return element;
  }
  element.inner = text.substr(close + 1, stop - close - 1);
  element.end = stop + ending.size();
  return element;
}

std::string attribute(const std::string& attributes, const std::string& name) {
  const std::string key = name + "=\"";
  const std::size_t at = attributes.find(key);
  if (at == std::string::npos) return std::string();
  const std::size_t end = attributes.find('"', at + key.size());
  return end == std::string::npos ? std::string() : attributes.substr(at + key.size(), end - at - key.size());
}

bool parse(const std::string& text, Catalogue& catalogue, std::string& error) {
  const Element ts = findElement(text, "TS", 0, text.size());
  if (!ts.found) {
    error = "no <TS> element";
    return false;
  }
  catalogue.language = attribute(ts.attributes, "language");
  const std::size_t first = text.find("<context>");
  catalogue.header = text.substr(0, first == std::string::npos ? text.find("</TS>") : first);
  std::size_t at = 0;
  while (true) {
    const Element context = findElement(text, "context", at, text.size());
    if (!context.found) break;
    at = context.end;
    Context parsed;
    parsed.name = decode(findElement(context.inner, "name", 0, context.inner.size()).inner);
    std::size_t m = 0;
    while (true) {
      const Element message = findElement(context.inner, "message", m, context.inner.size());
      if (!message.found) break;
      m = message.end;
      Message item;
      item.raw = context.inner.substr(message.begin, message.end - message.begin);
      item.numerus = attribute(message.attributes, "numerus") == "yes";
      const std::string& body = message.inner;
      const Element source = findElement(body, "source", 0, body.size());
      if (!source.found) {
        error = "a message of " + parsed.name + " without its source";
        return false;
      }
      item.source = decode(source.inner);
      item.rawSource = body.substr(source.begin, source.end - source.begin);
      const Element comment = findElement(body, "comment", 0, body.size());
      if (comment.found) {
        item.comment = decode(comment.inner);
        item.rawComment = body.substr(comment.begin, comment.end - comment.begin);
      }
      const Element translation = findElement(body, "translation", 0, body.size());
      if (!translation.found) {
        error = "a message of " + parsed.name + " without its translation: " + item.source;
        return false;
      }
      item.type = attribute(translation.attributes, "type");
      if (item.numerus) {
        std::size_t n = 0;
        while (true) {
          const Element form = findElement(translation.inner, "numerusform", n, translation.inner.size());
          if (!form.found) break;
          n = form.end;
          item.translations.push_back(decode(form.inner));
        }
      } else {
        item.translations.push_back(decode(translation.inner));
      }
      parsed.messages.push_back(std::move(item));
    }
    catalogue.contexts.push_back(std::move(parsed));
  }
  return true;
}

bool translated(const Message& message) {
  if (message.translations.empty()) return false;
  return std::all_of(message.translations.begin(), message.translations.end(),
                     [](const std::string& text) { return !text.empty(); });
}

// The placeholders of a text: %1 to %99, with or without L, and %n.
std::set<std::string> placeholders(const std::string& text) {
  std::set<std::string> found;
  for (std::size_t i = 0; i + 1 < text.size(); ++i) {
    if (text[i] != '%') continue;
    std::size_t j = i + 1;
    if (text[j] == 'L' && j + 1 < text.size()) ++j;
    if (text[j] == 'n') {
      found.insert("%n");
      continue;
    }
    std::size_t k = j;
    while (k < text.size() && k - j < 2 && text[k] >= '0' && text[k] <= '9') ++k;
    if (k > j && text[j] != '0') found.insert("%" + text.substr(j, k - j));
  }
  return found;
}

// Whether a text has a mnemonic: an & before a character that is not
// another & or a space (&& is a literal ampersand).
bool hasAccelerator(const std::string& text) {
  for (std::size_t i = 0; i + 1 < text.size(); ++i) {
    if (text[i] != '&') continue;
    if (text[i + 1] == '&') {
      ++i;
      continue;
    }
    if (text[i + 1] != ' ') return true;
  }
  return false;
}

struct Counts {
  int unfinished = 0;
  int finished = 0;
  int obsolete = 0;
  std::vector<std::string> problems;
};

Counts examine(const Catalogue& catalogue) {
  Counts counts;
  for (const Context& context : catalogue.contexts) {
    if (context.name == kAnyContext) {
      counts.problems.push_back("the build's own context " + std::string(kAnyContext) + " in the source file");
      continue;
    }
    for (const Message& message : context.messages) {
      if (message.type == "vanished" || message.type == "obsolete") {
        ++counts.obsolete;
        continue;
      }
      if (message.type == "unfinished" || !translated(message)) {
        ++counts.unfinished;
        continue;
      }
      ++counts.finished;
      std::set<std::string> wanted = placeholders(message.source);
      const std::string where = context.name + " \"" + message.source + "\"";
      for (const std::string& text : message.translations) {
        std::set<std::string> got = placeholders(text);
        if (message.numerus) {
          // A form may say the number in words ("one body"): %n need not
          // be in each.
          wanted.erase("%n");
          got.erase("%n");
        }
        if (got != wanted) {
          counts.problems.push_back(where + ": the placeholders of \"" + text + "\" differ from the source's");
        }
        if (hasAccelerator(message.source) != hasAccelerator(text)) {
          counts.problems.push_back(where + ": \"" + text + "\" " +
                                    (hasAccelerator(message.source) ? "has no" : "has an") +
                                    " accelerator (&), the source " +
                                    (hasAccelerator(message.source) ? "has one" : "has none"));
        }
      }
    }
  }
  return counts;
}

struct Listed {
  std::string code;
  int baseline = -1;
};

std::vector<Listed> readList(const std::string& text, std::string& error) {
  std::vector<Listed> listed;
  std::istringstream lines(text);
  std::string line;
  int number = 0;
  while (std::getline(lines, line)) {
    ++number;
    const std::size_t hash = line.find('#');
    if (hash != std::string::npos) line.erase(hash);
    std::istringstream words(line);
    Listed entry;
    if (!(words >> entry.code)) continue;
    std::string count;
    words >> count;
    if (count.empty() || count.find_first_not_of("0123456789") != std::string::npos) {
      error = "line " + std::to_string(number) + ": \"<code> <unfinished entries>\" expected";
      return {};
    }
    entry.baseline = std::stoi(count);
    listed.push_back(entry);
  }
  return listed;
}

// The language list with each listed language's baseline set to its count
// in `counts`; other lines, and a language line's trailing comment with the
// space before it ("de 120  # note"), are kept as they are.
std::string rewriteBaselines(const std::string& text, const std::map<std::string, int>& counts) {
  std::ostringstream out;
  std::istringstream lines(text);
  std::string line;
  while (std::getline(lines, line)) {
    const std::size_t hash = line.find('#');
    const std::string content = line.substr(0, hash);
    std::istringstream words(content);
    std::string code;
    if ((words >> code) && counts.count(code) != 0) {
      std::string tail;
      if (hash != std::string::npos) {
        const std::size_t last = content.find_last_not_of(" \t");
        tail = line.substr(last == std::string::npos ? hash : last + 1);
      }
      line = code + " " + std::to_string(counts.at(code)) + tail;
    }
    out << line << "\n";
  }
  return out.str();
}

// Checks every listed language; with `write`, sets each baseline to the
// count instead. Returns the number of problems.
int checkLanguages(const fs::path& list, bool write) {
  bool ok = false;
  const std::string text = readFile(list, ok);
  if (!ok) {
    std::cerr << list.string() << ": cannot read\n";
    return 1;
  }
  std::string error;
  const std::vector<Listed> listed = readList(text, error);
  if (!error.empty()) {
    std::cerr << list.string() << ": " << error << "\n";
    return 1;
  }
  const fs::path folder = list.has_parent_path() ? list.parent_path() : fs::path(".");
  int problems = 0;
  std::set<std::string> codes;
  std::map<std::string, int> counts;
  for (const Listed& entry : listed) {
    codes.insert(entry.code);
    const fs::path file = folder / ("mitcad_" + entry.code + ".ts");
    bool read = false;
    const std::string ts = readFile(file, read);
    Catalogue catalogue;
    if (!read || !parse(ts, catalogue, error)) {
      std::cerr << file.string() << ": " << (read ? error : std::string("missing")) << "\n";
      ++problems;
      continue;
    }
    const Counts found = examine(catalogue);
    counts[entry.code] = found.unfinished;
    for (const std::string& problem : found.problems) {
      std::cerr << file.string() << ": " << problem << "\n";
    }
    problems += static_cast<int>(found.problems.size());
    if (found.obsolete > 0) {
      std::cerr << file.string() << ": " << found.obsolete
                << " obsolete or vanished entries (refresh it with tools/translations-update.cmake, which drops them)\n";
      ++problems;
    }
    std::cout << entry.code << ": " << found.finished << " translated, " << found.unfinished << " unfinished (baseline "
              << entry.baseline << ")\n";
    if (!write && found.unfinished > entry.baseline) {
      std::cerr << file.string() << ": " << found.unfinished << " unfinished entries, more than the baseline "
                << entry.baseline << " of " << list.string() << "\n";
      ++problems;
    } else if (!write && found.unfinished < entry.baseline) {
      std::cerr << file.string() << ": " << found.unfinished << " unfinished entries: lower its baseline in "
                << list.string() << " from " << entry.baseline << " to " << found.unfinished << "\n";
      ++problems;
    }
  }
  std::error_code walkError;
  for (const fs::directory_entry& entry : fs::directory_iterator(folder, walkError)) {
    const std::string name = entry.path().filename().string();
    if (name.rfind("mitcad_", 0) == 0 && entry.path().extension() == ".ts") {
      const std::string code = name.substr(7, name.size() - 7 - 3);
      if (codes.count(code) == 0) {
        std::cerr << entry.path().string() << ": not in " << list.string() << " (add the line \"" << code
                  << " <unfinished entries>\")\n";
        ++problems;
      }
    }
  }
  if (write && problems == 0) {
    if (!writeFile(list, rewriteBaselines(text, counts))) {
      std::cerr << list.string() << ": cannot write\n";
      return 1;
    }
    std::cout << "baselines written to " << list.string() << "\n";
  }
  return problems;
}

// The catalogue the build compiles: the source with the context-free copy.
std::string prepared(const std::string& text, const Catalogue& catalogue) {
  std::set<std::pair<std::string, std::string>> seen;
  std::string any = "<context>\n    <name>" + std::string(kAnyContext) + "</name>\n";
  for (const Context& context : catalogue.contexts) {
    // The language's own name ("English" in Finnish is Suomi) is not a word
    // of the interface.
    if (context.name == "LanguageName") continue;
    for (const Message& message : context.messages) {
      // Finished translations only, as lrelease -nounfinished compiles.
      if (!message.type.empty() || !translated(message)) continue;
      if (!seen.insert({message.source, message.comment}).second) continue;
      any += "    " + message.raw + "\n";
    }
  }
  any += "</context>\n";
  const std::size_t end = text.rfind("</TS>");
  return text.substr(0, end) + any + text.substr(end);
}

// One code point of UTF-8 at `i`, its length.
std::size_t codePointLength(const std::string& text, std::size_t i) {
  const auto byte = static_cast<unsigned char>(text[i]);
  std::size_t length = 1;
  if (byte >= 0xf0) length = 4;
  else if (byte >= 0xe0) length = 3;
  else if (byte >= 0xc0) length = 2;
  return std::min(length, text.size() - i);
}

std::string accented(char c) {
  static const std::map<char, const char*> letters = {
      {'a', "\xc3\xa1"}, {'c', "\xc3\xa7"}, {'e', "\xc3\xa9"}, {'i', "\xc3\xad"}, {'n', "\xc3\xb1"},
      {'o', "\xc3\xb3"}, {'s', "\xc5\xa1"}, {'u', "\xc3\xba"}, {'y', "\xc3\xbd"}, {'z', "\xc5\xbe"},
      {'A', "\xc3\x85"}, {'C', "\xc3\x87"}, {'E', "\xc3\x89"}, {'I', "\xc3\x8d"}, {'N', "\xc3\x91"},
      {'O', "\xc3\x93"}, {'S', "\xc5\xa0"}, {'U', "\xc3\x9a"}, {'Y', "\xc3\x9d"}, {'Z', "\xc5\xbd"}};
  const auto found = letters.find(c);
  return found != letters.end() ? std::string(found->second) : std::string(1, c);
}

// The pseudo-language's text of a source.
std::string pseudo(const std::string& source) {
  if (source.empty()) return source;
  // Leading and trailing white space, a trailing ellipsis, colon or full
  // stop stay outside the brackets (code that tests for them keeps working).
  std::size_t begin = 0;
  while (begin < source.size() && (source[begin] == ' ' || source[begin] == '\n')) ++begin;
  std::size_t end = source.size();
  while (end > begin) {
    if (source[end - 1] == ' ' || source[end - 1] == '\n' || source[end - 1] == ':' || source[end - 1] == '.') {
      --end;
    } else if (end - begin >= 3 && source.compare(end - 3, 3, "\xe2\x80\xa6") == 0) {
      end -= 3;
    } else {
      break;
    }
  }
  if (end == begin) return source;
  std::string out = source.substr(0, begin) + "[";
  std::size_t letters = 0;
  for (std::size_t i = begin; i < end;) {
    const char c = source[i];
    if (c == '%' && i + 1 < end) {
      // A placeholder as it is: %1, %12, %L1, %n, %Ln.
      std::size_t j = i + 1;
      if (source[j] == 'L' && j + 1 < end) ++j;
      if (source[j] == 'n') {
        out += source.substr(i, j + 1 - i);
        i = j + 1;
        continue;
      }
      std::size_t k = j;
      while (k < end && k - j < 2 && source[k] >= '0' && source[k] <= '9') ++k;
      if (k > j) {
        out += source.substr(i, k - i);
        i = k;
        continue;
      }
    }
    if (c == '<') {
      // Markup as it is.
      const std::size_t close = source.find('>', i);
      if (close != std::string::npos && close < end) {
        out += source.substr(i, close + 1 - i);
        i = close + 1;
        continue;
      }
    }
    if (c == '&' && i + 1 < end) {
      // An accelerator stays one, on the accented letter; && stays &&.
      const std::size_t semicolon = source.find(';', i);
      if (source[i + 1] == '&') {
        out += "&&";
        i += 2;
        continue;
      }
      if (semicolon != std::string::npos && semicolon < end && semicolon - i <= 8 &&
          source.find(' ', i) > semicolon) {
        // An entity of rich text (&nbsp;).
        out += source.substr(i, semicolon + 1 - i);
        i = semicolon + 1;
        continue;
      }
      out += '&';
      ++i;
      continue;
    }
    const std::size_t length = codePointLength(source, i);
    if (length == 1) {
      out += accented(c);
      if ((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z')) ++letters;
    } else {
      out += source.substr(i, length);
    }
    i += length;
  }
  // A third longer than the letters, at least two: room for languages
  // whose words are longer than English.
  out += ' ';
  for (std::size_t n = 0; n < std::max<std::size_t>(2, letters / 3); ++n) out += "\xc2\xb7";
  out += "]";
  out += source.substr(end);
  return out;
}

std::string pseudoCatalogue(const Catalogue& catalogue) {
  std::string out =
      "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE TS>\n<TS version=\"2.1\" language=\"en\" "
      "sourcelanguage=\"en\">\n";
  for (const Context& context : catalogue.contexts) {
    if (context.name == kAnyContext) continue;
    out += "<context>\n    <name>" + encode(context.name) + "</name>\n";
    for (const Message& message : context.messages) {
      if (message.type == "vanished" || message.type == "obsolete") continue;
      out += message.numerus ? "    <message numerus=\"yes\">\n" : "    <message>\n";
      out += "        " + message.rawSource + "\n";
      if (!message.rawComment.empty()) out += "        " + message.rawComment + "\n";
      const std::string text = encode(pseudo(message.source));
      if (message.numerus) {
        out += "        <translation>\n            <numerusform>" + text + "</numerusform>\n            <numerusform>" +
               text + "</numerusform>\n        </translation>\n";
      } else {
        out += "        <translation>" + text + "</translation>\n";
      }
      out += "    </message>\n";
    }
    out += "</context>\n";
  }
  out += "</TS>\n";
  return out;
}

int transform(const fs::path& in, const fs::path& out, bool pseudoLanguage) {
  bool ok = false;
  const std::string text = readFile(in, ok);
  Catalogue catalogue;
  std::string error;
  if (!ok || !parse(text, catalogue, error)) {
    std::cerr << in.string() << ": " << (ok ? error : std::string("cannot read")) << "\n";
    return 1;
  }
  const std::string result = pseudoLanguage ? pseudoCatalogue(catalogue) : prepared(text, catalogue);
  if (!writeFile(out, result)) {
    std::cerr << out.string() << ": cannot write\n";
    return 1;
  }
  return 0;
}

int selfTest() {
  int failures = 0;
  const auto expect = [&failures](bool condition, const std::string& what) {
    if (!condition) {
      std::cerr << "self-test: " << what << "\n";
      ++failures;
    }
  };
  const std::string ts =
      "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE TS>\n<TS version=\"2.1\" language=\"fi\" "
      "sourcelanguage=\"en\">\n<context>\n    <name>QObject</name>\n"
      "    <message>\n        <source>Sweep</source>\n        <translation>Pyyhk&#xe4;isy</translation>\n    "
      "</message>\n"
      "    <message>\n        <source>&amp;Open %1</source>\n        <translation>Avaa %1</translation>\n    "
      "</message>\n"
      "    <message>\n        <source>%1 of %2</source>\n        <translation>%1/%3</translation>\n    </message>\n"
      "    <message>\n        <source>Close</source>\n        <comment>a window</comment>\n        <translation "
      "type=\"unfinished\"></translation>\n    </message>\n"
      "    <message numerus=\"yes\">\n        <source>%n body(s)</source>\n        <translation>\n            "
      "<numerusform>yksi kappale</numerusform>\n            <numerusform>%n kappaletta</numerusform>\n        "
      "</translation>\n    </message>\n"
      "    <message>\n        <source>Old</source>\n        <translation type=\"vanished\">Vanha</translation>\n    "
      "</message>\n"
      "</context>\n<context>\n    <name>View</name>\n"
      "    <message>\n        <source>Sweep</source>\n        <translation>Toinen</translation>\n    </message>\n"
      "    <message>\n        <source>Tab<byte value=\"x9\"/>&apos;x&apos; &lt;b&gt;</source>\n        "
      "<translation type=\"unfinished\">Sarkain</translation>\n    </message>\n"
      "</context>\n</TS>\n";
  Catalogue catalogue;
  std::string error;
  expect(parse(ts, catalogue, error), "parse: " + error);
  expect(catalogue.contexts.size() == 2 && catalogue.language == "fi", "two contexts, language fi");
  if (catalogue.contexts.size() == 2) {
    const auto& first = catalogue.contexts[0].messages;
    expect(first.size() == 6, "six messages in QObject");
    if (first.size() == 6) {
      expect(first[0].translations.at(0) == "Pyyhk\xc3\xa4isy", "a character reference decoded");
      expect(first[1].source == "&Open %1", "an entity decoded");
      expect(first[3].comment == "a window" && first[3].type == "unfinished", "a disambiguation, unfinished");
      expect(first[4].numerus && first[4].translations.size() == 2, "two numerus forms");
    }
    const auto& second = catalogue.contexts[1].messages;
    expect(second.size() == 2 && second[1].source == "Tab\t'x' <b>", "lupdate's byte element decoded");
  }
  const Counts counts = examine(catalogue);
  expect(counts.unfinished == 2, "two unfinished, not " + std::to_string(counts.unfinished));
  expect(counts.finished == 5, "five finished, not " + std::to_string(counts.finished));
  expect(counts.obsolete == 1, "one vanished");
  expect(counts.problems.size() == 2, "two problems (a placeholder, an accelerator), not " +
                                          std::to_string(counts.problems.size()));
  expect(placeholders("%1 and %L2, %n, 100%") == std::set<std::string>{"%1", "%2", "%n"}, "placeholders");
  expect(hasAccelerator("&File") && !hasAccelerator("A && B") && !hasAccelerator("A & B"), "accelerators");
  const std::string prepared_ = prepared(ts, catalogue);
  expect(prepared_.find(std::string("<name>") + kAnyContext + "</name>") != std::string::npos, "the context-free copy");
  Catalogue again;
  expect(parse(prepared_, again, error) && again.contexts.size() == 3, "the prepared catalogue parses");
  if (again.contexts.size() == 3) {
    // Sweep once (the first context's translation), &Open, %1 of %2, the
    // numerus message; not the unfinished nor the vanished ones.
    expect(again.contexts[2].messages.size() == 4, "four context-free messages, not " +
                                                       std::to_string(again.contexts[2].messages.size()));
  }
  const std::string shown = pseudo("&Save %1 as\xe2\x80\xa6");
  expect(shown.rfind("[&\xc5\xa0\xc3\xa1v\xc3\xa9 %1 \xc3\xa1\xc5\xa1 ", 0) == 0, "pseudo text: " + shown);
  expect(shown.size() >= 3 && shown.compare(shown.size() - 3, 3, "\xe2\x80\xa6") == 0, "the ellipsis kept outside");
  expect(placeholders(pseudo("%1: %L2 of %n")) == placeholders("%1: %L2 of %n"), "pseudo placeholders kept");
  expect(hasAccelerator(pseudo("E&xit")) && !hasAccelerator(pseudo("A && B")), "pseudo accelerators kept");
  expect(pseudo(" min") == " [m\xc3\xad\xc3\xb1 \xc2\xb7\xc2\xb7]", "pseudo leading space kept: " + pseudo(" min"));
  expect(pseudo("<b>Bold</b>").find("<b>") != std::string::npos, "pseudo markup kept");
  Catalogue pseudoParsed;
  expect(parse(pseudoCatalogue(catalogue), pseudoParsed, error), "the pseudo-language parses");
  const Counts pseudoCounts = examine(pseudoParsed);
  expect(pseudoCounts.unfinished == 0 && pseudoCounts.problems.empty() && pseudoCounts.obsolete == 0,
         "the pseudo-language is complete and keeps placeholders and accelerators");
  expect(decode(encode("a<b>&'\"\t")) == "a<b>&'\"\t", "encode and decode");
  std::string listError;
  const std::vector<Listed> listed = readList("# comment\nfi 12\n\nxx 0 # a test\n", listError);
  expect(listError.empty() && listed.size() == 2 && listed[0].code == "fi" && listed[0].baseline == 12,
         "the language list");
  readList("fi many\n", listError);
  expect(!listError.empty(), "a bad baseline is an error");
  const std::string rewritten = rewriteBaselines("# comment\nfi 3 # note\nde 120  # two spaces\nxx 0\n\n",
                                                 {{"fi", 7}, {"de", 118}});
  expect(rewritten == "# comment\nfi 7 # note\nde 118  # two spaces\nxx 0\n\n",
         "baselines rewritten with their comments kept: " + rewritten);
  if (failures == 0) std::cout << "translations self-test passed\n";
  return failures == 0 ? 0 : 1;
}

void usage() {
  std::cerr << "usage: mitcad-translations check|baseline LANGUAGES.TXT\n"
               "       mitcad-translations prepare|pseudo IN.ts OUT.ts\n"
               "       mitcad-translations --self-test\n";
}

} // namespace

int main(int argc, char* argv[]) {
  const std::vector<std::string> args(argv + 1, argv + argc);
  if (args.size() == 1 && args[0] == "--self-test") return selfTest();
  if (args.size() == 2 && (args[0] == "check" || args[0] == "baseline")) {
    const int problems = checkLanguages(args[1], args[0] == "baseline");
    if (problems != 0) std::cerr << problems << " problem(s)\n";
    return problems == 0 ? 0 : 1;
  }
  if (args.size() == 3 && (args[0] == "prepare" || args[0] == "pseudo")) {
    return transform(args[1], args[2], args[0] == "pseudo");
  }
  usage();
  return 2;
}
