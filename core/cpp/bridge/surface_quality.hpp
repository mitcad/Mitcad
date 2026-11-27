// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/surface_quality.rs (mitcad#2517): the
// surface quality analyses' samples of a body's faces, a triangulation with
// each vertex's outward normal and principal curvatures (Zebra, Curvature
// Map, Environment Map), and the faces' u and v iso curves trimmed to them
// with the normal curvature along each (Isocurve Analysis).
//
// Curvatures are in 1/mm, signed by the body's outside: positive where a
// face bulges outwards (a ball's outside, a shaft), negative where it is
// hollow (a bore), zero on a plane; NaN where the surface has none (a
// cone's apex, a mesh body's faces).

#include <cstdint>
#include <string>

#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated
// mitcad_bridge/kernel/surface_quality.h.
struct SurfaceSampleMesh;
struct IsoCurveSet;

// The faces named (every face of the shape when none is named, in its face
// order) triangulated within the surface deviation (mm) and the angle
// (radians) on a copy, as face_meshes triangulates them, each vertex with
// the surface's outward normal and its two principal curvatures. A name
// that finds no face is an error.
SurfaceSampleMesh surface_samples(const geometry::Shape& shape, rust::Slice<const rust::String> faces,
                                  double deviation, double angle, std::uint64_t face_triangles);

// The u and v iso curves of the faces named (every face when none is
// named): `count` of each per face, at (i + 1) / (count + 1) of the face's
// parameter range, trimmed to the face, as polylines within `deviation` mm
// of the curve, each point with the outward normal and the surface's normal
// curvature along the curve. A face without a surface has none.
IsoCurveSet face_isocurves(const geometry::Shape& shape, rust::Slice<const rust::String> faces, std::uint32_t count,
                           double deviation);

} // namespace mitcad::bridge
