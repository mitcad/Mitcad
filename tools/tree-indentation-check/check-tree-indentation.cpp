// SPDX-License-Identifier: MIT
//
// Keeps every list and tree of the app on the one compact indentation
// (docs/ui-conventions.md, "Lists and trees"; mitcad#1214): a QTreeView or
// QTreeWidget made in app/ takes its indentation from
// panel::setCompactIndentation (app/framework/PanelTree.hpp), never from the
// style's default (20 to 30 pixels a level) or a number of its own.
//
//   mitcad-check-tree-indentation DIR
//                                  scans the .cpp, .hpp, .h and .mm files
//                                  under DIR (folders named tests left
//                                  out); prints every view made without the
//                                  helper and every indentation of its own
//                                  as file:line and fails if any
//   mitcad-check-tree-indentation --self-test
//                                  checks the scan on planted sources
//
// The files of one name (Name.hpp, Name.cpp and Name.mm in one folder) are
// one unit. A unit's views are, outside comments and strings: each
// `new QTreeWidget` or `new QTreeView`, each such object made on the stack
// (QTreeWidget tree;), and each class of the unit derived from either,
// counted once or as many times as the unit makes it. The unit must call
// setCompactIndentation at least as many times as it has views, less its
// recorded exceptions (kAllowed, each with its reason). A subclass may call
// it in its constructor; a view made from a subclass of another unit is
// that unit's.
//
// An indentation of its own fails anywhere but in the helper's file: a
// setIndentation( call, or an AppKit outline's indentationPerLevel set to a
// number (the macOS frontend's outline views take the helper's values or the
// shared view's indentation()).

#include <algorithm>
#include <cctype>
#include <cstddef>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <map>
#include <regex>
#include <sstream>
#include <string>
#include <vector>

namespace {

// The helper's own file: it sets the indentation for everyone.
const char* const kHelper = "framework/PanelTree.hpp";

// The recorded exceptions: a unit (a path relative to the scanned folder,
// without its extension), how many views it may make without the helper and
// why (docs/ui-conventions.md "Lists and trees").
struct Allowed {
  const char* unit;
  int count;
  const char* reason;
};
const Allowed kAllowed[] = {
    {"platform/macos/NativeTestInput", 1,
     "the fixture of a UI test of the native refresh: an empty view no user sees"},
};

struct Source {
  std::string relative; // the file's path relative to the scanned folder
  std::string text;
};

bool identifierChar(char c) { return std::isalnum(static_cast<unsigned char>(c)) != 0 || c == '_'; }

// The source text without its comments and the contents of its strings and
// character literals (raw strings too): spaces in their place, lines kept.
std::string codeOnly(const std::string& text) {
  std::string out = text;
  const std::size_t n = text.size();
  const auto blank = [&out](std::size_t from, std::size_t to) {
    for (std::size_t k = from; k < to && k < out.size(); ++k) {
      if (out[k] != '\n') {
        out[k] = ' ';
      }
    }
  };
  std::size_t i = 0;
  while (i < n) {
    const char c = text[i];
    if (c == '/' && i + 1 < n && text[i + 1] == '/') {
      const std::size_t end = text.find('\n', i);
      const std::size_t stop = end == std::string::npos ? n : end;
      blank(i, stop);
      i = stop;
    } else if (c == '/' && i + 1 < n && text[i + 1] == '*') {
      const std::size_t end = text.find("*/", i + 2);
      const std::size_t stop = end == std::string::npos ? n : end + 2;
      blank(i, stop);
      i = stop;
    } else if (c == 'R' && i + 1 < n && text[i + 1] == '"' && (i == 0 || !identifierChar(text[i - 1]))) {
      // A raw string: R"delimiter( … )delimiter".
      const std::size_t open = text.find('(', i + 2);
      if (open == std::string::npos) {
        break;
      }
      const std::string close = ")" + text.substr(i + 2, open - i - 2) + "\"";
      const std::size_t end = text.find(close, open + 1);
      const std::size_t stop = end == std::string::npos ? n : end + close.size();
      blank(i + 1, stop);
      i = stop;
    } else if (c == '"' || c == '\'') {
      const std::size_t start = i + 1;
      ++i;
      while (i < n && text[i] != c && text[i] != '\n') {
        i += text[i] == '\\' ? std::size_t{2} : std::size_t{1};
      }
      blank(start, std::min(i, n));
      ++i;
    } else {
      ++i;
    }
  }
  return out;
}

int lineAt(const std::string& text, std::size_t position) {
  return 1 + static_cast<int>(std::count(text.begin(), text.begin() + static_cast<std::ptrdiff_t>(position), '\n'));
}

// The lines of a pattern's matches in a text.
std::vector<int> matchLines(const std::string& text, const std::regex& pattern) {
  std::vector<int> lines;
  for (auto it = std::sregex_iterator(text.begin(), text.end(), pattern); it != std::sregex_iterator(); ++it) {
    lines.push_back(lineAt(text, static_cast<std::size_t>(it->position(0))));
  }
  return lines;
}

// A view of the type `type` made with new or on the stack.
std::regex madePattern(const std::string& type) {
  return std::regex("\\bnew\\s+(" + type + ")\\b|\\b(" + type + ")\\s+[A-Za-z_][A-Za-z0-9_]*\\s*[;({]");
}

struct Place {
  std::string file;
  int line;
  std::string what;
};

const Allowed* allowance(const std::string& unit) {
  for (const Allowed& allowed : kAllowed) {
    if (unit == allowed.unit) {
      return &allowed;
    }
  }
  return nullptr;
}

// Checks one unit; prints its findings to `out`. Returns how many it printed.
int checkUnit(const std::string& unit, const std::vector<Source>& files, std::ostream& out) {
  static const std::regex base = madePattern("QTreeWidget|QTreeView");
  static const std::regex subclass(
      "\\b(?:class|struct)\\s+([A-Za-z_][A-Za-z0-9_]*)\\s*(?:final\\s*)?:[^{;]*\\b(?:QTreeWidget|QTreeView)\\s*[{,]");
  static const std::regex call("\\bsetCompactIndentation\\s*\\(");
  static const std::regex own("\\bsetIndentation\\s*\\(|\\bindentationPerLevel\\s*=\\s*[0-9]"
                              "|\\bsetIndentationPerLevel\\s*:\\s*[0-9]");
  std::vector<std::string> code;
  for (const Source& file : files) {
    code.push_back(codeOnly(file.text));
  }
  // The subclasses the unit defines, where.
  std::vector<std::pair<std::string, Place>> classes;
  for (std::size_t f = 0; f < files.size(); ++f) {
    for (auto it = std::sregex_iterator(code[f].begin(), code[f].end(), subclass); it != std::sregex_iterator();
         ++it) {
      classes.push_back({(*it)[1].str(),
                         {files[f].relative, lineAt(code[f], static_cast<std::size_t>(it->position(0))),
                          "a view class " + (*it)[1].str()}});
    }
  }
  std::vector<Place> views;
  std::vector<Place> numbers;
  int calls = 0;
  for (std::size_t f = 0; f < files.size(); ++f) {
    for (const int line : matchLines(code[f], base)) {
      views.push_back({files[f].relative, line, "a QTreeWidget or QTreeView"});
    }
    calls += static_cast<int>(matchLines(code[f], call).size());
    if (files[f].relative != kHelper) {
      for (const int line : matchLines(code[f], own)) {
        numbers.push_back({files[f].relative, line, "an indentation of its own"});
      }
    }
  }
  for (const auto& [name, defined] : classes) {
    std::vector<Place> made;
    const std::regex pattern = madePattern(name);
    for (std::size_t f = 0; f < files.size(); ++f) {
      for (const int line : matchLines(code[f], pattern)) {
        made.push_back({files[f].relative, line, "a " + name});
      }
    }
    if (made.empty()) {
      made.push_back(defined);
    }
    views.insert(views.end(), made.begin(), made.end());
  }
  int printed = 0;
  for (const Place& place : numbers) {
    out << place.file << ":" << place.line << ": " << place.what
        << ": use panel::setCompactIndentation (app/framework/PanelTree.hpp)\n";
    ++printed;
  }
  const Allowed* allowed = allowance(unit);
  const int limit = allowed != nullptr ? allowed->count : 0;
  const int viewCount = static_cast<int>(views.size());
  if (viewCount - limit > calls) {
    for (const Place& place : views) {
      out << place.file << ":" << place.line << ": " << place.what << " made without panel::setCompactIndentation ("
          << viewCount << " view(s) in the unit, " << calls << " call(s)";
      if (allowed != nullptr) {
        out << ", " << limit << " allowed: " << allowed->reason;
      }
      out << ")\n";
      ++printed;
    }
  }
  if (printed != 0) {
    out << "  Give each view its kind with panel::setCompactIndentation(view, panel::Indent::Tree, GroupedList or "
           "Flat) where it is made, or record the exception with its reason in kAllowed "
           "(tools/tree-indentation-check/check-tree-indentation.cpp).\n";
  }
  return printed;
}

bool isSource(const std::filesystem::path& path) {
  const std::string extension = path.extension().string();
  return extension == ".cpp" || extension == ".hpp" || extension == ".h" || extension == ".mm";
}

int scan(const std::filesystem::path& root) {
  std::map<std::string, std::vector<Source>> units;
  const std::filesystem::recursive_directory_iterator end;
  for (auto it = std::filesystem::recursive_directory_iterator(root); it != end; ++it) {
    if (it->is_directory() && it->path().filename() == "tests") {
      it.disable_recursion_pending();
      continue;
    }
    if (!it->is_regular_file() || !isSource(it->path())) {
      continue;
    }
    std::ifstream in(it->path(), std::ios::binary);
    std::stringstream text;
    text << in.rdbuf();
    const std::string relative = std::filesystem::relative(it->path(), root).generic_string();
    std::filesystem::path unit = std::filesystem::path(relative);
    unit.replace_extension();
    units[unit.generic_string()].push_back({relative, text.str()});
  }
  int found = 0;
  for (auto& [unit, files] : units) {
    std::sort(files.begin(), files.end(), [](const Source& a, const Source& b) { return a.relative < b.relative; });
    found += checkUnit(unit, files, std::cout);
  }
  return found;
}

int selfTest() {
  int failures = 0;
  const auto expect = [&failures](const char* what, const std::string& unit, const std::vector<Source>& files,
                                  int count) {
    std::ostringstream sink;
    const int got = checkUnit(unit, files, sink);
    if (got != count) {
      std::cerr << "self-test: " << what << ": " << got << " finding(s), expected " << count << "\n"
                << sink.str();
      ++failures;
    }
  };
  const auto one = [](const std::string& text) { return std::vector<Source>{{"print/Planted.cpp", text}}; };
  // The planted view without the helper fails; with it, it passes.
  expect("a view without the helper", "print/Planted", one("auto* list = new QTreeWidget;\nlist->clear();"), 1);
  expect("a view with the helper", "print/Planted",
         one("auto* list = new QTreeWidget;\npanel::setCompactIndentation(list, panel::Indent::Flat);"), 0);
  expect("a QTreeView", "print/Planted", one("m_view = new QTreeView(this);"), 1);
  expect("a view on the stack", "print/Planted", one("QTreeWidget tree;\ntree.show();"), 1);
  expect("two views, one call", "print/Planted",
         one("a = new QTreeWidget;\nb = new QTreeWidget(this);\npanel::setCompactIndentation(a, panel::Indent::Tree);"),
         2);
  // Not views: items, pointers, casts, forward declarations, comments and
  // strings.
  expect("not views", "print/Planted",
         one("class QTreeWidget;\nQTreeWidget* m_list = nullptr;\nauto* item = new QTreeWidgetItem(m_list);\n"
             "auto* t = qobject_cast<QTreeView*>(w);\nQPointer<QTreeView> kept(t);\nusing QTreeWidget::QTreeWidget;\n"
             "connect(m_list, &QTreeWidget::itemClicked, this, f);"),
         0);
  expect("comments and strings", "print/Planted",
         one("// auto* t = new QTreeWidget;\n/* QTreeView view; */\n"
             "s = \"new QTreeWidget\"; r = R\"(new QTreeView)\";"),
         0);
  // A subclass: once, or as many times as the unit makes it; its
  // constructor may call the helper; the class in the header, the call in
  // the source of the same unit.
  expect("a subclass without the helper", "print/Planted",
         one("class List : public QTreeWidget {\npublic:\n  using QTreeWidget::QTreeWidget;\n};"), 1);
  expect("a subclass that calls it", "configurations/Palette",
         {{"configurations/Palette.hpp", "class Palette final : public QTreeWidget {\n  Palette();\n};"},
          {"configurations/Palette.cpp",
           "Palette::Palette() : QTreeWidget(nullptr) {\n  panel::setCompactIndentation(this, "
           "panel::Indent::GroupedList);\n}"}},
         0);
  expect("a subclass made twice, one call", "projects/Planted",
         one("class Keyed : public QTreeWidget {};\na = new Keyed;\npanel::setCompactIndentation(a, "
             "panel::Indent::Tree);\nb = new Keyed;"),
         2);
  // An indentation of its own fails, anywhere but in the helper's file.
  expect("a number of its own", "print/Planted",
         one("auto* list = new QTreeWidget;\npanel::setCompactIndentation(list, panel::Indent::Flat);\n"
             "list->setIndentation(14);"),
         1);
  expect("an AppKit number", "platform/macos/Planted",
         {{"platform/macos/Planted.mm", "outline.indentationPerLevel = 14;\n[outline setIndentationPerLevel:10];"}}, 2);
  expect("AppKit from the shared view", "platform/macos/Planted",
         {{"platform/macos/Planted.mm",
           "outline.indentationPerLevel = tree->indentation();\no.indentationPerLevel = "
           "mitcad::panel::kTreeIndentation;"}},
         0);
  expect("the helper's own file", "framework/PanelTree",
         {{kHelper, "inline void setCompactIndentation(QTreeView* view, Indent kind) {\n"
                    "  view->setIndentation(indentationOf(kind));\n}"}},
         0);
  // A recorded exception holds its count; one more fails.
  expect("a recorded exception", kAllowed[0].unit,
         {{std::string(kAllowed[0].unit) + ".mm", "auto* tree = new QTreeView;"}}, 0);
  expect("over a recorded exception", kAllowed[0].unit,
         {{std::string(kAllowed[0].unit) + ".mm", "auto* tree = new QTreeView;\nauto* more = new QTreeView;"}}, 2);
  if (failures == 0) {
    std::cout << "self-test passed\n";
  }
  return failures == 0 ? 0 : 1;
}

} // namespace

int main(int argc, char* argv[]) {
  if (argc == 2 && std::string(argv[1]) == "--self-test") {
    return selfTest();
  }
  if (argc != 2) {
    std::cerr << "usage: mitcad-check-tree-indentation DIR | --self-test\n";
    return 2;
  }
  const std::filesystem::path root(argv[1]);
  if (!std::filesystem::is_directory(root)) {
    std::cerr << "not a folder: " << root.string() << "\n";
    return 2;
  }
  const int found = scan(root);
  if (found != 0) {
    std::cout << found << " list or tree finding(s): the indentation is not the app's one\n";
    return 1;
  }
  std::cout << "every list and tree takes its indentation from panel::setCompactIndentation\n";
  return 0;
}
