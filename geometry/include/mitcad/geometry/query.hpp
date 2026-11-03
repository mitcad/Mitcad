// SPDX-License-Identifier: MIT
#pragma once

// Measurements and descriptions of shapes.

#include <array>
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

// The area of faces that are not a body of their own (a section's caps),
// each face once, integrated as mass_properties integrates a body's faces
// (mitcad#140), so that every area MitCad reports agrees (mitcad#1200).
double faces_area(const std::vector<TopoDS_Face>& faces);

// Axis-aligned box around the geometry (not enlarged by tolerances).
struct BoundingBox {
  bool empty = true;
  gp_Pnt min;
  gp_Pnt max;
};

BoundingBox bounding_box(const Shape& shape);

// A box that holds the shape, found from the faces' control points and the
// shape's tolerances: looser than bounding_box's, but without its cost on
// bodies of many curved faces. The tight box where bounding_box has
// computed it already.
BoundingBox enclosing_box(const Shape& shape);

// The tight box of each face (edge) at these indices of the shape's face
// (edge) order, as bounding_box measures a body; empty for a face without
// geometry or a degenerate edge (the faces and edges queries, mitcad#714).
std::vector<BoundingBox> face_bounding_boxes(const Shape& shape, const std::vector<int>& faces);
std::vector<BoundingBox> edge_bounding_boxes(const Shape& shape, const std::vector<int>& edges);

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

// A box {xmin, ymin, zmin, xmax, ymax, zmax} that holds each face, in the
// shape's face order, worked out once per shape on all cores: no point of
// a face lies outside its box, so a point farther than a distance from the
// box is farther from the face (the .f3d import's face lookups through a
// point, which measured the point's distance from every face of a body of
// 23 000 faces, mitcad#119). Faces whose surfaces bound no such box (offset,
// swept and other surfaces) get an unbounded one.
std::vector<std::array<double, 6>> face_boxes(const Shape& shape);

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
// The same of Shapes: only the faces of `before` whose boxes (face_boxes,
// kept with each shape) hold the centre of a face of `after` are measured,
// with the same result (mitcad#658).
std::vector<NewFace> new_faces(const std::vector<const Shape*>& before, const Shape& after, int count);

// The volume a change makes as the faces it changed bound it (mm³): the
// faces of `after` that `before` does not have, as new_faces tells them,
// less those of `before` that `after` does not have, integrated about the
// centre of their boxes; and how uncertain that is from those faces'
// edges (a third of each edge's length times its tolerance beyond 1e-7 mm
// times its distance from that centre). (.ipt import: the check of an
// item's change, mitcad#235.)
struct ChangedVolume {
  double volume = 0.0;
  double slack = 0.0;
};

ChangedVolume changed_volume(const std::vector<TopoDS_Shape>& before, const std::vector<TopoDS_Shape>& after);
// The same of Shapes, measuring only the faces whose boxes may hold a
// changed face's centre, as new_faces of Shapes does (mitcad#658).
ChangedVolume changed_volume(const std::vector<const Shape*>& before, const std::vector<const Shape*>& after);

} // namespace mitcad::geometry
