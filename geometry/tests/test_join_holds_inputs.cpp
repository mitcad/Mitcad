// SPDX-License-Identifier: MIT
// A union holds every shape it joins (mitcad#2053, GitHub mitcad/Mitcad#463):
// a point inside each input solid lies in or on the union, checked on every
// union the union operation would return, warned by OCCT or not; a union
// that misses one is mended or fails, and is never returned.
//
// The check itself (detail::points_inside, detail::missing_inputs) on
// candidates that miss inputs and on unions that hold them all: boxes that
// meet only along edges, a thin plate on a box, a rod and a ring (a
// torus). No synthetic layout makes OCCT's union drop a shape, so the check
// is tried on such candidates directly. The shapes that lie wholly inside
// others (detail::contained_inputs, detail::lies_inside), which a wrong
// union leaves out.
//
// The reported design (`github-463/` under MITCAD_MITCAD_REPORTED, a STEP
// file in a zip archive, never committed): a hub with 30 blades, triangles,
// rods and a ring, joined as the reporter's Combine joined them, the hub
// first. The Join kept the hub and the six flat blades on it and dropped
// the 24 other bodies without an error. Six of the bodies are each a rod
// with three thin fins, two of which touch at their tips (an edge on four
// faces), and five more are each the bare rod of one of them, lying wholly
// inside it: OCCT's exact union of such a pair gives nothing of either
// (its common is empty, its cut the whole rod, its general fuse loses a
// piece of the rod), and the union of all lost them and everything they
// touch. The classifier puts every point of each rod inside its finned
// rod, so the union leaves the rods out; OCCT's exact union of the other
// 26 holds every body. Without port patch 2170 it was not valid: each
// finned rod's side face (its cylinder, without a seam) passes one vertex
// at u = 0 and at u = 2 pi, an inner wire touching it there, and OCCT's
// union of such a rod with a body that trims that face (the ring, a
// triangle) closed loops at the vertex's other point, a period away,
// leaving faces it could not orient, with open wires (mitcad#2170). The
// test checks that the union of all is one valid body holding every body
// (the classifier's points, and the common volume of each body), and,
// should it fail, that the message does not call an
// invalid union empty. Skipped without the variable or the folder.
//
// Every body of the reported design, the five bare rods too, is checked by
// the volume it has in common with the union, both ways (mitcad#2217):
// short by at most 1e-6 relative (no material lost), over by at most
// kExcessLimit (see there). The common of a bare rod with a body holding it
// whose side face lies on the rod's cylinder, built from scratch (a rod
// with fins, its faces merged or not, alone or in a union), is checked the
// same way: OCCT's common of a reported bare rod with its finned rod came
// out empty before port patch 2170 (mitcad#2053).
//
// test_join_holds_inputs <work folder>: the archive's STEP file is unpacked
// there.

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iterator>
#include <memory>
#include <string>
#include <vector>

#include <zlib.h>

#include <BRepAlgoAPI_BooleanOperation.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_List.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

#include "../src/union_groups.hpp"
#include "check.hpp"
#include "mitcad/geometry/import.hpp"
#include "mitcad/geometry/pattern.hpp"
#include "mitcad/geometry/query.hpp"
#include "mitcad/io/step.hpp"

namespace test {
namespace {

using mitcad::geometry::Shape;
using mitcad::geometry::ShapePtr;
namespace detail = mitcad::geometry::detail;

std::string env(const char* name) {
#ifdef _MSC_VER
  char* value = nullptr;
  std::size_t length = 0;
  std::string out;
  if (_dupenv_s(&value, &length, name) == 0 && value != nullptr) {
    out = value;
    std::free(value);
  }
  return out;
#else
  const char* value = std::getenv(name);
  return value != nullptr ? std::string(value) : std::string();
#endif
}

std::string work_folder;

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

// The volume integrated to a relative error of 1e-9: with the default
// integration, the reported ring's common with the union (the ring in
// pieces) came out 1.2e-5 larger than the ring, more than the 1e-6 a
// body's volume in the union is checked to.
double accurate_volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

int solid_count(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer it(shape, TopAbs_SOLID); it.More(); it.Next()) {
    ++n;
  }
  return n;
}

// OCCT's boolean (common or cut) of `argument` with `tool`, the inputs left
// as they were: whether it was done, whether its result is valid, the
// result and its volume (integrated to 1e-9).
struct Measured {
  bool done = false;
  bool valid = false;
  TopoDS_Shape shape;
  double volume = 0.0;
};

template <typename Operation> Measured measured(const TopoDS_Shape& argument, const TopoDS_Shape& tool) {
  Operation operation;
  NCollection_List<TopoDS_Shape> arguments;
  NCollection_List<TopoDS_Shape> tools;
  arguments.Append(argument);
  tools.Append(tool);
  operation.SetArguments(arguments);
  operation.SetTools(tools);
  operation.SetNonDestructive(true);
  operation.Build();
  Measured out;
  out.done = operation.IsDone();
  if (out.done) {
    out.shape = operation.Shape();
    out.valid = BRepCheck_Analyzer(out.shape).IsValid();
    out.volume = accurate_volume_of(out.shape);
  }
  return out;
}

// What a common is made of where it measures more than the body: its
// faces, how many of them lie on one of the body's own surfaces (the
// others come from the union), the largest tolerance of its edges and the
// length of its edges with a tolerance above 1e-7 mm; and the excess as a
// layer over those edges seen from the shape's middle (the volume
// integration's reference point): an excess that is the gap between the
// faces' boundaries along such edges, not material, gives a layer of the
// order of their tolerances.
void print_excess(const TopoDS_Shape& body, const TopoDS_Shape& common, double excess) {
  std::vector<const Geom_Surface*> own;
  for (TopExp_Explorer it(body, TopAbs_FACE); it.More(); it.Next()) {
    own.push_back(BRep_Tool::Surface(TopoDS::Face(it.Current())).get());
  }
  int faces = 0;
  int on_own = 0;
  for (TopExp_Explorer it(common, TopAbs_FACE); it.More(); it.Next()) {
    ++faces;
    const Geom_Surface* surface = BRep_Tool::Surface(TopoDS::Face(it.Current())).get();
    on_own += std::find(own.begin(), own.end(), surface) != own.end() ? 1 : 0;
  }
  double largest = 0.0;
  double loose_length = 0.0;
  for (TopExp_Explorer it(common, TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    const double tolerance = BRep_Tool::Tolerance(edge);
    largest = std::max(largest, tolerance);
    if (tolerance > 1e-7) {
      GProp_GProps props;
      BRepGProp::LinearProperties(edge, props);
      loose_length += props.Mass();
    }
  }
  Bnd_Box box;
  BRepBndLib::Add(common, box);
  double x0 = 0.0;
  double y0 = 0.0;
  double z0 = 0.0;
  double x1 = 0.0;
  double y1 = 0.0;
  double z1 = 0.0;
  box.Get(x0, y0, z0, x1, y1, z1);
  const double reach = 0.5 * std::sqrt((x1 - x0) * (x1 - x0) + (y1 - y0) * (y1 - y0) + (z1 - z0) * (z1 - z0));
  const double layer = loose_length > 0.0 && reach > 0.0 ? 3.0 * excess / (reach * loose_length) : 0.0;
  std::printf("    its common: %d faces, %d on the body's own surfaces; edge tolerance up to %.3g mm, %.3f mm "
              "of edges above 1e-7 mm; the excess as a layer over them %.3g mm\n",
              faces, on_own, largest, loose_length, layer);
}

ShapePtr body(const TopoDS_Shape& shape, int index) {
  return mitcad::geometry::import_body(shape, "F" + std::to_string(index + 1), 0);
}

ShapePtr box(int index, double x, double y, double z, double dx, double dy, double dz) {
  return body(BRepPrimAPI_MakeBox(gp_Pnt(x, y, z), dx, dy, dz).Shape(), index);
}

// The solids of the shapes listed, as one compound: a candidate union that
// holds only those.
TopoDS_Shape candidate(const std::vector<ShapePtr>& shapes, const std::vector<std::size_t>& kept) {
  BRep_Builder builder;
  TopoDS_Compound compound;
  builder.MakeCompound(compound);
  for (const std::size_t i : kept) {
    for (TopExp_Explorer it(shapes[i]->occt(), TopAbs_SOLID); it.More(); it.Next()) {
      builder.Add(compound, it.Current());
    }
  }
  return compound;
}

std::vector<const Shape*> pointers(const std::vector<ShapePtr>& shapes) {
  std::vector<const Shape*> out;
  for (const ShapePtr& shape : shapes) {
    out.push_back(shape.get());
  }
  return out;
}

// Whether each input's volume lies in the union, by OCCT's classifier
// (its booleans got the reported design's coincident rods wrong: an empty
// common, a general fuse that lost a piece): the input lies inside the
// union (detail::lies_inside: its inside points, vertices, points along
// its edges and across its faces), and of up to 500 points inside it,
// drawn over its box (a fixed sequence, at most 20 000 draws: a thin
// rod along a diagonal fills a hundredth of its box), none lies outside
// every solid of the union (points within 1e-3 mm of a boundary count as
// either). Each solid is classified on its own: a union of solids that
// touch (the reported design's gives eight) has faces two solids bound,
// which a classifier of the whole compound counts as one crossing.
void check_inputs_held(const Shape& joined, const std::vector<const Shape*>& inputs) {
  std::vector<std::unique_ptr<BRepClass3d_SolidClassifier>> in_union;
  for (TopExp_Explorer it(joined.occt(), TopAbs_SOLID); it.More(); it.Next()) {
    in_union.push_back(std::make_unique<BRepClass3d_SolidClassifier>(it.Current()));
  }
  const auto outside_union = [&in_union](const gp_Pnt& point) {
    for (const std::unique_ptr<BRepClass3d_SolidClassifier>& solid : in_union) {
      solid->Perform(point, 1e-3);
      if (solid->State() != TopAbs_OUT) {
        return false;
      }
    }
    return true;
  };
  int short_of = 0;
  for (std::size_t i = 0; i < inputs.size(); ++i) {
    const bool inside = detail::lies_inside(*inputs[i], joined);
    BRepClass3d_SolidClassifier in_input(inputs[i]->occt());
    Bnd_Box box;
    BRepBndLib::AddOptimal(inputs[i]->occt(), box, false, false);
    double x0 = 0.0;
    double y0 = 0.0;
    double z0 = 0.0;
    double x1 = 0.0;
    double y1 = 0.0;
    double z1 = 0.0;
    box.Get(x0, y0, z0, x1, y1, z1);
    unsigned state = 12345u;
    const auto next = [&state] {
      state = state * 1664525u + 1013904223u;
      return static_cast<double>(state >> 8) / 16777216.0;
    };
    int outside = 0;
    int sampled = 0;
    for (int k = 0; k < 20000 && sampled < 500; ++k) {
      const gp_Pnt point(x0 + (x1 - x0) * next(), y0 + (y1 - y0) * next(), z0 + (z1 - z0) * next());
      in_input.Perform(point, 1e-3);
      if (in_input.State() != TopAbs_IN) {
        continue;
      }
      ++sampled;
      outside += outside_union(point) ? 1 : 0;
    }
    if (!inside || outside != 0 || sampled == 0) {
      std::fprintf(stderr, "  input %zu: inside %d, %d of %d sampled points outside the union\n", i, inside ? 1 : 0,
                   outside, sampled);
      ++short_of;
    }
  }
  CHECK(short_of == 0);
}

void test_points_inside_solids() {
  // A box, a thin plate on it, a rod and a ring apart: points found in
  // each, inside it.
  std::vector<ShapePtr> shapes;
  shapes.push_back(box(0, 0, 0, 0, 10, 10, 10));
  shapes.push_back(box(1, 0, 0, 10, 10, 10, 0.01));
  shapes.push_back(body(BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(30, 0, 0), gp_Dir(1, 0, 0)), 1.5, 170).Shape(), 2));
  shapes.push_back(body(BRepPrimAPI_MakeTorus(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 211.5, 2.25).Shape(), 3));
  const std::vector<detail::InsidePoints> points = detail::points_inside(pointers(shapes));
  CHECK(points.size() == shapes.size());
  for (const detail::InsidePoints& solid : points) {
    CHECK(!solid.points.empty());
    BRepClass3d_SolidClassifier classifier(shapes[solid.shape]->occt());
    for (const gp_Pnt& point : solid.points) {
      classifier.Perform(point, 1e-7);
      CHECK(classifier.State() == TopAbs_IN);
    }
  }
  // Each shape holds its own points; the others' lie outside it.
  for (std::size_t i = 0; i < shapes.size(); ++i) {
    std::vector<std::size_t> others;
    for (std::size_t k = 0; k < shapes.size(); ++k) {
      if (k != i) {
        others.push_back(k);
      }
    }
    CHECK(detail::missing_inputs(shapes[i]->occt(), points, 1e-7) == others);
  }
}

void test_candidate_missing_inputs() {
  // Boxes that meet only along edges, as the reported design's triangles
  // meet: the second on the first's corner edge, the third on the
  // second's; a thin plate on the first; a rod, a quarter of it in the
  // third box near its corner, the rest out of it.
  std::vector<ShapePtr> shapes;
  shapes.push_back(box(0, 0, 0, 0, 10, 10, 10));
  shapes.push_back(box(1, 10, 10, 0, 10, 10, 10));
  shapes.push_back(box(2, 20, 0, 0, 10, 10, 10));
  shapes.push_back(box(3, 0, 0, 10, 10, 10, 0.05));
  shapes.push_back(body(BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(25, 1, 9), gp_Dir(1, 0, 0)), 0.75, 20).Shape(), 4));
  const std::vector<const Shape*> inputs = pointers(shapes);
  const std::vector<detail::InsidePoints> points = detail::points_inside(inputs);
  // Candidates that miss one or several of them, as OCCT's union of the
  // reported design missed bodies without a warning.
  CHECK((detail::missing_inputs(candidate(shapes, {0, 1, 3, 4}), points, 1e-7) == std::vector<std::size_t>{2}));
  CHECK((detail::missing_inputs(candidate(shapes, {0, 2, 4}), points, 1e-7) == std::vector<std::size_t>{1, 3}));
  CHECK((detail::missing_inputs(candidate(shapes, {0, 1, 2, 3}), points, 1e-7) == std::vector<std::size_t>{4}));
  CHECK((detail::missing_inputs(TopoDS_Compound(), points, 1e-7) == std::vector<std::size_t>{0, 1, 2, 3, 4}));
  CHECK(detail::missing_inputs(candidate(shapes, {0, 1, 2, 3, 4}), points, 1e-7).empty());
  // The union holds them all, and is what it was: the boxes, the plate on
  // the first, the rod's 15 mm out of the third.
  const ShapePtr joined = mitcad::geometry::unite(inputs);
  CHECK(detail::missing_inputs(joined->occt(), points, 1e-7).empty());
  CHECK(BRepCheck_Analyzer(joined->occt()).IsValid());
  CHECK_NEAR_TOL(volume_of(joined->occt()), 3000.0 + 5.0 + kPi * 0.75 * 0.75 * 15.0, 1e-6);
  check_inputs_held(*joined, inputs);
  std::printf("  union of boxes meeting along edges: %d solids\n", solid_count(joined->occt()));
}

void test_contained_inputs() {
  // A block, a small block inside it, a block overlapping it, and a copy
  // of it: the first (inside its copy) and the small one are left out, the
  // copy, which then has nothing left to lie in, and the overlapping one
  // stay.
  std::vector<ShapePtr> shapes;
  shapes.push_back(box(0, 0, 0, 0, 10, 10, 10));
  shapes.push_back(box(1, 2, 2, 2, 2, 2, 2));
  shapes.push_back(box(2, 8, 0, 0, 4, 10, 10));
  shapes.push_back(box(3, 0, 0, 0, 10, 10, 10));
  const std::vector<const Shape*> inputs = pointers(shapes);
  CHECK(detail::lies_inside(*shapes[1], *shapes[0]));
  CHECK(!detail::lies_inside(*shapes[2], *shapes[0]));
  CHECK(!detail::lies_inside(*shapes[0], *shapes[1]));
  CHECK(!detail::lies_inside(*shapes[1], *shapes[2]));
  CHECK(detail::lies_inside(*shapes[0], *shapes[3]));
  CHECK((detail::contained_inputs(inputs) == std::vector<std::size_t>{0, 1}));
  const ShapePtr joined = mitcad::geometry::unite(inputs);
  CHECK(BRepCheck_Analyzer(joined->occt()).IsValid());
  CHECK_NEAR_TOL(volume_of(joined->occt()), 1200.0, 1e-6);
  check_inputs_held(*joined, inputs);
  // A bare rod inside a body of the same rod with a thin fin, as the
  // reported design's rods lie in its finned rods.
  const TopoDS_Shape cylinder = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0)), 1.5, 50).Shape();
  BRepAlgoAPI_Fuse fuse;
  NCollection_List<TopoDS_Shape> arguments;
  NCollection_List<TopoDS_Shape> tools;
  arguments.Append(cylinder);
  tools.Append(BRepPrimAPI_MakeBox(gp_Pnt(0, 0, -0.5), 50, 20, 1).Shape());
  fuse.SetArguments(arguments);
  fuse.SetTools(tools);
  fuse.SetNonDestructive(true);
  fuse.Build();
  CHECK(fuse.IsDone() && solid_count(fuse.Shape()) == 1);
  const ShapePtr finned = body(fuse.Shape(), 4);
  const ShapePtr rod = body(cylinder, 5);
  CHECK(detail::lies_inside(*rod, *finned));
  CHECK(!detail::lies_inside(*finned, *rod));
  CHECK((detail::contained_inputs({rod.get(), finned.get()}) == std::vector<std::size_t>{0}));
  CHECK((detail::contained_inputs({finned.get(), rod.get()}) == std::vector<std::size_t>{1}));
}

// The fuse of `shape` with `fins` (OCCT's, the inputs left as they were),
// with its faces merged (ShapeUpgrade_UnifySameDomain) when `merge`.
TopoDS_Shape finned_rod(const TopoDS_Shape& shape, const std::vector<TopoDS_Shape>& fins, bool merge) {
  BRepAlgoAPI_Fuse fuse;
  NCollection_List<TopoDS_Shape> arguments;
  NCollection_List<TopoDS_Shape> tools;
  arguments.Append(shape);
  for (const TopoDS_Shape& fin : fins) {
    tools.Append(fin);
  }
  fuse.SetArguments(arguments);
  fuse.SetTools(tools);
  fuse.SetNonDestructive(true);
  fuse.Build();
  CHECK(fuse.IsDone());
  if (!fuse.IsDone()) {
    return {};
  }
  if (!merge) {
    return fuse.Shape();
  }
  ShapeUpgrade_UnifySameDomain unify(fuse.Shape(), true, true, false);
  unify.Build();
  return unify.Shape();
}

// A bare rod's common with a body holding it whose side face lies on the
// rod's cylinder (mitcad#2217; the reported design's five bare rods lie
// so in its finned rods): the rod, both ways, within 1e-6 relative, valid,
// and its cut by the body empty. The rod's seam on top (+Z); the bodies:
// the rod with a fin beside the seam along its whole length, with its
// faces merged (its side face then one face without a seam, from one side
// of the fin's root round to the other, as the reported finned rods'),
// with a fin over the seam merged, with three short fins (their roots
// holes in the side face), and the merged rod with a fin beside the seam
// joined with a plate across it (as the reported union trims the finned
// rods' side faces).
void test_rod_common_inside_finned_rod() {
  const double radius = 1.5;
  const double length = 50.0;
  const TopoDS_Shape rod =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0), gp_Dir(0, 0, 1)), radius, length).Shape();
  const double own = accurate_volume_of(rod);
  CHECK_NEAR_TOL(own, kPi * radius * radius * length, 1e-9);
  const TopoDS_Shape beside = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, -0.25), length, 12, 0.5).Shape();
  const TopoDS_Shape over = BRepPrimAPI_MakeBox(gp_Pnt(0, -0.25, 0), length, 0.5, 12).Shape();
  const std::vector<TopoDS_Shape> short_fins{
      BRepPrimAPI_MakeBox(gp_Pnt(10, 0, -0.25), 30, 12, 0.5).Shape(),
      BRepPrimAPI_MakeBox(gp_Pnt(10, -12, -0.25), 30, 12, 0.5).Shape(),
      BRepPrimAPI_MakeBox(gp_Pnt(15, -0.25, -12), 20, 0.5, 12).Shape(),
  };
  struct Holder {
    const char* name;
    TopoDS_Shape shape;
  };
  std::vector<Holder> holders{
      {"a fin beside the seam", finned_rod(rod, {beside}, false)},
      {"a fin beside the seam, merged", finned_rod(rod, {beside}, true)},
      {"a fin over the seam, merged", finned_rod(rod, {over}, true)},
      {"three short fins", finned_rod(rod, short_fins, false)},
      {"three short fins, merged", finned_rod(rod, short_fins, true)},
  };
  {
    const ShapePtr fins = body(holders[1].shape, 0);
    const ShapePtr plate = box(1, 20, -10, -10, 1, 20, 20);
    const ShapePtr joined = mitcad::geometry::unite({fins.get(), plate.get()});
    holders.push_back({"a fin beside the seam, merged, joined with a plate", joined->occt()});
  }
  int wrong = 0;
  for (const Holder& holder : holders) {
    if (holder.shape.IsNull()) {
      ++wrong;
      continue;
    }
    const Measured common = measured<BRepAlgoAPI_Common>(rod, holder.shape);
    const Measured reversed = measured<BRepAlgoAPI_Common>(holder.shape, rod);
    const Measured cut = measured<BRepAlgoAPI_Cut>(rod, holder.shape);
    const bool right = common.done && common.valid && std::abs(common.volume - own) <= 1e-6 * own && reversed.done &&
                       reversed.valid && std::abs(reversed.volume - own) <= 1e-6 * own && cut.done &&
                       cut.volume <= 1e-6 * own;
    if (!right) {
      std::fprintf(stderr,
                   "  %s: common %.9f (done %d, valid %d), the other way %.9f (done %d, valid %d), cut %.9f "
                   "(done %d) of the rod's %.9f\n",
                   holder.name, common.volume, common.done ? 1 : 0, common.valid ? 1 : 0, reversed.volume,
                   reversed.done ? 1 : 0, reversed.valid ? 1 : 0, cut.volume, cut.done ? 1 : 0, own);
      ++wrong;
    }
  }
  CHECK(wrong == 0);
}

// The reported design's STEP file, unpacked from its archive into the work
// folder; empty when the folder or the archive is not there.
std::string reported_step() {
  const std::string reported = env("MITCAD_MITCAD_REPORTED");
  if (reported.empty() || work_folder.empty()) {
    return {};
  }
  const std::filesystem::path folder = std::filesystem::path(reported) / "github-463";
  if (!std::filesystem::is_directory(folder)) {
    return {};
  }
  std::filesystem::path archive;
  for (const auto& entry : std::filesystem::directory_iterator(folder)) {
    if (entry.path().extension() == ".zip") {
      archive = entry.path();
    }
  }
  if (archive.empty()) {
    return {};
  }
  std::ifstream in(archive, std::ios::binary);
  const std::string zip((std::istreambuf_iterator<char>(in)), std::istreambuf_iterator<char>());
  // The archive's first entry: its local header, then its deflated data.
  const auto u16 = [&zip](std::size_t at) {
    return static_cast<std::uint32_t>(static_cast<unsigned char>(zip[at])) |
           (static_cast<std::uint32_t>(static_cast<unsigned char>(zip[at + 1])) << 8);
  };
  const auto u32 = [&u16](std::size_t at) { return u16(at) | (u16(at + 2) << 16); };
  if (zip.size() < 30 || u32(0) != 0x04034b50u || u16(8) != 8) {
    std::fprintf(stderr, "the archive's first entry is not deflated\n");
    return {};
  }
  const std::size_t packed = u32(18);
  const std::size_t size = u32(22);
  const std::size_t data = 30 + u16(26) + u16(28);
  if (data + packed > zip.size()) {
    std::fprintf(stderr, "the archive is cut short\n");
    return {};
  }
  std::string text(size, '\0');
  z_stream stream{};
  if (inflateInit2(&stream, -MAX_WBITS) != Z_OK) {
    return {};
  }
  stream.next_in = reinterpret_cast<Bytef*>(const_cast<char*>(zip.data() + data));
  stream.avail_in = static_cast<uInt>(packed);
  stream.next_out = reinterpret_cast<Bytef*>(text.data());
  stream.avail_out = static_cast<uInt>(size);
  const int status = inflate(&stream, Z_FINISH);
  inflateEnd(&stream);
  if (status != Z_STREAM_END) {
    std::fprintf(stderr, "the archive's entry could not be unpacked\n");
    return {};
  }
  std::filesystem::create_directories(work_folder);
  const std::string path = (std::filesystem::path(work_folder) / "reported-463.step").string();
  std::ofstream out(path, std::ios::binary);
  out.write(text.data(), static_cast<std::streamsize>(text.size()));
  return out ? path : std::string();
}

// OCCT's exact union of the shapes as the union makes it (the first the
// argument, the others tools), and the shapes it misses.
std::vector<std::size_t> exact_union_misses(const std::vector<const Shape*>& shapes) {
  BRepAlgoAPI_Fuse fuse;
  NCollection_List<TopoDS_Shape> arguments;
  NCollection_List<TopoDS_Shape> tools;
  arguments.Append(shapes[0]->occt());
  for (std::size_t i = 1; i < shapes.size(); ++i) {
    tools.Append(shapes[i]->occt());
  }
  fuse.SetArguments(arguments);
  fuse.SetTools(tools);
  fuse.SetNonDestructive(true);
  fuse.SetRunParallel(true);
  fuse.Build();
  return detail::missing_inputs(fuse.Shape(), detail::points_inside(shapes), 1e-7);
}

void test_reported_hub_with_blades_and_ring() {
  const std::string path = reported_step();
  if (path.empty()) {
    std::printf("test_reported_hub_with_blades_and_ring: skipped (MITCAD_MITCAD_REPORTED has no "
                "github-463 archive)\n");
    return;
  }
  const std::vector<mitcad::io::Body> read = mitcad::io::read_step(path);
  CHECK(read.size() == 31);
  std::vector<ShapePtr> bodies;
  for (std::size_t i = 0; i < read.size(); ++i) {
    bodies.push_back(body(read[i].shape, static_cast<int>(i)));
  }
  // The hub, the largest, is the Combine's target: first; the 30 tools
  // after it in the file's order.
  std::size_t hub = 0;
  for (std::size_t i = 1; i < bodies.size(); ++i) {
    if (volume_of(bodies[i]->occt()) > volume_of(bodies[hub]->occt())) {
      hub = i;
    }
  }
  std::vector<const Shape*> inputs{bodies[hub].get()};
  for (std::size_t i = 0; i < bodies.size(); ++i) {
    if (i != hub) {
      inputs.push_back(bodies[i].get());
    }
  }
  // OCCT's exact union of all missed 24 of the 31 bodies without a warning
  // (mitcad#2053): its split of the finned rods' side faces closed loops
  // across the period (mitcad#2170). With port patch 2170 it holds every
  // body; the patch finds a finned rod's own vertex also where the boolean
  // put a new vertex in its place (a match by identity alone left the 24
  // missed).
  const std::vector<std::size_t> missed = exact_union_misses(inputs);
  std::printf("  OCCT's exact union of all misses %zu bodies\n", missed.size());
  CHECK(missed.empty());
  // The five bare rods lie inside finned rods; without them OCCT's exact
  // union holds every body.
  const std::vector<std::size_t> inside = detail::contained_inputs(inputs);
  CHECK((inside == std::vector<std::size_t>{2, 5, 8, 11, 14}));
  std::vector<const Shape*> rest;
  for (std::size_t i = 0; i < inputs.size(); ++i) {
    if (std::find(inside.begin(), inside.end(), i) == inside.end()) {
      rest.push_back(inputs[i]);
    }
  }
  CHECK(exact_union_misses(rest).empty());
  // The hub joined with the other 30 bodies (mitcad#2170, port patch 2170:
  // the finned rods' side faces split into valid faces): one valid body
  // holding every body.
  ShapePtr joined;
  try {
    joined = mitcad::geometry::unite(inputs);
  } catch (const std::exception& error) {
    const std::string message = error.what();
    std::printf("  the union fails: %s\n", message.c_str());
    CHECK(false);
    // Where it fails, why in words (mitcad#2170): not "an empty result"
    // of the bodies OCCT's first union missed when the union of the
    // bodies not lying inside others is invalid.
    CHECK(message.find("union") != std::string::npos);
    CHECK(message.find("empty") == std::string::npos);
    return;
  }
  std::printf("  %zu bodies joined: %d solids, volume %.6f\n", inputs.size(), solid_count(joined->occt()),
              volume_of(joined->occt()));
  CHECK(BRepCheck_Analyzer(joined->occt()).IsValid());
  CHECK(detail::missing_inputs(joined->occt(), detail::points_inside(inputs), 1e-7).empty());
  check_inputs_held(*joined, inputs);
  // Every body's volume in the union, for the hub, each tool and each of
  // the five bare rods lying wholly inside finned rods (mitcad#2217): the
  // volume it has in common with the union, integrated to 1e-9, falls
  // short of its own volume by at most 1e-6 relative (no material lost),
  // and exceeds it by at most kExcessLimit. A common cannot hold more than
  // the body; the ring's common, its torus cut by the intersection curves
  // of 30 bodies, measured 1.09e-6 over the ring (21135.164 against
  // 21135.141 mm3) in mitcad#2170's check. An excess above 1e-6 is printed
  // with what the common is made of (print_excess) to tell its cause.
  constexpr double kExcessLimit = 1e-4;
  int short_of = 0;
  for (std::size_t i = 0; i < inputs.size(); ++i) {
    const Measured common = measured<BRepAlgoAPI_Common>(inputs[i]->occt(), joined->occt());
    const double own = accurate_volume_of(inputs[i]->occt());
    const double held = common.done ? common.volume : 0.0;
    const bool rod = std::find(inside.begin(), inside.end(), i) != inside.end();
    if (own - held > 1e-6 * own || held - own > kExcessLimit * own) {
      std::fprintf(stderr, "  body %zu%s: %.9f of its %.9f in the union (common done %d, valid %d)\n", i,
                   rod ? " (a bare rod inside a finned rod)" : "", held, own, common.done ? 1 : 0,
                   common.valid ? 1 : 0);
      ++short_of;
    } else if (held - own > 1e-6 * own) {
      std::printf("  body %zu: its common with the union measures %.9f, its own volume %.9f (%.3g over)\n", i, held,
                  own, (held - own) / own);
      print_excess(inputs[i]->occt(), common.shape, held - own);
    }
  }
  CHECK(short_of == 0);
}

} // namespace
} // namespace test

int main(int argc, char** argv) {
  if (argc > 1) {
    test::work_folder = argv[1];
  }
  test::guarded("test_points_inside_solids", test::test_points_inside_solids);
  test::guarded("test_candidate_missing_inputs", test::test_candidate_missing_inputs);
  test::guarded("test_contained_inputs", test::test_contained_inputs);
  test::guarded("test_rod_common_inside_finned_rod", test::test_rod_common_inside_finned_rod);
  test::guarded("test_reported_hub_with_blades_and_ring", test::test_reported_hub_with_blades_and_ring);
  if (test::failures != 0) {
    std::printf("%d check(s) failed\n", test::failures);
  }
  return test::failures == 0 ? 0 : 1;
}
