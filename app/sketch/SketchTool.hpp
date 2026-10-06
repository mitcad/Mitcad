// SPDX-License-Identifier: MIT
#pragma once

// The interactive tools of sketch mode: drawing tools with a live preview,
// typed values and constraints inferred while drawing, and tools that pick
// sketch geometry (trim, extend, dimensions, constraints). A tool turns
// clicks into `sketch.*` commands through SketchOp, one undo step each.

#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include <QString>

#include "SketchModel.hpp"
#include "SketchSnap.hpp"

namespace mitcad::sketch {

class SketchController;

// What a tool shows while it works, in sketch coordinates.
struct ToolPreview {
  enum class Style {
    Draw,         // the geometry the next click makes
    Construction, // construction geometry it makes
    Highlight,    // what it acts on
    Remove,       // what it takes away (trim)
    Guide,        // helper lines (axes, radii)
  };
  struct Path {
    std::vector<V2> points;
    Style style = Style::Draw;
    bool closed = false;
  };

  std::vector<Path> paths;
  std::vector<V2> points;                       // points it makes
  std::vector<std::pair<V2, QString>> labels;   // values beside the geometry
  QString glyph;                                // a constraint inferred at the cursor
  std::optional<V2> glyphAt;                    // where it is shown (else by the cursor)
  std::optional<DimensionData> dimension;       // a dimension being placed

  void add(std::vector<V2> polyline, Style style = Style::Draw, bool closed = false) {
    paths.push_back({std::move(polyline), style, closed});
  }
  void line(const V2& a, const V2& b, Style style = Style::Draw) { add({a, b}, style); }
  void curve(const Curve2& curve, double step, Style style = Style::Draw) {
    add(curve.sample(step), style, curve.isClosed());
  }
};

class SketchTool {
public:
  explicit SketchTool(SketchController& controller) : m_c(controller) {}
  virtual ~SketchTool() = default;
  SketchTool(const SketchTool&) = delete;
  SketchTool& operator=(const SketchTool&) = delete;

  // The command's name, for hints and logs ("Line").
  virtual QString name() const = 0;
  virtual void start() {}
  // The cursor moved; `snap` is where it snapped (picking tools get the
  // raw position).
  virtual void move(const Snap& snap) { (void)snap; }
  virtual void press(const Snap& snap) = 0;
  virtual void release(const Snap& snap) { (void)snap; }
  virtual void doubleClick(const Snap& snap) { (void)snap; }
  // Enter: typed values or the end of a chain; false when there was
  // nothing to confirm.
  virtual bool confirm() { return false; }
  // Esc: one step back; false when the tool was at its start (it then
  // ends).
  virtual bool cancel() { return false; }
  // A typed value changed.
  virtual void fieldsEdited() {}
  virtual void paint(ToolPreview& preview) const { (void)preview; }
  // Drawing tools snap the cursor; picking tools do not.
  virtual bool snaps() const { return true; }

protected:
  SketchController& m_c;
};

// Drawing tools.
std::unique_ptr<SketchTool> lineTool(SketchController& c);
// mode: two_point, three_point, center
std::unique_ptr<SketchTool> rectangleTool(SketchController& c, const QString& mode);
// mode: center, two_point, three_point, two_tangent, three_tangent
std::unique_ptr<SketchTool> circleTool(SketchController& c, const QString& mode);
// mode: three_point, center, tangent
std::unique_ptr<SketchTool> arcTool(SketchController& c, const QString& mode);
// mode: circumscribed, inscribed, edge
std::unique_ptr<SketchTool> polygonTool(SketchController& c, const QString& mode);
// mode: center_to_center, overall, center_point, three_point_arc, center_point_arc
std::unique_ptr<SketchTool> slotTool(SketchController& c, const QString& mode);
std::unique_ptr<SketchTool> ellipseTool(SketchController& c);
std::unique_ptr<SketchTool> splineTool(SketchController& c, bool controlPoints);
std::unique_ptr<SketchTool> pointTool(SketchController& c);
std::unique_ptr<SketchTool> textTool(SketchController& c);

// Tools that pick sketch geometry.
std::unique_ptr<SketchTool> trimTool(SketchController& c);
std::unique_ptr<SketchTool> extendTool(SketchController& c);
std::unique_ptr<SketchTool> dimensionTool(SketchController& c);
// type: coincident, horizontal_vertical, parallel, perpendicular, tangent,
// equal, concentric, collinear, midpoint, symmetric, smooth, fix
std::unique_ptr<SketchTool> constraintTool(SketchController& c, const QString& type);
// Applies a constraint to the selection when it fits the type; false when
// it does not (the tool then picks).
bool constrainSelection(SketchController& c, const QString& type);
// The name of a constraint type for menus and logs ("Horizontal/Vertical").
QString constraintName(const QString& type);

} // namespace mitcad::sketch
