// SPDX-License-Identifier: MIT
#pragma once

// Fillets and chamfers on named edges, in edge sets.
//
// OCCT always extends a selected edge along its tangent chain (the edges
// that continue it smoothly) where the faces at the vertex between them go
// on tangentially; it stops where another sharp edge meets that vertex. A
// set with `tangent_chain` (constant and chord length fillets, chamfers)
// therefore also takes Mitcad's own chain first: every edge that meets one
// of its edges end to end in the same direction (within 0.01 rad) and lies
// between faces at an angle, and so on from those (mitcad#224); where OCCT
// cannot build that, the set takes OCCT's chain alone, with a note. OCCT's
// chain of a fillet stops where its edges turn although the faces go on
// tangentially across a shallow crease, unless the next edge is selected
// (port patch 0180, mitcad#241); the rounding then ends at that vertex, cut
// by the face beyond the crease. A set with
// `tangent_chain` false whose chain
// reaches an edge that no set selects therefore fails with an "unsupported:"
// message, as do options OCCT lacks (see the model's fillet and
// chamfer features); the importer then falls back to the stored body.
//
// The contours go to OCCT in the order the sets list their edges: where two
// roundings of a face narrower than both cannot be one blend, the later
// rides on the earlier one's contact line, and the `.f3d` files' bodies make
// the rounding of the edge listed first the whole one. Where that order
// does not build, the contours are tried once more in the body's order of
// its edges (OCCT's join of roundings over a narrow face depends on which
// end it meets first).
//
// When OCCT's dressup fails at the sizes asked, the same OCCT algorithm is
// tried again on the body with the pieces of its edges merged (a whole
// circle in two arcs between the same faces, on which it fails), then, for
// fillets, with tighter tolerances (knife edges), keeping the names of
// every piece, and last on the body with the strips narrower than the
// import's tolerance of 0.01 at the selected edges' vertices collapsed into
// the one edge they stand for (a band of half a micrometre between two
// chamfers, at whose two vertices two roundings end; mitcad#299). A result that OCCT's checker rejects only where the input
// already was invalid, away from the dressup, is taken as it is.
//
// OCCT builds most roundings and bevels that take a neighbouring face away
// exactly (a radius as large as the face is wide) at their size, the
// pocket's corners where two of them take the walls away included (the
// port's patch 1100, mitcad#1106). A fillet of constant radius whose
// roundings each take a face away along a straight edge between planes
// (the ball rolling on that face's far edge) round a loop of such edges,
// two meeting at each corner as mirror images (a tray's outer top edges
// after its walls were bevelled all across), whose corners OCCT cannot
// join, is built at its size as swept sections mitred at the corners
// (src/mitred_dressup.hpp, mitcad#1118). Where it still fails, a fillet or
// chamfer is built once more with its sizes 1e-3 (relative) smaller, which
// leaves a strip that narrow where the face was; the strip is then removed
// (OCCT's defeaturing extends the faces round it to meet, the dressup's
// face takes its names), and the result has an info, not a warning, that
// it is 0.1 % smaller (Shape::infos). When that
// fails too and every selected edge is a whole circle between two faces of
// revolution about its axis with straight sections, the dressup is built
// as rings (src/ring_dressup.hpp): its cross-section turned about the
// axis, cut from the body or joined to it, running over any face it meets.
//
// Where OCCT walks a rounding (a ball rolling on a narrow face's far edge,
// a rounding between a plane and a cylinder it does not build in closed
// form), it makes B-spline faces and edges even where they are exactly a
// cylinder, lines and arcs. The new faces whose B-splines are a plane,
// cylinder, cone, sphere or torus within 1e-7 get that surface, and their
// B-spline edges that are lines or circles within it get those curves
// (src/canonical.hpp, mitcad#328), as in the bodies the `.f3d` files
// store: a later rounding along them failed in OCCT's walk on the
// B-splines and builds on the analytic faces.

#include <string>
#include <utility>
#include <vector>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

enum class FilletSize { Constant, ChordLength, Variable, Asymmetric, Spline };

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
  // Spline (mitcad#464): the radius a clamped B-spline of the position
  // along the chain (0 at the start vertex, 1 at the other end, by arc
  // length): its full knot vector and control radii, of degree
  // knots.size() - radii.size() - 1 (1 to 3). Not around a closed chain.
  std::vector<double> knots;
  std::vector<double> radii;
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
// `flip` takes the other face. Without a reference face the face is chosen
// once per tangent chain, on the first edge the set lists that lies in it
// (not per edge, and not on whichever edge of the chain has the lowest
// index), and the geometry kernel measures that face's side along the whole
// chain.
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
