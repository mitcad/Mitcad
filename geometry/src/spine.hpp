// SPDX-License-Identifier: MIT
#pragma once

// Paths and what is swept along them in the sweep family (F3): wires from
// path curves, positions and parts by arc length, frames that follow a path
// without twisting, and named pipe shells and section lofts. Internal to
// the geometry library.

#include <optional>
#include <string>
#include <utility>
#include <vector>

#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <Geom_Curve.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_Circ.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>

#include "mitcad/geometry/path_sweep.hpp"
#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// The edge of a curve in model space.
TopoDS_Edge model_edge(const ModelCurve& curve);

// The path's curves joined into a wire; throws when they do not join.
TopoDS_Wire path_wire(const Path& path);

// A wire measured along its length.
class Spine {
public:
  explicit Spine(const TopoDS_Wire& wire);

  const TopoDS_Wire& wire() const { return m_wire; }
  double length() const { return m_length; }
  bool closed() const { return m_closed; }
  int edge_count() const { return static_cast<int>(m_pieces.size()); }

  // The point and the unit tangent at arc length s (clamped to the wire).
  gp_Pnt point(double s) const;
  gp_Dir tangent(double s) const;

  // The normal of the plane the wire lies in; none for a straight or a
  // twisted wire.
  std::optional<gp_Dir> plane_normal() const;

  // The arc length where the wire crosses the plane (the crossing nearest
  // to `near`), or of its point nearest to `near` when it does not cross.
  double locate(const gp_Pln& plane, const gp_Pnt& near) const;

  // The arc length of the wire's point nearest to `near`.
  double nearest(const gp_Pnt& near) const;

  // The arc lengths where the wire crosses the plane, in order along it
  // (mitcad#469). An end of an open wire within `tolerance` of the plane
  // counts as a crossing there (a rail drawn to end where a path ends),
  // unless a crossing is found within `tolerance` of it.
  std::vector<double> crossings(const gp_Pln& plane, double tolerance) const;

  // The part between arc lengths `from` < `to`; on a closed wire `from`
  // may be negative and the part runs on through the wire's start.
  TopoDS_Wire part(double from, double to) const;

  // The wire's edges in order (mitcad#1697), `i` from 0 to edge_count():
  // the arc length where each starts and its length; its point and unit
  // tangent (along the wire) at `local` along it from its start; its
  // curvature there with the unit normal towards the centre of the
  // curvature, none where it runs straight; and its circle when it is a
  // circular arc, the circle's axis turned so that the wire runs round it
  // counter-clockwise.
  double edge_start(int i) const { return m_pieces.at(static_cast<std::size_t>(i)).start; }
  double edge_length(int i) const { return m_pieces.at(static_cast<std::size_t>(i)).length; }
  gp_Pnt edge_point(int i, double local) const;
  gp_Dir edge_tangent(int i, double local) const;
  std::optional<std::pair<double, gp_Dir>> edge_bend(int i, double local) const;
  std::optional<gp_Circ> edge_circle(int i) const;
  bool edge_straight(int i) const;

  // (mitcad#1709) The edge's curve and its parameter range, and whether
  // the wire runs along it the way its parameter grows.
  struct EdgeCurve {
    occ::handle<Geom_Curve> curve;
    double first = 0.0;
    double last = 0.0;
    bool forward = true;
  };
  EdgeCurve edge_curve(int i) const;

private:
  struct Piece {
    TopoDS_Edge edge;
    occ::handle<Geom_Curve> curve;
    double first = 0.0; // the edge's parameter range
    double last = 0.0;
    bool forward = true;
    double start = 0.0; // arc length along the wire where it starts
    double length = 0.0;
  };

  const Piece& piece_at(double s, double& local) const;
  // The curve parameter `local` along the piece from its start in the
  // wire's direction.
  double parameter(const Piece& piece, double local) const;
  void add_part(std::vector<TopoDS_Edge>& edges, double from, double to) const;

  TopoDS_Wire m_wire;
  std::vector<Piece> m_pieces;
  double m_length = 0.0;
  bool m_closed = false;
};

// Frames at arc lengths `stations` that follow the spine without twisting
// about it (rotation minimizing): z along the tangent, x turned from
// `normal` at arc length `s0`, where the frames start.
std::vector<gp_Ax3> follow_frames(const Spine& spine, double s0, const gp_Dir& normal,
                                  const std::vector<double>& stations);

// A loop of a profile and the names of the faces its edges sweep.
struct NamedWire {
  TopoDS_Wire wire;
  std::vector<std::pair<TopoDS_Edge, std::string>> edges;
};

// The loops of a face, the outer one first, with the names of their edges.
std::vector<NamedWire> face_loops(const TopoDS_Face& face,
                                  const std::vector<std::pair<TopoDS_Edge, std::string>>& names);

// How a pipe shell turns its section along the spine. `Support`: with the
// normal of the faces of `support` the spine's edges lie on (each edge
// carries its curve on one of them; see `on_faces`).
// `Auxiliary` (mitcad#161): with the normal towards `auxiliary`, a curve
// beside the spine (see `normal_spine`), its point where it crosses the
// plane square to the spine, or with `curvilinear` (mitcad#539) at the same
// fraction of its length as the spine's point. `Frenet` (mitcad#539): the
// spine's Frenet frame.
struct PipeMode {
  enum class Kind { CorrectedFrenet, Binormal, Fixed, Support, Auxiliary, Frenet };
  Kind kind = Kind::CorrectedFrenet;
  gp_Dir binormal;
  gp_Ax2 fixed;
  TopoDS_Shape support;
  TopoDS_Wire auxiliary;
  bool curvilinear = false;
};

// A copy of the wire whose edges each carry their curve on the first of
// the faces they lie on, within `tolerance` (a pipe shell's `Support`
// mode). Throws when an edge lies on none of them.
TopoDS_Wire on_faces(const TopoDS_Wire& wire, const std::vector<TopoDS_Face>& faces, double tolerance);

// Whether every edge of the wire lies on one of the faces within
// `tolerance` (as `on_faces` takes them).
bool lies_on_faces(const TopoDS_Wire& wire, const std::vector<TopoDS_Face>& faces, double tolerance);

// A curve beside a wire that runs beside faces (mitcad#161: a sweep along
// a thread's crest guided by its flank): through the points `offset` off
// the wire's points along the normal of the faces at their points nearest
// to them, taken at right angles to the wire. A pipe shell's `Auxiliary`
// mode then turns its section with the faces' normal, as `Support` does
// for a wire on the faces.
TopoDS_Wire normal_spine(const TopoDS_Wire& wire, const std::vector<TopoDS_Face>& faces, double offset);

// The unit normal of the faces (a compound; with their orientation) at
// their point nearest to `point`, or nothing where none is found.
std::optional<gp_Dir> guide_normal(const TopoDS_Shape& faces, const gp_Pnt& point);

// Keeps a section at its angle to the spine: a fixed binormal along the
// normal of a planar spine, else the corrected Frenet frame.
PipeMode perpendicular_mode(const Spine& spine);

// The same for the spine of a path: along a lone helix (mitcad#385) the
// helix's axis as the fixed binormal, so the section turns with the helix
// about its axis (a screw motion) and keeps its angle to the axis.
PipeMode perpendicular_mode(const Spine& spine, const Path& path);

// Sets the pipe shell's mode (its trihedron) to `mode` (mitcad#543: shared
// by sweeps and lofts along a centre line). Throws for a `Support` the
// spine does not lie on.
void set_mode(BRepOffsetAPI_MakePipeShell& pipe, const PipeMode& mode);

// The mode of a frame of a sweep or of a loft along a centre line
// (mitcad#539, mitcad#543): the Frenet frame, an auxiliary curve (by the
// plane rule, or the fraction rule with `curvilinear`) or a constant
// binormal; none of them set: `fallback`.
PipeMode frame_mode(bool frenet, const Path& auxiliary, bool curvilinear,
                    const std::optional<gp_Dir>& constant_binormal, const PipeMode& fallback);

// The loop swept along the spine into a solid: sides named after its edges,
// the cap at the spine's start `first` and at its end `last`. `tolerance`
// bounds the approximation of the swept surfaces. The spine's corners are
// joined as `corners` asks (mitcad#1448: a pipe's Corners); where that
// cannot be built, the other kind, then the loop turned through them.
ShapePtr pipe_shell(const NamedWire& loop, const TopoDS_Wire& spine, const PipeMode& mode,
                    const NameList& first, const NameList& last, double tolerance = 1.0e-4,
                    PipeCorners corners = PipeCorners::Mitered);

// The solid through the sections (wires, or vertices first and last), side
// faces named after the edges of `named` (one of the sections), caps
// `first` and `last`. `compatible` lets the algorithm match the wires' edges
// and starts; a closed loft repeats its first section at the end. `solid`
// false: the side faces only (a shell, no caps).
ShapePtr through_sections(const std::vector<TopoDS_Shape>& sections, const NamedWire& named,
                          const NameList& first, const NameList& last, bool ruled, bool compatible,
                          bool solid = true);

// The outer solid with the holes' solids cut out; the names carry.
ShapePtr with_holes(const ShapePtr& outer, const std::vector<ShapePtr>& holes, const char* what);

class FaceNamer;

// Names the unnamed faces of the namer's result that the section bounds or
// is (the caps of a sweep), else the unnamed face nearest to the section.
void name_caps(FaceNamer& namer, const TopoDS_Shape& section, const NameList& names);

// Makes a closed shell or solid a solid with its material inside.
TopoDS_Shape oriented_solid(const TopoDS_Shape& shape, const char* what);

// A unit vector at right angles to `direction`.
gp_Dir any_normal(const gp_Dir& direction);

} // namespace mitcad::geometry::detail
