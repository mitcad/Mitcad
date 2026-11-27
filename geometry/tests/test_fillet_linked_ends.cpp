// SPDX-License-Identifier: MIT
// A convex and a concave rounding whose contacts cross before their ends,
// the ends joined by a sharp edge that neither rounds (port patch 1780,
// mitcad#1987).
//
// A block 2.127 wide and 4 deep standing on a plate, its top sloping up at
// 45 degrees from a front face h high: its sides are rounded with 2 where
// they stand on the plate (concave) and with 0.5 along the top (convex).
// On each side the foot's contact (2 above the plate) and the top's (0.71
// below the top edge, in height) cross 2.71 - h behind the front face; the
// foot's rounding ends at the front face's bottom vertex, the top's at its
// top vertex. At h = 1.5 and 2 the contacts cross: OCCT's fillet built an
// invalid result or failed ("intersect each other"). It now reports the
// four contours as overlapping (Failure_Overlap), and MitCad's fillet
// rounds the top's edges first and the feet on that result, the foot's
// ball riding over the top's blend from the crossing to the front face.
// That second rounding's end drops the side's edge with the front face,
// which the new blend covers (its wire was not closed before 1780). The
// result is valid, every face named, and its volume that of the two
// roundings made one after the other with OCCT's fillet (within a
// millionth). At h = 3 the contacts do not cross: the four are built
// together, as before, and the same.
//
// The same block standing against a taller wall, the wall's foot rounded
// with 2 as well: that concave rounding meets the foot's at the block's
// back corners, and comes after the top's roundings with the feet (built
// first, its blend's end would take a part of the feet's edges away).
//
// With no front face (h = 0, the top running down into the plate, as in
// GitHub #371's Fillet1) both roundings of a side end at one vertex, a foot
// corner (FootCorners in geometry/src/dressup.cpp): the `.f3d` files' body
// there is the feet's rounding first and the top's on that result, which
// runs on over the foot's blend down to the plate. MitCad's fillet builds
// it so: valid, every face named, the volume of the two passes made with
// OCCT's fillet alone (the wall's foot with the feet). OCCT's single build
// reported an overlap (alone) or failed ("intersect each other", against
// the wall).

#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_Map.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/import.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kHalf  = 1.0635; // half the block's width
constexpr double kPlate = 3.0;    // the plate's top
constexpr double kFoot  = 2.0;    // the feet's rounding (concave)
constexpr double kTop   = 0.5;    // the top's rounding (convex)

// The plate with the block (and the wall behind it), as one body.
ShapePtr block_body(double h, bool wall) {
  TopoDS_Shape body = BRepPrimAPI_MakeBox(gp_Pnt(-10, -6, 0), gp_Pnt(10, 10, kPlate)).Shape();
  BRepBuilderAPI_MakePolygon polygon;
  polygon.Add(gp_Pnt(-kHalf, 0, kPlate));
  if (h > 0) {
    polygon.Add(gp_Pnt(-kHalf, 0, kPlate + h));
  }
  polygon.Add(gp_Pnt(-kHalf, 4, kPlate + h + 4));
  polygon.Add(gp_Pnt(-kHalf, 4, kPlate));
  polygon.Close();
  const TopoDS_Face section = BRepBuilderAPI_MakeFace(polygon.Wire()).Face();
  body = BRepAlgoAPI_Fuse(body, BRepPrimAPI_MakePrism(section, gp_Vec(2 * kHalf, 0, 0)).Shape()).Shape();
  if (wall) {
    body = BRepAlgoAPI_Fuse(body, BRepPrimAPI_MakeBox(gp_Pnt(-6, 4, kPlate), gp_Pnt(6, 6, 15)).Shape()).Shape();
  }
  ShapeUpgrade_UnifySameDomain unify(body);
  unify.Build();
  return import_body(unify.Shape(), "F1", 0);
}

// The edges of the body between points a and b that pass the test.
template <class F>
std::vector<int> edges_where(const Shape& body, F&& want) {
  std::vector<int> out;
  for (int e = 0; e < body.edge_count(); ++e) {
    const BRepAdaptor_Curve c(TopoDS::Edge(body.edge(e)));
    if (want(c.Value(c.FirstParameter()), c.Value(c.LastParameter()))) {
      out.push_back(e);
    }
  }
  return out;
}

// The block's sides' feet on the plate.
std::vector<int> feet(const Shape& body) {
  return edges_where(body, [](const gp_Pnt& a, const gp_Pnt& b) {
    return std::abs(a.Z() - kPlate) < 1e-9 && std::abs(b.Z() - kPlate) < 1e-9 && std::abs(a.X() - b.X()) < 1e-9 &&
           std::abs(std::abs(a.X()) - kHalf) < 1e-9 && std::abs(a.Y() - b.Y()) > 1e-6;
  });
}

// The block's sloping top's edges with its sides.
std::vector<int> tops(const Shape& body) {
  return edges_where(body, [](const gp_Pnt& a, const gp_Pnt& b) {
    return std::abs(a.X() - b.X()) < 1e-9 && std::abs(std::abs(a.X()) - kHalf) < 1e-9 &&
           std::abs(a.Y() - b.Y()) > 1e-6 && std::abs(a.Z() - b.Z()) > 1e-6;
  });
}

// The wall's foot on the plate, beside the block.
std::vector<int> wall_feet(const Shape& body) {
  return edges_where(body, [](const gp_Pnt& a, const gp_Pnt& b) {
    return std::abs(a.Z() - kPlate) < 1e-9 && std::abs(b.Z() - kPlate) < 1e-9 && std::abs(a.Y() - 4) < 1e-9 &&
           std::abs(b.Y() - 4) < 1e-9 && std::abs(a.X() - b.X()) > 1e-6;
  });
}

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

// OCCT's fillet alone on a copy of the shape: the edges (by their index in
// `body`, whose shape `shape` is) with their radii, the contours stopping
// where their edges turn as in MitCad's fillet.
struct Occt {
  bool built = false;
  int failure = 0;
  int faulty = 0;
  TopoDS_Shape shape;
};
Occt occt_fillet(const TopoDS_Shape& shape, const std::vector<TopoDS_Shape>& edges, const std::vector<double>& radii) {
  BRepFilletAPI_MakeFillet round(shape);
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> selected;
  for (const TopoDS_Shape& e : edges) {
    selected.Add(e);
  }
  round.StopChainsAtTurns(selected);
  for (std::size_t i = 0; i < edges.size(); ++i) {
    round.Add(radii[i], TopoDS::Edge(edges[i]));
  }
  Occt out;
  try {
    OCC_CATCH_SIGNALS
    round.Build();
  } catch (const Standard_Failure& failure) {
    std::printf("  OCCT: %s\n", failure.what());
    return out;
  }
  out.built = round.IsDone();
  out.failure = static_cast<int>(round.Failure());
  out.faulty = round.NbFaultyContours();
  if (out.built) {
    out.shape = round.Shape();
  }
  return out;
}

std::vector<TopoDS_Shape> shapes_of(const Shape& body, const std::vector<int>& edges) {
  std::vector<TopoDS_Shape> out;
  for (const int e : edges) {
    out.push_back(body.edge(e));
  }
  return out;
}

void test_block(double h, bool wall) {
  std::printf("block %g high at the front%s\n", h, wall ? ", against a wall" : "");
  const ShapePtr body = block_body(h, wall);
  const std::vector<int> foot = feet(*body), top = tops(*body), behind = wall ? wall_feet(*body) : std::vector<int>();
  CHECK(foot.size() == 2);
  CHECK(top.size() == 2);
  CHECK(!wall || behind.size() == 2);
  std::vector<int> concave = foot;
  concave.insert(concave.end(), behind.begin(), behind.end());
  const bool cross = h < 2.0 + 0.5 * std::sqrt(2.0);

  // OCCT's fillet alone, the top's roundings first and the concave ones on
  // that result (the edges found again there by their ends).
  const ShapePtr copy = import_body(BRepBuilderAPI_Copy(body->occt()).Shape(), "F9", 0);
  const Occt first = occt_fillet(copy->occt(), shapes_of(*copy, top), std::vector<double>(top.size(), kTop));
  CHECK(first.built);
  if (!first.built) {
    return;
  }
  const ShapePtr rounded_top = import_body(first.shape, "F8", 0);
  std::vector<int> later = feet(*rounded_top);
  if (wall) {
    const std::vector<int> more = wall_feet(*rounded_top);
    later.insert(later.end(), more.begin(), more.end());
  }
  CHECK(later.size() == concave.size());
  const Occt second =
      occt_fillet(rounded_top->occt(), shapes_of(*rounded_top, later), std::vector<double>(later.size(), kFoot));
  CHECK(second.built);
  if (!second.built) {
    return;
  }
  CHECK(BRepCheck_Analyzer(second.shape).IsValid());
  const double expected = volume_of(second.shape);

  // OCCT's fillet of all together: the overlap reported where the contacts
  // cross, else built as one after the other.
  if (!wall) {
    const ShapePtr again = import_body(BRepBuilderAPI_Copy(body->occt()).Shape(), "F9", 0);
    std::vector<int> all = foot;
    all.insert(all.end(), top.begin(), top.end());
    std::vector<double> radii(foot.size(), kFoot);
    radii.insert(radii.end(), top.size(), kTop);
    const Occt together = occt_fillet(again->occt(), shapes_of(*again, all), radii);
    if (cross) {
      CHECK(!together.built);
      CHECK(together.failure == static_cast<int>(ChFi3d_Builder::Failure_Overlap));
      CHECK(together.faulty == 4);
    } else {
      CHECK(together.built);
      if (together.built) {
        CHECK(BRepCheck_Analyzer(together.shape).IsValid());
        CHECK_NEAR(volume_of(together.shape), expected);
      }
    }
  }

  // MitCad's fillet of both sets.
  FilletSet feet_set, top_set;
  for (const int e : concave) {
    feet_set.edges.push_back(body->edge_name(e));
  }
  feet_set.radius = kFoot;
  for (const int e : top) {
    top_set.edges.push_back(body->edge_name(e));
  }
  top_set.radius = kTop;
  ShapePtr result;
  try {
    result = fillet("F3", *body, {feet_set, top_set});
  } catch (const std::exception& failure) {
    std::printf("  MitCad: %s\n", failure.what());
  }
  CHECK(result != nullptr);
  if (!result) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result->occt()).IsValid());
  CHECK(result->notes().empty());
  for (int f = 0; f < result->face_count(); ++f) {
    CHECK(!result->face_names(f).empty());
  }
  CHECK_NEAR(volume_of(result->occt()), expected);
}


// The block with no front face (h = 0): the feet first, then the top.
void test_foot_corner(bool wall) {
  std::printf("block with no front face%s\n", wall ? ", against a wall" : "");
  const ShapePtr body = block_body(0.0, wall);
  const std::vector<int> foot = feet(*body), top = tops(*body), behind = wall ? wall_feet(*body) : std::vector<int>();
  CHECK(foot.size() == 2);
  CHECK(top.size() == 2);
  CHECK(!wall || behind.size() == 2);
  std::vector<int> concave = foot;
  concave.insert(concave.end(), behind.begin(), behind.end());

  // OCCT's fillet alone, the concave roundings first and the top's on that
  // result (its edges found again there by their ends).
  const ShapePtr copy = import_body(BRepBuilderAPI_Copy(body->occt()).Shape(), "F9", 0);
  const Occt first = occt_fillet(copy->occt(), shapes_of(*copy, concave), std::vector<double>(concave.size(), kFoot));
  CHECK(first.built);
  if (!first.built) {
    return;
  }
  const ShapePtr rounded_feet = import_body(first.shape, "F8", 0);
  const std::vector<int> later = tops(*rounded_feet);
  CHECK(later.size() == 2);
  const Occt second =
      occt_fillet(rounded_feet->occt(), shapes_of(*rounded_feet, later), std::vector<double>(later.size(), kTop));
  CHECK(second.built);
  if (!second.built) {
    return;
  }
  CHECK(BRepCheck_Analyzer(second.shape).IsValid());
  const double expected = volume_of(second.shape);

  // MitCad's fillet of both sets.
  FilletSet feet_set, top_set;
  for (const int e : concave) {
    feet_set.edges.push_back(body->edge_name(e));
  }
  feet_set.radius = kFoot;
  for (const int e : top) {
    top_set.edges.push_back(body->edge_name(e));
  }
  top_set.radius = kTop;
  ShapePtr result;
  try {
    result = fillet("F3", *body, {feet_set, top_set});
  } catch (const std::exception& failure) {
    std::printf("  MitCad: %s\n", failure.what());
  }
  CHECK(result != nullptr);
  if (!result) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result->occt()).IsValid());
  CHECK(result->notes().empty());
  for (int f = 0; f < result->face_count(); ++f) {
    CHECK(!result->face_names(f).empty());
  }
  CHECK_NEAR(volume_of(result->occt()), expected);
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_linked_ends) {
  try {
    for (const bool wall : {false, true}) {
      for (const double h : {1.5, 2.0, 3.0}) {
        test::test_block(h, wall);
      }
      test::test_foot_corner(wall);
    }
  } catch (const Standard_Failure& e) {
    std::fprintf(stderr, "unexpected OCCT error: %s\n", e.what());
    ++test::failures;
  }
  if (test::failures > 0) {
    std::fprintf(stderr, "%d check(s) failed\n", test::failures);
    return 1;
  }
  std::printf("test_fillet_linked_ends: all checks passed\n");
  return 0;
}
