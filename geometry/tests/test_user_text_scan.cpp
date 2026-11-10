// SPDX-License-Identifier: MIT
//
// The geometry's thrown texts say what is wrong in words (mitcad#1612,
// docs/ui-conventions.md "What a message says"): a face, an edge or a
// vertex named by its topological name, or a point printed as numbers, goes
// only into an error's detail and places, through placed_error() and its
// helpers (mitcad/geometry/user_error.hpp), never into the message a person
// reads.
//
//   test_user_text_scan DIR    scans DIR's .cpp files; prints each finding
//                              as file:line and fails if any
//   test_user_text_scan --self-test
//                              checks the scan on planted sources
//
// The scan reads the source text. It splits each file into statements (at
// ';' and a block's braces outside string literals and comments) and
// flags:
//   1. a statement that builds a text naming a place ("edge " + …,
//      "face " + …, "vertex " + …, point_text(…), a "%.4g" or "%g" point)
//      without placed_error() or one of its helpers in it;
//   2. a throw whose statement reads a place's name (face_names(…),
//      edge_name(…), vertex_name(…)) without them.
// A statement that builds an agents' text on its own (the detail it then
// passes to placed_error(), a note the model cleans) is listed in kAllowed
// by its file and a piece of its text, with the reason.

#include <filesystem>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

namespace {

struct Statement {
  std::size_t line = 0;
  std::string text; // code only: comments left out, string literals kept
};

// The statements of a C++ source: from after one ';' or block brace to
// the next (a lambda's body split into its statements too), with the line
// it starts on.
std::vector<Statement> statements(const std::string& source) {
  std::vector<Statement> out;
  Statement current;
  std::size_t line = 1;
  int depth = 0;
  std::vector<bool> braces; // each open brace: a block's (true) or a list's
  bool started = false;
  const auto finish = [&]() {
    if (started) {
      out.push_back(current);
    }
    current = Statement{};
    started = false;
  };
  for (std::size_t i = 0; i < source.size(); ++i) {
    const char c = source[i];
    if (c == '\n') {
      ++line;
      current.text += ' ';
      continue;
    }
    // Comments.
    if (c == '/' && i + 1 < source.size() && source[i + 1] == '/') {
      while (i + 1 < source.size() && source[i + 1] != '\n') {
        ++i;
      }
      continue;
    }
    if (c == '/' && i + 1 < source.size() && source[i + 1] == '*') {
      i += 2;
      while (i + 1 < source.size() && !(source[i] == '*' && source[i + 1] == '/')) {
        if (source[i] == '\n') {
          ++line;
        }
        ++i;
      }
      ++i;
      continue;
    }
    // Preprocessor lines.
    if (c == '#' && (current.text.find_first_not_of(' ') == std::string::npos)) {
      while (i + 1 < source.size() && source[i + 1] != '\n') {
        ++i;
      }
      continue;
    }
    if (!started && c != ' ' && c != '\t' && c != '\r') {
      started = true;
      current.line = line;
    }
    // String and character literals, kept.
    if (c == '"' || c == '\'') {
      current.text += c;
      for (++i; i < source.size() && source[i] != c; ++i) {
        if (source[i] == '\\' && i + 1 < source.size()) {
          current.text += source[i++];
        }
        if (source[i] == '\n') {
          ++line;
        }
        current.text += source[i];
      }
      current.text += c;
      continue;
    }
    if (c == '(' || c == '[') {
      ++depth;
    } else if ((c == ')' || c == ']') && depth > 0) {
      --depth;
    }
    // A block's brace (at depth 0, or a lambda's body after its ')') ends
    // a statement; an initializer list's does not.
    bool ends = c == ';';
    if (c == '{') {
      const std::size_t last = current.text.find_last_not_of(' ');
      const bool block = depth == 0 || (last != std::string::npos && current.text[last] == ')');
      braces.push_back(block);
      ends = block;
    } else if (c == '}') {
      ends = braces.empty() || braces.back();
      if (!braces.empty()) {
        braces.pop_back();
      }
    }
    current.text += c;
    if (ends) {
      finish();
    }
  }
  finish();
  return out;
}

bool contains(const std::string& text, const char* piece) { return text.find(piece) != std::string::npos; }

bool placed(const std::string& text) {
  for (const char* call : {"placed_error(", "face_error(", "edge_error(", "vertex_error(", "with_detail(",
                           "no_face_error(", "no_edge_error(", "profile_error("}) {
    if (contains(text, call)) {
      return true;
    }
  }
  return false;
}

// Builds a text that names a place: rule 1.
// A string literal that ends in "edge ", "face ", "vertex " (or their
// plurals) and goes on with '+': a name follows ("… of edge " + name).
bool names_place(const std::string& text) {
  for (const char* piece : {"point_text(", "%.4g", "%g, %g"}) {
    if (contains(text, piece)) {
      return true;
    }
  }
  for (const std::string word : {"edge \"", "face \"", "vertex \"", "edges \"", "faces \"", "vertices \""}) {
    for (std::size_t at = text.find(word); at != std::string::npos; at = text.find(word, at + 1)) {
      // The word starts the literal or follows a space in it.
      if (at > 0 && text[at - 1] != '"' && text[at - 1] != ' ') {
        continue;
      }
      const std::size_t next = text.find_first_not_of(' ', at + word.size());
      if (next != std::string::npos && text[next] == '+') {
        return true;
      }
    }
  }
  return false;
}

// Throws with a place's name read: rule 2.
bool throws_name(const std::string& text) {
  if (!contains(text, "throw ")) {
    return false;
  }
  for (const char* piece : {"face_names(", "edge_name(", "vertex_name(", "edge_names(", "vertex_names("}) {
    if (contains(text, piece)) {
      return true;
    }
  }
  return false;
}

struct Allowed {
  const char* file;
  const char* piece;
  const char* reason;
};

// Statements that build an agents' text on their own.
const Allowed kAllowed[] = {
    {"boundary_fill.cpp", "std::string point_text(", "point_text()'s own head"},
    {"boundary_fill.cpp", "\"(%g, %g, %g)\"", "point_text(): a point printed for an error's detail"},
    {"cutouts.cpp", "first = \"the chamfer of \"", "the detail of the chamfer's overflow, placed_error() below"},
    {"dressup.cpp", "\"an unnamed edge\" : \"edge \" + name",
     "describe_edge(): an edge's name for an error's detail (edges_error())"},
    {"dressup.cpp", "\"an unnamed face\") : \"face \" + names.front()",
     "a warning's text (the model makes its message plain, notice.rs plain_message)"},
    {"dressup_check.cpp", "\"face \" + name", "a warning's text (the model makes its message plain)"},
    {"brep_import.cpp", "note(report,", "an import note for the log, by index"},
    {"cutouts.cpp", "first += \", at its corner with the \"", "the detail of the chamfer's overflow"},
    {"dressup.cpp", "const std::string edges_named =", "the detail of the roundings' meeting, placed_error() below"},
    {"dressup_check.cpp", "\" of edge \" + inner(", "a warning's text (the model makes its message plain)"},
    {"face_join.cpp", "joined->add_info(", "an information's text (the model makes its message plain)"},
    {"cutouts.cpp", "std::snprintf(touch, sizeof touch,",
     "the touch point for the detail of the chamfer reaching a far edge, placed_error() below"},
};

bool allowed(const std::string& file, const std::string& text) {
  for (const Allowed& a : kAllowed) {
    if (file == a.file && contains(text, a.piece)) {
      return true;
    }
  }
  return false;
}

// The findings of one source.
std::vector<std::string> scan(const std::string& file, const std::string& source) {
  std::vector<std::string> findings;
  for (const Statement& s : statements(source)) {
    if ((names_place(s.text) || throws_name(s.text)) && !placed(s.text) && !allowed(file, s.text)) {
      findings.push_back(file + ":" + std::to_string(s.line) + ": " + s.text.substr(0, 160));
    }
  }
  return findings;
}

int self_test() {
  int failed = 0;
  const auto expect = [&](const std::string& source, std::size_t count) {
    const std::size_t found = scan("planted.cpp", source).size();
    if (found != count) {
      std::cerr << "self-test: " << found << " findings, expected " << count << ", in:\n" << source << "\n";
      ++failed;
    }
  };
  // Names in a message.
  expect("void f() { throw std::invalid_argument(\"edge \" + name + \" folds\"); }", 1);
  expect("void f() {\n  throw std::runtime_error(\"face \" + body.face_names(n).front()\n + \" is gone\");\n}", 1);
  expect("void f() { std::string why = \"vertex \" + v + \" is no end\"; throw std::invalid_argument(why); }", 1);
  expect("void f() { std::snprintf(b, sizeof b, \"at (%.4g, %.4g, %.4g)\", x, y, z); }", 1);
  expect("void f() { throw std::invalid_argument(\"cannot meet \" + body.face_names(n).front()); }", 1);
  // Placed: the names in the detail.
  expect("void f() { throw std::invalid_argument(face_error(face, \"the face is not planar\",\n"
         "  \"face \" + face + \" is not planar\")); }",
         0);
  expect("void f() { throw std::runtime_error(placed_error(\"a point\", \"the point \" + point_text(p), {})); }", 0);
  // Comments and other text are not read.
  expect("// \"edge \" + name\nvoid f() { /* \"face \" + x */ throw std::runtime_error(\"failed\"); }", 0);
  expect("void f() { const auto& n = body.face_names(i); places.push_back(face_place(n.front())); }", 0);
  // A lambda's statements on their own.
  expect("int f() { return run(\"x\", [&] { const auto& n = body.face_names(i);\n"
         "  throw std::runtime_error(\"failed\"); }); }",
         0);
  expect("int f() { return run(\"x\", [&] { if (bad) {\n throw std::runtime_error(\"edge \" + n); } }); }", 1);
  // Allowed by file.
  if (!scan("brep_import.cpp", "void f() { note(report, \"face \" + std::to_string(i) + \": none\"); }").empty()) {
    std::cerr << "self-test: an allowed statement was flagged\n";
    ++failed;
  }
  if (failed == 0) {
    std::cout << "self-test passed\n";
  }
  return failed == 0 ? 0 : 1;
}

} // namespace

int main(int argc, char** argv) {
  if (argc != 2) {
    std::cerr << "usage: test_user_text_scan DIR | --self-test\n";
    return 2;
  }
  if (std::string(argv[1]) == "--self-test") {
    return self_test();
  }
  std::vector<std::string> findings;
  std::size_t files = 0;
  for (const auto& entry : std::filesystem::directory_iterator(argv[1])) {
    if (entry.path().extension() != ".cpp") {
      continue;
    }
    std::ifstream in(entry.path(), std::ios::binary);
    std::stringstream text;
    text << in.rdbuf();
    ++files;
    for (std::string& finding : scan(entry.path().filename().string(), text.str())) {
      findings.push_back(std::move(finding));
    }
  }
  for (const std::string& finding : findings) {
    std::cout << finding << "\n";
  }
  if (!findings.empty()) {
    std::cout << findings.size()
              << " thrown texts name a face, an edge or a vertex, or print a point, outside placed_error() "
                 "(docs/ui-conventions.md, \"What a message says\")\n";
    return 1;
  }
  std::cout << files << " files: no names or points in a message\n";
  return files == 0 ? 1 : 0;
}
