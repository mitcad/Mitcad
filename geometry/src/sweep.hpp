// SPDX-License-Identifier: MIT
#pragma once

// Building blocks of the profile features (extrude, revolve, hole): profile
// faces with the names of the faces their edges sweep, named prisms, tapers,
// trimming by a target and fusing named solids. Internal to the geometry
// library.

#include <functional>
#include <string>
#include <utility>
#include <vector>

#include <Bnd_Box.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/target.hpp"

namespace mitcad::geometry::detail {

// A planar face to sweep, the full names of the faces its edges sweep (an
// edge may have several), and the keys of the profile regions it covers.
struct SweepFace {
  TopoDS_Face face;
  std::vector<std::pair<TopoDS_Edge, std::string>> edges;
  NameList regions;
};

// <feature>:<role>(<region>) for each region of the face, e.g. the names
// of its start cap.
NameList cap_names(const std::string& feature, const std::string& role, const SweepFace& face);

// The regions' faces; edges sweep <feature>:side(<segment>).
std::vector<SweepFace> region_faces(const std::string& feature, const Frame& frame,
                                    const std::vector<Region>& regions);

// Touching faces merged into one, as when several profiles of a sketch
// are swept together; the edges they shared disappear.
std::vector<SweepFace> merged(const std::vector<SweepFace>& faces);

// The walls of a thin extrusion along the regions' loops: one face per loop,
// edges sweeping <feature>:outer(<segment>) on side 1 and
// <feature>:inner(<segment>) on side 2 (see WallLocation).
std::vector<SweepFace> wall_faces(const std::string& feature, const Frame& frame,
                                  const std::vector<Region>& regions, const ThinWall& wall);

// The face moved or turned; the edge names follow.
SweepFace transformed(const SweepFace& face, const gp_Trsf& trsf);

// The prism of the face along `vector`: side faces named after the edges,
// the cap at the face `near` and the opposite cap `far`.
ShapePtr named_prism(const SweepFace& face, const gp_Vec& vector, const NameList& near,
                     const NameList& far);

// Names the faces a revolution by `angle` about `axis` swept from the
// edges where its history has none (OCCT reports no face for an edge at
// right angles to the axis): the face through the edge's middle turned by
// half the angle.
ShapePtr with_swept_names(const Shape& solid,
                          const std::vector<std::pair<TopoDS_Edge, std::string>>& edges,
                          const gp_Ax1& axis, double angle);

// Leans every face of the solid except the named caps about its line on
// the neutral plane, by `angle` away from the material (see
// ExtrudeSide::taper) as it runs along `pull`.
ShapePtr tapered(const Shape& solid, const gp_Pln& neutral, const gp_Dir& pull, double angle,
                 const NameList& caps);

// The plane of a plane target or of a planar face target, offset, with its
// normal turned along `along`.
gp_Pln target_plane(const Target& target, const gp_Dir& along);

// The faces of a face target (all pieces); throws when there are none.
std::vector<TopoDS_Face> target_faces(const Target& target);

// How far a sweep along `along` from the start (whose bounds are `start`)
// must run to pass the target completely. Throws when the target lies
// behind the start.
double reach(const Target& target, const gp_Dir& along, const Bnd_Box& start);

// The shape a sweep is split with to end at the target, large enough to
// cross everything within `around` and the target's own faces (planes and
// extended faces are bounded).
TopoDS_Shape target_tool(const Target& target, const gp_Dir& along, const Bnd_Box& around);

// A bounded face of the plane, wide enough to cross everything in `around`.
TopoDS_Face plane_tool(const gp_Pln& plane, const Bnd_Box& around);

// Splits a sweep with the tool of its target and keeps the part next to
// the face named `near`: up to the first face of the target reached, or to
// the far side of a body target run through. The tool's faces in the result
// are named `far`. Throws when the target does not cut across the whole
// sweep (the kept part would still hold the face named `unreached`) or
// cuts the face named `near`.
ShapePtr trimmed(const Shape& sweep, const Target& target, const TopoDS_Shape& tool,
                 const gp_Dir& along, const std::string& near, const NameList& far,
                 const std::string& unreached);

// Splits the solid with a tool and keeps the pieces for which `keep` is
// true; tool faces in them are named `tool_names`. Unfinished names.
ShapePtr split_keep(const Shape& solid, const TopoDS_Shape& tool, const NameList& tool_names,
                    const std::function<bool(const Shape&)>& keep, const char* what);

// Fuses the solids into one shape (several solids if they do not touch),
// merging the faces that end up in one plane or surface; names carry.
ShapePtr fused(const std::vector<ShapePtr>& parts, const char* what);

// The shape with its split names numbered (FaceNamer::finish) and checked.
ShapePtr finished(const Shape& shape, const char* what);

// A point inside the solid.
gp_Pnt interior_point(const TopoDS_Shape& solid);

// The bounding box of a shape, not enlarged by tolerances.
Bnd_Box bounds_of(const TopoDS_Shape& shape);

// True when the shape has a face carrying exactly this name.
bool has_face_named(const Shape& shape, const std::string& name);

} // namespace mitcad::geometry::detail
