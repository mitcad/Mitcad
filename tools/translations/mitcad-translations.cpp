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
//       templates of core/'s Rust sources against the generated list
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
std::string messageTemplate(const std::string& text) {
  std::string out;
  std::string fixed; // the text with a space for each placeholder
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

// The message templates of one Rust source: its string literals outside
// comments, test modules and the arguments of panics, assertions,
// expectations and logs, as messageTemplate takes them.
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
    const std::string t = messageTemplate(text);
    if (!t.empty()) found.insert(t);
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
  const std::set<std::string> scanned = scanCore(root, files);
  const fs::path list = root / kTemplatesFile;
  if (files == 0) {
    std::cerr << (root / "core").string() << ": no Rust sources\n";
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
