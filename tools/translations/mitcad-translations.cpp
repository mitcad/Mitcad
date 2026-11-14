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
//   mitcad-translations messages SOURCE-ROOT [--write]
//       the ctest repo.model-messages (mitcad#1284): the model's message
//       templates of core/'s Rust sources, with their cleaned forms as the
//       application shows them (mitcad#1635), against the generated list
//       app/framework/ModelMessageTemplates.cpp, or (--write) the list
//       written again (tools/translations-update.cmake, before lupdate reads
//       it into the model_<code>.ts files)
//   mitcad-translations --self-test
//       checks the parser, the checks, the pseudo-language and the scan of
//       the model's messages on planted catalogues and sources
//
// A language's model_<code>.ts (the model's messages, mitcad#1284) is
// checked with its mitcad_<code>.ts and counted in its baseline.
//
// English, the source language, has mitcad_en.ts too (mitcad#1756): only
// the plural messages (%n), with English's two forms, so that the English
// interface says "1 degree of freedom" and "17 degrees of freedom" where
// the source says "degree(s)". check finds a message there that is not a
// plural one, or a form that keeps a bracketed plural.
//
// The .ts files are lupdate's XML; this reads them with a small scanner of
// its own (the elements context, name, message, source, comment,
// translation and numerusform, the five named entities, numeric character
// references and lupdate's <byte value="..."/>), plain C++17.

#include <algorithm>
#include <cstdio>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <iterator>
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

// Whether a text writes a plural as a word with its ending in brackets:
// "body(ies)", "face(s)", "crash(es)".
bool hasBracketedPlural(const std::string& text) {
  for (const char* ending : {"(s)", "(es)", "(ies)"}) {
    for (std::size_t at = text.find(ending); at != std::string::npos; at = text.find(ending, at + 1)) {
      const char before = at > 0 ? text[at - 1] : ' ';
      if ((before >= 'a' && before <= 'z') || (before >= 'A' && before <= 'Z')) return true;
    }
  }
  return false;
}

// The English catalogue (mitcad#1756, mitcad_en.ts: the source language's
// plural forms, lupdate -pluralonly): plural messages only, each form a
// real English one, never the source's "(s)".
std::vector<std::string> pluralCatalogueProblems(const Catalogue& catalogue) {
  std::vector<std::string> problems;
  for (const Context& context : catalogue.contexts) {
    for (const Message& message : context.messages) {
      const std::string where = context.name + " \"" + message.source + "\"";
      if (!message.numerus) {
        problems.push_back(where + ": not a plural message (the English catalogue has plural forms only)");
        continue;
      }
      for (const std::string& text : message.translations) {
        if (hasBracketedPlural(text)) {
          problems.push_back(where + ": the form \"" + text + "\" keeps a bracketed plural");
        }
      }
    }
  }
  return problems;
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
    Counts found = examine(catalogue);
    if (entry.code == "en") {
      const std::vector<std::string> plural = pluralCatalogueProblems(catalogue);
      found.problems.insert(found.problems.end(), plural.begin(), plural.end());
    }
    for (const std::string& problem : found.problems) {
      std::cerr << file.string() << ": " << problem << "\n";
    }
    problems += static_cast<int>(found.problems.size());
    if (found.obsolete > 0) {
      std::cerr << file.string() << ": " << found.obsolete
                << " obsolete or vanished entries (refresh it with tools/translations-update.cmake, which drops them)\n";
      ++problems;
    }
    // The model's messages of the language (mitcad#1284), counted with it.
    const fs::path model = folder / ("model_" + entry.code + ".ts");
    if (fs::exists(model)) {
      const std::string modelText = readFile(model, read);
      Catalogue modelCatalogue;
      if (!read || !parse(modelText, modelCatalogue, error)) {
        std::cerr << model.string() << ": " << (read ? error : std::string("cannot read")) << "\n";
        ++problems;
      } else {
        const Counts modelFound = examine(modelCatalogue);
        for (const std::string& problem : modelFound.problems) {
          std::cerr << model.string() << ": " << problem << "\n";
        }
        problems += static_cast<int>(modelFound.problems.size());
        if (modelFound.obsolete > 0) {
          std::cerr << model.string() << ": " << modelFound.obsolete
                    << " obsolete or vanished entries (refresh it with tools/translations-update.cmake)\n";
          ++problems;
        }
        found.unfinished += modelFound.unfinished;
        found.finished += modelFound.finished;
      }
    }
    counts[entry.code] = found.unfinished;
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
    const bool model = name.rfind("model_", 0) == 0;
    if ((name.rfind("mitcad_", 0) == 0 || model) && entry.path().extension() == ".ts") {
      const std::size_t prefix = model ? 6 : 7;
      const std::string code = name.substr(prefix, name.size() - prefix - 3);
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

// The model's messages (mitcad#1284, docs/translations.md "The model's
// messages"): the message texts of core/'s Rust sources as templates, their
// values as %1, %2, ... The application matches a message to its template
// and translates it (app/framework/ModelMessages.cpp); the templates are
// app/framework/ModelMessageTemplates.cpp, which lupdate reads into the
// model_<code>.ts files.

bool asciiLetter(char c) { return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z'); }
bool asciiDigit(char c) { return c >= '0' && c <= '9'; }
bool identChar(char c) { return asciiLetter(c) || asciiDigit(c) || c == '_'; }

// A format argument of Rust's: {}, {name}, {0}, {:.2}, {name:>8}.
bool placeholderSpec(const std::string& inner) {
  std::size_t j = 0;
  while (j < inner.size() && identChar(inner[j])) ++j;
  if (j == inner.size()) return true;
  if (inner[j] != ':') return false;
  return inner.find_first_of("{}", j) == std::string::npos;
}

// The template of a literal's text, or "" when it is no message: a text
// with two words of letters side by side, not markup, not a value list.
// With `names`, the name of each placeholder's argument ("{face}": "face";
// "" for {}, {0} or one with a format spec).
std::string messageTemplate(const std::string& text, std::vector<std::string>* names = nullptr) {
  std::string out;
  std::string fixed; // the text with a space for each placeholder
  std::vector<std::string> arguments;
  int count = 0;
  for (std::size_t i = 0; i < text.size(); ++i) {
    const char c = text[i];
    if (c == '{') {
      if (i + 1 < text.size() && text[i + 1] == '{') {
        out += '{';
        fixed += '{';
        ++i;
        continue;
      }
      const std::size_t close = text.find('}', i + 1);
      if (close != std::string::npos && placeholderSpec(text.substr(i + 1, close - i - 1))) {
        ++count;
        out += "%" + std::to_string(count);
        fixed += ' ';
        const std::string inner = text.substr(i + 1, close - i - 1);
        arguments.push_back(std::all_of(inner.begin(), inner.end(), identChar) ? inner : std::string());
        i = close;
        // "%1" followed by a digit would read as %10.
        if (i + 1 < text.size() && asciiDigit(text[i + 1])) return std::string();
        continue;
      }
      out += c;
      fixed += c;
      continue;
    }
    if (c == '}' && i + 1 < text.size() && text[i + 1] == '}') {
      out += '}';
      fixed += '}';
      ++i;
      continue;
    }
    out += c;
    fixed += c;
  }
  if (count > 99) return std::string();
  for (const char c : out) {
    const auto byte = static_cast<unsigned char>(c);
    if (byte >= 1 && byte <= 31 && c != '\n') return std::string();
  }
  for (std::size_t i = 0; i + 1 < fixed.size(); ++i) {
    if (fixed[i] == '%' && asciiDigit(fixed[i + 1])) return std::string();
  }
  bool words = false;
  for (std::size_t p = 2; p + 2 < fixed.size() && !words; ++p) {
    words = fixed[p] == ' ' && asciiLetter(fixed[p - 1]) && asciiLetter(fixed[p - 2]) && asciiLetter(fixed[p + 1]) &&
            asciiLetter(fixed[p + 2]);
  }
  if (!words) return std::string();
  const std::size_t first = fixed.find_first_not_of(" \t");
  if (first != std::string::npos && fixed[first] == '<') return std::string();
  if (fixed.find("=\"") != std::string::npos) return std::string();
  if (names != nullptr) *names = arguments;
  return out;
}

// A line without its leading blanks and its trailing blanks and carriage
// return.
std::string trimmedLine(const std::string& line) {
  std::size_t begin = 0;
  while (begin < line.size() && (line[begin] == ' ' || line[begin] == '\t')) ++begin;
  std::size_t end = line.size();
  if (end > begin && line[end - 1] == '\r') --end;
  while (end > begin && (line[end - 1] == ' ' || line[end - 1] == '\t')) --end;
  return line.substr(begin, end - begin);
}

// Whether `rest` starts with a module's head, `mod tests {` (pub, pub(crate)).
bool moduleStart(const std::string& rest) {
  std::size_t i = 0;
  const auto blanks = [&rest](std::size_t at) {
    while (at < rest.size() && (rest[at] == ' ' || rest[at] == '\t')) ++at;
    return at;
  };
  i = blanks(i);
  const auto tryMod = [&rest, &blanks](std::size_t at) {
    if (rest.compare(at, 3, "mod") != 0) return false;
    std::size_t j = at + 3;
    const std::size_t afterMod = blanks(j);
    if (afterMod == j) return false;
    j = afterMod;
    const std::size_t name = j;
    while (j < rest.size() && identChar(rest[j])) ++j;
    if (j == name) return false;
    j = blanks(j);
    return j < rest.size() && rest[j] == '{';
  };
  if (rest.compare(i, 3, "pub") == 0) {
    std::size_t j = i + 3;
    if (j < rest.size() && rest[j] == '(') {
      std::size_t k = j + 1;
      while (k < rest.size() && rest[k] >= 'a' && rest[k] <= 'z') ++k;
      if (k > j + 1 && k < rest.size() && rest[k] == ')') j = k + 1;
    }
    const std::size_t after = blanks(j);
    if (after > j && tryMod(after)) return true;
  }
  return tryMod(i);
}

unsigned long hexValue(const std::string& digits) {
  unsigned long value = 0;
  for (const char c : digits) {
    int digit = 0;
    if (c >= '0' && c <= '9') digit = c - '0';
    else if (c >= 'a' && c <= 'f') digit = c - 'a' + 10;
    else if (c >= 'A' && c <= 'F') digit = c - 'A' + 10;
    value = value * 16 + static_cast<unsigned long>(digit);
  }
  return value;
}

std::vector<std::string> kindedTemplates(const std::string& text, const std::vector<std::string>& names);

// The message templates of one Rust source: its string literals outside
// comments, test modules and the arguments of panics, assertions,
// expectations and logs, as messageTemplate takes them, and each one's forms
// with the names of kinds put in (kindedTemplates, mitcad#1638, #1650).
void scanRust(const std::string& s, std::set<std::string>& found) {
  static const std::set<std::string> excluded = {
      "panic!", "unreachable!", "unimplemented!", "todo!", "assert!", "assert_eq!", "assert_ne!",
      "debug_assert!", "debug_assert_eq!", "debug_assert_ne!", "expect", "expect_err", "debug!", "trace!",
      "info!", "warn!", "error!", "eprintln!", "println!", "eprint!", "print!", "log!", "dbg!"};
  const std::size_t n = s.size();
  const auto at = [&s, n](std::size_t i) { return i < n ? s[i] : '\0'; };
  const auto before = [&s](std::size_t i, std::size_t back) { return i >= back ? s[i - back] : '\0'; };
  std::vector<std::string> stack;
  bool skipping = false;
  long braces = 0;
  long skipDepth = 0;
  int pendingTest = 0;
  std::size_t lineStart = 0;
  const auto emit = [&](const std::string& text) {
    if (skipping) return;
    for (const std::string& name : stack) {
      if (excluded.count(name) != 0) return;
    }
    std::vector<std::string> names;
    const std::string t = messageTemplate(text, &names);
    if (t.empty()) return;
    found.insert(t);
    for (const std::string& kinded : kindedTemplates(t, names)) found.insert(kinded);
  };
  std::size_t i = 0;
  while (i < n) {
    char c = s[i];
    if (c == '\n') {
      const std::string line = trimmedLine(s.substr(lineStart, i - lineStart));
      if (line == "#[cfg(test)]") {
        pendingTest = 2;
      } else if (pendingTest == 2 &&
                 (line.empty() || (line.size() >= 3 && line.compare(0, 2, "#[") == 0 && line.back() == ']'))) {
        pendingTest = 2;
      } else if (pendingTest == 2) {
        pendingTest = 0;
      }
      ++i;
      lineStart = i;
      if (pendingTest == 2 && moduleStart(s.substr(i, 200)) && !skipping) {
        skipping = true;
        skipDepth = braces;
        pendingTest = 0;
      }
      continue;
    }
    if (c == '/' && at(i + 1) == '/') {
      while (i < n && s[i] != '\n') ++i;
      continue;
    }
    if (c == '/' && at(i + 1) == '*') {
      int nest = 1;
      i += 2;
      while (i < n && nest > 0) {
        if (s.compare(i, 2, "/*") == 0) {
          ++nest;
          i += 2;
        } else if (s.compare(i, 2, "*/") == 0) {
          --nest;
          i += 2;
        } else {
          ++i;
        }
      }
      continue;
    }
    if (c == '\'') {
      // A character literal ('"', '\'', '°') or a lifetime ('a).
      const char next = at(i + 1);
      if (next == '\\') {
        std::size_t j = i + 3;
        while (j < n && s[j] != '\'') ++j;
        i = j + 1;
        continue;
      }
      const auto lead = static_cast<unsigned char>(next);
      const std::size_t length = lead < 128 ? 1 : (lead < 224 ? 2 : (lead < 240 ? 3 : 4));
      if (at(i + 1 + length) == '\'') {
        i += 2 + length;
        continue;
      }
      ++i;
      continue;
    }
    const bool bytePrefix = before(i, 1) == 'b' && !identChar(before(i, 2));
    if (c == 'r' && (!identChar(before(i, 1)) || bytePrefix) && (at(i + 1) == '"' || at(i + 1) == '#')) {
      std::size_t j = i + 1;
      std::size_t hashes = 0;
      while (at(j) == '#') {
        ++hashes;
        ++j;
      }
      if (at(j) == '"') {
        const std::string close = "\"" + std::string(hashes, '#');
        const std::size_t end = s.find(close, j + 1);
        if (end == std::string::npos) break;
        if (!bytePrefix) emit(s.substr(j + 1, end - j - 1));
        i = end + close.size();
        continue;
      }
    }
    if (c == '"') {
      const bool bytes = bytePrefix;
      std::string text;
      ++i;
      while (i < n) {
        c = s[i];
        if (c == '"') {
          ++i;
          break;
        }
        if (c == '\\') {
          const char e = at(i + 1);
          if (e == 'n') {
            text += '\n';
            i += 2;
          } else if (e == 't') {
            text += '\t';
            i += 2;
          } else if (e == 'r') {
            text += '\r';
            i += 2;
          } else if (e == '0') {
            text += '\x01'; // a NUL: no message
            i += 2;
          } else if (e == '\\' || e == '"' || e == '\'') {
            text += e;
            i += 2;
          } else if (e == 'x') {
            text += static_cast<char>(hexValue(i + 2 <= n ? s.substr(i + 2, 2) : std::string()));
            i += 4;
          } else if (e == 'u') {
            const std::size_t close = s.find('}', i);
            if (close == std::string::npos) {
              i = n;
              break;
            }
            appendUtf8(text, hexValue(s.substr(i + 3, close - i - 3)));
            i = close + 1;
          } else if (e == '\n' || e == '\r') {
            // A line continued: its break and the next line's indent go.
            ++i;
            while (i < n && (s[i] == ' ' || s[i] == '\t' || s[i] == '\r' || s[i] == '\n')) ++i;
          } else {
            text += c;
            ++i;
          }
          continue;
        }
        text += c;
        ++i;
      }
      if (!bytes) emit(text);
      continue;
    }
    if (c == '(') {
      std::string name;
      std::size_t j = i;
      if (before(j, 1) == '!') {
        name = "!";
        --j;
      }
      while (j > 0 && identChar(s[j - 1])) {
        name.insert(name.begin(), s[j - 1]);
        --j;
      }
      if (name == "!") name.clear();
      stack.push_back(name);
      ++i;
      continue;
    }
    if (c == ')') {
      if (!stack.empty()) stack.pop_back();
      ++i;
      continue;
    }
    if (c == '{') {
      ++braces;
      ++i;
      continue;
    }
    if (c == '}') {
      --braces;
      ++i;
      if (skipping && braces == skipDepth) skipping = false;
      continue;
    }
    ++i;
  }
}

// Whether a Rust source below the root (`core/...`, with /) is scanned: not
// tests, fuzzing, benchmarks, tools, nor the texts agents and the command
// line read (mcp, chat, cli).
bool scannedSource(const std::string& relative) {
  static const std::set<std::string> folders = {"model-tests", "model-testkit", "fuzz", "mcp", "tests",
                                                "benches", "examples", "bin", "ffi-staticlib", "target"};
  if (relative.size() < 3 || relative.compare(relative.size() - 3, 3, ".rs") != 0) return false;
  std::size_t from = 0;
  while (true) {
    const std::size_t slash = relative.find('/', from);
    if (slash == std::string::npos) break;
    if (folders.count(relative.substr(from, slash - from)) != 0) return false;
    from = slash + 1;
  }
  const std::string name = relative.substr(from);
  if (name.compare(0, 4, "test") == 0 || name == "mcp.rs" || name == "chat.rs" || name == "cli.rs") return false;
  for (const char* suffix : {"_tests.rs", "_test.rs", "_corpus.rs"}) {
    const std::size_t length = std::strlen(suffix);
    if (name.size() >= length && name.compare(name.size() - length, length, suffix) == 0) return false;
  }
  return true;
}

std::set<std::string> scanCore(const fs::path& root, int& files) {
  std::set<std::string> found;
  files = 0;
  std::vector<std::string> sources;
  std::error_code error;
  for (fs::recursive_directory_iterator it(root / "core", error), end; !error && it != end; it.increment(error)) {
    if (!it->is_regular_file(error)) continue;
    const std::string relative = it->path().lexically_relative(root).generic_string();
    if (scannedSource(relative)) sources.push_back(relative);
  }
  std::sort(sources.begin(), sources.end());
  for (const std::string& relative : sources) {
    bool ok = false;
    const std::string text = readFile(root / relative, ok);
    if (!ok) continue;
    ++files;
    scanRust(text, found);
  }
  return found;
}

// The cleaned forms of the templates (mitcad#1635). A message that names
// topology, a body's uid or an internal id is shown cleaned of them
// (`plain_message` and `FeatureError::from_text`, core/model-base/src/
// notice.rs, mitcad#1327 and #1612): "line c4 does not exist" reads "a line
// does not exist", "body F2.b0 (from Extrude1) belongs to …" "a body (from
// Extrude1) belongs to …". So a template whose value comes after the word
// of a kind ("line c%1", "body %1", a list "faces %1 and %2"), or that has
// an id ("c%1 and c%2 are tangent"), has its cleaned form beside it, made
// as that cleaner makes it: after the word of a kind the value taken out,
// the word kept, "a" or "an" before a single one unless its noun phrase has
// a word like "the", "its" or "the result's" (withArticle, mitcad#1638); an
// id elsewhere its kind ("a curve", "curves"); a value followed by " mm" is
// a length and stays. Then the cleaner's tidying and the values numbered
// again. The words are notice.rs's; its test `cleaned_templates_are_listed`
// checks the list against the cleaner itself. A value whose argument names
// a kind of topology ("at {vertex}") and is not after the word of a kind is
// listed with that kind put in ("at a vertex", kindedTemplate, mitcad#1638);
// a list of them ("into {region_names}") with the kind's word for several, a name
// of any kind ("of {topo}") with each kind (kindedTemplates, mitcad#1650).

// The words after which the cleaner keeps the word and takes the name out
// (notice.rs, KINDS), and those after which it puts no article (`cleaned`,
// DETERMINERS).
const char* const kCleanedKinds[] = {"face",     "faces",       "edge",        "edges",     "vertex",
                                     "vertices", "corner",      "corners",     "region",    "regions",
                                     "profile",  "profiles",    "body",        "bodies",    "point",
                                     "points",   "curve",       "curves",      "line",      "lines",
                                     "arc",      "arcs",        "circle",      "circles",   "constraint",
                                     "constraints", "dimension", "dimensions", "feature",   "features",
                                     "pattern",  "offset"};
const char* const kCleanedDeterminers[] = {"the",  "a",    "an",    "its", "their",   "this", "that",
                                           "each", "every", "one", "another", "which", "no"};
// What joins a list of names into one ("faces %1 and %2").
const char* const kCleanedJoins[] = {", ", " and ", ", and ", " or ", ", or ", " + "};
// The words that end a noun phrase, looking back from the word of its kind
// (notice.rs, PHRASE_ENDS), and those of them after which a verb comes
// (VERB_LEADS): "does not fit face %1" reads "does not fit a face".
const char* const kPhraseEnds[] = {
    "at",     "on",      "of",     "in",      "into",   "onto",    "to",      "from",    "for",    "with",
    "without", "by",     "behind", "between", "through", "along",  "across",  "over",    "under",  "above",
    "below",  "beyond",  "near",   "after",   "before", "than",    "as",      "about",   "around", "against",
    "past",   "via",     "within", "inside",  "outside", "per",    "and",     "or",      "but",    "nor",
    "if",     "when",    "where",  "while",   "because", "then",   "is",      "are",     "was",    "were",
    "be",     "been",    "being",  "has",     "have",   "had",     "do",      "does",    "did",    "not",
    "can",    "cannot",  "could",  "must",    "may",    "might",   "will",    "would",   "shall",  "should",
    "it",     "they",    "there",  "here",    "what",   "who",     "create",  "delete",  "add",    "remove",
    "make",   "made",    "move",   "moved",   "change", "cut",     "fit",     "own",     "reach",  "develop",
    "use",    "keep",    "pick",   "select",  "join",   "split",   "need",    "find",    "found",  "take",
    "took",   "give",    "gave",   "get",     "got",    "put",     "meet",    "cross",   "follow", "show",
    "hide"};
const char* const kVerbLeads[] = {"to",  "do",  "does",  "did",   "not",   "can",   "cannot", "could",
                                  "must", "may", "might", "will", "would", "shall", "should"};

bool listedWord(const std::string& word, const char* const* words, std::size_t count) {
  for (std::size_t i = 0; i < count; ++i) {
    if (word == words[i]) return true;
  }
  return false;
}

std::string lowerAscii(std::string text) {
  for (char& c : text) {
    if (c >= 'A' && c <= 'Z') c = static_cast<char>(c - 'A' + 'a');
  }
  return text;
}

// The length of the template's placeholder at `i` (%1 to %99), or 0.
std::size_t templatePlaceholderAt(const std::string& text, std::size_t i) {
  if (text[i] != '%' || i + 1 >= text.size() || !asciiDigit(text[i + 1]) || text[i + 1] == '0') return 0;
  return i + 2 < text.size() && asciiDigit(text[i + 2]) ? 3 : 2;
}

// Where the run of letters that ends at `end` starts (`end` when none).
std::size_t lettersStart(const std::string& text, std::size_t end) {
  std::size_t start = end;
  while (start > 0 && asciiLetter(text[start - 1])) --start;
  return start;
}

// A value of a template: its placeholder, and where the name it may be
// starts (before an id's letter: "c%1" is a sketch's curve c4).
struct TemplateValue {
  std::size_t name = 0;
  std::size_t placeholder = 0;
  std::size_t end = 0;
  char prefix = 0; // the id's letter, or 0
};

std::vector<TemplateValue> templateValues(const std::string& text) {
  std::vector<TemplateValue> values;
  for (std::size_t i = 0; i < text.size();) {
    const std::size_t length = templatePlaceholderAt(text, i);
    if (length == 0) {
      ++i;
      continue;
    }
    TemplateValue value;
    value.name = i;
    value.placeholder = i;
    value.end = i + length;
    // An id is a whole word: "c4", not "c4x".
    if (i > 0 && std::strchr("cpkF", text[i - 1]) != nullptr &&
        (i == 1 || !(identChar(text[i - 2]) || text[i - 2] == '.')) &&
        (i + length == text.size() || !identChar(text[i + length]))) {
      value.name = i - 1;
      value.prefix = text[i - 1];
    }
    values.push_back(value);
    i += length;
  }
  return values;
}

// A character of a word of a noun phrase: a letter, an apostrophe ("the
// result's") or a hyphen ("pre-existing").
bool phraseChar(char c) { return asciiLetter(c) || c == '\'' || c == '-'; }

// `out`, which ends with the word of a kind at `start` (its name taken out),
// with "a" or "an" put in as notice.rs's `cleaned` puts it (mitcad#1638):
// none for several ("faces") or where the noun phrase has a word like "the"
// or a possessive ("the driven dimension", "the result's face"); otherwise
// before the phrase, its words looked back over up to three until a word
// that ends it ("meets a boundary curve", "at a bend edge"), or before the
// word of the kind when a verb comes first ("does not fit a face").
std::string withArticle(std::string out, std::size_t start) {
  const std::string word = out.substr(start);
  if (word.empty() || lowerAscii(word).back() == 's') return out;
  std::vector<std::size_t> starts; // the phrase's words before the kind's, nearest first
  std::size_t at = start;
  std::size_t place = start;
  while (true) {
    std::size_t gap = at;
    while (gap > 0 && std::strchr(" \t\n\r", out[gap - 1]) != nullptr) --gap;
    if (gap == at || gap == 0 || !phraseChar(out[gap - 1])) {
      place = starts.empty() ? start : starts.back();
      break;
    }
    std::size_t wordStart = gap;
    while (wordStart > 0 && phraseChar(out[wordStart - 1])) --wordStart;
    const std::string previous = lowerAscii(out.substr(wordStart, gap - wordStart));
    if (std::none_of(previous.begin(), previous.end(), asciiLetter)) {
      place = starts.empty() ? start : starts.back();
      break;
    }
    if (listedWord(previous, kCleanedDeterminers, std::size(kCleanedDeterminers)) ||
        (previous.size() >= 2 && previous.compare(previous.size() - 2, 2, "'s") == 0)) {
      return out;
    }
    if (listedWord(previous, kPhraseEnds, std::size(kPhraseEnds)) || previous.back() == 's') {
      if (listedWord(previous, kVerbLeads, std::size(kVerbLeads))) {
        place = starts.size() >= 2 ? starts[starts.size() - 2] : start;
      } else {
        place = starts.empty() ? start : starts.back();
      }
      break;
    }
    if (starts.size() == 3) {
      place = start;
      break;
    }
    starts.push_back(wordStart);
    at = wordStart;
  }
  std::size_t firstEnd = place;
  while (firstEnd < out.size() && phraseChar(out[firstEnd])) ++firstEnd;
  std::string first = out.substr(place, firstEnd - place);
  const bool vowel = std::strchr("aeiou", lowerAscii(first.substr(0, 1))[0]) != nullptr;
  const std::size_t head = out.find_last_not_of(" \t\n\r\f\v", place == 0 ? std::string::npos : place - 1);
  const bool sentence = place == 0 || head == std::string::npos || std::strchr(".!?", out[head]) != nullptr;
  const bool capital = first[0] >= 'A' && first[0] <= 'Z' && sentence;
  std::string article = vowel ? "an " : "a ";
  if (capital) article[0] = static_cast<char>(article[0] - 'a' + 'A');
  if (place == start) {
    first = lowerAscii(first);
  } else if (capital && lowerAscii(first.substr(1)) == first.substr(1)) {
    first[0] = static_cast<char>(first[0] - 'A' + 'a');
  }
  out.replace(place, firstEnd - place, article + first);
  return out;
}

// A template's text after its names were taken out, tidied as the cleaner
// does, its values numbered again in their order; "" when that is `text`.
std::string tidiedTemplate(std::string out, const std::string& text) {
  const std::pair<const char*, const char*> tidy[] = {{"()", ""}, {"( ", "("}, {" )", ")"}, {" ,", ","}, {"  ", " "}};
  for (const auto& rule : tidy) {
    const std::string from = rule.first;
    for (std::size_t found = out.find(from); found != std::string::npos; found = out.find(from)) {
      out.replace(found, from.size(), rule.second);
    }
  }
  const std::size_t first = out.find_first_not_of(" \t\n\r\f\v");
  if (first == std::string::npos) return std::string();
  out = out.substr(first, out.find_last_not_of(" \t\n\r\f\v") + 1 - first);
  std::string numbered;
  int count = 0;
  for (std::size_t i = 0; i < out.size();) {
    const std::size_t length = templatePlaceholderAt(out, i);
    if (length == 0) {
      numbered += out[i++];
      continue;
    }
    numbered += "%" + std::to_string(++count);
    i += length;
    // "%1" followed by a digit would read as %10.
    if (i < out.size() && asciiDigit(out[i])) return std::string();
  }
  return numbered == text ? std::string() : numbered;
}

// Whether the value whose name starts at `s` comes after the word of a kind
// ("line %1", "faces %1").
bool afterKindWord(const std::string& text, std::size_t s) {
  if (s < 2 || text[s - 1] != ' ') return false;
  const std::size_t wordStart = lettersStart(text, s - 1);
  return wordStart < s - 1 &&
         listedWord(lowerAscii(text.substr(wordStart, s - 1 - wordStart)), kCleanedKinds, std::size(kCleanedKinds));
}

// "a curve" or "curves" put in for a name at `out`'s end, with a capital at
// the start of a sentence.
std::string kindPhrase(const std::string& out, const char* one, const char* many, int count) {
  const bool vowel = std::strchr("aeiou", one[0]) != nullptr;
  std::string phrase = count > 1 ? std::string(many) : std::string(vowel ? "an " : "a ") + one;
  const std::size_t last = out.find_last_not_of(" \t\n\r\f\v");
  if (last == std::string::npos || std::strchr(".!?", out[last]) != nullptr) {
    phrase[0] = static_cast<char>(phrase[0] - 'a' + 'A');
  }
  return phrase;
}

// The cleaned form of a template, or "" when it has none.
std::string cleanedTemplate(const std::string& text) {
  // A renamed reference's warning has a cleaner of its own.
  if (text.find(" found as ") != std::string::npos) return std::string();
  const std::vector<TemplateValue> values = templateValues(text);
  // The names taken out: from the start of the first to the end of the
  // last of a list; after the word of a kind, or an id anywhere ("c%1 and
  // c%2 are tangent": "curves are tangent").
  struct Taken {
    std::size_t start = 0;
    std::size_t end = 0;
    int count = 1;
    char prefix = 0;
    bool afterKind = false;
  };
  std::vector<Taken> taken;
  for (std::size_t v = 0; v < values.size(); ++v) {
    const TemplateValue& value = values[v];
    Taken name;
    name.start = value.name;
    name.end = value.end;
    name.prefix = value.prefix;
    name.afterKind = afterKindWord(text, value.name);
    // A length is no name.
    if ((!name.afterKind && value.prefix == 0) || (value.prefix == 0 && text.compare(value.end, 3, " mm") == 0)) {
      continue;
    }
    for (std::size_t next = v + 1; next < values.size(); ++next) {
      const TemplateValue& other = values[next];
      if (other.prefix != value.prefix || other.name < name.end ||
          !listedWord(text.substr(name.end, other.name - name.end), kCleanedJoins, std::size(kCleanedJoins))) {
        break;
      }
      name.end = other.end;
      ++name.count;
      v = next;
    }
    taken.push_back(name);
  }
  if (taken.empty()) return std::string();
  std::string out;
  std::size_t at = 0;
  for (const Taken& name : taken) {
    out += text.substr(at, name.start - at);
    at = name.end;
    if (!name.afterKind) {
      // An id elsewhere: its kind in its stead ("a curve", "points"), with a
      // capital at the start of a sentence.
      const char* const kinds[][2] = {{"curve", "curves"}, {"point", "points"}, {"constraint", "constraints"},
                                      {"feature", "features"}};
      const std::size_t kind = name.prefix == 'c' ? 0 : name.prefix == 'p' ? 1 : name.prefix == 'k' ? 2 : 3;
      out += kindPhrase(out, kinds[kind][0], kinds[kind][1], name.count);
      continue;
    }
    // The word of the kind stays: "a line", "the line", "faces".
    out.pop_back(); // its space
    out = withArticle(out, lettersStart(out, out.size()));
  }
  out += text.substr(at);
  // What taking the names out left, tidied as the cleaner does.
  return tidiedTemplate(out, text);
}

// The kind of topology an argument's name names: "face", "seed_edge"; none
// for others. A list of names, which the cleaner shows as their kind's word
// for several ("edges", mitcad#1650), is named so on purpose: "region_names",
// "split_face_names"; `several` says so. A plural alone ("{edges} edges")
// is a count, not names.
const char* const kNamedKinds[][2] = {
    {"face", "faces"}, {"edge", "edges"}, {"vertex", "vertices"}, {"region", "regions"}};

// Whether `name` is `word` or ends with "_" and `word`.
bool namesWord(const std::string& name, const std::string& word) {
  return name == word ||
         (name.size() > word.size() && name.compare(name.size() - word.size() - 1, word.size() + 1, "_" + word) == 0);
}

int namedKind(const std::string& name, bool* several = nullptr) {
  for (std::size_t k = 0; k < std::size(kNamedKinds); ++k) {
    const std::string kind = kNamedKinds[k][0];
    for (int many = 0; many < 2; ++many) {
      if (namesWord(name, many == 0 ? kind : kind + "_names")) {
        if (several != nullptr) *several = many == 1;
        return static_cast<int>(k);
      }
    }
  }
  return -1;
}

// Whether the value that ends at `end` is followed by the word of the kind
// `kind`, one or several ("%1 edges"): a count of them, not a name.
bool beforeOwnKindWord(const std::string& text, std::size_t end, int kind) {
  if (end >= text.size() || text[end] != ' ') return false;
  std::size_t wordEnd = end + 1;
  while (wordEnd < text.size() && asciiLetter(text[wordEnd])) ++wordEnd;
  const std::string word = lowerAscii(text.substr(end + 1, wordEnd - end - 1));
  return word == kNamedKinds[kind][0] || word == kNamedKinds[kind][1];
}

// The template with the names of kinds put in (mitcad#1638), or "" when it
// has none: a value whose argument names a kind of topology ("{vertex}",
// "{seed_edge}") outside the word of a kind is shown as that kind by the
// cleaner ("at V{…}": "at a vertex"; a list "edges"), so that form is
// listed and translated whole. The cleaned forms of it follow from it.
std::string kindedTemplate(const std::string& text, const std::vector<std::string>& names) {
  const std::vector<TemplateValue> values = templateValues(text);
  if (values.size() != names.size()) return std::string();
  std::string out;
  std::size_t at = 0;
  bool taken = false;
  for (std::size_t v = 0; v < values.size(); ++v) {
    const TemplateValue& value = values[v];
    bool several = false;
    const int kind = namedKind(names[v], &several);
    const std::size_t s = value.placeholder;
    // A name is a word of its own ("Edge%1" is no name).
    const bool startsWord = s == 0 || !(identChar(text[s - 1]) || text[s - 1] == '.');
    if (kind < 0 || afterKindWord(text, s) || value.prefix != 0 || !startsWord ||
        beforeOwnKindWord(text, value.end, kind)) {
      continue;
    }
    std::size_t end = value.end;
    int count = several ? 2 : 1;
    while (v + 1 < values.size()) {
      const TemplateValue& other = values[v + 1];
      if (namedKind(names[v + 1]) != kind || other.prefix != 0 ||
          !listedWord(text.substr(end, other.placeholder - end), kCleanedJoins, std::size(kCleanedJoins))) {
        break;
      }
      end = other.end;
      ++count;
      ++v;
    }
    out += text.substr(at, s - at);
    at = end;
    out += kindPhrase(out, kNamedKinds[kind][0], kNamedKinds[kind][1], count);
    taken = true;
  }
  if (!taken) return std::string();
  out += text.substr(at);
  return tidiedTemplate(out, text);
}

// The templates with the names of kinds put in (kindedTemplate): one, or
// none; three for an argument named "topo" or "*_topo", a topological name
// of any kind (a projection's source, mitcad#1650), one with each kind it
// may be ("the projection of a face", "... an edge", "... a vertex").
std::vector<std::string> kindedTemplates(const std::string& text, const std::vector<std::string>& names) {
  for (std::size_t v = 0; v < names.size(); ++v) {
    if (!namesWord(names[v], "topo")) continue;
    std::vector<std::string> all;
    for (const char* const kind : {"face", "edge", "vertex"}) {
      std::vector<std::string> named = names;
      named[v] = names[v].substr(0, names[v].size() - 4) + kind;
      for (const std::string& form : kindedTemplates(text, named)) {
        if (std::find(all.begin(), all.end(), form) == all.end()) all.push_back(form);
      }
    }
    return all;
  }
  const std::string kinded = kindedTemplate(text, names);
  if (kinded.empty()) return {};
  return {kinded};
}

// The templates with their cleaned forms, and theirs in turn: a value that
// follows a name taken out comes after the word of its kind then ("feature
// edge %1 %2": "a feature edge %1", "a feature edge").
std::set<std::string> withCleanedForms(const std::set<std::string>& templates) {
  std::set<std::string> all = templates;
  std::vector<std::string> pending(templates.begin(), templates.end());
  while (!pending.empty()) {
    const std::string cleaned = cleanedTemplate(pending.back());
    pending.pop_back();
    if (!cleaned.empty() && all.insert(cleaned).second) pending.push_back(cleaned);
  }
  return all;
}

// Whether a template puts a count before a word whose plural is in brackets
// ("%1 face(s)", "%2 new object file(s)"; mitcad#1759): the brackets would
// stay in every language, and the template has no count the catalogues could
// give their forms. core/ writes one whole message for one and for several
// ("a face is ...", "{n} faces are ...") or puts the count after a colon. Up
// to two words may stand between; a determiner or a word that ends a noun
// phrase between ("%1 of the face(s)") makes the value a name, not a count.
bool countedBracketedPlural(const std::string& text) {
  for (std::size_t at = text.find('%'); at != std::string::npos; at = text.find('%', at + 1)) {
    std::size_t i = at + 1;
    const std::size_t number = i;
    while (i < text.size() && asciiDigit(text[i])) ++i;
    if (i == number) continue;
    for (int words = 0; words < 3 && i < text.size() && text[i] == ' '; ++words) {
      const std::size_t start = ++i;
      while (i < text.size() && (asciiLetter(text[i]) || text[i] == '-')) ++i;
      if (i == start) break;
      for (const char* ending : {"(s)", "(es)", "(ies)"}) {
        if (text.compare(i, std::strlen(ending), ending) == 0 && asciiLetter(text[i - 1])) return true;
      }
      const std::string word = lowerAscii(text.substr(start, i - start));
      if (listedWord(word, kCleanedDeterminers, std::size(kCleanedDeterminers)) ||
          listedWord(word, kPhraseEnds, std::size(kPhraseEnds))) {
        break;
      }
    }
  }
  return false;
}

// A template as one line: its backslashes doubled, its line breaks \n (the
// order of the generated list).
std::string lineForm(const std::string& text) {
  std::string out;
  for (const char c : text) {
    if (c == '\\') out += "\\\\";
    else if (c == '\n') out += "\\n";
    else out += c;
  }
  return out;
}

const char* const kTemplatesFile = "app/framework/ModelMessageTemplates.cpp";
const char* const kTemplateLine = "    QT_TRANSLATE_NOOP(\"ModelMessage\", \"";

std::string templatesSource(const std::set<std::string>& templates) {
  std::map<std::string, std::string> ordered;
  for (const std::string& text : templates) ordered.emplace(lineForm(text), text);
  std::string out =
      "// SPDX-License-Identifier: MIT\n"
      "// The templates of the model's user-facing messages (mitcad#1284,\n"
      "// docs/translations.md \"The model's messages\"): every message text of the\n"
      "// Rust sources of core/, found by `mitcad-translations messages`, with its\n"
      "// values as %1, %2, ... Generated, do not edit: the target\n"
      "// update_translations writes it again (tools/translations-update.cmake),\n"
      "// and the ctest repo.model-messages fails while it differs from the sources.\n"
      "#include \"ModelMessages.hpp\"\n\n#include <QCoreApplication>\n\nnamespace mitcad {\n\n"
      "const char* const kModelMessageTemplates[] = {\n";
  for (const auto& entry : ordered) {
    std::string literal;
    for (const char c : entry.second) {
      if (c == '\\') literal += "\\\\";
      else if (c == '"') literal += "\\\"";
      else if (c == '\n') literal += "\\n";
      else literal += c;
    }
    out += kTemplateLine + literal + "\"),\n";
  }
  out += "};\n\nconst int kModelMessageTemplateCount =\n"
         "    static_cast<int>(sizeof(kModelMessageTemplates) / sizeof(kModelMessageTemplates[0]));\n\n"
         "} // namespace mitcad\n";
  return out;
}

// The templates a generated list holds.
std::set<std::string> templatesOf(const std::string& source) {
  std::set<std::string> found;
  std::istringstream lines(source);
  std::string line;
  const std::string head = kTemplateLine;
  while (std::getline(lines, line)) {
    if (line.compare(0, head.size(), head) != 0 || line.size() < head.size() + 3 ||
        line.compare(line.size() - 3, 3, "\"),") != 0) {
      continue;
    }
    const std::string literal = line.substr(head.size(), line.size() - head.size() - 3);
    std::string text;
    for (std::size_t i = 0; i < literal.size(); ++i) {
      if (literal[i] == '\\' && i + 1 < literal.size()) {
        const char e = literal[++i];
        text += e == 'n' ? '\n' : e;
      } else {
        text += literal[i];
      }
    }
    found.insert(text);
  }
  return found;
}

// messages ROOT [--write]: the templates of core/ against the generated
// list (the ctest repo.model-messages), or the list written again.
int messages(const fs::path& root, bool write) {
  int files = 0;
  // The messages as the sources write them and as they are shown cleaned.
  const std::set<std::string> scanned = withCleanedForms(scanCore(root, files));
  const fs::path list = root / kTemplatesFile;
  if (files == 0) {
    std::cerr << (root / "core").string() << ": no Rust sources\n";
    return 1;
  }
  // No count before a bracketed plural (mitcad#1759): refused before the
  // list is written or compared.
  int counted = 0;
  for (const std::string& text : scanned) {
    if (countedBracketedPlural(text)) {
      std::cerr << "count before (s): " << lineForm(text) << "\n";
      ++counted;
    }
  }
  if (counted != 0) {
    std::cerr << counted
              << " model message templates put a count before a bracketed plural: write one whole message "
                 "for one and for several in core/, or the count after a colon (docs/translations.md)\n";
    return 1;
  }
  if (write) {
    if (!writeFile(list, templatesSource(scanned))) {
      std::cerr << list.string() << ": cannot write\n";
      return 1;
    }
    std::cout << scanned.size() << " model message templates of " << files << " sources written to " << list.string()
              << "\n";
    return 0;
  }
  bool ok = false;
  const std::set<std::string> listed = templatesOf(readFile(list, ok));
  if (!ok) {
    std::cerr << list.string() << ": cannot read\n";
    return 1;
  }
  std::vector<std::string> added;
  std::vector<std::string> gone;
  std::set_difference(scanned.begin(), scanned.end(), listed.begin(), listed.end(), std::back_inserter(added));
  std::set_difference(listed.begin(), listed.end(), scanned.begin(), scanned.end(), std::back_inserter(gone));
  std::cout << scanned.size() << " model message templates in " << files << " sources, " << listed.size()
            << " listed\n";
  if (added.empty() && gone.empty()) return 0;
  for (std::size_t i = 0; i < added.size() && i < 20; ++i) std::cerr << "new: " << lineForm(added[i]) << "\n";
  for (std::size_t i = 0; i < gone.size() && i < 20; ++i) std::cerr << "gone: " << lineForm(gone[i]) << "\n";
  std::cerr << list.string() << ": " << added.size() << " templates new in core/, " << gone.size()
            << " no longer there: refresh it and the model_<code>.ts files with the target update_translations "
               "(docs/translations.md), then translate the new ones\n";
  return 1;
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
  // The English catalogue's plural forms (mitcad#1756): the planted one's
  // non-plural messages are problems, its forms have no bracketed plural.
  expect(hasBracketedPlural("%n body(ies)") && hasBracketedPlural("Face(s)") && !hasBracketedPlural("f (s) x") &&
             !hasBracketedPlural("%n bodies"),
         "bracketed plurals");
  {
    const std::string english =
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE TS>\n<TS version=\"2.1\" language=\"en\" "
        "sourcelanguage=\"en\">\n<context>\n    <name>QObject</name>\n"
        "    <message numerus=\"yes\">\n        <source>%n body(ies)</source>\n        <translation>\n            "
        "<numerusform>%n body</numerusform>\n            <numerusform>%n bodies</numerusform>\n        "
        "</translation>\n    </message>\n"
        "    <message numerus=\"yes\">\n        <source>%n face(s)</source>\n        <translation>\n            "
        "<numerusform>%n face</numerusform>\n            <numerusform>%n face(s)</numerusform>\n        "
        "</translation>\n    </message>\n"
        "    <message>\n        <source>Sweep</source>\n        <translation>Sweep</translation>\n    </message>\n"
        "</context>\n</TS>\n";
    Catalogue plural;
    expect(parse(english, plural, error), "the English catalogue parses: " + error);
    expect(pluralCatalogueProblems(plural).size() == 2,
           "two problems in the English catalogue (a bracketed form, a message that is no plural), not " +
               std::to_string(pluralCatalogueProblems(plural).size()));
  }
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
  // The model's messages (mitcad#1284).
  expect(messageTemplate("feature {uid} does not exist") == "feature %1 does not exist", "a named argument");
  expect(messageTemplate("{slot}: the value must be {:.2} or more") == "%1: the value must be %2 or more",
         "a format spec");
  expect(messageTemplate("{}").empty() && messageTemplate("Fillet").empty() && messageTemplate("{} {}").empty(),
         "no words, no message");
  expect(messageTemplate("<svg width=\"1\" height=\"2\">").empty(), "markup is no message");
  expect(messageTemplate("use {{\"cmd\": x}} here please") == "use {\"cmd\": x} here please", "doubled braces");
  expect(messageTemplate("{a}0 mm left over").empty(), "a digit after a placeholder");
  expect(messageTemplate("at 5%2 of the length").empty(), "a percent and a digit");
  expect(messageTemplate("a tab\there in it").empty(), "a control character");
  // A count before a bracketed plural is refused (mitcad#1759); a name before
  // one, a plural of its own and a count after a colon are not.
  expect(countedBracketedPlural("%1 face(s) left out") && countedBracketedPlural("%1: %2 key(s) of the scan") &&
             countedBracketedPlural("and %1 more fastener warning(s)") &&
             countedBracketedPlural("%4 new object file(s) removed") && countedBracketedPlural("%1 body(ies)") &&
             countedBracketedPlural("%2 sketch(es) moved"),
         "a count before a bracketed plural");
  expect(!countedBracketedPlural("%1 of the face(s) moved") && !countedBracketedPlural("%1 faces left out") &&
             !countedBracketedPlural("faces left out: %1") && !countedBracketedPlural("%1 (s) and face(s)") &&
             !countedBracketedPlural("the %n face(s)") && !countedBracketedPlural("%1 is one of four new face(s)"),
         "no count before a bracketed plural");
  {
    const std::string rust =
        "fn f() {\n"
        "    let a = invalid(format!(\"the radius {r} must be positive\"));\n"
        "    panic!(\"never shown to anybody\");\n"
        "    log::warn!(\"a log line only\");\n"
        "    x.expect(&format!(\"an expectation text {}\", 1));\n"
        "    let b = \"a long message \\\n"
        "             on two lines\";\n"
        "    let c = r#\"raw text with \"quotes\" inside\"#;\n"
        "    let d = b\"byte string text here\";\n"
        "    let e = '\"'; let f = \"after the quote char\";\n"
        "    // \"a comment text here\"\n"
        "    /* \"a block comment here\" */ let g = \"after the block comment\";\n"
        "}\n"
        "#[cfg(test)]\n"
        "mod tests {\n"
        "    fn g() { let x = \"inside a test module\"; }\n"
        "}\n"
        "fn h<'a>(s: &'a str) { let y = \"after the test module\"; }\n";
    std::set<std::string> found;
    scanRust(rust, found);
    const std::set<std::string> wanted = {"the radius %1 must be positive", "a long message on two lines",
                                          "raw text with \"quotes\" inside", "after the quote char",
                                          "after the block comment", "after the test module"};
    expect(found == wanted, "the scan of a planted Rust source: " + std::to_string(found.size()) + " templates");
  }
  expect(scannedSource("core/model/src/document.rs") && !scannedSource("core/model/src/tests.rs") &&
             !scannedSource("core/mcp/src/lib.rs") && !scannedSource("core/ffi/src/chat.rs") &&
             !scannedSource("core/model/src/x_tests.rs") && !scannedSource("core/model-tests/a/src/lib.rs") &&
             !scannedSource("core/update/src/bin/tool.rs") && !scannedSource("core/model/src/lib.txt"),
         "the sources scanned");
  {
    const std::set<std::string> some = {"a \"quoted\" text here", "a back\\slash text", "two lines\nof text"};
    expect(templatesOf(templatesSource(some)) == some, "the generated list read back");
  }
  // The cleaned forms (mitcad#1635), as notice.rs's cleaner shows them.
  {
    const std::pair<const char*, const char*> cleanedForms[] = {
        {"line c%1 does not exist", "a line does not exist"},
        {"body %1 (from %2) belongs to %3; a feature works on the bodies of its own component",
         "a body (from %1) belongs to %2; a feature works on the bodies of its own component"},
        {"%1: curve c%2 does not exist", "%1: a curve does not exist"},
        {"Edge %1 is not round", "An edge is not round"},
        {"the edge %1 of %2 is gone", "the edge of %1 is gone"},
        {"%1 has no face %2", "%1 has no face"},
        {"faces %1 and %2 meet at an edge", "faces meet at an edge"},
        {"the mirror line c%1 is not a line", "the mirror line is not a line"},
        {"c%1 and c%2 are tangent at p%3: there is no corner to round",
         "Curves are tangent at a point: there is no corner to round"},
        {"%1: c%2 has no geometry", "%1: a curve has no geometry"},
        {"its offset %1 mm from the placements", ""},
        {"region %1 found as %2 after a change in %3", ""},
        {"the radius %1 must be positive", ""},
        // The article looks back over the noun phrase (mitcad#1638).
        {"%1 can move what the driven dimension %2 measures", "%1 can move what the driven dimension measures"},
        {"%1 makes a new body %2", "%1 makes a new body"},
        {"the result's face %1 is gone", "the result's face is gone"},
        {"its run's nosing profile %1 is open", "its run's nosing profile is open"},
        {"the bend meets boundary curve %1 there", "the bend meets a boundary curve there"},
        {"the faces at bend edge %1", "the faces at a bend edge"},
        {"%1 does not fit face %2", "%1 does not fit a face"},
        {"unknown embedded curve %1", "an unknown embedded curve"},
        {"%1: bend line %2 is open", "%1: a bend line is open"},
        {"%1: the thread cut body %2 into %3 pieces", "%1: the thread cut a body into %2 pieces"},
        {"Create Profile %1", "Create a profile"},
        {"DXF line %1: %2", "A DXF line: %1"},
        {"it would change pre-existing body %1", "it would change a pre-existing body"},
        {"Edge %1 is not round. Edge %2 is", "An edge is not round. An edge is"},
    };
    for (const auto& form : cleanedForms) {
      const std::string made = cleanedTemplate(form.first);
      expect(made == form.second, std::string("the cleaned form of \"") + form.first + "\": \"" + made + "\"");
    }
    expect(withCleanedForms({"line c%1 does not exist"}) ==
               std::set<std::string>{"line c%1 does not exist", "a line does not exist"},
           "a template and its cleaned form listed");
  }
  // A name put in as its kind where no word of a kind is before it
  // (mitcad#1638), told by the argument's name.
  {
    struct Kinded {
      const char* text;
      std::vector<std::string> names;
      const char* form;
    };
    const Kinded kinded[] = {
        {"the hole at %1 is too small", {"vertex"}, "the hole at a vertex is too small"},
        {"the sketch is on %1 of %2", {"face", "name"}, "the sketch is on a face of %1"},
        {"%1 is not a cylinder, cone or torus (%2)", {"face", ""}, "A face is not a cylinder, cone or torus (%1)"},
        {"the edge %1 is gone from %2", {"edge", "name"}, ""},
        {"the seam on %1 and %2 is open", {"first_edge", "second_edge"}, "the seam on edges is open"},
        {"the hole at %1 is too small", {""}, ""},
        {"%1.Edge%2 is on another body", {"base", "edge"}, ""},
        // A list of names in one value, named so on purpose (mitcad#1650):
        // the word for several.
        {"region %1 is split into %2 after a change in %3", {"", "region_names", "name"},
         "region %1 is split into regions after a change in %2"},
        {"the seam on %1 is open", {"open_edge_names"}, "the seam on edges is open"},
        {"%1 is not split into %2", {"", "facets"}, ""},
        // A count named like its kind is no list of names: none listed.
        {"%1 of its %2 edges read from the file", {"", "edges"}, ""},
        {"%1: the stored sketch has no geometry, its stored shape %2 edges", {"what", "edges"}, ""},
        {"the %1 planar faces of the mesh do not build a closed solid (%2 left out)", {"faces", ""}, ""},
        {"vertex index %1 of %2 vertices", {"i", "vertices"}, ""},
        {"a flange on %1 edges whose edges were not read from the file", {"edges"}, ""},
        {"%1%2 faces, %3 edges, %4 vertices", {"indent", "faces", "edges", "vertices"}, ""},
        {"the envelope's %1 faces do not build a closed solid (%2 left out)", {"faces", ""}, ""},
        {"%1 edges have no root half-edge with a mate (left out)", {"broken_edges"}, ""},
        // A value followed by its own kind's word is a count of them.
        {"%1 edges are open", {"edge"}, ""},
        {"%1 edge is open", {"seam_edge"}, ""},
    };
    for (const Kinded& form : kinded) {
      const std::string made = kindedTemplate(form.text, form.names);
      expect(made == form.form, std::string("the form with its kinds of \"") + form.text + "\": \"" + made + "\"");
    }
    std::set<std::string> found;
    scanRust("fn f() { let m = format!(\"the hole at {vertex} is too small\"); }\n", found);
    expect(found == std::set<std::string>{"the hole at %1 is too small", "the hole at a vertex is too small"},
           "a template and its form with its kinds listed");
    // A name of any kind (mitcad#1650): a form for each kind it may be.
    expect(kindedTemplates("the projection of %1 is gone", {"topo"}) ==
               std::vector<std::string>{"the projection of a face is gone", "the projection of an edge is gone",
                                        "the projection of a vertex is gone"},
           "a name of any kind as each kind");
    expect(kindedTemplates("%1 of %2: %3", {"name", "source_topo", ""}) ==
               std::vector<std::string>{"%1 of a face: %2", "%1 of an edge: %2", "%1 of a vertex: %2"},
           "a name of any kind among other values");
    expect(kindedTemplates("the hole at %1 is too small", {"vertex"}) ==
               std::vector<std::string>{"the hole at a vertex is too small"},
           "one form for a name of one kind");
    expect(kindedTemplates("the hole at %1 is too small", {"spot"}).empty(), "no form for another name");
    found.clear();
    scanRust("fn f() { let m = format!(\"the projection of {topo} is gone\"); }\n", found);
    expect(found == std::set<std::string>{"the projection of %1 is gone", "the projection of a face is gone",
                                          "the projection of an edge is gone", "the projection of a vertex is gone"},
           "a template and its forms of each kind listed");
  }
  if (failures == 0) std::cout << "translations self-test passed\n";
  return failures == 0 ? 0 : 1;
}

void usage() {
  std::cerr << "usage: mitcad-translations check|baseline LANGUAGES.TXT\n"
               "       mitcad-translations prepare|pseudo IN.ts OUT.ts\n"
               "       mitcad-translations messages SOURCE-ROOT [--write]\n"
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
  if ((args.size() == 2 || (args.size() == 3 && args[2] == "--write")) && args[0] == "messages") {
    return messages(args[1], args.size() == 3);
  }
  usage();
  return 2;
}
