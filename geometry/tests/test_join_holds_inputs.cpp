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
// short by at most 1e-6 relative (no material lost), over by at most 1e-6
// (the ring too: its common's volume moved by 4e-6 with the point it is
// integrated about until port patch 2284 integrated across the joints of
// its faces' pcurves, mitcad#2262, mitcad#2267, mitcad#2284, see there;
// now the ring's common and the ring agree to 1e-9 about each of three
// points), and the common's cut by the body empty; each body's numbers go
// to common-volumes.txt in the work folder, for the ring with what its
// common is made of, down to each face's vector integral of its normal
// and the joints of its pcurves (mitcad#2267). The common of a bare rod
// with a body holding it
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
#include <map>
#include <memory>
#include <string>
#include <utility>
#include <vector>

#include <zlib.h>

#include <BRepAlgoAPI_BooleanOperation.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepGProp.hxx>
#include <BRepGProp_Domain.hxx>
#include <BRepGProp_Face.hxx>
#include <BRepGProp_Vinert.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_Curve.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

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
using Ancestors = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

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

const char* surface_type_name(GeomAbs_SurfaceType type) {
  switch (type) {
  case GeomAbs_Plane:
    return "plane";
  case GeomAbs_Cylinder:
    return "cylinder";
  case GeomAbs_Cone:
    return "cone";
  case GeomAbs_Sphere:
    return "sphere";
  case GeomAbs_Torus:
    return "torus";
  case GeomAbs_BezierSurface:
  case GeomAbs_BSplineSurface:
    return "spline surface";
  default:
    return "other surface";
  }
}

const char* curve_type_name(GeomAbs_CurveType type) {
  switch (type) {
  case GeomAbs_Line:
    return "line";
  case GeomAbs_Circle:
    return "circle";
  case GeomAbs_Ellipse:
    return "ellipse";
  case GeomAbs_BezierCurve:
  case GeomAbs_BSplineCurve:
    return "B-spline";
  default:
    return "other curve";
  }
}

// The volume of a shape's faces integrated (to a relative error of 1e-9)
// about `point` instead of the shape's middle, which BRepGProp takes: the
// same for every point where the faces' boundaries close up; where they
// leave slivers open between two faces (each face integrated over the
// domain its own curves bound), the slivers' flux moves with the point
// (mitcad#2217). With it the integration's own bound of its error: each
// face's integral times the relative error BRepGProp_Vinert reports for it
// (its two Gauss orders compared), summed, and the largest such relative
// error of one face (mitcad#2262). Each face's integral grows with the
// point's distance from the shape, so does this bound: a difference
// between the volumes about two points that stays within it is the
// integration's error, not a gap between faces.
struct Integral {
  double volume = 0.0;
  double error = 0.0;
  double worst = 0.0;
};

// One face's share of the volume about `point` (BRepGProp_Vinert, to a
// relative error of 1e-9), with the relative error the integration reports
// for it.
Integral face_integral(const TopoDS_Face& face, const gp_Pnt& point) {
  BRepGProp_Vinert integral;
  integral.SetLocation(point);
  BRepGProp_Face surface(face);
  if (face.NbChildren() == 0) {
    integral.Perform(surface, 1e-9);
  } else {
    BRepGProp_Domain domain(face);
    integral.Perform(surface, domain, 1e-9);
  }
  Integral out;
  out.volume = integral.Mass();
  out.error = std::abs(integral.Mass()) * integral.GetEpsilon();
  out.worst = integral.GetEpsilon();
  return out;
}

bool oriented(const TopoDS_Shape& shape) {
  return shape.Orientation() == TopAbs_FORWARD || shape.Orientation() == TopAbs_REVERSED;
}

Integral volume_about(const TopoDS_Shape& shape, const gp_Pnt& point) {
  Integral out;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    if (!oriented(face)) {
      continue;
    }
    const Integral one = face_integral(face, point);
    out.volume += one.volume;
    out.error += one.error;
    out.worst = std::max(out.worst, one.worst);
  }
  return out;
}

// Where a shell's volume moves with the point it is integrated about
// (mitcad#2267). A face's share of the volume about a point O is a third
// of the integral of (P - O) . n over its area, so it moves with O by a
// third of O's offset dotted with the face's vector integral of its normal
// over its area, A; the shell's volume moves by the sum of the faces' A,
// which is zero for a shell that closes. A face's A is also half the
// integral of (P - O) x dP round its boundary, whatever O: from its edges'
// 3D curves that is the face's share of the shell's surface, and the
// faces' shares sum to zero exactly, each edge's curve run once each way.
//
// BRepGProp_Gauss integrates a face by Green's theorem in its parameters:
// along each of its edges' pcurves (the edge's orientation in the forward
// face) the integrand is first integrated in u from the face's lowest u
// (its UV bounds) to the pcurve's point, at the pcurve's v, and that is
// integrated along the pcurve against dv. Along the pcurves alone that is
// exact only for a loop of pcurves that closes in the parameters. Where one
// pcurve ends at (u, v) and the next one starts at (u', v'), within the
// vertex's tolerance in space but not at the same parameters, such an
// integral leaves out a strip from the lowest u to u, (v' - v) wide: on a
// torus face reaching round the ring a joint 1e-6 mm apart along v leaves
// out about 1e-6 mm times the strip's length round the ring, some 1e-4
// mm^2. A gap along u leaves out nothing. Port patch 2284 integrates each
// such joint as the segment from one pcurve's end to the next one's start
// (mitcad#2284), so the loops close and the strips are integrated too, and
// adds to the volume the ribbon from each edge's curve on the face to its
// 3D curve, so the faces close along the edges' 3D curves.
//
// FaceFlux records for one face: `integrated`, A as the integration gives
// it (its volume about the middle and about points `offset` away from it
// along X, Y and Z); `edges`, A from the edges' 3D curves; `pcurves`, A
// from the edges' curves on the face (the pcurves lifted to the surface)
// with straight chords across the joints; the joints of its pcurves (each
// pcurve's end paired with the nearest start of another in the same wire),
// their largest gaps along u and v and in space; `strips`, the vector
// integral of the normal over the strips an integration along the pcurves
// alone leaves out (minus the sum of each joint's v gap times the integral
// of the normal from the lowest u to the joint at its v): with port patch
// 2284 `integrated` equals `edges` (the joints and the ribbons to the 3D
// curves integrated), and `pcurves` where the pcurves lie on the 3D
// curves; along the pcurves alone it was `pcurves` plus `strips`; and `closure`, the volume about the middle those
// strips hold (each joint's v gap times the integral of (P - middle) . n /
// 3 from the lowest u), which, taken from the face's volume, gives the
// volume an integration along the pcurves alone, the joints left open,
// gave.
struct FaceFlux {
  gp_Vec integrated{0.0, 0.0, 0.0};
  gp_Vec edges{0.0, 0.0, 0.0};
  gp_Vec pcurves{0.0, 0.0, 0.0};
  gp_Vec strips{0.0, 0.0, 0.0};
  double closure = 0.0;
  int wires = 0;
  int edge_count = 0;
  int joints = 0;
  int unpaired = 0;
  int without_pcurve = 0;
  double du = 0.0;
  double dv = 0.0;
  double apart = 0.0;
  double u_low = 0.0;
  double u_high = 0.0;
  double v_low = 0.0;
  double v_high = 0.0;
};

// An 8-point Gauss-Legendre rule on [-1, 1].
constexpr double kGaussNodes[8] = {-0.9602898564975363, -0.7966664774136267, -0.5255324099163290,
                                   -0.1834346424956498, 0.1834346424956498,  0.5255324099163290,
                                   0.7966664774136267,  0.9602898564975363};
constexpr double kGaussWeights[8] = {0.1012285362903763, 0.2223810344533745, 0.3137066458778873,
                                     0.3626837833783620, 0.3626837833783620, 0.3137066458778873,
                                     0.2223810344533745, 0.1012285362903763};

// Calls at(t, weight) at the nodes of the 8-point rule on each of `pieces`
// equal pieces of [a, b]: the sum of weight * f(t) integrates f over [a, b].
template <typename At> void gauss_nodes(double a, double b, int pieces, At&& at) {
  const double h = (b - a) / static_cast<double>(pieces);
  for (int p = 0; p < pieces; ++p) {
    const double mid = a + (static_cast<double>(p) + 0.5) * h;
    for (int k = 0; k < 8; ++k) {
      at(mid + 0.5 * h * kGaussNodes[k], 0.5 * h * kGaussWeights[k]);
    }
  }
}

// Half the integral of (C(t) - origin) x C'(t) along a curve, from its
// first to its last parameter, over its C2 intervals.
gp_Vec half_moment(const BRepAdaptor_Curve& curve, const gp_Pnt& origin) {
  const int intervals = curve.NbIntervals(GeomAbs_C2);
  NCollection_Array1<double> bounds(1, intervals + 1);
  curve.Intervals(bounds, GeomAbs_C2);
  gp_Vec sum(0.0, 0.0, 0.0);
  for (int i = 1; i <= intervals; ++i) {
    gauss_nodes(bounds.Value(i), bounds.Value(i + 1), 16, [&](double t, double weight) {
      gp_Pnt point;
      gp_Vec tangent;
      curve.D1(t, point, tangent);
      sum += gp_Vec(origin, point).Crossed(tangent) * weight;
    });
  }
  return sum * 0.5;
}

FaceFlux face_flux(const TopoDS_Face& face, const gp_Pnt& middle, double offset) {
  FaceFlux out;
  const double at_middle = face_integral(face, middle).volume;
  const double along_x = face_integral(face, middle.Translated(gp_Vec(offset, 0.0, 0.0))).volume;
  const double along_y = face_integral(face, middle.Translated(gp_Vec(0.0, offset, 0.0))).volume;
  const double along_z = face_integral(face, middle.Translated(gp_Vec(0.0, 0.0, offset))).volume;
  out.integrated = gp_Vec(3.0 * (at_middle - along_x) / offset, 3.0 * (at_middle - along_y) / offset,
                          3.0 * (at_middle - along_z) / offset);
  // As BRepGProp_Face and BRepGProp_Domain take it: the forward face, its
  // UV bounds, the normal reversed for a reversed face.
  const double sign = face.Orientation() == TopAbs_REVERSED ? -1.0 : 1.0;
  const TopoDS_Face forward = TopoDS::Face(face.Oriented(TopAbs_FORWARD));
  const BRepAdaptor_Surface surface(forward);
  out.u_low = surface.FirstUParameter();
  out.u_high = surface.LastUParameter();
  out.v_low = surface.FirstVParameter();
  out.v_high = surface.LastVParameter();
  gp_Vec edges(0.0, 0.0, 0.0);
  gp_Vec pcurves(0.0, 0.0, 0.0);
  for (TopExp_Explorer wire(forward, TopAbs_WIRE); wire.More(); wire.Next()) {
    ++out.wires;
    std::vector<gp_Pnt2d> starts;
    std::vector<gp_Pnt2d> ends;
    for (TopExp_Explorer it(wire.Current(), TopAbs_EDGE); it.More(); it.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
      if (!oriented(edge)) {
        continue;
      }
      ++out.edge_count;
      const double direction = edge.Orientation() == TopAbs_REVERSED ? -1.0 : 1.0;
      double first = 0.0;
      double last = 0.0;
      const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, forward, first, last);
      if (pcurve.IsNull()) {
        ++out.without_pcurve;
        continue;
      }
      if (!BRep_Tool::Degenerated(edge)) {
        edges += half_moment(BRepAdaptor_Curve(edge), middle) * direction;
      }
      pcurves += half_moment(BRepAdaptor_Curve(edge, forward), middle) * direction;
      const gp_Pnt2d at_first = pcurve->Value(first);
      const gp_Pnt2d at_last = pcurve->Value(last);
      starts.push_back(direction > 0.0 ? at_first : at_last);
      ends.push_back(direction > 0.0 ? at_last : at_first);
    }
    std::vector<bool> taken(starts.size(), false);
    for (const gp_Pnt2d& end : ends) {
      std::size_t best = starts.size();
      double nearest = 0.0;
      for (std::size_t j = 0; j < starts.size(); ++j) {
        const double distance = end.SquareDistance(starts[j]);
        if (!taken[j] && (best == starts.size() || distance < nearest)) {
          best = j;
          nearest = distance;
        }
      }
      if (best == starts.size()) {
        ++out.unpaired;
        continue;
      }
      taken[best] = true;
      const gp_Pnt2d& start = starts[best];
      ++out.joints;
      const double gap_u = start.X() - end.X();
      const double gap_v = start.Y() - end.Y();
      out.du = std::max(out.du, std::abs(gap_u));
      out.dv = std::max(out.dv, std::abs(gap_v));
      const gp_Pnt from = surface.Value(end.X(), end.Y());
      const gp_Pnt to = surface.Value(start.X(), start.Y());
      out.apart = std::max(out.apart, from.Distance(to));
      pcurves += gp_Vec(middle, from).Crossed(gp_Vec(middle, to)) * 0.5;
      // The strip the joint leaves out, at its middle, the parameters kept
      // within the UV bounds as BRepGProp_Gauss keeps them.
      const double strip_u = std::clamp(0.5 * (end.X() + start.X()), out.u_low, out.u_high);
      const double strip_v = std::clamp(0.5 * (end.Y() + start.Y()), out.v_low, out.v_high);
      gp_Vec strip(0.0, 0.0, 0.0);
      double flux = 0.0;
      if (strip_u - out.u_low > 1e-12) {
        gauss_nodes(out.u_low, strip_u, 32, [&](double s, double weight) {
          gp_Pnt point;
          gp_Vec along_u;
          gp_Vec along_v;
          surface.D1(s, strip_v, point, along_u, along_v);
          const gp_Vec normal = along_u.Crossed(along_v) * sign;
          strip += normal * weight;
          flux += weight * gp_Vec(middle, point).Dot(normal) / 3.0;
        });
      }
      out.strips -= strip * gap_v;
      out.closure += flux * gap_v;
    }
  }
  out.edges = edges * sign;
  out.pcurves = pcurves * sign;
  return out;
}

const char* orientation_name(TopAbs_Orientation orientation) {
  return orientation == TopAbs_REVERSED ? "reversed" : "forward";
}

// For each face of `shape` (`label` names them), the record of face_flux;
// the sums of the faces' vectors; and the closures' change of the shape's
// volume about each of `points` (each face's closure moved from the
// middle to the point: plus a third of the point's offset dotted with the
// face's strips).
struct ShellFlux {
  std::vector<std::string> lines;
  gp_Vec integrated{0.0, 0.0, 0.0};
  gp_Vec edges{0.0, 0.0, 0.0};
  gp_Vec pcurves{0.0, 0.0, 0.0};
  gp_Vec strips{0.0, 0.0, 0.0};
  double closure[3] = {0.0, 0.0, 0.0};
  int faces = 0;
};

ShellFlux shell_flux(const TopoDS_Shape& shape, const char* label, const gp_Pnt& middle, double offset,
                     const gp_Pnt (&points)[3]) {
  ShellFlux out;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    if (!oriented(face)) {
      continue;
    }
    const FaceFlux flux = face_flux(face, middle, offset);
    ++out.faces;
    out.integrated += flux.integrated;
    out.edges += flux.edges;
    out.pcurves += flux.pcurves;
    out.strips += flux.strips;
    for (int k = 0; k < 3; ++k) {
      out.closure[k] += flux.closure + gp_Vec(middle, points[k]).Dot(flux.strips) / 3.0;
    }
    const gp_Vec off_edges = flux.integrated - flux.edges;
    // With the joints integrated (port patch 2284), off the loop of its
    // pcurves closed by chords at the joints.
    const gp_Vec off_loop = flux.integrated - flux.pcurves;
    char text[1400];
    std::snprintf(text, sizeof(text),
                  "%s face %d (%s, %s, %d wires, %d edges, u %.9f to %.9f, v %.9f to %.9f): its normal over "
                  "its area integrated (%.9g, %.9g, %.9g) mm2, the ring's surface within its edges' 3D curves "
                  "(%.9g, %.9g, %.9g), the integrated off it by (%.3g, %.3g, %.3g); %d joints of its pcurves "
                  "(%d unpaired, %d edges without a pcurve), apart by up to %.3g along u, %.3g along v, %.3g mm "
                  "in space; the strips the pcurves alone leave out (%.3g, %.3g, %.3g) mm2, the integrated off "
                  "its pcurves with the joints closed by (%.3g, %.3g, %.3g); the strips hold %.3g mm3 about the "
                  "middle",
                  label, out.faces, surface_type_name(BRepAdaptor_Surface(face, false).GetType()),
                  orientation_name(face.Orientation()), flux.wires, flux.edge_count, flux.u_low, flux.u_high,
                  flux.v_low, flux.v_high, flux.integrated.X(), flux.integrated.Y(), flux.integrated.Z(),
                  flux.edges.X(), flux.edges.Y(), flux.edges.Z(), off_edges.X(), off_edges.Y(), off_edges.Z(),
                  flux.joints, flux.unpaired, flux.without_pcurve, flux.du, flux.dv, flux.apart, flux.strips.X(),
                  flux.strips.Y(), flux.strips.Z(), off_loop.X(), off_loop.Y(), off_loop.Z(), flux.closure);
    out.lines.emplace_back(text);
  }
  return out;
}

// The edges of a common with a tolerance above 1e-7 mm, grouped by how
// they were made (mitcad#2262): a piece of one of the body's own edges
// (its 3D curve the body's edge's curve) or a section curve the boolean
// built; the type of its 3D curve; and the surfaces of the union's faces
// beside it other than the body's (the surface the body's face was cut
// by; "not an edge of the union" where the common did not take the
// union's edge). For each group: how many, their length, their largest
// tolerance; the largest distance, at 33 points of each edge, between its
// 3D curve and its curve on each of the common's faces beside it (its
// pcurve on that face's surface): what the tolerance has to cover; how
// many of the edges lie between two faces on different surfaces (or are a
// face's seam), and the largest distance between the two faces' curves on
// their surfaces there. Two faces on one surface share one pcurve of the
// edge, so the domains the volume integration bounds by the pcurves meet
// exactly whatever the 3D curve's tolerance; only faces on two surfaces
// can leave a sliver between them in the integral.
std::vector<std::string> edge_makeup(const TopoDS_Shape& body, const TopoDS_Shape& common,
                                     const TopoDS_Shape& joined) {
  std::vector<const Geom_Curve*> own_curves;
  for (TopExp_Explorer it(body, TopAbs_EDGE); it.More(); it.Next()) {
    TopLoc_Location location;
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(TopoDS::Edge(it.Current()), location, first, last);
    if (!curve.IsNull()) {
      own_curves.push_back(curve.get());
    }
  }
  Ancestors common_faces;
  TopExp::MapShapesAndAncestors(common, TopAbs_EDGE, TopAbs_FACE, common_faces);
  Ancestors union_faces;
  TopExp::MapShapesAndAncestors(joined, TopAbs_EDGE, TopAbs_FACE, union_faces);
  struct Group {
    int edges = 0;
    double length = 0.0;
    double tolerance = 0.0;
    double deviation = 0.0;
    int two_surfaces = 0;
    double gap = 0.0;
  };
  std::map<std::string, Group> groups;
  constexpr int kSamples = 33;
  for (int e = 1; e <= common_faces.Extent(); ++e) {
    const TopoDS_Edge& edge = TopoDS::Edge(common_faces.FindKey(e));
    const double tolerance = BRep_Tool::Tolerance(edge);
    if (tolerance <= 1e-7 || BRep_Tool::Degenerated(edge)) {
      continue;
    }
    TopLoc_Location location;
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, location, first, last);
    if (curve.IsNull()) {
      continue;
    }
    const bool own = std::find(own_curves.begin(), own_curves.end(), curve.get()) != own_curves.end();
    const BRepAdaptor_Curve in_space(edge);
    std::string beside;
    if (union_faces.Contains(edge)) {
      for (const TopoDS_Shape& face : union_faces.FindFromKey(edge)) {
        const GeomAbs_SurfaceType type = BRepAdaptor_Surface(TopoDS::Face(face), false).GetType();
        if (type != GeomAbs_Torus) {
          beside += beside.empty() ? "" : " and ";
          beside += surface_type_name(type);
        }
      }
      if (beside.empty()) {
        beside = "torus only";
      }
    } else {
      beside = "not an edge of the union";
    }
    const std::string key = std::string(own ? "a piece of the body's own edge" : "a section curve") + ", " +
                            curve_type_name(in_space.GetType()) + ", beside the union's " + beside;
    Group& group = groups[key];
    ++group.edges;
    GProp_GProps props;
    BRepGProp::LinearProperties(edge, props);
    group.length += props.Mass();
    group.tolerance = std::max(group.tolerance, tolerance);
    std::vector<TopoDS_Face> faces;
    for (const TopoDS_Shape& face : common_faces.FindFromIndex(e)) {
      faces.push_back(TopoDS::Face(face));
    }
    std::vector<std::unique_ptr<BRepAdaptor_Curve>> on_faces;
    for (const TopoDS_Face& face : faces) {
      on_faces.push_back(std::make_unique<BRepAdaptor_Curve>(edge, face));
    }
    bool apart = false;
    if (faces.size() == 2) {
      TopLoc_Location l0;
      TopLoc_Location l1;
      const occ::handle<Geom_Surface>& s0 = BRep_Tool::Surface(faces[0], l0);
      const occ::handle<Geom_Surface>& s1 = BRep_Tool::Surface(faces[1], l1);
      apart = s0 != s1 || !l0.IsEqual(l1) || BRep_Tool::IsClosed(edge, faces[0]);
    }
    group.two_surfaces += apart ? 1 : 0;
    const double t0 = in_space.FirstParameter();
    const double t1 = in_space.LastParameter();
    for (int k = 0; k < kSamples; ++k) {
      const double t = t0 + (t1 - t0) * static_cast<double>(k) / static_cast<double>(kSamples - 1);
      const gp_Pnt point = in_space.Value(t);
      for (const std::unique_ptr<BRepAdaptor_Curve>& on_face : on_faces) {
        group.deviation = std::max(group.deviation, point.Distance(on_face->Value(t)));
      }
      if (apart) {
        group.gap = std::max(group.gap, on_faces[0]->Value(t).Distance(on_faces[1]->Value(t)));
      }
    }
  }
  std::vector<std::string> out;
  for (const auto& [key, group] : groups) {
    char text[640];
    std::snprintf(text, sizeof(text),
                  "edges: %s: %d edges, %.3f mm, tolerance up to %.3g mm; the 3D curve within %.3g mm of its "
                  "curves on the faces; %d between faces on two surfaces, their curves apart by up to %.3g mm",
                  key.c_str(), group.edges, group.length, group.tolerance, group.deviation, group.two_surfaces,
                  group.gap);
    out.emplace_back(text);
  }
  return out;
}

// The middle of a shape's box and the half of its diagonal.
gp_Pnt middle_of(const TopoDS_Shape& shape, double& reach) {
  Bnd_Box box;
  BRepBndLib::Add(shape, box);
  double x0 = 0.0;
  double y0 = 0.0;
  double z0 = 0.0;
  double x1 = 0.0;
  double y1 = 0.0;
  double z1 = 0.0;
  box.Get(x0, y0, z0, x1, y1, z1);
  reach = 0.5 * std::sqrt((x1 - x0) * (x1 - x0) + (y1 - y0) * (y1 - y0) + (z1 - z0) * (z1 - z0));
  return gp_Pnt(0.5 * (x0 + x1), 0.5 * (y0 + y1), 0.5 * (z0 + z1));
}

// What a common is made of where it measures more than the body (and
// always for the ring): its faces, how many of them lie on one of the
// body's own surfaces (the others come from the union), the largest
// tolerance of its edges and the length of its edges with a tolerance
// above 1e-7 mm; the excess as a layer over those edges seen from the
// shape's middle (the volume integration's reference point): an excess
// that is the gap between the faces' boundaries along such edges, not
// material, gives a layer of the order of their tolerances; the volume of
// the common and of the body about the middle and points a diagonal away
// from it along X and Z (volume_about): an excess of open slivers changes
// with the point, the body's own volume does not; at each point the
// integration's own error bound, relative to the body's volume; and the
// common's loose edges by how they were made (edge_makeup); each face's
// vector integral of its normal, of the common and of the body
// (shell_flux, mitcad#2267). `spread` is the largest difference,
// relative, between the common's and the body's volume about one of the
// three points (each face's loop closed at its joints by the integration,
// port patch 2284); `open_spread` the same with the strips the joints
// leave out of an integration along the pcurves alone taken away, as the
// integration gave it before that patch (recorded, not checked).
struct Makeup {
  std::vector<std::string> lines;
  double spread = 0.0;
  double open_spread = 0.0;
};

Makeup excess_makeup(const TopoDS_Shape& body, const TopoDS_Shape& common, const TopoDS_Shape& joined,
                     double excess) {
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
  double reach = 0.0;
  const gp_Pnt middle = middle_of(common, reach);
  const double layer = loose_length > 0.0 && reach > 0.0 ? 3.0 * excess / (reach * loose_length) : 0.0;
  const gp_Pnt along_x = middle.Translated(gp_Vec(2.0 * reach, 0.0, 0.0));
  const gp_Pnt along_z = middle.Translated(gp_Vec(0.0, 0.0, 2.0 * reach));
  const Integral common_at[3] = {volume_about(common, middle), volume_about(common, along_x),
                                 volume_about(common, along_z)};
  const Integral body_at[3] = {volume_about(body, middle), volume_about(body, along_x),
                               volume_about(body, along_z)};
  double relative[3] = {0.0, 0.0, 0.0};
  Makeup out;
  for (int k = 0; k < 3; ++k) {
    relative[k] = body_at[k].volume != 0.0 ? (common_at[k].volume - body_at[k].volume) / body_at[k].volume : 0.0;
    out.spread = std::max(out.spread, std::abs(relative[k]));
  }
  const double scale = body_at[0].volume != 0.0 ? std::abs(body_at[0].volume) : 1.0;
  char text[640];
  std::snprintf(text, sizeof(text),
                "its common: %d faces, %d on the body's own surfaces; edge tolerance up to %.3g mm, %.3f mm of "
                "edges above 1e-7 mm; the excess as a layer over them %.3g mm",
                faces, on_own, largest, loose_length, layer);
  out.lines.emplace_back(text);
  std::snprintf(text, sizeof(text),
                "about the middle, a diagonal along X and along Z: common %.9f, %.9f, %.9f, body %.9f, %.9f, "
                "%.9f; the common %+.3g, %+.3g, %+.3g relative to the body",
                common_at[0].volume, common_at[1].volume, common_at[2].volume, body_at[0].volume, body_at[1].volume,
                body_at[2].volume, relative[0], relative[1], relative[2]);
  out.lines.emplace_back(text);
  std::snprintf(text, sizeof(text),
                "the integration's own error bound about those points, relative to the body's volume: common "
                "%.3g, %.3g, %.3g (one face's relative error up to %.3g), body %.3g, %.3g, %.3g (one face's up "
                "to %.3g)",
                common_at[0].error / scale, common_at[1].error / scale, common_at[2].error / scale,
                std::max({common_at[0].worst, common_at[1].worst, common_at[2].worst}), body_at[0].error / scale,
                body_at[1].error / scale, body_at[2].error / scale,
                std::max({body_at[0].worst, body_at[1].worst, body_at[2].worst}));
  out.lines.emplace_back(text);
  // Where the common's shell closes (mitcad#2267, mitcad#2284): each
  // face's vector integral of its normal over its area against its share
  // of the ring's surface, the joints of its pcurves and the strips they
  // leave out of an integration along the pcurves alone (shell_flux), for
  // the common's faces and the body's; the spread the summed vectors give
  // against the one measured; and the volumes about the three points with
  // those strips taken away, as the integration gave them before port
  // patch 2284 integrated across the joints.
  const gp_Pnt points[3] = {middle, along_x, along_z};
  const ShellFlux common_flux = shell_flux(common, "the common's", middle, 2.0 * reach, points);
  const ShellFlux body_flux = shell_flux(body, "the body's", middle, 2.0 * reach, points);
  for (const ShellFlux* flux : {&common_flux, &body_flux}) {
    std::snprintf(text, sizeof(text),
                  "%s %d faces summed: their normals over their areas integrated (%.3g, %.3g, %.3g) mm2, the "
                  "ring's surface within their edges' 3D curves (%.3g, %.3g, %.3g), through their pcurves with "
                  "the joints closed (%.3g, %.3g, %.3g), the strips the pcurves alone leave out at the joints "
                  "(%.3g, %.3g, %.3g)",
                  flux == &common_flux ? "the common's" : "the body's", flux->faces, flux->integrated.X(),
                  flux->integrated.Y(), flux->integrated.Z(), flux->edges.X(), flux->edges.Y(), flux->edges.Z(),
                  flux->pcurves.X(), flux->pcurves.Y(), flux->pcurves.Z(), flux->strips.X(), flux->strips.Y(),
                  flux->strips.Z());
    out.lines.emplace_back(text);
  }
  const double offset = 2.0 * reach;
  double left_open[3] = {0.0, 0.0, 0.0};
  double left_open_body[3] = {0.0, 0.0, 0.0};
  double left_open_relative[3] = {0.0, 0.0, 0.0};
  for (int k = 0; k < 3; ++k) {
    left_open[k] = common_at[k].volume - common_flux.closure[k];
    left_open_body[k] = body_at[k].volume - body_flux.closure[k];
    left_open_relative[k] =
        left_open_body[k] != 0.0 ? (left_open[k] - left_open_body[k]) / left_open_body[k] : 0.0;
    out.open_spread = std::max(out.open_spread, std::abs(left_open_relative[k]));
  }
  std::snprintf(text, sizeof(text),
                "the common's summed vector moves its volume from the middle to the points along X and Z by "
                "%+.3g, %+.3g relative to the body (measured %+.3g, %+.3g); with the strips at the joints left "
                "out (the integration before port patch 2284), about the three points: common %.9f, %.9f, "
                "%.9f, body %.9f, %.9f, %.9f; the common %+.3g, %+.3g, %+.3g relative to the body",
                -offset * common_flux.integrated.X() / 3.0 / scale,
                -offset * common_flux.integrated.Z() / 3.0 / scale, (common_at[1].volume - common_at[0].volume) / scale,
                (common_at[2].volume - common_at[0].volume) / scale, left_open[0], left_open[1], left_open[2],
                left_open_body[0], left_open_body[1], left_open_body[2], left_open_relative[0], left_open_relative[1],
                left_open_relative[2]);
  out.lines.emplace_back(text);
  for (std::string& line : edge_makeup(body, common, joined)) {
    out.lines.push_back(std::move(line));
  }
  for (const ShellFlux* flux : {&common_flux, &body_flux}) {
    out.lines.insert(out.lines.end(), flux->lines.begin(), flux->lines.end());
  }
  return out;
}

// Whether a shape has a toroidal face (the reported design's ring).
bool has_torus(const TopoDS_Shape& shape) {
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    if (BRepAdaptor_Surface(TopoDS::Face(it.Current()), false).GetType() == GeomAbs_Torus) {
      return true;
    }
  }
  return false;
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
  // and exceeds it by at most 1e-6, the bare rods too (a looser limit
  // would hide a small wrong common of a rod with its finned rod), the
  // ring's too. The ring's torus is cut by the section curves of the
  // bodies crossing it into 40 faces, all on the ring's own surfaces; its
  // common measured 1.09e-6 over the ring (21135.163943 against
  // 21135.140975 mm3, the check of mitcad#2217), its cut by the ring
  // empty, and about the middle, a diagonal along X and one along Z it
  // measured 21135.163838, 21135.164173 and 21135.077405 mm3 while the ring
  // measured 21135.140975 about each: a spread of 4.1e-6 relative with the
  // point it was integrated about (mitcad#2262), where a shell that closes
  // has one volume about every point. The cause, found (mitcad#2267,
  // mitcad#2284): not the integration's own error (its bound about the
  // three points at most 6.5e-10 of the ring), not a gap between faces
  // (the common's 40 faces' shares of the ring's surface from their
  // edges' 3D curves sum to (2.3e-14, -1.9e-14, 4.1e-11) mm2, through
  // their pcurves with the joints closed to (-6.4e-12, -6.7e-14, -4.2e-11)),
  // but the joints of each face's pcurves: BRepGProp_Gauss integrated a
  // face along its pcurves alone, each point's strip integrated from the
  // face's lowest u (shell_flux), exact only where each pcurve starts at
  // the parameters where the one before it ends. The pcurves of the
  // common's faces meet at their vertices within the vertices' tolerance
  // but apart along v; the strips they left out summed to (-1.53e-06,
  // 0.000108, 0.000396) mm2, the whole of the integrated normals' sum,
  // almost all of it on the one torus face reaching round the ring (159
  // joints, apart by up to 4.07e-08 along v and 9.17e-08 mm in space,
  // strips (-1.57e-06, 0.000108, 0.000397) mm2). With every face's loop
  // closed at its joints the common measured 21135.140974536,
  // 21135.140974536 and 21135.140974539 mm3 about the three points, the
  // ring 21135.140974537 about each: within 1.2e-13. Port patch 2284
  // integrates each such joint as the segment from one pcurve's end to the
  // next one's start (and, for the volume, the ribbon from each edge's
  // curve on a face to its 3D curve; the common's faces on one surface
  // share their edges' pcurves, so theirs cancel), so the ring's common
  // is checked within 1e-6 both
  // ways like every body, and its volume about each of the three points
  // within 1e-9 of the ring's (makeup.spread); the record keeps, for the
  // ring, the strips the joints leave out of an integration along the
  // pcurves alone and the volumes that integration gave. Material the
  // common would hold outside the body is checked on its own, for every
  // body within 1e-6: the common's cut by the body is empty. Each body's
  // numbers, and for the ring and an excess above 1e-6 what the common is
  // made of (excess_makeup), are written to common-volumes.txt in the work
  // folder, kept after a passing run.
  constexpr double kRingSpreadLimit = 1e-9;
  const std::string record_path = (std::filesystem::path(work_folder) / "common-volumes.txt").string();
  std::ofstream record(record_path);
  int wrong = 0;
  for (std::size_t i = 0; i < inputs.size(); ++i) {
    const TopoDS_Shape& input = inputs[i]->occt();
    const Measured common = measured<BRepAlgoAPI_Common>(input, joined->occt());
    const double own = accurate_volume_of(input);
    const double held = common.done ? common.volume : 0.0;
    const Measured outside = common.done ? measured<BRepAlgoAPI_Cut>(common.shape, input) : Measured{};
    const bool rod = std::find(inside.begin(), inside.end(), i) != inside.end();
    const bool ring = has_torus(input);
    char line[400];
    std::snprintf(line, sizeof(line),
                  "body %zu%s: common %.9f (done %d, valid %d), own %.9f, %+.3g relative; the common's cut by the "
                  "body %.9f (done %d)",
                  i, rod ? " (a bare rod inside a finned rod)" : (ring ? " (the ring)" : ""), held,
                  common.done ? 1 : 0, common.valid ? 1 : 0, own, (held - own) / own, outside.volume,
                  outside.done ? 1 : 0);
    record << line << "\n";
    Makeup makeup;
    const bool described = common.done && (ring || held - own > 1e-6 * own);
    if (described) {
      makeup = excess_makeup(input, common.shape, joined->occt(), held - own);
      for (const std::string& part : makeup.lines) {
        record << "  " << part << "\n";
      }
    }
    const bool right = common.done && own - held <= 1e-6 * own && held - own <= 1e-6 * own && outside.done &&
                       outside.volume <= 1e-6 * own && (!ring || makeup.spread <= kRingSpreadLimit);
    if (!right) {
      std::fprintf(stderr, "  %s\n", line);
      ++wrong;
    }
    if (described) {
      std::FILE* to = right ? stdout : stderr;
      if (right) {
        std::fprintf(to, "  %s\n", line);
      }
      for (const std::string& part : makeup.lines) {
        std::fprintf(to, "    %s\n", part.c_str());
      }
    }
  }
  std::printf("  each body's common with the union: %s\n", record ? record_path.c_str() : "(not written)");
  CHECK(wrong == 0);
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
