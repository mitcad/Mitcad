// SPDX-License-Identifier: MIT
#pragma once

// Surface modelling (mitcad#455): Blend Surface, Boundary Surface, Match
// Surface and Fair Surface. None changes its inputs. Lengths are
// millimetres.

#include <optional>
#include <string>
#include <vector>

#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>

#include "mitcad/geometry/path_sweep.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/surface_offsets.hpp"
#include "mitcad/geometry/surfaces.hpp"

namespace mitcad::geometry {

// Edges of a body that join into one chain: references and free edges of
// openings; `face` the face beside the chain where an edge lies between
// two faces (empty: each edge's one face).
struct EdgeChain {
  const Shape* body = nullptr;
  std::vector<std::string> edges;
  std::vector<OpeningEdges> openings;
  std::string face;
};

// One end of a blend: its chain, how much of the face beside the chain the
// blend repeats (G0 through the edges, G1 tangent, G2 also curved as the
// face) and how full it leaves (0 the plain bridge; > -1).
struct BlendEnd {
  EdgeChain chain;
  Continuity continuity = Continuity::G1;
  double bulge = 0.0;
};

// A new surface body of one face "<feature>:blend" bridging the two
// chains (open or both closed), each end meeting its face as asked. The
// blend is cubic across (quintic with a G2 end) and fitted through points
// along the chains.
ShapePtr blend_surface(const std::string& feature, const BlendEnd& from, const BlendEnd& to);

// A curve of a boundary surface: a chain of sketch curves (in order, end
// to end) on `plane`, a chain of a body's edges, or a point.
struct BoundaryCurve {
  enum class Kind { Curves, Edges, Point };
  Kind kind = Kind::Curves;
  Path curves;
  gp_Pln plane;
  EdgeChain edges;
  gp_Pnt point;
  // How a boundary loop meets the face beside the edges (the plane along
  // sketch curves).
  Continuity continuity = Continuity::G0;
};

// A new surface body of one face "<feature>:boundary" filling the closed
// loop of boundaries (in order), meeting each as its continuity asks. The
// face's edges are the boundaries' curves; chains of sketch curves whose
// ends lie within 1e-6 of the largest coordinate of those ends (at least
// 1e-6 mm, as the model joins them) share a corner (mitcad#2220).
//
// The face also runs through the `guides` (mitcad#2526): sketch curves
// inside the loop (each curve of a guide's `curves` a guide of its own;
// they need not join each other or the loop), which neither bound nor
// split it, so its edges are the loop's alone. With guides, the face must
// lie within kGuideTolerance of every guide and of the loop's curves;
// refused in words otherwise (placed_error: the guide that the face
// misses and its gap in the detail, the guide and the farthest point
// painted), and a guide that is not
// sketch curves is refused.
inline constexpr double kGuideTolerance = 1.0e-2;
ShapePtr boundary_fill(const std::string& feature, const std::vector<BoundaryCurve>& loop,
                       const std::vector<BoundaryCurve>& guides = {});

// A new surface body of one face "<feature>:boundary" through a network:
// lofted through the curves `u` in order (the first or the last may be a
// point) when `v` is empty, else a Coons (two by two) or Gordon surface
// through every curve of `u` and of `v`, which must cross each other
// within 1e-3 mm. With `v`, each open curve is first cut back to its first
// and last crossing (curves may run on past the outer crossing curves,
// mitcad#2333). Refused in words (placed_error): a curve one way that does
// not meet a crossing curve (each such pair, its gap and nearest points in
// the detail, both curves painted by points along them), or curves that
// cross but no surface runs through (the fit's report in the detail, the
// crossings painted).
ShapePtr boundary_network(const std::string& feature, const std::vector<BoundaryCurve>& u,
                          const std::vector<BoundaryCurve>& v);

// The surface body with its face `face` (its only face when empty; one
// bounded by its surface's parameter range alone) rebuilt: the side
// nearest the target's chain moved onto it and meeting the target's face
// beside the chain with `continuity`, the opposite side kept. The face
// keeps its name; the body's faces are sewn again.
ShapePtr match_surface(const Shape& body, const std::string& face, const EdgeChain& target,
                       Continuity continuity);

// How Fair Surface moves a B-spline face's control points.
struct Fairing {
  double strength = 1.0; // the share of a full step per iteration, (0, 1]
  int iterations = 1;
  bool keep_edges = true; // also hold the row next to the boundary
  double tolerance = 0.1; // the farthest a control point moves
};

// The body with its B-spline faces among `faces` (references; every face
// when empty; each bounded by its surface's parameter range alone)
// faired: inner control points moved towards a net of less bending,
// boundary rows kept. Faces of other surface types stay as they are; the
// faces keep their names.
ShapePtr fair_surface(const Shape& body, const std::vector<std::string>& faces, const Fairing& fairing);

} // namespace mitcad::geometry
