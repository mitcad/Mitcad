// SPDX-License-Identifier: MIT
#pragma once

// Measurements and descriptions of shapes.

#include <string>
#include <vector>

#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

struct MassProperties {
  double volume = 0.0;
  double area = 0.0;
  gp_Pnt center; // of the volume, or of the area for shapes without volume
};

MassProperties mass_properties(const Shape& shape);
double volume(const Shape& shape);

// Axis-aligned box around the geometry (not enlarged by tolerances).
struct BoundingBox {
  bool empty = true;
  gp_Pnt min;
  gp_Pnt max;
};

BoundingBox bounding_box(const Shape& shape);

struct FaceInfo {
  NameList names;
  std::string surface; // plane, cylinder, cone, sphere, torus, bspline, ...
  double area = 0.0;
};

struct EdgeInfo {
  std::string name;  // empty when unnamed
  std::string curve; // line, circle, ellipse, bspline, ...
  double length = 0.0;
};

// In the shape's face and edge order (Shape::face, Shape::edge).
std::vector<FaceInfo> face_infos(const Shape& shape);
std::vector<EdgeInfo> edge_infos(const Shape& shape);

// The middle of an edge: half its length along it from its start, the unit
// tangent there, and its length.
struct EdgeMiddle {
  std::string name;
  gp_Pnt point;
  gp_Vec tangent;
  double length = 0.0;
};

// Every named edge's middle, in the shape's edge order: what path_point
// gives for each edge alone, in one pass (.f3d import, T1d).
std::vector<EdgeMiddle> edge_middles(const Shape& shape);

// Points inside a face, spread over it: the middles of the cells of a grid
// over its parameters that lie inside it, at most `count`.
struct FacePoints {
  std::string name; // its first name
  std::vector<gp_Pnt> points;
};

// Every named face's points, in the shape's face order (.f3d import: the
// faces a replace face replaced, P5). A face too thin for the grid has none.
std::vector<FacePoints> face_points(const Shape& shape, int count = 9);

} // namespace mitcad::geometry
