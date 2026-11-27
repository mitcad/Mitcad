// SPDX-License-Identifier: MIT
#pragma once

// Surface modelling (mitcad#455): Offset Surface, Ruled Surface and
// Extend. None changes its inputs. Lengths are millimetres, angles
// radians.

#include <optional>
#include <string>
#include <vector>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

#include "mitcad/geometry/shape.hpp"
// Curves of free edges (mitcad#2395).
#include "mitcad/geometry/sketch_ref.hpp"

namespace mitcad::geometry {

// A new surface body of the faces (references; every face when empty;
// with `chain` also the faces joined to them through smooth edges) offset
// `distance` along their normals, the faces' orientations applied
// (against them when negative; 0 copies them). Faces
// "<feature>:offset(<face>)" after the face each comes from.
ShapePtr offset_surface(const std::string& feature, const Shape& body, const std::vector<std::string>& faces,
                        double distance, bool chain);

// Free edges of a body (they have no names): those of its open boundary
// at `index` (open_boundaries, surfaces.hpp), only those of the faces
// named in `faces` when any are.
struct OpeningEdges {
  int index = 0;
  std::vector<std::string> faces;
  std::vector<gp_Pnt> at; // of those, the edge nearest each point (none: all)
};

// Which way a ruled strip leaves its edge: along the normal of the face
// beside it, continuing the face (square to the edge, away from the face),
// at `angle` from the normal (or from `direction`) turned towards the
// face's continuation, or along `direction`.
enum class RuledMode { Normal, Tangent, Angle, Direction };
// Neighbouring strips at a sharp corner: leaning to meet along the
// corner's bisector, or a sector turned about the corner between them.
enum class RuledCorners { Mitre, Round };

struct RuledSpec {
  std::string feature;
  std::vector<std::string> edges; // references that join into chains (a strip each)
  std::vector<OpeningEdges> openings; // and free edges
  std::string face;               // the face beside the chain; empty: each edge's one face
  double distance = 0.0;          // > 0
  RuledMode mode = RuledMode::Normal;
  double angle = 0.0;
  std::optional<gp_Dir> direction;
  bool flip = false;
  RuledCorners corners = RuledCorners::Mitre;
};

// A new surface body: a strip `distance` wide off the chain of the body's
// edges, a face "<feature>:ruled(<edge>)" per named edge ("ruled(<face>)"
// for a free edge, after the face it leaves) and
// "<feature>:corner(<vertex>)" per round corner, sewn into one body.
ShapePtr ruled_surface(const Shape& body, const RuledSpec& spec);

// How Extend continues a surface: the surface itself, or leaving the edge
// tangent to it (G1) or also with its curvature (G2).
enum class ExtendMode { Natural, Tangent, Curvature };

// The surface body with the faces along the edges (references to free
// edges, and free edges of openings, each running along a side of its
// face's parameter range; a face bounded by that range alone) extended
// `distance` past them along the surface. A planar face trimmed inside its
// plane (mitcad#1955) is extended past straight edges of any direction:
// the plane continued square to each edge, joined to the face as one face.
// Natural also extends a face trimmed inside its surface's range, or past
// an edge that runs across that range (mitcad#2525: the rounded corners of
// an offset shell), and a plane past a curved edge (in every mode): the
// surface continued as it runs (a B-spline's end polynomials carried on),
// geodesics square to the edge `distance` long, the face's other edges
// kept and the extension's sides continuing its neighbouring edges. The
// faces keep their names; the body's faces are sewn again, far corners of
// neighbouring faces' extensions within 0.01 mm of each other together.
ShapePtr extend_surface(const Shape& body, const std::vector<std::string>& edges,
                        const std::vector<OpeningEdges>& openings, double distance, ExtendMode mode);

// Curves of free edges (mitcad#2395: a loft's section of a surface body's
// free edges): the free edges `opening` picks, as the features above pick
// an opening's edges, in the opening's order, each with its curve as
// curves_of gives an edge's (sketch_ref.hpp) and the name of the face it
// bounds (empty when that face has none). Throws for an opening the body
// does not have.
struct FreeEdgeCurve {
  ModelCurve curve;
  std::string face;
};
std::vector<FreeEdgeCurve> opening_curves(const Shape& body, const OpeningEdges& opening);

} // namespace mitcad::geometry
