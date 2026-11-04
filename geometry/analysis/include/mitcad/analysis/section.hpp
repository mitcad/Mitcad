// SPDX-License-Identifier: MIT
#pragma once

// Planar sections of bodies and the clipped view of section analysis.

#include <cstddef>
#include <string>
#include <vector>

#include "mitcad/analysis/common.hpp"

namespace mitcad::analysis {

// A plane through `origin` with normal `normal` (any length but zero).
struct Plane {
  Vec3 origin;
  Vec3 normal{0.0, 0.0, 1.0};
};

struct Section {
  // Compound of the section edges (the intersection with all faces).
  TopoDS_Shape curves;
  // Compound of the planar faces where the plane cuts the solids, with
  // holes; empty when the shape has no solids.
  TopoDS_Shape faces;
  std::size_t edge_count = 0;
  std::size_t face_count = 0;
  double length = 0.0; // of the curves, mm
  double area = 0.0;   // of the faces, mm^2
};

// Intersects a shape with a plane. A mesh body (faces with triangles and
// no surface) is cut triangle by triangle: one straight edge for each
// triangle the plane crosses, and faces only when its triangles close up
// (mitcad#392: OCCT's booleans crash on faces without a surface). A shape
// mixing mesh faces with faces on surfaces throws Error.
Section section(const TopoDS_Shape& shape, const Plane& plane);

// A face of a clipped shape and what is left of it: its faces in the
// clipped shape, none when it lies wholly on the removed side.
struct ClippedFace {
  TopoDS_Shape face;
  std::vector<TopoDS_Shape> pieces;
};

// The ways clip cuts a shape the plane crosses, tried in this order until
// one works (mitcad#1230): the plain boolean against the slab beyond the
// plane; the same with a fuzzy value (a small multiple of the shape's
// largest tolerance), for a plane that runs along edges and seams of the
// shape (a revolved body cut in its profile's plane); the plain boolean
// with the plane moved by kClipShift along its normal, the baseline
// tolerance, which no one sees.
enum class ClipAttempt { Plain = 1, Fuzzy = 2, Moved = 3 };

// How far the last attempt moves the plane along its normal, mm.
inline constexpr double kClipShift = 1e-6;

// How clip cut the shape: the attempt that worked, its fuzzy value or how
// far it moved the plane (mm; 0 for none), and why the attempts before it
// failed (empty after the plain one).
struct ClipReport {
  ClipAttempt attempt = ClipAttempt::Plain;
  double fuzzy = 0.0;
  double shift = 0.0;
  std::string failed;
};

// The shape with the half-space on the normal side of the plane removed,
// as section analysis shows it (the normal points towards the viewer).
// Faces lying on the plane stay whole: the removed half-space only touches
// them. With `faces`, what became of each face of the shape, in TopExp
// order (the caps on the plane come from none), so that the clipped
// shape's faces can keep the shape's names (mitcad#233). A mesh body is
// clipped triangle by triangle: each mesh face's piece is a new mesh face
// (the face itself when the plane removes nothing of it), and a closed
// mesh gets its section faces as planar caps facing along the normal;
// when nothing is removed at all, the result is the shape itself. Other
// shapes the plane does not reach (clip_side) are not cut at all
// (mitcad#438): the result is the shape itself when it lies behind the
// plane (each face its own piece), and an empty compound when it lies
// wholly on the normal side (no face has a piece).
//
// Any other shape is cut by the attempts of ClipAttempt in turn; with
// `report`, how it was cut. When every attempt fails, Error says so with
// each attempt's reason. The shape itself is never changed (the booleans
// leave their inputs as they are).
TopoDS_Shape clip(const TopoDS_Shape& shape, const Plane& plane,
                  std::vector<ClippedFace>* faces = nullptr, ClipReport* report = nullptr);

// One attempt of clip on a shape with surfaces, whichever side of the
// plane it lies on: the result as clip gives it, or Error when that cut
// fails. For tests of each attempt; clip tries them in turn.
TopoDS_Shape clip_by(const TopoDS_Shape& shape, const Plane& plane, ClipAttempt attempt,
                     std::vector<ClippedFace>* faces = nullptr);

// The signed distances (mm, along the plane's normal) of a box's corners
// from the plane: the least and the greatest. Throws Error for a zero
// normal; both 0 for an empty box.
struct PlaneReach {
  double min = 0.0;
  double max = 0.0;
};
PlaneReach plane_reach(const Bounds& box, const Plane& plane);

// What clip does with a shape whose bounding box is `box` (bounds(), or
// any box that holds the shape): keeps it whole when the box lies behind
// the plane, touching it at most (within Precision::Confusion); removes it
// all when the box lies on the normal side, touching the plane at most;
// cuts it otherwise, and for an empty box. A looser box only cuts more
// shapes, and a cut keeps or removes such a shape whole as well.
enum class ClipSide { Kept, Removed, Cut };
ClipSide clip_side(const Bounds& box, const Plane& plane);

} // namespace mitcad::analysis
