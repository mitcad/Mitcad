// SPDX-License-Identifier: MIT
// Joins of a spiral stair's slabs and treads with helical stringers whose
// sides lie on the treads' cylinders (mitcad#400): OCCT's face merge of a
// ring of cylinder pieces cut through from one end to the other, with the
// port's patch 0680, and the union that joins the slabs and treads before
// the stringers.

#include <cmath>
#include <cstdio>
#include <string>
#include <utility>
#include <vector>

#include <BOPAlgo_PaveFiller.hxx>
#include <BOPDS_DS.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_List.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"
#include "mitcad/geometry/geometry.hpp"
#include "bundled_test.hpp"

namespace {

using namespace mitcad::geometry;
using test::kPi;

// The stair: the slabs inside its inner cylinder, the treads between the
// cylinders, each tread a step of 4.5 degrees and 170 mm above the one
// before, 250 mm thick as the slabs.
constexpr double kInner = 5100.0;
constexpr double kOuter = 6050.0;
constexpr double kStep = kPi / 40.0;
constexpr double kRise = 170.0;
constexpr double kThickness = 250.0;
constexpr double kPitch = kRise * 2.0 * kPi / kStep;

double volume(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

int count(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
  int n = 0;
  for (TopExp_Explorer ex(shape, type); ex.More(); ex.Next()) {
    ++n;
  }
  return n;
}

// The faces of the shape on a cylinder of the radius.
int faces_on_cylinder(const TopoDS_Shape& shape, double radius) {
  int n = 0;
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(ex.Current()));
    if (surface.GetType() == GeomAbs_Cylinder && std::abs(surface.Cylinder().Radius() - radius) < 1e-6) {
      ++n;
    }
  }
  return n;
}

TopoDS_Shape fused(const std::vector<TopoDS_Shape>& shapes) {
  BRepAlgoAPI_Fuse fuse;
  NCollection_List<TopoDS_Shape> arguments;
  NCollection_List<TopoDS_Shape> tools;
  arguments.Append(shapes.front());
  for (std::size_t i = 1; i < shapes.size(); ++i) {
    tools.Append(shapes[i]);
  }
  fuse.SetArguments(arguments);
  fuse.SetTools(tools);
  fuse.SetNonDestructive(true);
  fuse.SetRunParallel(true);
  fuse.Build();
  CHECK(fuse.IsDone() && !fuse.HasErrors());
  return fuse.Shape();
}

// OCCT's shapes, without Mitcad's: a slab whose rim is four quarters (a
// sketch's circle split by its lines), and a tread revolved over a step.
TopoDS_Shape quarter_slab(double z) {
  std::vector<TopoDS_Shape> quarters;
  for (int k = 0; k < 4; ++k) {
    gp_Ax2 axes(gp_Pnt(0, 0, z), gp_Dir(0, 0, 1), gp_Dir(std::cos(k * kPi / 2.0), std::sin(k * kPi / 2.0), 0));
    quarters.push_back(BRepPrimAPI_MakeCylinder(axes, kInner, kThickness, kPi / 2.0).Shape());
  }
  return fused(quarters);
}

TopoDS_Shape revolved_tread(double angle, double z) {
  const gp_Dir out(std::cos(angle), std::sin(angle), 0);
  const auto at = [&](double radius, double height) {
    return gp_Pnt(out.X() * radius, out.Y() * radius, height);
  };
  BRepBuilderAPI_MakePolygon section(at(kInner, z), at(kOuter, z), at(kOuter, z + kThickness),
                                     at(kInner, z + kThickness), true);
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(section.Wire(), true).Face();
  return BRepPrimAPI_MakeRevol(face, gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), kStep).Shape();
}

// OCCT's face merge after the union, as the booleans run it.
TopoDS_Shape merged(const TopoDS_Shape& shape) {
  ShapeUpgrade_UnifySameDomain unify(shape, true, true, false);
  unify.SetSafeInputMode(true);
  unify.Build();
  return unify.Shape();
}

void test_merge_of_a_ring_cut_through_from_end_to_end() {
  // Two slabs (z -80 to 170 and 90 to 340) and the first two treads on
  // their rim: the first from the bottom of the rim, the second up to its
  // top, one step on. The treads' inner sides cut the slabs' rim through
  // from end to end; what is left of it reaches from the second tread round
  // to the first, the whole period, without winding round the cylinder. A
  // third tread half a turn on, within the rim's height, is a hole in it.
  // Port patch 0680: the merge built the rim's pieces into a face with
  // seams it used once, left pieces under it and was invalid; the rim is
  // now one face without a seam (mitcad#400).
  const std::vector<TopoDS_Shape> parts{quarter_slab(-80.0), quarter_slab(-80.0 + kRise),
                                        revolved_tread(0.0, -80.0), revolved_tread(kStep, -80.0 + kRise),
                                        revolved_tread(kPi, 0.0)};
  const TopoDS_Shape stair = fused(parts);
  CHECK(BRepCheck_Analyzer(stair).IsValid());
  CHECK(faces_on_cylinder(stair, kInner) > 10);
  const TopoDS_Shape result = merged(stair);
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(count(result, TopAbs_SOLID) == 1);
  CHECK(faces_on_cylinder(result, kInner) == 1);
  CHECK_NEAR_TOL(volume(result), volume(stair), 1e-9);
}

void test_merge_of_a_whole_ring() {
  // The two slabs alone: their rims' pieces make a ring round the cylinder,
  // merged into one face with a seam as before 0680.
  const TopoDS_Shape slabs = fused({quarter_slab(-80.0), quarter_slab(-80.0 + kRise)});
  const TopoDS_Shape result = merged(slabs);
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(faces_on_cylinder(result, kInner) == 1);
  CHECK_NEAR_TOL(volume(result), volume(slabs), 1e-9);
}

// Mitcad's shapes, named as features name them.

ShapePtr extruded(const std::string& feature, const Region& region, double from) {
  ExtrudeSpec spec;
  spec.feature = feature;
  spec.regions = {region};
  spec.start = from;
  spec.end = from + kThickness;
  return extrude(spec);
}

Segment arc(double radius, double from, double to) {
  Segment s;
  s.kind = SegmentKind::Arc;
  s.center = gp_Pnt2d(0, 0);
  s.radius = radius;
  s.start_angle = from;
  s.end_angle = to;
  s.start = gp_Pnt2d(radius * std::cos(from), radius * std::sin(from));
  s.end = gp_Pnt2d(radius * std::cos(to), radius * std::sin(to));
  return s;
}

Segment line(const gp_Pnt2d& a, const gp_Pnt2d& b) {
  Segment s;
  s.kind = SegmentKind::Line;
  s.start = a;
  s.end = b;
  return s;
}

// A slab whose rim is the 80 arcs of a sketch's circle split where the
// treads' lines cross it.
ShapePtr slab(const std::string& feature, double z) {
  Region region;
  region.name = "r{" + feature + "}";
  Loop loop;
  for (int i = 0; i < 80; ++i) {
    Segment s = arc(kInner, i * kStep, (i + 1) * kStep);
    s.name = "c3[c" + std::to_string(i) + ",c" + std::to_string(i + 1) + "]";
    loop.segments.push_back(s);
  }
  region.loops.push_back(loop);
  return extruded(feature, region, z);
}

// A tread: the annular cell between the cylinders over a step from `angle`.
ShapePtr tread(const std::string& feature, double angle, double z) {
  Segment inner = arc(kInner, angle, angle + kStep);
  Segment outer = arc(kOuter, angle, angle + kStep);
  Segment first = line(inner.start, outer.start);
  Segment second = line(outer.end, inner.end);
  inner.name = "c3[c8,c9]";
  outer.name = "c5[c8,c9]";
  first.name = "c8[c3,c5]";
  second.name = "c9[c3,c5]";
  std::swap(inner.start, inner.end);
  Region region;
  region.name = "r{" + feature + "}";
  region.loops.push_back(Loop{{first, outer, second, inner}});
  return extruded(feature, region, z);
}

// A stringer: a rectangle in the plane through the axis, from radius `from`
// to `to` and 550 deep under the treads' line, half a turn up the stair.
ShapePtr stringer(const std::string& feature, double from, double to) {
  HelixSweepSpec spec;
  spec.feature = feature;
  spec.frame.x_axis = gp_Dir(1, 0, 0);
  spec.frame.y_axis = gp_Dir(0, 0, 1);
  Region profile = rectangle_region(from, -550.0, to - from, 549.9);
  profile.name = "r{c6[c9,c7],c7[c6,c8],c8[c7,c9],c9[c8,c6]}";
  for (std::size_t i = 0; i < 4; ++i) {
    profile.loops[0].segments[i].name = "c" + std::to_string(6 + i);
  }
  spec.regions = {profile};
  spec.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
  spec.pitch = kPitch;
  spec.revolutions = 0.5;
  return helix_sweep(spec);
}

bool has_face_named(const Shape& shape, const std::string& prefix) {
  for (int i = 0; i < shape.face_count(); ++i) {
    for (const std::string& name : shape.face_names(i)) {
      if (name.rfind(prefix, 0) == 0) {
        return true;
      }
    }
  }
  return false;
}

std::vector<TopoDS_Shape> occt_of(const std::vector<ShapePtr>& shapes) {
  std::vector<TopoDS_Shape> out;
  for (const ShapePtr& shape : shapes) {
    out.push_back(shape->occt());
  }
  return out;
}

// The face pairs OCCT's boolean intersects among the shapes.
int face_pairs(const std::vector<TopoDS_Shape>& shapes) {
  BOPAlgo_PaveFiller filler;
  NCollection_List<TopoDS_Shape> arguments;
  for (const TopoDS_Shape& shape : shapes) {
    arguments.Append(shape);
  }
  filler.SetArguments(arguments);
  filler.SetNonDestructive(true);
  filler.SetRunParallel(true);
  filler.Perform();
  CHECK(!filler.HasErrors());
  return filler.PDS()->InterfFF().Length();
}

void test_stair_joined_with_its_stringers() {
  // Seven slabs, eight treads on their rim from its bottom up past its top,
  // and two stringers half a turn up the stair: the inner one's inner side
  // on the slabs' and treads' inner cylinder, the outer one's outer side on
  // the treads' outer cylinder (mitcad#400).
  std::vector<ShapePtr> elementary;
  for (int k = 0; k < 7; ++k) {
    elementary.push_back(slab("F" + std::to_string(2 + k), -80.0 + kRise * k));
  }
  for (int i = 0; i < 8; ++i) {
    elementary.push_back(tread("F" + std::to_string(10 + i), i * kStep, -80.0 + kRise * i));
  }
  const std::vector<ShapePtr> stringers{stringer("F43", kInner, kInner + 150.0),
                                        stringer("F45", kOuter - 150.0, kOuter)};
  std::vector<ShapePtr> all = elementary;
  all.insert(all.end(), stringers.begin(), stringers.end());
  std::vector<const Shape*> inputs;
  for (const ShapePtr& shape : all) {
    inputs.push_back(shape.get());
  }
  const ShapePtr stair = unite(inputs);
  CHECK(BRepCheck_Analyzer(stair->occt()).IsValid());
  CHECK(count(stair->occt(), TopAbs_SOLID) == 1);
  CHECK_NEAR_TOL(volume(stair->occt()), volume(fused(occt_of(all))), 1e-9);
  // The rim's 560 pieces merged (with 0680; a thousand faces before): one
  // face on the inner cylinder beside the stringer's, the treads' outer
  // sides, their ends and tops, the stringers' faces.
  CHECK(count(stair->occt(), TopAbs_FACE) <= 60);
  CHECK(has_face_named(*stair, "F43:side("));
  CHECK(has_face_named(*stair, "F45:side("));
  CHECK(has_face_named(*stair, "F2:"));
  CHECK(has_face_named(*stair, "F17:"));

  // The work the order saves: the stringers meet the faces of the slabs'
  // and treads' union (unite joins them first) instead of every piece of
  // the rim, a free-form face against a cylinder it lies on being the
  // costliest pair to intersect.
  std::vector<const Shape*> first;
  for (const ShapePtr& shape : elementary) {
    first.push_back(shape.get());
  }
  const ShapePtr steps = unite(first);
  std::vector<TopoDS_Shape> staged{steps->occt()};
  for (const ShapePtr& shape : stringers) {
    staged.push_back(shape->occt());
  }
  const int at_once = face_pairs(occt_of(all));
  const int after = face_pairs(staged);
  std::printf("face pairs: %d at once, %d with the steps joined first\n", at_once, after);
  CHECK(after * 5 < at_once);
}

} // namespace

MITCAD_BUNDLED_TEST(helical_joins) {
  test::guarded("merge of a ring cut through from end to end", test_merge_of_a_ring_cut_through_from_end_to_end);
  test::guarded("merge of a whole ring", test_merge_of_a_whole_ring);
  test::guarded("stair joined with its stringers", test_stair_joined_with_its_stringers);
  if (test::failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", test::failures);
    return 1;
  }
  std::printf("helical joins: all checks passed\n");
  return 0;
}
