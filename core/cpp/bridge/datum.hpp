// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/datum.rs.

#include <cstdint>
#include <memory>

#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/datum.h.
struct DatumVec;
struct DatumSurface;
struct DatumCurve;
struct DatumPointing;
struct DatumFaceEntry;
struct DatumEdgeEntry;

DatumSurface datum_face_geometry(const geometry::Shape& shape, rust::Str face);
DatumCurve datum_edge_geometry(const geometry::Shape& shape, rust::Str edge);
DatumVec datum_vertex_point(const geometry::Shape& shape, rust::Str vertex);
DatumPointing datum_path_point(const geometry::Shape& shape, rust::Slice<const rust::String> edges,
                               std::uint8_t mode, double value, const DatumVec& near);
DatumPointing datum_face_point(const geometry::Shape& shape, rust::Str face, const DatumVec& near);
std::shared_ptr<geometry::Shape> datum_plane_shape(const DatumVec& origin, const DatumVec& normal,
                                                   const DatumVec& x_axis, double size);
std::shared_ptr<geometry::Shape> datum_axis_shape(const DatumVec& origin,
                                                  const DatumVec& direction, double size);
std::shared_ptr<geometry::Shape> datum_point_shape(const DatumVec& point);
rust::Vec<DatumFaceEntry> datum_face_geometries(const geometry::Shape& shape);
rust::Vec<DatumEdgeEntry> datum_edge_geometries(const geometry::Shape& shape);

} // namespace mitcad::bridge
