// SPDX-License-Identifier: MIT
#include "SketchPicture.hpp"

#include <algorithm>
#include <memory>
#include <optional>
#include <vector>

#include <QFont>
#include <QLineF>
#include <QPainter>
#include <QPainterPath>
#include <QPen>
#include <QPointF>
#include <QPolygonF>
#include <QRectF>
#include <QStringList>
#include <QTransform>

#include "sketch/SketchModel.hpp"

namespace mitcad::printing {
namespace {

using sketch::Curve2;
using sketch::SketchModel;
using sketch::V2;

// On paper (mm), whatever the scale.
constexpr double kLineWidth = 0.35;
constexpr double kThinWidth = 0.18;
constexpr double kPointArm = 1.0;
constexpr double kGlyph = 2.8;
constexpr double kGlyphGap = 0.4;
constexpr double kGlyphOffset = 1.5;
constexpr double kDimensionText = 3.0;
constexpr double kArrowLength = 2.5;
constexpr double kArrowWidth = 0.8;
// The picture's curves as polylines: points at most this far apart (mm).
constexpr double kSampleStep = 0.25;
// A font's texts are laid out at this pixel size and scaled to their height.
constexpr int kTextPixels = 100;

QPointF at(const V2& v) { return {v.x, v.y}; }

// A text's glyphs in sketch coordinates (y up): its lines from its anchor,
// aligned as the sketch aligns them, `height` mm high, turned by `angle`.
QPainterPath textPath(const QString& text, const QFont& font, const V2& anchor, double height, double angle,
                      const QString& align, const QString& valign) {
  QFont sized = font;
  sized.setPixelSize(kTextPixels);
  QPainterPath glyphs;
  const QStringList lines = text.split(QLatin1Char('\n'));
  for (int i = 0; i < lines.size(); ++i) {
    glyphs.addText(QPointF(0.0, i * kTextPixels * 1.2), sized, lines[i]);
  }
  const QRectF box = glyphs.boundingRect();
  const double dx = align == QStringLiteral("center")  ? -box.center().x()
                    : align == QStringLiteral("right") ? -box.right()
                                                       : 0.0;
  const double dy = valign == QStringLiteral("top")      ? -box.top()
                    : valign == QStringLiteral("middle") ? -box.center().y()
                    : valign == QStringLiteral("bottom") ? -box.bottom()
                                                         : 0.0;
  const double k = height / kTextPixels;
  QTransform place;
  place.translate(anchor.x, anchor.y);
  place.rotateRadians(angle);
  place.scale(k, -k); // the glyphs' y runs down, the sketch's up
  place.translate(dx, dy);
  return place.map(glyphs);
}

// One symbol of a constraint (or "fix") beside an entity.
struct Glyph {
  QString entity;
  QString type;
};

struct Drawn {
  std::vector<QLineF> lines;
  std::vector<QPolygonF> curves;
  std::vector<std::pair<QPointF, double>> circles;
  std::vector<QLineF> thinLines;
  std::vector<QPolygonF> thinCurves;
  std::vector<std::pair<QPointF, double>> thinCircles;
  std::vector<QPointF> crosses;
  std::vector<QPainterPath> outlines; // texts' outlines, stroked
  std::vector<QPainterPath> fonts;    // texts in their font, filled
  std::vector<std::pair<V2, std::vector<QString>>> glyphRows; // an anchor and its symbols
  std::vector<SketchDimensionDrawing> dimensions;
};

void addCurve(Drawn& drawn, const Curve2& curve, bool thin) {
  if (curve.type == Curve2::Type::Line) {
    (thin ? drawn.thinLines : drawn.lines).emplace_back(at(curve.a), at(curve.b));
  } else if (curve.type == Curve2::Type::Circle) {
    (thin ? drawn.thinCircles : drawn.circles).emplace_back(at(curve.center), curve.radius);
  } else {
    QPolygonF points;
    for (const V2& p : curve.sample(kSampleStep)) {
      points << at(p);
    }
    if (curve.isClosed() && !points.isEmpty()) {
      points << points.first();
    }
    (thin ? drawn.thinCurves : drawn.curves).push_back(points);
  }
}

std::optional<V2> anchorOf(const SketchModel& model, const QString& id) {
  for (const sketch::PointData& p : model.points) {
    if (p.id == id) {
      return p.at;
    }
  }
  for (const sketch::CurveData& c : model.curves) {
    if (c.id == id) {
      if (c.curve.type == Curve2::Type::Circle) {
        return c.curve.center + V2{0.0, c.curve.radius};
      }
      return c.curve.middle();
    }
  }
  return std::nullopt;
}

// The fixed point at the origin that stands for it: no symbol, no cross.
bool isOrigin(const sketch::PointData& p) { return p.fixed && p.at.x == 0.0 && p.at.y == 0.0; }

// The symbols of the constraints, one per entity and type, as sketch mode
// shows them (a coincidence at its point, a symmetry at both its points, a
// tangency at the point it applies at), and the fixed entities' "fix".
std::vector<Glyph> glyphsOf(const SketchModel& model) {
  std::vector<Glyph> all;
  const auto add = [&all](const QString& entity, const QString& type) {
    if (entity.isEmpty() || !printsConstraint(type)) {
      return;
    }
    const bool shown = std::any_of(all.begin(), all.end(),
                                   [&](const Glyph& g) { return g.entity == entity && g.type == type; });
    if (!shown) {
      all.push_back({entity, type});
    }
  };
  for (const sketch::ConstraintData& k : model.constraints) {
    const QString& t = k.type;
    if (t == QStringLiteral("midpoint")) {
      add(k.json.value(QStringLiteral("point")).toString(), t);
    } else if (t == QStringLiteral("symmetric")) {
      add(k.json.value(QStringLiteral("a")).toString(), t);
      add(k.json.value(QStringLiteral("b")).toString(), t);
    } else if ((t == QStringLiteral("tangent") || t == QStringLiteral("smooth")) &&
               !k.json.value(QStringLiteral("at")).toString().isEmpty()) {
      add(k.json.value(QStringLiteral("at")).toString(), t);
    } else {
      for (const QString& entity : k.refs) {
        add(entity, t);
      }
    }
  }
  for (const sketch::PointData& p : model.points) {
    if (p.fixed && !p.reference && !isOrigin(p) && p.axis.isEmpty()) {
      add(p.id, QStringLiteral("fix"));
    }
  }
  for (const sketch::CurveData& c : model.curves) {
    if (c.fixed && !c.reference && c.axis.isEmpty()) {
      add(c.id, QStringLiteral("fix"));
    }
  }
  return all;
}

// A constraint's symbol in the box round `c` (sketch coordinates), `h` its
// half size: the shapes sketch mode draws.
void paintSymbol(QPainter& painter, const QString& t, const V2& c, double h) {
  // (x, y) of the symbol's square, y down as sketch mode draws it.
  const auto p = [&](double x, double y) { return QPointF(c.x + x * h, c.y - y * h); };
  const auto L = [&](double x1, double y1, double x2, double y2) { painter.drawLine(QLineF(p(x1, y1), p(x2, y2))); };
  painter.setBrush(Qt::NoBrush);
  if (t == QStringLiteral("horizontal") || t == QStringLiteral("horizontal_points")) {
    L(-1, 0, 1, 0);
  } else if (t == QStringLiteral("vertical") || t == QStringLiteral("vertical_points")) {
    L(0, -1, 0, 1);
  } else if (t == QStringLiteral("parallel")) {
    L(-0.9, 0.8, -0.1, -0.8);
    L(0.1, 0.8, 0.9, -0.8);
  } else if (t == QStringLiteral("perpendicular")) {
    L(-1, 0.9, 1, 0.9);
    L(0, 0.9, 0, -1);
  } else if (t == QStringLiteral("tangent")) {
    painter.drawEllipse(p(0, 0.25), h * 0.6, h * 0.6);
    L(-1, -0.35, 1, -0.35);
  } else if (t == QStringLiteral("equal")) {
    L(-0.9, -0.35, 0.9, -0.35);
    L(-0.9, 0.35, 0.9, 0.35);
  } else if (t == QStringLiteral("concentric")) {
    painter.drawEllipse(p(0, 0), h, h);
    painter.drawEllipse(p(0, 0), h * 0.45, h * 0.45);
  } else if (t == QStringLiteral("midpoint")) {
    QPolygonF triangle;
    triangle << p(0, -1) << p(1, 0.8) << p(-1, 0.8) << p(0, -1);
    painter.drawPolyline(triangle);
  } else if (t == QStringLiteral("collinear")) {
    L(-1, 0.6, 1, -0.6);
  } else if (t == QStringLiteral("symmetric")) {
    L(-1, -1, -0.4, 0);
    L(-0.4, 0, -1, 1);
    L(1, -1, 0.4, 0);
    L(0.4, 0, 1, 1);
  } else if (t == QStringLiteral("smooth")) {
    QPainterPath path(p(-1, 0.6));
    path.cubicTo(p(-0.2, 0.6), p(0.2, -0.6), p(1, -0.6));
    painter.drawPath(path);
  } else if (t == QStringLiteral("equal_spacing")) {
    L(-1, 0, 1, 0);
    L(-1, -0.7, -1, 0.7);
    L(0, -0.7, 0, 0.7);
    L(1, -0.7, 1, 0.7);
  } else if (t == QStringLiteral("fix")) {
    painter.drawRect(QRectF(p(-0.75, -0.05), p(0.75, 1.0)).normalized());
    L(-0.45, -0.05, -0.45, -0.5);
    L(0.45, -0.05, 0.45, -0.5);
    L(-0.45, -0.5, 0.45, -0.5);
  } else {
    // A type without a shape of its own: its initial.
    QFont font;
    painter.save();
    painter.setPen(Qt::NoPen);
    painter.setBrush(Qt::black);
    painter.drawPath(textPath(t.left(1).toUpper(), font, c, h * 1.6, 0.0, QStringLiteral("center"),
                              QStringLiteral("middle")));
    painter.restore();
  }
}

void paint(QPainter& painter, double scale, const Drawn& drawn) {
  const double s = scale > 0.0 ? scale : 1.0;
  painter.setBrush(Qt::NoBrush);
  painter.setPen(QPen(Qt::black, kLineWidth / s, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
  for (const QLineF& line : drawn.lines) {
    painter.drawLine(line);
  }
  for (const QPolygonF& curve : drawn.curves) {
    painter.drawPolyline(curve);
  }
  for (const auto& [center, radius] : drawn.circles) {
    painter.drawEllipse(center, radius, radius);
  }
  for (const QPointF& p : drawn.crosses) {
    const double arm = kPointArm / s;
    painter.drawLine(QLineF(p.x() - arm, p.y(), p.x() + arm, p.y()));
    painter.drawLine(QLineF(p.x(), p.y() - arm, p.x(), p.y() + arm));
  }
  for (const QPainterPath& outline : drawn.outlines) {
    painter.drawPath(outline);
  }
  // Construction geometry: thin and dashed (in pen widths: 1.8 mm, 1.1 mm).
  QPen thin(Qt::black, kThinWidth / s, Qt::SolidLine, Qt::FlatCap, Qt::RoundJoin);
  thin.setDashPattern({10.0, 6.0});
  painter.setPen(thin);
  for (const QLineF& line : drawn.thinLines) {
    painter.drawLine(line);
  }
  for (const QPolygonF& curve : drawn.thinCurves) {
    painter.drawPolyline(curve);
  }
  for (const auto& [center, radius] : drawn.thinCircles) {
    painter.drawEllipse(center, radius, radius);
  }
  painter.setPen(Qt::NoPen);
  painter.setBrush(Qt::black);
  for (const QPainterPath& text : drawn.fonts) {
    painter.drawPath(text);
  }
  // Dimensions: thin lines, filled arrowheads, their values.
  const QPen dimension(Qt::black, kThinWidth / s, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin);
  const QFont valueFont;
  for (const SketchDimensionDrawing& d : drawn.dimensions) {
    painter.setPen(dimension);
    painter.setBrush(Qt::NoBrush);
    for (const auto& [a, b] : d.lines) {
      painter.drawLine(QLineF(at(a), at(b)));
    }
    for (const std::vector<V2>& arc : d.arcs) {
      QPolygonF points;
      for (const V2& p : arc) {
        points << at(p);
      }
      painter.drawPolyline(points);
    }
    painter.setPen(Qt::NoPen);
    painter.setBrush(Qt::black);
    for (const auto& [tip, towards] : d.arrows) {
      const V2 u = sketch::unit(towards);
      const V2 n = sketch::perp(u);
      const V2 base = tip - u * (kArrowLength / s);
      QPolygonF head;
      head << at(tip) << at(base + n * (kArrowWidth / s)) << at(base - n * (kArrowWidth / s));
      painter.drawPolygon(head);
    }
    if (!d.text.isEmpty()) {
      painter.drawPath(textPath(d.text, valueFont, d.textAt, kDimensionText / s, 0.0, QStringLiteral("center"),
                                QStringLiteral("middle")));
    }
  }
  // Constraint symbols in a row of boxes beside their entities.
  const double box = kGlyph / s;
  for (const auto& [anchor, types] : drawn.glyphRows) {
    for (std::size_t i = 0; i < types.size(); ++i) {
      const V2 center = anchor + V2{(kGlyphOffset + kGlyph / 2.0 + static_cast<double>(i) * (kGlyph + kGlyphGap)) / s,
                                    (kGlyphOffset + kGlyph / 2.0) / s};
      painter.setPen(QPen(Qt::black, kThinWidth / s));
      painter.setBrush(Qt::NoBrush);
      painter.drawRect(QRectF(center.x - box / 2.0, center.y - box / 2.0, box, box));
      painter.setPen(QPen(Qt::black, kThinWidth / s, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
      paintSymbol(painter, types[i], center, box / 2.0 - 0.5 / s);
    }
  }
}

// A part of the picture's extent: a box in sketch mm (y up) and how far the
// part reaches beyond it on paper (mm), whatever the scale.
struct Reach {
  double left = 0.0;
  double right = 0.0;
  double bottom = 0.0;
  double top = 0.0;
  double outLeft = 0.0;
  double outRight = 0.0;
  double outBottom = 0.0;
  double outTop = 0.0;
};

// The extent in sketch mm of the geometry and the paper-sized parts painted
// at `scale`, with kSketchMargin round it.
Reach reachAt(const Reach& geometry, const std::vector<Reach>& paperSized, double scale) {
  const double s = scale > 0.0 ? scale : 1.0;
  Reach out = geometry;
  for (const Reach& r : paperSized) {
    out.left = std::min(out.left, r.left - r.outLeft / s);
    out.right = std::max(out.right, r.right + r.outRight / s);
    out.bottom = std::min(out.bottom, r.bottom - r.outBottom / s);
    out.top = std::max(out.top, r.top + r.outTop / s);
  }
  out.left -= kSketchMargin;
  out.right += kSketchMargin;
  out.bottom -= kSketchMargin;
  out.top += kSketchMargin;
  out.outLeft = out.outRight = out.outBottom = out.outTop = 0.0;
  return out;
}

} // namespace

bool printsConstraint(const QString& type) { return type != QStringLiteral("coincident"); }

bool sketchPrintsAnything(const SketchModel& model) {
  // As sketchPicture draws them: a curve but an axis's line, a point no
  // curve uses (not an axis's, not the origin), a text with an outline or
  // in its font. A dimension stands for geometry of its own.
  QStringList used;
  for (const sketch::CurveData& c : model.curves) {
    if (c.axis.isEmpty()) {
      return true;
    }
    used << c.points << c.start << c.end << c.center;
  }
  for (const sketch::PointData& p : model.points) {
    if (!used.contains(p.id) && p.axis.isEmpty() && !isOrigin(p)) {
      return true;
    }
  }
  for (const sketch::TextData& t : model.texts) {
    if (!t.outline.empty() || (!t.text.isEmpty() && !t.code)) {
      return true;
    }
  }
  return !model.dimensions.empty();
}

PrintPicture sketchPicture(const SketchModel& model, const SketchPrintOptions& options,
                           const std::vector<SketchDimensionDrawing>& dimensions) {
  auto drawn = std::make_shared<Drawn>();
  QStringList used; // the points the curves use
  for (const sketch::CurveData& c : model.curves) {
    used << c.points << c.start << c.end << c.center;
    if (!c.axis.isEmpty()) {
      continue; // an axis's line stands for the axis
    }
    addCurve(*drawn, c.curve, c.construction || c.centerline);
  }
  for (const sketch::PointData& p : model.points) {
    if (!used.contains(p.id) && p.axis.isEmpty() && !isOrigin(p)) {
      drawn->crosses.push_back(at(p.at));
    }
  }
  for (const sketch::TextData& t : model.texts) {
    if (!t.outline.empty()) {
      QPainterPath outline;
      for (const Curve2& curve : t.outline) {
        if (curve.type == Curve2::Type::Line) {
          outline.moveTo(at(curve.a));
          outline.lineTo(at(curve.b));
        } else {
          const std::vector<V2> points = curve.sample(kSampleStep);
          if (points.empty()) {
            continue;
          }
          outline.moveTo(at(points.front()));
          for (const V2& p : points) {
            outline.lineTo(at(p));
          }
        }
      }
      drawn->outlines.push_back(outline);
    } else if (!t.text.isEmpty() && !t.code) {
      QFont font;
      if (!t.font.isEmpty()) {
        font.setFamily(t.font);
      }
      font.setBold(t.bold);
      font.setItalic(t.italic);
      drawn->fonts.push_back(textPath(t.text, font, t.at, t.height, t.angle, t.align, t.valign));
    }
  }
  if (options.constraints) {
    for (const Glyph& glyph : glyphsOf(model)) {
      const std::optional<V2> anchor = anchorOf(model, glyph.entity);
      if (!anchor) {
        continue;
      }
      bool placed = false;
      for (auto& [where, types] : drawn->glyphRows) {
        if (where == *anchor) {
          types.push_back(glyph.type);
          placed = true;
          break;
        }
      }
      if (!placed) {
        drawn->glyphRows.emplace_back(*anchor, std::vector<QString>{glyph.type});
      }
    }
  }
  if (options.dimensions) {
    drawn->dimensions = dimensions;
  }

  // The extent: the geometry's in sketch mm, and the parts drawn at their
  // paper size (crosses, values, arrowheads, symbols) each as a box round
  // its place that reaches so many paper mm beyond it, divided by the scale
  // it is painted at (mitcad#1308). (Bounds kept by hand: QRectF's union
  // drops a rectangle of no size, a point's.)
  Reach geometry;
  bool hasGeometry = false;
  const auto includePoint = [&](const QPointF& p) {
    geometry.left = hasGeometry ? std::min(geometry.left, p.x()) : p.x();
    geometry.right = hasGeometry ? std::max(geometry.right, p.x()) : p.x();
    geometry.bottom = hasGeometry ? std::min(geometry.bottom, p.y()) : p.y();
    geometry.top = hasGeometry ? std::max(geometry.top, p.y()) : p.y();
    hasGeometry = true;
  };
  const auto include = [&includePoint](const QRectF& r) {
    includePoint(r.topLeft());
    includePoint(r.bottomRight());
  };
  auto paperSized = std::make_shared<std::vector<Reach>>();
  // Round the point `p`: `outLeft` mm to its left on paper, and so on.
  const auto includePaper = [&paperSized](const V2& p, double outLeft, double outRight, double outBottom,
                                          double outTop) {
    Reach r;
    r.left = r.right = p.x;
    r.bottom = r.top = p.y;
    r.outLeft = outLeft;
    r.outRight = outRight;
    r.outBottom = outBottom;
    r.outTop = outTop;
    paperSized->push_back(r);
  };
  for (const QLineF& l : drawn->lines) {
    includePoint(l.p1());
    includePoint(l.p2());
  }
  for (const QLineF& l : drawn->thinLines) {
    includePoint(l.p1());
    includePoint(l.p2());
  }
  const auto includeCurves = [&include](const std::vector<QPolygonF>& curves) {
    for (const QPolygonF& c : curves) {
      include(c.boundingRect());
    }
  };
  includeCurves(drawn->curves);
  includeCurves(drawn->thinCurves);
  const auto includeCircles = [&include](const std::vector<std::pair<QPointF, double>>& circles) {
    for (const auto& [center, radius] : circles) {
      include(QRectF(center.x() - radius, center.y() - radius, 2.0 * radius, 2.0 * radius));
    }
  };
  includeCircles(drawn->circles);
  includeCircles(drawn->thinCircles);
  for (const QPointF& p : drawn->crosses) {
    includePaper(V2{p.x(), p.y()}, kPointArm, kPointArm, kPointArm, kPointArm);
  }
  for (const QPainterPath& path : drawn->outlines) {
    include(path.boundingRect());
  }
  for (const QPainterPath& path : drawn->fonts) {
    include(path.boundingRect());
  }
  for (const SketchDimensionDrawing& d : drawn->dimensions) {
    for (const auto& [a, b] : d.lines) {
      includePoint(at(a));
      includePoint(at(b));
    }
    for (const std::vector<V2>& arc : d.arcs) {
      for (const V2& p : arc) {
        includePoint(at(p));
      }
    }
    for (const auto& [tip, towards] : d.arrows) {
      // The head's corners from its tip, as paint() draws it.
      const V2 u = sketch::unit(towards);
      const V2 n = sketch::perp(u);
      const V2 corners[] = {V2{}, u * -kArrowLength + n * kArrowWidth, u * -kArrowLength - n * kArrowWidth};
      double outLeft = 0.0;
      double outRight = 0.0;
      double outBottom = 0.0;
      double outTop = 0.0;
      for (const V2& c : corners) {
        outLeft = std::max(outLeft, -c.x);
        outRight = std::max(outRight, c.x);
        outBottom = std::max(outBottom, -c.y);
        outTop = std::max(outTop, c.y);
      }
      includePaper(tip, outLeft, outRight, outBottom, outTop);
    }
    const double half = 0.3 * kDimensionText * static_cast<double>(d.text.size());
    includePaper(d.textAt, half, half, kDimensionText / 2.0, kDimensionText / 2.0);
  }
  for (const auto& [anchor, types] : drawn->glyphRows) {
    const double length = kGlyphOffset + static_cast<double>(types.size()) * (kGlyph + kGlyphGap);
    includePaper(anchor, 0.0, length, 0.0, kGlyphOffset + kGlyph);
  }

  PrintPicture picture;
  picture.name = model.name;
  if (!hasGeometry && paperSized->empty()) {
    // Nothing drawn: no size, so that it is not valid() and nothing is
    // printed ("There is nothing to print."), not a blank page of the
    // margins alone.
    return picture;
  }
  if (!hasGeometry) {
    // Only parts at their paper size (lone points): round the first's place.
    geometry = paperSized->front();
    geometry.outLeft = geometry.outRight = geometry.outBottom = geometry.outTop = 0.0;
  }
  // The extent at 1:1 is the picture, its top left at (left, top) of the
  // sketch, as the sketch's y runs up.
  const Reach whole = reachAt(geometry, *paperSized, 1.0);
  const double left = whole.left;
  const double top = whole.top;
  picture.size = QSizeF(whole.right - whole.left, whole.top - whole.bottom);
  picture.paint = [drawn, left, top](QPainter& painter, double scale) {
    painter.save();
    // Sketch (x, y) to the picture's (x - left, top - y).
    painter.setTransform(QTransform(1.0, 0.0, 0.0, -1.0, -left, top), true);
    paint(painter, scale, *drawn);
    painter.restore();
  };
  picture.extent = [geometry, paperSized, left, top](double scale) {
    const Reach r = reachAt(geometry, *paperSized, scale);
    return QRectF(r.left - left, top - r.top, r.right - r.left, r.top - r.bottom);
  };
  return picture;
}

} // namespace mitcad::printing
