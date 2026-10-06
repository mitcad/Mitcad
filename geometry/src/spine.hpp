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

#include <Geom_Curve.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
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

  // The part between arc lengths `from` < `to`; on a closed wire `from`
  // may be negative and the part runs on through the wire's start.
  TopoDS_Wire part(double from, double to) const;

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

// How a pipe shell turns its section along the spine.
struct PipeMode {
  enum class Kind { CorrectedFrenet, Binormal, Fixed };
  Kind kind = Kind::CorrectedFrenet;
  gp_Dir binormal;
  gp_Ax2 fixed;
};

// Keeps a section at its angle to the spine: a fixed binormal along the
// normal of a planar spine, else the corrected Frenet frame.
PipeMode perpendicular_mode(const Spine& spine);

// The loop swept along the spine into a solid: sides named after its edges,
// the cap at the spine's start `first` and at its end `last`. `tolerance`
// bounds the approximation of the swept surfaces.
ShapePtr pipe_shell(const NamedWire& loop, const TopoDS_Wire& spine, const PipeMode& mode,
                    const NameList& first, const NameList& last, double tolerance = 1.0e-4);

// The solid through the sections (wires, or vertices first and last), side
// faces named after the edges of `named` (one of the sections), caps
// `first` and `last`. `compatible` lets the algorithm match the wires' edges
// and starts; a closed loft repeats its first section at the end.
ShapePtr through_sections(const std::vector<TopoDS_Shape>& sections, const NamedWire& named,
                          const NameList& first, const NameList& last, bool ruled, bool compatible);

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
