// SPDX-License-Identifier: MIT
// Fill Cavity (mitcad#1818): the empty spaces of a body below a cap plane,
// named so that the model tells what each touches (the cap, a side or the
// bottom of the box, the body's faces); the body is not changed.

#include <cstddef>
#include <cstdio>
#include <initializer_list>
#include <memory>
#include <string>
#include <vector>

#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/fill_cavity.hpp"
#include "mitcad/geometry/input_check.hpp"
#include "mitcad/geometry/primitive.hpp"
#include "mitcad/geometry/query.hpp"

namespace test {
namespace {

using mitcad::geometry::BooleanOp;
using mitcad::geometry::BooleanResult;
using mitcad::geometry::CavitySpec;
using mitcad::geometry::Shape;

// A box with its lowest corner at `corner` and sides along the axes.
ShapePtr box(const std::string& feature, const gp_Pnt& corner, double length, double width, double height) {
  mitcad::geometry::PrimitiveSpec spec;
  spec.feature = feature;
  spec.frame.origin = corner;
  spec.kind = mitcad::geometry::PrimitiveKind::Box;
  spec.a = length;
  spec.b = width;
  spec.c = height;
  return mitcad::geometry::primitive(spec);
}

// The V of the slit: the triangle (18, 15), (20, 15), (20, 10) in XZ,
// from y = 5 to 15, its faces F3:slit(k).
ShapePtr v_prism() {
  BRepBuilderAPI_MakePolygon polygon(gp_Pnt(18, 5, 15), gp_Pnt(20, 5, 15), gp_Pnt(20, 5, 10), true);
  const BRepBuilderAPI_MakeFace face(polygon.Wire(), true);
  BRepPrimAPI_MakePrism prism(face.Face(), gp_Vec(0, 10, 0));
  prism.Build();
  std::vector<Shape::NamedFace> names;
  int k = 0;
  for (TopExp_Explorer it(prism.Shape(), TopAbs_FACE); it.More(); it.Next()) {
    names.push_back({it.Current(), {"F3:slit(" + std::to_string(k++) + ")"}});
  }
  return std::make_shared<Shape>(prism.Shape(), names);
}

ShapePtr only_piece(const BooleanResult& result) {
  CHECK(result.pieces.size() == 1);
  return result.pieces.empty() ? nullptr : result.pieces.front().shape;
}

// A block 40 x 20 x 15 with a wall 20 x 20 x 5 on its right half (to 20
// high), and a blind V slit along the wall's foot in the lower half, 5 deep,
// 2 wide at the top and 10 long: 14000 - 50 mm3.
ShapePtr slit_block() {
  const ShapePtr block = box("F1", gp_Pnt(0, 0, 0), 40, 20, 15);
  const ShapePtr wall = box("F2", gp_Pnt(20, 0, 15), 20, 20, 5);
  const ShapePtr joined = only_piece(mitcad::geometry::boolean(BooleanOp::Join, {block.get()}, *wall));
  const ShapePtr prism = v_prism();
  return only_piece(mitcad::geometry::boolean(BooleanOp::Cut, {joined.get()}, *prism));
}

bool named(const Shape& shape, const std::string& start) {
  for (int i = 0; i < shape.face_count(); ++i) {
    for (const std::string& name : shape.face_names(i)) {
      if (name.compare(0, start.size(), start) == 0) {
        return true;
      }
    }
  }
  return false;
}

// The cap on the lower half's top (15 high), the box the block's extent
// grown by 1 mm on the sides and below.
CavitySpec cap_at_15() {
  CavitySpec spec;
  spec.feature = "F9";
  spec.top = "F9:fill";
  spec.origin = {0.0, 0.0, 15.0};
  spec.x_axis = {1.0, 0.0, 0.0};
  spec.normal = {0.0, 0.0, 1.0};
  spec.min = {-1.0, -1.0, -16.0};
  spec.max = {41.0, 21.0, 0.0};
  return spec;
}

void test_the_slit_is_closed_by_the_cap() {
  const ShapePtr body = slit_block();
  CHECK_NEAR(mitcad::geometry::volume(*body), 13950.0);
  const std::vector<ShapePtr> pieces = mitcad::geometry::cavity_pieces(*body, cap_at_15());
  // The slit and the space round the block.
  CHECK(pieces.size() == 2);
  ShapePtr slit;
  ShapePtr outside;
  double total = 0.0;
  for (const ShapePtr& piece : pieces) {
    total += mitcad::geometry::volume(*piece);
    if (named(*piece, "F9:fill_side") || named(*piece, "F9:fill_bottom")) {
      outside = piece;
    } else {
      slit = piece;
    }
  }
  // The box less the block below the cap: 42 x 22 x 16 - (12000 - 50).
  CHECK_NEAR(total, 14784.0 - 11950.0);
  CHECK(slit != nullptr);
  CHECK(outside != nullptr);
  if (!slit || !outside) {
    return;
  }
  CHECK_NEAR(mitcad::geometry::volume(*slit), 50.0);
  CHECK(named(*slit, "F9:fill"));
  CHECK(named(*slit, "F3:slit("));
  CHECK(named(*outside, "F9:fill"));
  CHECK(named(*outside, "F9:fill_bottom"));
  // Joined, the slit is gone: the block's volume and none of its walls.
  const ShapePtr filled = only_piece(mitcad::geometry::boolean(BooleanOp::Join, {body.get()}, *slit));
  if (filled) {
    CHECK_NEAR(mitcad::geometry::volume(*filled), 14000.0);
    // The slit's slanted wall and its ends are gone. Its vertical wall
    // lies in the plane of the wall's outer face above it, which the join
    // merges with its coplanar neighbours: the name stays only as an alias
    // of that outer face (F2:side3), never on a face of its own.
    for (const char* gone : {"F3:slit(0)", "F3:slit(2)", "F3:slit(3)", "F3:slit(4)"}) {
      CHECK(!named(*filled, gone));
    }
    for (int i = 0; i < filled->face_count(); ++i) {
      const mitcad::geometry::NameList& names = filled->face_names(i);
      bool slit_name = false;
      bool wall = false;
      for (const std::string& name : names) {
        slit_name = slit_name || name.compare(0, 8, "F3:slit(") == 0;
        wall = wall || name == "F2:side3";
      }
      CHECK(!slit_name || wall);
      if (slit_name && !wall) {
        for (const std::string& name : names) {
          std::fprintf(stderr, "  face %d: %s\n", i, name.c_str());
        }
      }
    }
    CHECK(named(*filled, "F9:fill"));
  }
}

void test_a_lower_cap_takes_the_slit_below_it() {
  const ShapePtr body = slit_block();
  CavitySpec spec = cap_at_15();
  spec.origin = {0.0, 0.0, 13.0};
  spec.min = {-1.0, -1.0, -14.0};
  const std::vector<ShapePtr> pieces = mitcad::geometry::cavity_pieces(*body, spec);
  // The V 3 deep of its 5: 1.2 wide at 13 high, 1.8 mm2 over 10 mm.
  bool found = false;
  for (const ShapePtr& piece : pieces) {
    if (!named(*piece, "F9:fill_side") && !named(*piece, "F9:fill_bottom")) {
      found = true;
      CHECK_NEAR(mitcad::geometry::volume(*piece), 18.0);
      CHECK(named(*piece, "F9:fill"));
    }
  }
  CHECK(found);
}

void test_a_box_the_body_does_not_reach_is_whole() {
  const ShapePtr body = slit_block();
  CavitySpec spec = cap_at_15();
  spec.origin = {100.0, 0.0, 15.0};
  const std::vector<ShapePtr> pieces = mitcad::geometry::cavity_pieces(*body, spec);
  CHECK(pieces.size() == 1);
  if (pieces.size() == 1) {
    CHECK_NEAR(mitcad::geometry::volume(*pieces[0]), 42.0 * 22.0 * 16.0);
    CHECK(named(*pieces[0], "F9:fill"));
    CHECK(named(*pieces[0], "F9:fill_side(3)"));
  }
}

void test_a_box_without_volume_is_refused() {
  const ShapePtr body = slit_block();
  CavitySpec spec = cap_at_15();
  spec.min[2] = 0.0;
  CHECK(throws_with([&] { mitcad::geometry::cavity_pieces(*body, spec); }, "has no volume"));
}

} // namespace
} // namespace test

int main() {
  // The body is read, never changed (T0e).
  mitcad::geometry::enable_input_checks(true);
  test::guarded("test_the_slit_is_closed_by_the_cap", test::test_the_slit_is_closed_by_the_cap);
  test::guarded("test_a_lower_cap_takes_the_slit_below_it", test::test_a_lower_cap_takes_the_slit_below_it);
  test::guarded("test_a_box_the_body_does_not_reach_is_whole", test::test_a_box_the_body_does_not_reach_is_whole);
  test::guarded("test_a_box_without_volume_is_refused", test::test_a_box_without_volume_is_refused);
  const std::vector<std::string> changes = mitcad::geometry::input_changes();
  for (const std::string& change : changes) {
    std::fprintf(stderr, "input changed: %s\n", change.c_str());
  }
  CHECK(changes.empty());
  if (test::failures == 0) {
    std::puts("test_fill_cavity: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
