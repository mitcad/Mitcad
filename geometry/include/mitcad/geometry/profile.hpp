// SPDX-License-Identifier: MIT
#pragma once

// Sketch profiles: regions bounded by loops of named curve segments, placed
// in 3D by a sketch frame. Lengths are millimetres, angles radians.

#include <string>
#include <utility>
#include <vector>

#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

enum class SegmentKind { Line, Arc, Circle, Ellipse, EllipseArc, BSpline };

// One curve piece of a profile loop, in sketch coordinates. Only the fields
// of its kind are used.
struct Segment {
  std::string name; // segment key, e.g. "c3[c2,c4]"
  SegmentKind kind = SegmentKind::Line;
  gp_Pnt2d start, end;            // Line
  gp_Pnt2d center;                // Arc, Circle, Ellipse, EllipseArc
  double radius = 0.0;            // Arc, Circle; the major radius of an ellipse
  double minor_radius = 0.0;      // Ellipse, EllipseArc
  double rotation = 0.0;          // ellipse major axis, from the sketch x axis; where a whole circle starts
  double start_angle = 0.0;       // Arc, EllipseArc: counter-clockwise from
  double end_angle = 0.0;         // start to end (parameter angles of an ellipse)
  int degree = 0;                 // BSpline
  std::vector<gp_Pnt2d> poles;    //
  std::vector<double> weights;    // empty: non-rational
  std::vector<double> knots;      // distinct knots
  std::vector<int> multiplicities;
  bool periodic = false;
};

// A closed boundary; segments in order around it, each in either direction.
struct Loop {
  std::vector<Segment> segments;
};

// A planar region: the first loop is the outer boundary, the others holes.
struct Region {
  std::string name; // region key, e.g. "r{c1[c4,c2],...}"
  std::vector<Loop> loops;
};

// Places sketch coordinates in 3D: p = origin + u x_axis + v y_axis. The
// sketch normal is x_axis × y_axis.
struct Frame {
  gp_Pnt origin{0.0, 0.0, 0.0};
  gp_Dir x_axis{1.0, 0.0, 0.0};
  gp_Dir y_axis{0.0, 1.0, 0.0};

  gp_Dir normal() const { return x_axis.Crossed(y_axis); }
  gp_Pnt point(const gp_Pnt2d& p) const;
};

// The face of a region with its boundary edges and their segment names.
struct ProfileFace {
  TopoDS_Face face;
  std::vector<std::pair<TopoDS_Edge, std::string>> edges;
  // A hole of the region touches its outer boundary at a vertex they share;
  // the prism of such a face is not a manifold solid, so touching profiles
  // are merged before they are extruded.
  bool pinched = false;
};

// Throws std::invalid_argument for invalid geometry (open loops, zero
// sizes, a minor radius larger than the major one, ...).
ProfileFace make_profile(const Frame& frame, const Region& region);

// As above, with an explicit upper bound on gap healing instead of the
// imported-sketch default. Material-domain construction supplies its own
// modelling tolerance; larger gaps must not silently become closed contours.
ProfileFace make_profile(const Frame& frame, const Region& region, double max_gap);

// The edge of one segment placed by the frame, open or closed as the
// segment is (mitcad#1768: curves of a sketch on a curved face, laid onto
// the face). Throws std::invalid_argument for invalid geometry.
TopoDS_Edge segment_edge(const Frame& frame, const Segment& segment);

// The faces of the regions, for display; unnamed.
ShapePtr profile_shape(const Frame& frame, const std::vector<Region>& regions);

// Regions of the basic sketch shapes on the given frame, for previews.
Region rectangle_region(double x, double y, double width, double height);
Region circle_region(double cx, double cy, double radius);

} // namespace mitcad::geometry
