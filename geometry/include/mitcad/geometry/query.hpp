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

// The measures OCCT's fixed Gauss points give every face (BRepGProp's
// default, as other OCCT-based programs measure): the volume of the solids
// (each counted positive), the area and the centre of the solids by volume,
// else of the faces by area. Mitcad's own (mass_properties) integrate every
// face span by span (mitcad#140); these are for comparing with such
// programs' measures of the same shape.
MassProperties fixed_point_properties(const Shape& shape);

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

// A face an operation made or changed, with its area and up to `count`
// points spread over it (as face_points places them).
struct NewFace {
  double area = 0.0;
  std::vector<gp_Pnt> points;
};

// The faces of `after` that none of `before` has, in its face order: a face
// counts as one `before` has when it is the same face, or one on the same
// kind of surface with the same area and centre (a body rebuilt from the
// same data). (.f3d import: the geometric check of an item's change,
// mitcad#138.)
std::vector<NewFace> new_faces(const std::vector<TopoDS_Shape>& before, const TopoDS_Shape& after,
                               int count);

} // namespace mitcad::geometry
