// SPDX-License-Identifier: MIT
#pragma once

// Drawings (mitcad#471): the edges of bodies seen along a view with hidden
// lines removed, and the round holes a drawing dimensions and tabulates.
// Lengths are millimetres; angles radians.
//
// A view looks at the shapes from far away along -toward_eye (an
// orthographic projection). Its 2D coordinates are those of the plane at
// right angles to toward_eye through the model origin: x along x_axis, y
// along toward_eye x x_axis. The projection is OCCT's hidden line removal:
// HLRBRep_Algo on the exact B-rep (lines and circles stay lines and arcs,
// B-splines and Bézier curves come as their B-splines, other curves as
// B-splines within 1e-6 mm, each with a polyline of it too; mitcad#853),
// or HLRBRep_PolyAlgo on a triangulation
// of copies of the shapes when the exact one is not asked for or fails, or
// on the quick path (DrawingViewSpec::quick). The polygonal algorithm's
// segments are joined end to end and drawn as lines and arcs where they
// follow one within the triangulation's deviation (mitcad#962). The shapes
// are not modified.

#include <array>
#include <optional>
#include <string>
#include <vector>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

using Point2 = std::array<double, 2>;

// A plane cutting a section view: what lies on its normal side goes (Section
// Analysis' cut, analysis::clip), and the view looks along -normal at the
// cut, so toward_eye must be the normal.
struct DrawingSection {
  gp_Pnt origin;
  gp_Dir normal;
};

struct DrawingViewSpec {
  gp_Dir toward_eye{0.0, 0.0, 1.0};
  // At right angles to toward_eye.
  gp_Dir x_axis{1.0, 0.0, 0.0};
  // Also the hidden edges (DrawnCurve::visible false).
  bool hidden = false;
  // Also the edges between faces that meet tangentially.
  bool smooth = false;
  // The exact algorithm first; false: the polygonal one only.
  bool exact = true;
  // With `exact`: the quick path where the exact algorithm is slow, on
  // shapes with free-form faces (mitcad#962): the polygonal algorithm on a
  // triangulation within a 2000th of the shapes' size, its lines joined
  // and drawn as lines and arcs where they follow them within that
  // deviation, as polylines elsewhere (DrawnView::polygonal).
  bool quick = false;
  std::optional<DrawingSection> section;
  // The largest distance of a polyline from the curve it stands for.
  double deflection = 0.01;
};

enum class DrawnKind { Sharp, Smooth, Outline };

// One projected edge: a line (two points), an arc of a circle counter-
// clockwise from `start` to `end` (a full circle from 0 to 2 pi), a
// polyline, or a B-spline (mitcad#853): `degree`, its full knot vector
// (`poles.size() + degree + 1` knots), `poles` and `weights` (empty when it
// is not rational), from its start to its end, with `points` a polyline of
// it within the deflection.
struct DrawnCurve {
  enum class Type { Line, Arc, Polyline, Spline };
  bool visible = true;
  DrawnKind kind = DrawnKind::Sharp;
  Type type = Type::Line;
  std::vector<Point2> points;
  Point2 center{0.0, 0.0};
  double radius = 0.0;
  double start = 0.0;
  double end = 0.0;
  int degree = 0;
  std::vector<double> knots;
  std::vector<Point2> poles;
  std::vector<double> weights;
};

// The faces a section view's plane cuts, each as its closed loops
// (polylines, the last point not repeated), in view coordinates.
using CutFace = std::vector<std::vector<Point2>>;

struct DrawnView {
  std::vector<DrawnCurve> curves;
  std::vector<CutFace> cuts;
  // The polygonal algorithm made the view (asked for, or the exact one
  // failed).
  bool polygonal = false;
};

// The edges of the shapes seen along the view. Throws std::runtime_error
// when neither algorithm can project them.
DrawnView project_view(const std::vector<ShapePtr>& shapes, const DrawingViewSpec& spec);

// The polygonal algorithm's segments of one kind as project_view draws
// them (mitcad#962): joined end to end, then lines where they run straight
// within `tolerance`, arcs where at least three of them follow a circle
// within it (each segment's sagitta too, so a polygon's corners are not
// taken for a circle), polylines elsewhere. The curves are visible and
// sharp.
std::vector<DrawnCurve> fit_segments(const std::vector<std::array<Point2, 2>>& segments, double tolerance);

// A round hole: coaxial cylindrical faces with the material all round
// outside them (the pieces of a split cylinder taken together), the
// narrowest the drilled wall. A wider cylinder or a cone that meets it
// end to end on the side it opens to is its counterbore or countersink.
struct RoundHole {
  // The first name of the drilled wall's face; empty when unnamed.
  std::string face;
  // On the axis where the hole opens (the top of a counterbore or a
  // countersink), and the axis direction into the material. A hole open at
  // both ends opens towards +Z, else +Y, else +X.
  gp_Pnt opening;
  gp_Dir direction;
  double diameter = 0.0;
  bool through = false;
  // From the opening to the end of the drilled wall (also of a hole open
  // at both ends).
  double depth = 0.0;
  double cbore_diameter = 0.0; // 0: none
  double cbore_depth = 0.0;
  double csink_diameter = 0.0; // 0: none
  double csink_angle = 0.0;    // the full included angle
};

// The round holes of a shape, in a deterministic geometric order. An end of
// a hole is closed when material lies on its axis within twice its
// diameter beyond it (a flat bottom, a drill point).
std::vector<RoundHole> round_holes(const Shape& shape);

} // namespace mitcad::geometry
