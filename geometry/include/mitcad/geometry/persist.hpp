// SPDX-License-Identifier: MIT
#pragma once

// Shapes with their face names as bytes, for the store of computed results
// on disk (P7d, core/model/src/store.rs): a result read back is the shape
// the feature made, with the same names, notes and what it had measured of
// itself, so features after it and the application see no difference.
// Also what the caches' diagnostics need: a shape's memory, the process's.

#include <cstddef>
#include <string>
#include <string_view>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// The shape as bytes: a header with a format version, OCCT's binary B-rep
// (BinTools, format version 4) without the triangulations of faces with
// surfaces (they are made again for display), the names of the faces in
// TopExp order, the notes, and the measured properties and bounding box
// when the shape has them (the accurate volume of curved faces is costly).
std::string serialize_shape(const Shape& shape);

// A shape from serialize_shape's bytes. Throws std::runtime_error when the
// data is not such a shape or its faces do not match their names (the
// bytes are then of no use: what reads them computes the shape anew).
ShapePtr deserialize_shape(std::string_view data);

// What results stored by this build must have been stored by: the format
// of serialize_shape, OCCT's version, Mitcad's version and the size and
// time of the running program's file, which change with every build of
// it. Results stored by another build are not used.
std::string kernel_build_id();

// The memory the shape takes, estimated in bytes (the memory cache's
// budget): its faces, edges and vertices with their names, the poles and
// knots of B-spline curves and surfaces, and triangulations made for
// display. Parts it shares with other shapes are counted too.
std::size_t memory_estimate(const Shape& shape);

// The process's memory in bytes (OCCT's OSD_MemInfo); 0 where the system
// does not tell.
struct ProcessMemory {
  std::size_t resident = 0;      // in physical memory now (RSS, working set)
  std::size_t peak_resident = 0; // the most so far
  std::size_t heap = 0;          // allocated by malloc (OCCT allocates there too)
};
ProcessMemory process_memory();

// The machine's physical memory in bytes; 0 when unknown.
std::size_t physical_memory();

} // namespace mitcad::geometry
