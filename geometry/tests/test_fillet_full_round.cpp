// SPDX-License-Identifier: MIT
// Fillet's Full Round (mitcad#2279): the radius that rounds a face away
// between its two edges, worked out on the body.
//
// A block 20 wide (x), 30 high (y) and 30 long (z), its top's edge at x 20
// bevelled by d (a chamfer of the section's corner): the top, 20 - d wide,
// lies between the vertical side x = 0 and the bevel at 45 degrees. A full
// round of the top's two edges touches the side, the top and the bevel:
// its centre is at (r, 30 - r), r = (20 - d) / sqrt(2) (with the angled
// side not half the top's width). Built as one valid closed solid with no
// note or information; the top is gone and its names are on the rounding;
// every rounding face is a cylinder of radius r whose axis lies r from the
// side and from the bevel within 0.000001 mm; the volume is the section's
// (the block less the bevel's triangle d^2 / 2 and the two roundings'
// corners, r^2 (sqrt(2) - 3 pi / 8)) times 30. With the bevel's size
// changed (d 4, then 6) the radius follows.
//
// Refused in words: a block whose top narrows along its edges (a side at an
// angle to them: the radius would change along the edges), two edges that
// share no face, two edges that meet at a corner of the face, and a face
// named instead of its edges.

#include <cmath>
#include <cstdio>
#include <exception>
#include <memory>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/user_error.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kWidth = 20.0;
constexpr double kHeight = 30.0;
constexpr double kLength = 30.0;
constexpr double kTolerance = 1.0e-6; // mm

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

int count_of(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
  int n = 0;
  for (TopExp_Explorer ex(shape, type); ex.More(); ex.Next()) {
    ++n;
  }
  return n;
}

// Every edge of the shape between two faces (a seam: one face twice).
bool closed(const TopoDS_Shape& shape) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, faces);
  for (int i = 1; i <= faces.Extent(); ++i) {
    if (!BRep_Tool::Degenerated(TopoDS::Edge(faces.FindKey(i))) && faces(i).Extent() < 2) {
      return false;
    }
  }
  return true;
}

// The prism of the closed polygon swept by `along`, its faces named T:f0,
// T:f1, ... in their order.
ShapePtr prism(const std::vector<gp_Pnt>& polygon, const gp_Vec& along) {
  BRepBuilderAPI_MakePolygon outline;
  for (const gp_Pnt& p : polygon) {
    outline.Add(p);
  }
  outline.Close();
  const TopoDS_Face section = BRepBuilderAPI_MakeFace(outline.Wire(), true).Face();
  const TopoDS_Shape solid = BRepPrimAPI_MakePrism(section, along).Shape();
  std::vector<Shape::NamedFace> named;
  int i = 0;
  for (TopExp_Explorer ex(solid, TopAbs_FACE); ex.More(); ex.Next()) {
    named.push_back({ex.Current(), {"T:f" + std::to_string(i++)}});
  }
  return std::make_shared<Shape>(solid, named);
}

// The block whose top's edge at x 20 is bevelled by d, along z.
ShapePtr bevelled_block(double d) {
  return prism({gp_Pnt(0, 0, 0), gp_Pnt(kWidth, 0, 0), gp_Pnt(kWidth, kHeight - d, 0),
                gp_Pnt(kWidth - d, kHeight, 0), gp_Pnt(0, kHeight, 0)},
               gp_Vec(0, 0, kLength));
}

// The name of the edge of the body from a to b (either way round); empty
// for none.
std::string edge_named(const Shape& body, const gp_Pnt& a, const gp_Pnt& b) {
  for (int e = 0; e < body.edge_count(); ++e) {
    TopoDS_Vertex first;
    TopoDS_Vertex last;
    TopExp::Vertices(body.edge(e), first, last);
    if (first.IsNull() || last.IsNull()) {
      continue;
    }
    const gp_Pnt p = BRep_Tool::Pnt(first);
    const gp_Pnt q = BRep_Tool::Pnt(last);
    if ((p.Distance(a) < 1e-7 && q.Distance(b) < 1e-7) || (p.Distance(b) < 1e-7 && q.Distance(a) < 1e-7)) {
      return body.edge_name(e);
    }
  }
  return {};
}

// The first name of the planar face of the body on the plane through `at`
// with normal `normal`; empty for none.
std::string plane_named(const Shape& body, const gp_Pnt& at, const gp_Dir& normal) {
  for (int f = 0; f < body.face_count(); ++f) {
    const BRepAdaptor_Surface surface(body.face(f));
    if (surface.GetType() != GeomAbs_Plane) {
      continue;
    }
    const gp_Pln plane = surface.Plane();
    if (plane.Axis().Direction().IsParallel(normal, 1e-9) && plane.Distance(at) < 1e-7 &&
        !body.face_names(f).empty()) {
      return body.face_names(f).front();
    }
  }
  return {};
}

FilletSet full_round_of(std::vector<std::string> edges) {
  FilletSet set;
  set.edges = std::move(edges);
  set.size = FilletSize::FullRound;
  return set;
}

std::string notes_of(const Shape& shape) {
  std::string text;
  for (const std::string& note : shape.notes()) {
    text += (text.empty() ? "" : "; ") + note;
  }
  for (const std::string& info : shape.infos()) {
    text += (text.empty() ? "" : "; ") + info;
  }
  return text;
}

void check_full_round(double d) {
  const ShapePtr body = bevelled_block(d);
  const std::string side = edge_named(*body, gp_Pnt(0, kHeight, 0), gp_Pnt(0, kHeight, kLength));
  const std::string bevel =
      edge_named(*body, gp_Pnt(kWidth - d, kHeight, 0), gp_Pnt(kWidth - d, kHeight, kLength));
  const std::string top = plane_named(*body, gp_Pnt(0, kHeight, 0), gp_Dir(0, 1, 0));
  CHECK(!side.empty() && !bevel.empty() && !top.empty());
  const ShapePtr made = fillet("F9", *body, {full_round_of({side, bevel})});
  CHECK(made != nullptr);
  if (!made) {
    return;
  }
  const double r = (kWidth - d) / std::sqrt(2.0);
  std::printf("bevel %g: full round radius %.12g, %d faces, volume %.9f%s%s\n", d, r,
              count_of(made->occt(), TopAbs_FACE), volume_of(made->occt()), notes_of(*made).empty() ? "" : ", ",
              notes_of(*made).c_str());
  // One valid closed solid, nothing to tell.
  CHECK(BRepCheck_Analyzer(made->occt()).IsValid());
  CHECK(count_of(made->occt(), TopAbs_SOLID) == 1);
  CHECK(closed(made->occt()));
  CHECK(made->notes().empty() && made->infos().empty());
  // The section: the block less the bevel's triangle and the two
  // roundings' corners (a right angle's r^2 (1 - pi / 4), the 135 degrees
  // at the bevel's r^2 (tan(pi / 8) - pi / 8)).
  const double section = kWidth * kHeight - d * d / 2.0 - r * r * (std::sqrt(2.0) - 3.0 * kPi / 8.0);
  CHECK_NEAR_TOL(volume_of(made->occt()), section * kLength, 1e-9);
  // The top is gone: no plane there; its names are on the rounding only.
  CHECK(plane_named(*made, gp_Pnt(0, kHeight, 0), gp_Dir(0, 1, 0)).empty());
  int rounding = 0;
  int named_top = 0;
  for (int f = 0; f < made->face_count(); ++f) {
    const NameList& names = made->face_names(f);
    bool made_here = false;
    bool holds_top = false;
    for (const std::string& name : names) {
      made_here = made_here || name.rfind("F9:fillet(", 0) == 0;
      holds_top = holds_top || name == top;
    }
    named_top += holds_top ? 1 : 0;
    if (holds_top) {
      CHECK(made_here);
    }
    if (!made_here) {
      continue;
    }
    ++rounding;
    // A cylinder of radius r along z whose axis lies r from the side
    // (x = 0) and from the bevel (x + y = 50 - d): it touches both.
    const BRepAdaptor_Surface surface(made->face(f));
    CHECK(surface.GetType() == GeomAbs_Cylinder);
    if (surface.GetType() != GeomAbs_Cylinder) {
      continue;
    }
    const gp_Cylinder cylinder = surface.Cylinder();
    CHECK(std::abs(cylinder.Radius() - r) < kTolerance);
    CHECK(cylinder.Axis().Direction().IsParallel(gp_Dir(0, 0, 1), 1e-9));
    const gp_Pnt axis = cylinder.Axis().Location();
    const double to_side = axis.X();
    const double to_bevel = (kWidth - d + kHeight - axis.X() - axis.Y()) / std::sqrt(2.0);
    const double to_top = kHeight - axis.Y();
    std::printf("  rounding: radius %.12g, from the side %.12g, from the bevel %.12g, below the top %.12g\n",
                cylinder.Radius(), to_side, to_bevel, to_top);
    CHECK(std::abs(to_side - r) < kTolerance);
    CHECK(std::abs(to_bevel - r) < kTolerance);
    CHECK(std::abs(to_top - r) < kTolerance);
  }
  CHECK(rounding >= 1);
  CHECK(named_top >= 1);
}

void test_full_round() { check_full_round(4.0); }

// The bevel's size edited: the radius follows.
void test_full_round_follows_the_bevel() { check_full_round(6.0); }

// A block whose top narrows from 20 to 14 along its edges (its side at x 20
// leaning in along z): the radius would change along the edges.
void test_tapered_face_refused() {
  const ShapePtr body =
      prism({gp_Pnt(0, 0, 0), gp_Pnt(kWidth, 0, 0), gp_Pnt(kWidth - 6, 0, kLength), gp_Pnt(0, 0, kLength)},
            gp_Vec(0, kHeight, 0));
  const std::string side = edge_named(*body, gp_Pnt(0, kHeight, 0), gp_Pnt(0, kHeight, kLength));
  const std::string leaning = edge_named(*body, gp_Pnt(kWidth, kHeight, 0), gp_Pnt(kWidth - 6, kHeight, kLength));
  CHECK(!side.empty() && !leaning.empty());
  CHECK(throws_with([&] { fillet("F9", *body, {full_round_of({side, leaning})}); },
                    "the radius of a full round would change along the edges"));
  CHECK(throws_with([&] { fillet("F9", *body, {full_round_of({side, leaning})}); },
                    "the face between them narrows, or the faces beside it taper or twist"));
}

// Two edges that share no face: the top's edge at the side and the
// bottom's edge at x 20.
void test_edges_without_a_shared_face_refused() {
  const ShapePtr body = bevelled_block(4.0);
  const std::string side = edge_named(*body, gp_Pnt(0, kHeight, 0), gp_Pnt(0, kHeight, kLength));
  const std::string bottom = edge_named(*body, gp_Pnt(kWidth, 0, 0), gp_Pnt(kWidth, 0, kLength));
  CHECK(!side.empty() && !bottom.empty());
  CHECK(throws_with([&] { fillet("F9", *body, {full_round_of({side, bottom})}); },
                    "the edges of a full round share no face: pick the two edges of the face to round away"));
}

// Two edges of the top that meet at its corner; and the top itself.
void test_corner_and_face_refused() {
  const ShapePtr body = bevelled_block(4.0);
  const std::string side = edge_named(*body, gp_Pnt(0, kHeight, 0), gp_Pnt(0, kHeight, kLength));
  const std::string end = edge_named(*body, gp_Pnt(0, kHeight, 0), gp_Pnt(kWidth - 4, kHeight, 0));
  const std::string top = plane_named(*body, gp_Pnt(0, kHeight, 0), gp_Dir(0, 1, 0));
  CHECK(!side.empty() && !end.empty() && !top.empty());
  CHECK(throws_with([&] { fillet("F9", *body, {full_round_of({side, end})}); },
                    "the edges of a full round meet at a corner of the face"));
  FilletSet face;
  face.faces = {top};
  face.size = FilletSize::FullRound;
  CHECK(throws_with([&] { fillet("F9", *body, {face}); },
                    "a full round takes the two edges of the face it rounds away, not the face: pick the face's "
                    "two edges"));
  // What a person reads names no face or edge.
  try {
    fillet("F9", *body, {face});
  } catch (const std::exception& error) {
    const std::string said = error_message(error.what());
    CHECK(said.find("T:f") == std::string::npos);
  }
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_full_round) {
  test::guarded("test_full_round", test::test_full_round);
  test::guarded("test_full_round_follows_the_bevel", test::test_full_round_follows_the_bevel);
  test::guarded("test_tapered_face_refused", test::test_tapered_face_refused);
  test::guarded("test_edges_without_a_shared_face_refused", test::test_edges_without_a_shared_face_refused);
  test::guarded("test_corner_and_face_refused", test::test_corner_and_face_refused);
  if (test::failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", test::failures);
    return 1;
  }
  std::printf("all tests passed\n");
  return 0;
}
