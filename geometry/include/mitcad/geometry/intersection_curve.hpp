// SPDX-License-Identifier: MIT
#pragma once

// Intersection curves (mitcad#1405): the 3D curve two sketches' curves make
// when each is swept along its own sketch's normal, where the two swept
// surfaces cross. Lengths are millimetres. The curves are left as they are.

#include <string>
#include <vector>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

#include "mitcad/geometry/path_sweep.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/sketch_ref.hpp"

namespace mitcad::geometry {

// Where the surface of the chain `first` (curves end to end in order, as a
// path's) swept along `first_direction` crosses the surface of the chain
// `second` swept along `second_direction`. Each chain is swept both ways
// from where it lies, far enough to pass through the other chain wherever
// it is (beyond the box of both). The crossing's pieces come as curves_of
// gives an edge's curve, each once and in no particular order; a piece ends
// where a face of either surface ends (where two curves of a chain meet).
// Empty when the surfaces do not cross. Throws for directions less than
// 1e-9 radians apart (parallel either way), for chains whose curves do not
// join, and when the section fails.
std::vector<ModelCurve> intersection_curves(const Path& first, const gp_Dir& first_direction, const Path& second,
                                            const gp_Dir& second_direction);

// The curve tools (mitcad#1777).

// A helix about the axis through `origin` along `direction` (the way it
// rises), starting at origin + radius x_axis (x_axis at right angles to the
// direction), turning `revolutions` times right-handed about the direction
// (left-handed with `left_handed`) while it rises `pitch` and moves `growth`
// out from the axis per turn (negative: in).
struct HelixCurve {
  gp_Pnt origin;
  gp_Dir direction;
  gp_Dir x_axis;
  double radius = 0.0;
  double pitch = 0.0;
  double revolutions = 0.0;
  bool left_handed = false;
  double growth = 0.0;
};

// The helix as one curve, as curves_of gives an edge's: a line on the
// cylinder (no growth) or the cone (a growth) its turns lie on, made 3D
// within 1e-7 mm. Throws for a radius, pitch or revolutions that are not
// greater than zero, a growth that is not finite, and a helix that reaches
// its axis before its last turn.
std::vector<ModelCurve> helix_curve(const HelixCurve& helix);

// One side of a section of shapes: the whole shape when `faces` is empty,
// else the faces each name resolves to (Shape::find_faces).
struct SectionPart {
  ShapePtr shape;
  std::vector<std::string> faces;
};

// Where the shapes of `first` together meet the shapes of `second` together
// (OCCT's section of the two, approximated): the pieces as curves_of gives
// an edge's curve, each once and in no particular order; a piece ends where
// a face of either side ends. Empty when they do not meet. The shapes are
// left as they are. Throws for a side without shapes, a face name that
// finds no face of its shape, and when the section fails.
std::vector<ModelCurve> shape_section(const std::vector<SectionPart>& first, const std::vector<SectionPart>& second);

} // namespace mitcad::geometry
