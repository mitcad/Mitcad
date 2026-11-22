// SPDX-License-Identifier: MIT
// Port patch 2140 (mitcad#2140, GitHub mitcad/Mitcad#517): the offset of a
// crease that flattens to tangency at an end where its faces go on
// smoothly into a further face keeps the branch along the crease alone.
//
// A Y tube of OCCT's primitives: a cylinder of radius 10 up the z axis to
// z = 25, and two quarter tori of radius 30 (tube radius 10) turning from
// its top towards -x and +x, fused. The arms meet along a concave crease in
// the plane x = 0 from (0, -10, 25) over (0, 0, 51.46) to (0, 10, 25) that
// fades to tangency at both ends, where the tori run on smoothly into the
// cylinder. Shelled 2 mm inwards with its three ends open (Intersection
// join), the tori's offsets cross along the crease's offset and its
// continuation below z = 25 and touch along the circle where both meet the
// cylinder's offset; patch 1190 joined the branch to its continuation, and
// the thick solid raised an exception. Expected: one valid solid, the wall
// 2 mm thick (points in the trunk, the arms and under the crotch), the
// volume that of the tube less the same tube of radius 8 (the inward offset
// of each tube is the tube of radius 8, and with sharp joins the cavity is
// their union).

#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <NCollection_List.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Ax2.hxx>

#include "port_test.hpp"

namespace {

constexpr double kRadius = 10.0; // the tube
constexpr double kLine   = 25.0; // the straight part
constexpr double kBend   = 30.0; // the arc
constexpr double kWall   = 2.0;
constexpr double kPi     = 3.14159265358979323846;

// The first solid of a shape (null if none).
TopoDS_Shape solid_of(const TopoDS_Shape& shape) {
  if (shape.IsNull()) {
    return TopoDS_Shape();
  }
  TopExp_Explorer solids(shape, TopAbs_SOLID);
  return solids.More() ? solids.Current() : TopoDS_Shape();
}

int solids_in(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer solids(shape, TopAbs_SOLID); solids.More(); solids.Next()) {
    ++n;
  }
  return n;
}

// The Y tube of tube radius r: the cylinder and both arms, fused.
TopoDS_Shape y_tube(double r) {
  const TopoDS_Shape trunk = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp::DZ(), gp::DX()), r, kLine).Shape();
  // Each arm's axis is square to the plane y = 0, its start at the
  // cylinder's top, turning up from there (the x direction to the start,
  // axis x x direction = +z).
  const TopoDS_Shape left =
      BRepPrimAPI_MakeTorus(gp_Ax2(gp_Pnt(-kBend, 0, kLine), gp_Dir(0, -1, 0), gp_Dir(1, 0, 0)), kBend, r, kPi / 2)
          .Shape();
  const TopoDS_Shape right =
      BRepPrimAPI_MakeTorus(gp_Ax2(gp_Pnt(kBend, 0, kLine), gp_Dir(0, 1, 0), gp_Dir(-1, 0, 0)), kBend, r, kPi / 2)
          .Shape();
  BRepAlgoAPI_Fuse fuse;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(trunk);
  NCollection_List<TopoDS_Shape> tools;
  tools.Append(left);
  tools.Append(right);
  fuse.SetArguments(arguments);
  fuse.SetTools(tools);
  try {
    OCC_CATCH_SIGNALS
    fuse.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "fuse of the Y tube (r = %g): %s\n", r, failure.what());
    return TopoDS_Shape();
  }
  return fuse.IsDone() ? solid_of(fuse.Shape()) : TopoDS_Shape();
}

// The planar faces of the body whose centre lies at the point (the three
// open ends).
TopoDS_Shape planar_face_at(const TopoDS_Shape& body, const gp_Pnt& centre) {
  for (TopExp_Explorer faces(body, TopAbs_FACE); faces.More(); faces.Next()) {
    if (BRepAdaptor_Surface(TopoDS::Face(faces.Current())).GetType() != GeomAbs_Plane) {
      continue;
    }
    GProp_GProps props;
    BRepGProp::SurfaceProperties(faces.Current(), props);
    if (props.CentreOfMass().Distance(centre) < 1e-6) {
      return faces.Current();
    }
  }
  return TopoDS_Shape();
}

bool inside(const TopoDS_Shape& solid, const gp_Pnt& p) {
  BRepClass3d_SolidClassifier classifier(solid, p, 1e-7);
  return classifier.State() == TopAbs_IN;
}

bool outside(const TopoDS_Shape& solid, const gp_Pnt& p) {
  BRepClass3d_SolidClassifier classifier(solid, p, 1e-7);
  return classifier.State() == TopAbs_OUT;
}

// The wall between the radii 8 and 10 about a point of the path, across
// the direction `d` (square to the path): in the wall at 9 from the path,
// in the cavity at 7.5, on either side.
void check_wall_across(const TopoDS_Shape& shell, const gp_Pnt& on_path, const gp_Dir& d, const char* where) {
  for (const double side : {1.0, -1.0}) {
    const gp_Vec v = gp_Vec(d) * side;
    const bool wall = inside(shell, on_path.Translated(v * (kRadius - 0.5 * kWall)));
    const bool cavity = outside(shell, on_path.Translated(v * (kRadius - kWall - 0.5)));
    if (!wall || !cavity) {
      std::fprintf(stderr, "%s: wall %d, cavity %d on side %g\n", where, wall, cavity, side);
    }
    CHECK(wall);
    CHECK(cavity);
  }
}

void test_y_tube_shelled_inwards_with_its_ends_open() {
  const TopoDS_Shape body = y_tube(kRadius);
  const TopoDS_Shape cavity = y_tube(kRadius - kWall);
  CHECK(port::valid(body));
  CHECK(port::valid(cavity));
  if (body.IsNull() || cavity.IsNull()) {
    return;
  }
  // The crease: edges in the plane x = 0 above the cylinder.
  const std::vector<TopoDS_Edge> crease = port::edges_where(
      body, [](const gp_Pnt& p) { return std::abs(p.X()) < 1e-6 && p.Z() > kLine + 1.0; });
  CHECK(!crease.empty());
  NCollection_List<TopoDS_Shape> open;
  for (const gp_Pnt& centre :
       {gp_Pnt(0, 0, 0), gp_Pnt(-kBend, 0, kLine + kBend), gp_Pnt(kBend, 0, kLine + kBend)}) {
    const TopoDS_Shape end = planar_face_at(body, centre);
    CHECK(!end.IsNull());
    if (!end.IsNull()) {
      open.Append(end);
    }
  }
  if (open.Extent() != 3) {
    return;
  }
  BRepOffsetAPI_MakeThickSolid maker;
  bool                         threw = false;
  try {
    OCC_CATCH_SIGNALS
    maker.MakeThickSolidByJoin(body, open, -kWall, 1e-6, BRepOffset_Skin, false, false, GeomAbs_Intersection,
                               false);
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "shell of the Y tube raised: '%s'\n", failure.what());
    threw = true;
  }
  CHECK(!threw);
  if (threw) {
    return;
  }
  if (!maker.IsDone()) {
    std::fprintf(stderr, "shell of the Y tube not done: error %d\n", static_cast<int>(maker.MakeOffset().Error()));
  }
  CHECK(maker.IsDone());
  if (!maker.IsDone()) {
    return;
  }
  const TopoDS_Shape shell = maker.Shape();
  CHECK(port::valid(shell));
  CHECK(solids_in(shell) == 1);
  const TopoDS_Shape wall = solid_of(shell);
  if (wall.IsNull()) {
    return;
  }
  // (Valid is not enough: edges whose curves on a face collapsed leave a
  // valid shell of tolerances in millimetres.)
  if (port::max_tolerance(wall) > 1e-3) {
    std::fprintf(stderr, "the shell's largest tolerance %g\n", port::max_tolerance(wall));
  }
  CHECK(port::max_tolerance(wall) <= 1e-3);
  // The trunk, half way up.
  check_wall_across(wall, gp_Pnt(0, 0, 0.5 * kLine), gp::DX(), "trunk across x");
  check_wall_across(wall, gp_Pnt(0, 0, 0.5 * kLine), gp::DY(), "trunk across y");
  // Each arm 60 degrees round: across in its plane and square to it.
  for (const double sx : {-1.0, 1.0}) {
    const double a = kPi / 3;
    const gp_Pnt on_path(sx * (kBend - kBend * std::cos(a)), 0, kLine + kBend * std::sin(a));
    check_wall_across(wall, on_path, gp_Dir(-sx * std::cos(a), 0, std::sin(a)), "arm across its bend");
    check_wall_across(wall, on_path, gp::DY(), "arm across y");
  }
  // Under the crotch's highest point (z = 25 + sqrt(40^2 - 30^2)) the wall
  // reaches down to where the arms' offsets meet (z = 25 + sqrt(38^2 -
  // 30^2)).
  const double outer = kLine + std::sqrt(40.0 * 40.0 - kBend * kBend);
  const double inner = kLine + std::sqrt(38.0 * 38.0 - kBend * kBend);
  CHECK(outside(wall, gp_Pnt(0, 0, outer + 0.05)));
  CHECK(inside(wall, gp_Pnt(0, 0, 0.5 * (outer + inner))));
  CHECK(outside(wall, gp_Pnt(0, 0, inner - 0.05)));
  // The three ends are open.
  CHECK(outside(wall, gp_Pnt(0, 0, 0.5)));
  CHECK(outside(wall, gp_Pnt(-kBend + 0.5, 0, kLine + kBend)));
  CHECK(outside(wall, gp_Pnt(kBend - 0.5, 0, kLine + kBend)));
  const double expected = port::volume_of(body) - port::volume_of(cavity);
  const double got = port::volume_of(wall);
  if (std::abs(got - expected) > 1e-5 * expected) {
    std::fprintf(stderr, "wall volume %.9g, expected %.9g\n", got, expected);
  }
  CHECK_NEAR_TOL(got, expected, 1e-5);
}

} // namespace

MITCAD_PORT_TEST("2140", test_y_tube_shelled_inwards_with_its_ends_open);
