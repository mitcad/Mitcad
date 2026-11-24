// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/exchange.rs.

#include <cstddef>
#include <cstdint>
#include <memory>
#include <string>
#include <vector>

#include "bridge/shape.hpp"
#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::f3d {
// Defined in the generated mitcad_bridge/brep_import.h.
struct BrepBodyData;
} // namespace mitcad::f3d

namespace mitcad::brep {
// mitcad/geometry/brep_import.hpp.
struct Healed;
} // namespace mitcad::brep

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/exchange.h.
enum class BodyKind : std::uint8_t;
struct BodyInfo;
struct BodyPlacement;
struct FaceColor;
struct ExportSettings;
struct F3dBody;

std::shared_ptr<geometry::Shape> import_brep(rust::Str feature, rust::Slice<const std::uint8_t> data,
                                             std::uint32_t first_face);
std::size_t face_count(const geometry::Shape& shape);
rust::Vec<std::uint8_t> brep_data(const geometry::Shape& shape);
std::shared_ptr<geometry::Shape> adopt_body(rust::Str feature, const geometry::Shape& shape,
                                            std::uint32_t first_face);
std::shared_ptr<geometry::Shape> compound(const ShapeList& shapes);
BodyKind body_kind(const geometry::Shape& shape);

rust::Vec<BodyInfo> read_file(rust::Str path, double unit_mm, ShapeList& shapes);
void write_file(rust::Str path, const ShapeList& shapes, rust::Slice<const BodyInfo> bodies,
                const ExportSettings& settings);

rust::Vec<F3dBody> f3d_bodies(rust::Str path, bool history, bool owners, ShapeList& shapes);

// .f3d import (T1).
std::shared_ptr<geometry::Shape> f3d_build_body(const f3d::BrepBodyData& data);
std::shared_ptr<geometry::Shape> f3d_read_body(rust::Slice<const std::uint8_t> data);
void catch_occt_crashes();

// A body of an .f3d file built with what the builds of its other history
// states healed (mitcad#181), and what it keeps for theirs.
struct F3dBuilt {
  std::shared_ptr<geometry::Shape> shape;
  std::shared_ptr<const brep::Healed> healed;
  std::uint32_t reused = 0;
  // Why the body was healed as a whole although it had bases (empty when
  // it was not).
  std::string note;
};

// The builds a body's build takes healed faces from.
struct F3dBases {
  std::vector<std::shared_ptr<const brep::Healed>> bases;
};

std::unique_ptr<F3dBases> f3d_bases();
void f3d_bases_add(F3dBases& bases, const std::shared_ptr<F3dBuilt>& built);
std::shared_ptr<F3dBuilt> f3d_build_body_from(const f3d::BrepBodyData& data, const F3dBases& bases, bool keep);
std::shared_ptr<geometry::Shape> f3d_built_shape(const F3dBuilt& built);
std::uint32_t f3d_built_reused(const F3dBuilt& built);
bool f3d_built_kept(const F3dBuilt& built);
rust::String f3d_built_note(const F3dBuilt& built);

// Bodies built from their neutral B-rep data with a build report (the
// .ipt import).
F3dBody build_brep_body(const f3d::BrepBodyData& data, ShapeList& shapes);
bool shape_is_valid(const geometry::Shape& shape);

} // namespace mitcad::bridge
