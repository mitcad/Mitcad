// SPDX-License-Identifier: MIT
// Which triangles of a shown body's faces turn away from the eye
// (view/PickFacing.hpp, mitcad#2062): the view's pick places leave them
// untried. A box seen from above and from a corner, turned by a placement,
// with its triangulations wound the other way (mitcad#319), and the shapes
// whose facing is not known for certain: an inside-out box, an open shell,
// a perspective eye inside the box.
#include "UnitTests.hpp"
#include "view/PickFacing.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdio>
#include <optional>
#include <vector>

#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Poly_Triangulation.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shell.hxx>
#include <gp_Ax1.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "pick_facing.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::PickEye;
using mitcad::PickFacing;
using mitcad::pickFacing;

// A 10 mm box from the origin, meshed as the view meshes it.
TopoDS_Shape meshedBox() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(10.0, 10.0, 10.0).Shape();
  BRepMesh_IncrementalMesh(box, 0.1, false, 0.5);
  return box;
}

// The middle of a face's nodes, moved by the placement.
gp_Pnt middleOf(const TopoDS_Face& face, const gp_Trsf& placement) {
  TopLoc_Location location;
  const occ::handle<Poly_Triangulation> mesh = BRep_Tool::Triangulation(face, location);
  gp_XYZ sum(0.0, 0.0, 0.0);
  for (int n = 1; n <= mesh->NbNodes(); ++n) {
    sum += mesh->Node(n).Transformed(location.Transformation()).XYZ();
  }
  return gp_Pnt(sum / mesh->NbNodes()).Transformed(placement);
}

// Whether the face at a box side (its middle near `side`) is known, and
// how many of its triangles may face the eye: all, none.
enum class Facing { Unknown, All, None, Some };

Facing facingAt(const TopoDS_Shape& shape, const PickFacing& facing, const gp_Trsf& placement,
                const gp_Pnt& side) {
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    if (middleOf(face, placement).Distance(side) > 1e-6) {
      continue;
    }
    const std::vector<bool>* front = facing.Seek(face);
    if (front == nullptr) {
      return Facing::Unknown;
    }
    const auto towards = std::count(front->begin(), front->end(), true);
    return towards == 0 ? Facing::None
                        : (towards == static_cast<std::ptrdiff_t>(front->size()) ? Facing::All : Facing::Some);
  }
  return Facing::Unknown;
}

// Each face's triangulation wound the other way, its nodes kept.
void windBackwards(const TopoDS_Shape& shape) {
  BRep_Builder builder;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    TopLoc_Location location;
    const occ::handle<Poly_Triangulation> mesh = BRep_Tool::Triangulation(face, location);
    const occ::handle<Poly_Triangulation> backwards =
        new Poly_Triangulation(mesh->NbNodes(), mesh->NbTriangles(), mesh->HasUVNodes());
    for (int n = 1; n <= mesh->NbNodes(); ++n) {
      backwards->SetNode(n, mesh->Node(n));
      if (mesh->HasUVNodes()) {
        backwards->SetUVNode(n, mesh->UVNode(n));
      }
    }
    for (int t = 1; t <= mesh->NbTriangles(); ++t) {
      int a = 0;
      int b = 0;
      int c = 0;
      mesh->Triangle(t).Get(a, b, c);
      backwards->SetTriangle(t, Poly_Triangle(a, c, b));
    }
    builder.UpdateFace(face, backwards);
  }
}

const PickEye kFromAbove{gp_Pnt(5.0, 5.0, 100.0), gp_Dir(0.0, 0.0, -1.0), false};
const PickEye kFromCorner{gp_Pnt(100.0, 100.0, 100.0), gp_Dir(-1.0, -1.0, -1.0), false};

} // namespace

int pickFacingTests() {
  const gp_Trsf none;
  {
    // From above: the top all towards the eye, the bottom all away, the
    // sides edge-on still tried (a pick within their width can find them).
    const TopoDS_Shape box = meshedBox();
    const PickFacing facing = pickFacing(box, none, kFromAbove);
    CHECK(facing.Extent() == 6);
    CHECK(facingAt(box, facing, none, gp_Pnt(5, 5, 10)) == Facing::All);
    CHECK(facingAt(box, facing, none, gp_Pnt(5, 5, 0)) == Facing::None);
    CHECK(facingAt(box, facing, none, gp_Pnt(0, 5, 5)) == Facing::All);
    CHECK(facingAt(box, facing, none, gp_Pnt(5, 10, 5)) == Facing::All);
  }
  {
    // From a corner: its three faces towards the eye, the other three away;
    // the same in perspective from outside the box.
    const TopoDS_Shape box = meshedBox();
    for (const bool perspective : {false, true}) {
      PickEye eye = kFromCorner;
      eye.perspective = perspective;
      const PickFacing facing = pickFacing(box, none, eye);
      CHECK(facingAt(box, facing, none, gp_Pnt(10, 5, 5)) == Facing::All);
      CHECK(facingAt(box, facing, none, gp_Pnt(5, 10, 5)) == Facing::All);
      CHECK(facingAt(box, facing, none, gp_Pnt(5, 5, 10)) == Facing::All);
      CHECK(facingAt(box, facing, none, gp_Pnt(0, 5, 5)) == Facing::None);
      CHECK(facingAt(box, facing, none, gp_Pnt(5, 0, 5)) == Facing::None);
      CHECK(facingAt(box, facing, none, gp_Pnt(5, 5, 0)) == Facing::None);
    }
  }
  {
    // A placement turning the box over: its bottom faces the eye above.
    const TopoDS_Shape box = meshedBox();
    gp_Trsf over;
    over.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0)), std::acos(-1.0));
    const PickFacing facing = pickFacing(box, over, kFromAbove);
    // The box's bottom (z = 0) stays at z = 0, now its top; its top at -10.
    CHECK(facingAt(box, facing, over, gp_Pnt(5, -5, 0)) == Facing::All);
    CHECK(facingAt(box, facing, over, gp_Pnt(5, -5, -10)) == Facing::None);
  }
  {
    // Triangulations wound the other way (mitcad#319): the surfaces decide.
    const TopoDS_Shape box = meshedBox();
    windBackwards(box);
    const PickFacing facing = pickFacing(box, none, kFromAbove);
    CHECK(facing.Extent() == 6);
    CHECK(facingAt(box, facing, none, gp_Pnt(5, 5, 10)) == Facing::All);
    CHECK(facingAt(box, facing, none, gp_Pnt(5, 5, 0)) == Facing::None);
  }
  {
    // Not known for certain, so tried everywhere: an inside-out box, an
    // open shell (the box without its top), a perspective eye inside.
    const TopoDS_Shape box = meshedBox();
    CHECK(pickFacing(box.Reversed(), none, kFromAbove).IsEmpty());
    BRep_Builder builder;
    TopoDS_Shell open;
    builder.MakeShell(open);
    for (TopExp_Explorer it(box, TopAbs_FACE); it.More(); it.Next()) {
      if (middleOf(TopoDS::Face(it.Current()), none).Distance(gp_Pnt(5, 5, 10)) > 1e-6) {
        builder.Add(open, it.Current());
      }
    }
    CHECK(pickFacing(open, none, kFromAbove).IsEmpty());
    const PickEye inside{gp_Pnt(5, 5, 5), gp_Dir(0, 0, -1), true};
    CHECK(pickFacing(box, none, inside).IsEmpty());
    // An orthographic eye has no place: the box's facing stays known.
    const PickEye through{gp_Pnt(5, 5, 5), gp_Dir(0, 0, -1), false};
    CHECK(pickFacing(box, none, through).Extent() == 6);
  }
  return failures;
}
MITCAD_UNIT_TEST(pickFacingTests);
