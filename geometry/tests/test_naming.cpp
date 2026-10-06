// SPDX-License-Identifier: MIT
// The name grammar on the C++ side: composing, matching and parsing.

#include "check.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

void test_face_names_and_matching() {
  CHECK(face_name("F3", "side", "c1[c4,c2]") == "F3:side(c1[c4,c2])");
  CHECK(face_name("F7", "top") == "F7:top");
  // A reference matches the name itself and all its split pieces.
  CHECK(name_matches("F3:end(r{c5})", "F3:end(r{c5})"));
  CHECK(name_matches("F3:end(r{c5})#1", "F3:end(r{c5})"));
  CHECK(name_matches("F3:end(r{c5})#1#0", "F3:end(r{c5})#1"));
  CHECK(!name_matches("F3:end(r{c5})#10", "F3:end(r{c5})#1"));
  CHECK(!name_matches("F3:end(r{c5})", "F3:end(r{c5})#1"));
  CHECK(!name_matches("F31:top", "F3"));
  CHECK(face_matches({"F2:side(c1)", "F4:side(c9)"}, "F4:side(c9)"));
  CHECK(!face_matches({}, "F4:side(c9)"));
}

void test_edge_and_vertex_names_are_canonical() {
  // Byte-wise order, the same as the Rust model's canonical order.
  CHECK(make_edge_name("F3:side(c2)", "F10:end(r{c1})") == "E{F10:end(r{c1})|F3:side(c2)}");
  CHECK(make_edge_name("F2:a", "F2:b", 1) == "E{F2:a|F2:b}#1");
  CHECK(make_vertex_name({"F2:c", "F2:a", "F2:b", "F2:a"}) == "V{F2:a|F2:b|F2:c}");
  CHECK(make_vertex_name({"F2:b", "F2:a"}, 0) == "V{F2:a|F2:b}#0");
}

void test_composite_names_parse() {
  const auto edge = parse_composite_name("E{F2:end(r{c1[c4,c2],c2[c1,c3]})|F5:fillet(E{F2:a|F2:b}#1)}#3");
  CHECK(edge && edge->kind == 'E' && edge->index == 3);
  CHECK(edge && edge->faces.size() == 2);
  if (edge && edge->faces.size() == 2) {
    CHECK(edge->faces[0] == "F2:end(r{c1[c4,c2],c2[c1,c3]})");
    CHECK(edge->faces[1] == "F5:fillet(E{F2:a|F2:b}#1)");
  }
  const auto vertex = parse_composite_name("V{F2:a|F2:b#0|F3:c(V{F1:x|F1:y|F1:z})}");
  CHECK(vertex && vertex->kind == 'V' && vertex->index == -1 && vertex->faces.size() == 3);

  for (const char* bad : {"", "E{}", "E{F2:a}", "E{F2:a|F2:b", "E{F2:a|F2:b}#", "E{F2:a|F2:b}x",
                          "E{F2:a||F2:b}", "F2:side(c1)", "X{F2:a|F2:b}", "E{F2:a|F2:b}#99999999999"}) {
    if (parse_composite_name(bad)) {
      std::fprintf(stderr, "parsed an invalid name: %s\n", bad);
      CHECK(false);
    }
  }
}

} // namespace

void naming_tests() {
  test_face_names_and_matching();
  test_edge_and_vertex_names_are_canonical();
  test_composite_names_parse();
}

} // namespace test
