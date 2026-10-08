// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/analysis.rs: Mitcad shapes and their
// named sub-shapes measured with the analysis library
// (geometry/analysis), STEP files read with the exchange library
// (geometry/io).

#include <cstddef>
#include <memory>
#include <vector>

#include "bridge/shape.hpp"
#include "mitcad/analysis/interference.hpp"
#include "mitcad/analysis/section.hpp"
#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/analysis.h.
struct AnalysisProperties;
struct AnalysisMeasure;
struct AnalysisSeparation;
struct AnalysisPlane;
struct AnalysisComparison;
struct AnalysisEdgeMiddle;
struct AnalysisFacePoints;

class AnalysisInterferences {
public:
  explicit AnalysisInterferences(std::vector<analysis::Interference> found)
      : m_found(std::move(found)) {}

  std::size_t count() const { return m_found.size(); }
  std::size_t first(std::size_t index) const { return m_found.at(index).first; }
  std::size_t second(std::size_t index) const { return m_found.at(index).second; }
  double volume(std::size_t index) const { return m_found.at(index).volume; }
  std::shared_ptr<geometry::Shape> shape(std::size_t index) const;

private:
  std::vector<analysis::Interference> m_found;
};

class AnalysisSection {
public:
  explicit AnalysisSection(analysis::Section section) : m_section(std::move(section)) {}

  std::size_t edge_count() const { return m_section.edge_count; }
  std::size_t face_count() const { return m_section.face_count; }
  double length() const { return m_section.length; }
  double area() const { return m_section.area; }
  std::shared_ptr<geometry::Shape> curves() const;
  std::shared_ptr<geometry::Shape> faces() const;

private:
  analysis::Section m_section;
};

AnalysisProperties analysis_properties(const geometry::Shape& shape, double density);
AnalysisMeasure analysis_measure(const geometry::Shape& shape, rust::Str name);
AnalysisSeparation analysis_between(const geometry::Shape& a, rust::Str a_name,
                                    const geometry::Shape& b, rust::Str b_name);
std::unique_ptr<AnalysisInterferences> analysis_interferences(const ShapeList& bodies,
                                                              double min_volume);
std::unique_ptr<AnalysisSection> analysis_section(const geometry::Shape& shape,
                                                  const AnalysisPlane& plane);
std::shared_ptr<geometry::Shape> analysis_clip(const geometry::Shape& shape,
                                               const AnalysisPlane& plane);
AnalysisComparison analysis_compare_step(const ShapeList& bodies, rust::Str path,
                                         rust::Str step_body, std::size_t samples, double fuzzy);

// .f3d import (T1).
AnalysisComparison analysis_compare_shapes(const ShapeList& a, const ShapeList& b,
                                           std::size_t samples, double fuzzy, double seconds,
                                           double booleans_above);
rust::Vec<double> analysis_boundary_distances(const geometry::Shape& shape,
                                              rust::Slice<const double> points);
rust::Vec<bool> analysis_points_inside(const geometry::Shape& shape, rust::Slice<const double> points);
// Where a segment (two x, y, z triples) crosses the shape's faces: pairs of
// the fraction along it and -1 where it enters the material, +1 where it
// leaves, in order along it.
rust::Vec<double> analysis_segment_crossings(const geometry::Shape& shape, rust::Slice<const double> segment);

// The number of distinct faces of the shape.
std::size_t analysis_face_count(const geometry::Shape& shape);
// Every named edge's middle (geometry::edge_middles).
rust::Vec<AnalysisEdgeMiddle> analysis_edge_middles(const geometry::Shape& shape);
// Points inside every named face (geometry::face_points).
rust::Vec<AnalysisFacePoints> analysis_face_points(const geometry::Shape& shape);

} // namespace mitcad::bridge
