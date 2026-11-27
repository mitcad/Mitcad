// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/analysis.rs: Mitcad shapes and their
// named sub-shapes measured with the analysis library
// (geometry/analysis), STEP files read with the exchange library
// (geometry/io).

#include <array>
#include <cstddef>
#include <memory>
#include <optional>
#include <string>
#include <utility>
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
struct AnalysisFaceGap;
struct AnalysisPlane;
struct AnalysisComparison;
struct AnalysisEdgeMiddle;
struct AnalysisFacePoints;
struct AnalysisNewFace;

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

// Section Analysis of many bodies at once (mitcad#438): each body's
// section or clip, worked out on all cores, with its error.
class AnalysisSections {
public:
  AnalysisSections(std::vector<std::optional<analysis::Section>> sections, std::vector<std::string> errors)
      : m_sections(std::move(sections)), m_errors(std::move(errors)) {}

  std::size_t count() const { return m_sections.size(); }
  // Null when the section failed.
  std::unique_ptr<AnalysisSection> section(std::size_t index) const;
  // Why it failed; empty when it did not.
  rust::String error(std::size_t index) const { return rust::String::lossy(m_errors.at(index)); }

private:
  std::vector<std::optional<analysis::Section>> m_sections;
  std::vector<std::string> m_errors;
};

class AnalysisClips {
public:
  AnalysisClips(std::vector<std::shared_ptr<geometry::Shape>> shapes, std::vector<std::string> errors)
      : m_shapes(std::move(shapes)), m_errors(std::move(errors)) {}

  std::size_t count() const { return m_shapes.size(); }
  // The clipped body; null when the plane leaves the body whole (it is
  // shown as it is) or when its clip failed.
  std::shared_ptr<geometry::Shape> shape(std::size_t index) const { return m_shapes.at(index); }
  // Why the clip failed; empty when it did not.
  rust::String error(std::size_t index) const { return rust::String::lossy(m_errors.at(index)); }

private:
  std::vector<std::shared_ptr<geometry::Shape>> m_shapes;
  std::vector<std::string> m_errors;
};

AnalysisProperties analysis_properties(const geometry::Shape& shape, double density);
AnalysisMeasure analysis_measure(const geometry::Shape& shape, rust::Str name);
AnalysisSeparation analysis_between(const geometry::Shape& a, rust::Str a_name,
                                    const geometry::Shape& b, rust::Str b_name);
AnalysisFaceGap analysis_face_gap(const geometry::Shape& a, rust::Str a_name,
                                  const geometry::Shape& b, rust::Str b_name);
std::unique_ptr<AnalysisInterferences> analysis_interferences(const ShapeList& bodies,
                                                              double min_volume);
std::unique_ptr<AnalysisSection> analysis_section(const geometry::Shape& shape,
                                                  const AnalysisPlane& plane);
// Null when the plane leaves the body whole.
std::shared_ptr<geometry::Shape> analysis_clip(const geometry::Shape& shape,
                                               const AnalysisPlane& plane);
// The section and the clip of each shape at its plane (one plane per
// shape), on all cores. They stop when the calling thread's operations are
// asked to (geometry/cancel.hpp): the items not started then fail as
// cancelled.
std::unique_ptr<AnalysisSections> analysis_section_all(const ShapeList& shapes,
                                                       rust::Slice<const AnalysisPlane> planes);
std::unique_ptr<AnalysisClips> analysis_clip_all(const ShapeList& shapes,
                                                 rust::Slice<const AnalysisPlane> planes);
// What Section Analysis' worker thread may read of each shape (mitcad#438),
// made on the thread that owns the shapes: null when the plane passes the
// shape farther than its tolerances (its box, now kept with it, tells its
// clip and its empty section), else a copy that shares no face, edge or
// vertex with it, with the same names, its box and its mesh faces'
// triangles. The worker then never reads a sub-shape that the view or a
// model job may write into meanwhile.
std::unique_ptr<AnalysisClips> analysis_detach_all(const ShapeList& shapes,
                                                   rust::Slice<const AnalysisPlane> planes);
AnalysisComparison analysis_compare_step(const ShapeList& bodies, rust::Str path,
                                         rust::Str step_body, std::size_t samples, double fuzzy,
                                         double seconds, double booleans_above);

// .f3d import (T1).
AnalysisComparison analysis_compare_shapes(const ShapeList& a, const ShapeList& b,
                                           std::size_t samples, double fuzzy, double seconds,
                                           double booleans_above);
rust::Vec<double> analysis_boundary_distances(const geometry::Shape& shape,
                                              rust::Slice<const double> points);
// How many times analysis_boundary_distances has measured a point against a
// face with OCCT's distance query (cones, tori, and free-form faces it
// cannot place a point on from their grids), since the process started; for
// tests (mitcad#442).
std::size_t analysis_whole_face_searches();
// The point of the faces (indices into the shape's) nearest to `near` and
// the face's normal there, as geometry::face_point_normal finds them,
// where each face is a plane, cylinder or sphere onto which `near`
// projects inside the face (the boundary index's projections, kept with
// the shape); false otherwise, for geometry::face_point_normal to measure.
bool analysis_face_point(const geometry::Shape& shape, const std::vector<int>& faces,
                         const std::array<double, 3>& near, std::array<double, 3>& point,
                         std::array<double, 3>& normal);
rust::Vec<bool> analysis_points_inside(const geometry::Shape& shape, rust::Slice<const double> points);
// Where a segment (two x, y, z triples) crosses the shape's faces: pairs of
// the fraction along it and -1 where it enters the material, +1 where it
// leaves, in order along it.
rust::Vec<double> analysis_segment_crossings(const geometry::Shape& shape, rust::Slice<const double> segment);
rust::Vec<double> analysis_segments_crossings(const geometry::Shape& shape, rust::Slice<const double> segments);

// The number of distinct faces of the shape.
std::size_t analysis_face_count(const geometry::Shape& shape);
// Every named edge's middle (geometry::edge_middles).
rust::Vec<AnalysisEdgeMiddle> analysis_edge_middles(const geometry::Shape& shape);
// Points inside every named face (geometry::face_points).
rust::Vec<AnalysisFacePoints> analysis_face_points(const geometry::Shape& shape);
// The faces of `after` that none of `before` has (geometry::new_faces,
// mitcad#138).
rust::Vec<AnalysisNewFace> analysis_new_faces(const ShapeList& before, const geometry::Shape& after,
                                              std::size_t count);
// The volume a change makes from the faces it changed and its slack from
// their edges, two values (geometry::changed_volume, mitcad#235).
rust::Vec<double> analysis_changed_volume(const ShapeList& before, const ShapeList& after);

// A box that holds each face, six values per face in face order
// (geometry::face_boxes, mitcad#119).
rust::Vec<double> analysis_face_boxes(const geometry::Shape& shape);

} // namespace mitcad::bridge
