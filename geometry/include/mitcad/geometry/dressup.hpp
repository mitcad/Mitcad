// SPDX-License-Identifier: MIT
#pragma once

// Fillets and chamfers on named edges, in edge sets.
//
// OCCT always extends a selected edge along its tangent chain (the edges
// that continue it smoothly). A set with `tangent_chain` false whose chain
// reaches an edge that no set selects therefore fails with an "unsupported:"
// message, as do options OCCT lacks (see the model's fillet and
// chamfer features); the importer then falls back to the stored body.
//
// When OCCT's dressup fails at the sizes asked, the same OCCT algorithm is
// tried again on the body with the pieces of its edges merged (a whole
// circle in two arcs between the same faces, on which it fails), then, for
// fillets, with tighter tolerances (knife edges), keeping the names of
// every piece. A result that OCCT's checker rejects only where the input
// already was invalid, away from the dressup, is taken as it is.
//
// OCCT fails where a rounding or bevel takes a neighbouring face away
// exactly (a radius as large as the face is wide). A fillet or chamfer that
// fails is therefore built once more with its sizes 1e-3 (relative)
// smaller, which leaves a strip that narrow where the face was. When that
// fails too and every selected edge is a whole circle between two faces of
// revolution about its axis with straight sections, the dressup is built
// as rings (src/ring_dressup.hpp): its cross-section turned about the
// axis, cut from the body or joined to it, running over any face it meets.

#include <string>
#include <utility>
#include <vector>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

enum class FilletSize { Constant, ChordLength, Variable, Asymmetric };

// Edges with one size: the named edges and every edge of the named faces.
struct FilletSet {
  std::vector<std::string> edges;
  std::vector<std::string> faces;
  FilletSize size = FilletSize::Constant;
  double radius = 0.0;  // Constant; Variable: at the start; Asymmetric: the first distance
  double radius2 = 0.0; // Variable: at the end; Asymmetric: the second distance
  double chord = 0.0;   // ChordLength: the width across the fillet
  // Variable: the vertex (name) at the start of the chain; empty for the
  // start of OCCT's spine.
  std::string start_vertex;
  // Variable: radii between the ends, (position along the chain from the
  // start, 0 to 1; radius), positions increasing. The radius follows a
  // smooth interpolation of all the radii, level at the ends.
  std::vector<std::pair<double, double>> mid;
  // Asymmetric: the face the first distance is measured on; empty for the
  // first face of each edge's name. `flip` takes the edge's other face.
  std::string reference_face;
  bool flip = false;
  // Curvature continuous (G2) cross-sections, shaped by the tangency
  // weight (0.1 to 2; 1 is the default shape).
  bool curvature = false;
  double weight = 1.0;
  bool tangent_chain = true;
};

// Rounds the edge sets of the body. The new faces are named after `feature`:
//   <feature>:fillet(<edge>)    the rounding along an edge (also the edges
//                               the tangent chain adds)
//   <feature>:corner(<vertex>)  a blend where rounded edges meet
// with the edge and vertex names of the input body. The body's other faces
// keep their names.
//
// Asymmetric and curvature continuous (G2) sets are built from a chamfer:
// its contact lines on the faces stay, and its bevel becomes a surface of
// conic (asymmetric) or quintic (G2) cross-sections tangent to the faces,
// the G2 ones also following the faces' curvature. Their chains must end
// on planar faces or close on themselves; a chain that meets another
// rounded edge at a vertex is unsupported.
//
// Without rolling ball corners, a vertex where three or more rounded edges
// meet gets a setback corner: the roundings stop short of the vertex and a
// patch tangent to them and to the faces closes the corner.
ShapePtr fillet(const std::string& feature, const Shape& body, const std::vector<FilletSet>& sets,
                bool rolling_ball_corners = true);

// One constant radius set.
ShapePtr fillet(const std::string& feature, const Shape& body, const std::vector<std::string>& edges,
                double radius);

enum class ChamferType { EqualDistance, TwoDistances, DistanceAngle };

// Two distances and distance-angle chamfers measure `distance` (and the
// angle, from that face) on one face of each edge: the reference face when
// one is named, else the first face of the edge's name, E{<first>|<second>};
// `flip` takes the other face.
struct ChamferSpec {
  ChamferType type = ChamferType::EqualDistance;
  double distance = 0.0;
  double distance2 = 0.0; // TwoDistances
  double angle = 0.0;     // DistanceAngle, radians
  bool flip = false;
  std::string reference_face;
};

struct ChamferSet {
  std::vector<std::string> edges;
  std::vector<std::string> faces;
  ChamferSpec spec;
  bool tangent_chain = true;
};

// How the vertices where three or more bevelled edges meet are shaped:
// OCCT's own corner face, the bevels continued until they meet (miter: no
// face of its own), or a patch tangent to the bevels and the faces (blend).
enum class ChamferCorner { Chamfer, Miter, Blend };

// Bevels the edge sets; new faces are <feature>:chamfer(<edge>) and
// <feature>:corner(<vertex>). A miter or blend corner type where no corner
// is to be shaped makes the plain chamfer, with a note saying so.
ShapePtr chamfer(const std::string& feature, const Shape& body, const std::vector<ChamferSet>& sets,
                 ChamferCorner corner = ChamferCorner::Chamfer);

// One set of edges.
ShapePtr chamfer(const std::string& feature, const Shape& body, const std::vector<std::string>& edges,
                 const ChamferSpec& spec);

} // namespace mitcad::geometry
