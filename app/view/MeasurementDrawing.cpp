// SPDX-License-Identifier: MIT
#include "MeasurementDrawing.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <initializer_list>
#include <iterator>
#include <limits>

#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QFontDatabase>
#include <QFontMetricsF>
#include <QHash>
#include <QImage>
#include <QJsonValue>
#include <QPainter>
#include <QPainterPath>
#include <QPainterPathStroker>
#include <QPen>
#include <QPixmap>
#include <QtLogging>

namespace mitcad {
namespace {

// Pixels on screen (times a picture's scale): the only sizes not in text
// heights (mitcad#902).
constexpr double kTextFloor = 9.0;    // a text's least capital height (zoomed out)
constexpr double kTextCeiling = 16.0; // its most (zoomed in), as the reference drawing's proportions
constexpr double kLineWidth = 1.0;    // lines: ISO 128's thin line
constexpr double kSelectedWidth = 1.5; // a selected or hovered one's lines, at most
constexpr double kArrowLeast = 5.0;   // an arrowhead's least length
constexpr double kCentreLeast = 4.0;  // half a circle's centre mark, at least
// In text heights (the capital height on the picture, mitcad#894).
constexpr double kCapShare = 0.7;     // a font's capital height against its pixel size, if it does not say
constexpr double kArrowShare = 1.0;   // an arrowhead's length (and a datum triangle's height)
constexpr double kGapShare = 0.25;    // a leader's text from its end; a frame below its dimension line
constexpr double kExtensionGap = 0.5; // an extension line's visible gap at its point (mitcad#902)
constexpr double kBeyondShare = 0.8;  // an extension line past the dimension line
// An arrowhead in a plane seen obliquely keeps at least this share of its
// full width on screen, so that it reads as an arrowhead (mitcad#902).
constexpr double kLeastWidth = 0.6;
constexpr double kPadShare = 0.3;     // room round a text for clicks; a note's text from its leader
constexpr double kMarkShare = 0.5;    // half a circle's centre mark
constexpr double kDotShare = 0.22;    // a leader's dot's radius
constexpr double kCellShare = 2.0;    // a frame's cells and a datum label's square (ISO 1101: 2 h)
constexpr double kCellPad = 0.5;      // a text in its cell, each side
constexpr double kPitchShare = 1.6;   // a note's lines, baseline to baseline
constexpr double kSymbolShare = 0.6;  // half a frame's drawn symbol
constexpr double kStackShare = 0.72;  // stacked deviations against the value's size
constexpr double kStackGap = 0.4;     // between the stacked deviations
// Screen-facing labels (measurement objects without the model's drawing).
constexpr double kOffsetShare = 2.5;  // a length's dimension line from its ends
constexpr double kArcShare = 5.0;     // an angle's arc
constexpr double kBesideShare = 1.5;  // a label's middle from its line
// Auto's two inks (mitcad#902): near-black over what is light behind it
// (the light background, light bodies), a light grey over what is dark
// (the dark background, dark faces), against a picture of what lies
// behind (MeasurementBackdrop): lines and terminators piece by piece, an
// annotation's text and frames in one ink; and how light (qGray) what lies
// behind is at least for the near-black. Pieces of a line this long
// (pixels) take an ink each.
const QColor kDarkInk(0x1d, 0x21, 0x25);
const QColor kLightInk(0xbc, 0xc2, 0xc9);
constexpr int kLightBehind = 140;
constexpr double kInkPiece = 5.0;
// A dimension's text above its line (text heights; ISO 129-1).
constexpr double kAboveShare = 0.4;
// The lettering's letter spacing (percent): the narrow, tight spacing of
// ISO 3098 type B.
constexpr double kLetterSpacing = 94.0;
// An arrowhead's half angle (ISO 129-1: closed, filled, 30 degrees
// included): its half width against its length is tan 15 degrees.
constexpr double kArrowTan = 0.26794919243112270;
// ISO 129-1: a text reads from the bottom or from the right. One that would
// run down the picture steeper than this (the sine of 60 degrees) reads up
// instead, from the right: near-vertical texts read bottom to top.
constexpr double kSteep = 0.86602540378443865;
constexpr int kCircleSamples = 48; // points tried for a radius' rim
constexpr int kDotSamples = 16;    // a dot's outline
constexpr double kPi = 3.14159265358979323846;
// A plane seen at most this face on (the sine of the angle to the line of
// sight) is not drawn; from the second it is drawn whole.
constexpr double kFadeFrom = 0.08;
constexpr double kFadeFull = 0.3;
// How much a text's capitals are foreshortened across it at most, when
// its height on screen is held to the floor (a plane nearly edge on would
// otherwise take a huge text).
constexpr double kLeastAcross = 0.35;
// The bundled lettering's file (third_party/fonts/iso-lettering).
constexpr const char* kLetteringFile = "lettering.ttf";

QPointF unitOf(const QPointF& p) {
  const double n = std::hypot(p.x(), p.y());
  return n > 1e-12 ? QPointF(p.x() / n, p.y() / n) : QPointF(1.0, 0.0);
}

double lengthOf(const QPointF& p) { return std::hypot(p.x(), p.y()); }

double dotOf(const QPointF& a, const QPointF& b) { return a.x() * b.x() + a.y() * b.y(); }

// An arrowhead's length for a text this high on the picture.
double arrowLength(double textPixels, double scale) {
  return std::max(kArrowShare * textPixels, kArrowLeast * scale);
}

// A datum label's filled triangle (ISO 5459) facing the screen: its base on
// the feature at `foot`, square to the leader coming along `along`, its tip
// back up the leader; as high as an arrowhead is long.
QPolygonF datumTriangle(const QPointF& foot, const QPointF& along, double textPixels, double scale) {
  const QPointF dir = unitOf(along);
  const QPointF side(-dir.y(), dir.x());
  const double height = arrowLength(textPixels, scale);
  const double half = height / std::sqrt(3.0);
  QPolygonF triangle;
  triangle << foot - dir * height << foot + side * half << foot - side * half;
  return triangle;
}

// A leader's dot facing the screen.
QPolygonF screenDot(const QPointF& at, double textPixels) {
  const double r = kDotShare * textPixels;
  QPolygonF dot;
  for (int i = 0; i < kDotSamples; ++i) {
    const double t = 2.0 * kPi * i / kDotSamples;
    dot << at + QPointF(std::cos(t), std::sin(t)) * r;
  }
  return dot;
}

// The terminator `i` of a laid out annotation: the one laid out in its
// plane, else facing the screen (an arrowhead, a datum label's triangle,
// a dot).
QPolygonF terminatorOf(const MeasurementShape& shape, std::size_t i, double scale) {
  if (i < shape.heads.size() && shape.heads[i].size() >= 3) {
    return shape.heads[i];
  }
  const auto& [tip, along] = shape.arrows[i];
  if (shape.datumTerminator) {
    return datumTriangle(tip, along, shape.textPixels, scale);
  }
  if (shape.dotTerminator) {
    return screenDot(tip, shape.textPixels);
  }
  return measurementArrowHead(tip, along, shape.textPixels, scale);
}

// The capital height of `font`'s family against its pixel size: an H's
// ink as drawn (mitcad#902: a font's own cap height may be larger than
// its capitals are, and the text then smaller than the floor).
double capShare(const QFont& font) {
  static QHash<QString, double> known;
  const QString key = font.families().join(QLatin1Char(',')) + QLatin1Char('|') + font.family();
  if (const auto found = known.constFind(key); found != known.constEnd()) {
    return *found;
  }
  QFont probe = font;
  probe.setPixelSize(100);
  probe.setLetterSpacing(QFont::PercentageSpacing, 100.0);
  QPainterPath ink;
  ink.addText(QPointF(), probe, QStringLiteral("H"));
  const double cap = ink.boundingRect().height();
  const double share = cap > 10.0 && cap < 100.0 ? cap / 100.0 : kCapShare;
  known.insert(key, share);
  return share;
}

// `base` in the technical lettering, regular, its capitals `cap` pixels
// high.
QFont letteringFont(const QFont& base, double cap) {
  QFont font = base;
  const QString family = measurementFontFamily();
  if (!family.isEmpty()) {
    font.setFamilies({family});
  }
  font.setBold(false);
  font.setItalic(false);
  font.setPixelSize(std::max(1, static_cast<int>(std::lround(cap / capShare(font)))));
  if (!family.isEmpty()) {
    font.setLetterSpacing(QFont::PercentageSpacing, kLetterSpacing);
  }
  return font;
}

// A text at `at` (its baseline's start) in the painter's font, its glyphs
// filled in `ink` (mitcad#902: no outline stroke, no halo).
void lettering(QPainter& painter, const QPointF& at, const QString& text, const QColor& ink, const QColor& /*halo*/,
               double scale) {
  if (text.isEmpty()) {
    return;
  }
  // The depth symbol (ISO 129-1's ↧, U+21A7) drawn in lines (mitcad#902:
  // fonts draw it like a barred I at a small size): an arrow down onto a
  // bar, in the glyph's own advance, so that the text's width stays.
  const QChar depth(0x21A7);
  const QFont& font = painter.font();
  QPainterPath path;
  path.setFillRule(Qt::WindingFill);
  if (!text.contains(depth)) {
    path.addText(at, font, text);
  } else {
    const QFontMetricsF metrics(font);
    const double cap = font.pixelSize() * capShare(font);
    const double stroke = std::max(0.09 * cap, 0.9 * scale);
    QPointF pen = at;
    const QStringList parts = text.split(depth);
    for (qsizetype i = 0; i < parts.size(); ++i) {
      if (!parts[i].isEmpty()) {
        path.addText(pen, font, parts[i]);
        pen.rx() += metrics.horizontalAdvance(parts[i]);
      }
      if (i + 1 == parts.size()) {
        break;
      }
      const double advance = metrics.horizontalAdvance(depth);
      const double middle = pen.x() + advance / 2.0;
      const double half = std::min(0.4 * cap, advance / 2.0);
      const double base = pen.y() - stroke / 2.0;
      // The bar, the stem and an open head onto it, in strokes as the
      // lettering's.
      const double reach = std::min(0.38 * cap, advance / 2.0);
      QPainterPath strokes;
      strokes.moveTo(middle - half, base);
      strokes.lineTo(middle + half, base);
      strokes.moveTo(middle, base - cap + stroke / 2.0);
      strokes.lineTo(middle, base - stroke);
      strokes.moveTo(middle - reach, base - stroke - reach);
      strokes.lineTo(middle, base - stroke);
      strokes.lineTo(middle + reach, base - stroke - reach);
      QPainterPathStroker stroker;
      stroker.setWidth(stroke);
      stroker.setCapStyle(Qt::FlatCap);
      stroker.setJoinStyle(Qt::MiterJoin);
      path.addPath(stroker.createStroke(strokes));
      pen.rx() += advance;
    }
  }
  painter.save();
  painter.setPen(Qt::NoPen);
  painter.setBrush(ink);
  painter.drawPath(path);
  painter.restore();
}

gp_Pnt pointIn(const QJsonValue& value) {
  const QJsonArray a = value.toArray();
  return gp_Pnt(a.at(0).toDouble(), a.at(1).toDouble(), a.at(2).toDouble());
}

gp_Vec vectorIn(const QJsonValue& value) {
  const QJsonArray a = value.toArray();
  return gp_Vec(a.at(0).toDouble(), a.at(1).toDouble(), a.at(2).toDouble());
}

bool isPoint(const QJsonValue& value) {
  const QJsonArray a = value.toArray();
  return a.size() == 3 && a.at(0).isDouble() && a.at(1).isDouble() && a.at(2).isDouble();
}

// A unit vector square to `axis`.
gp_Vec squareTo(const gp_Vec& axis) {
  if (axis.Magnitude() < 1e-12) {
    return gp_Vec(1.0, 0.0, 0.0);
  }
  const gp_Vec n = axis.Normalized();
  gp_Vec e(1.0, 0.0, 0.0);
  if (std::abs(n.Y()) < std::abs(n.X()) && std::abs(n.Y()) <= std::abs(n.Z())) {
    e = gp_Vec(0.0, 1.0, 0.0);
  } else if (std::abs(n.Z()) < std::abs(n.X()) && std::abs(n.Z()) < std::abs(n.Y())) {
    e = gp_Vec(0.0, 0.0, 1.0);
  }
  const gp_Vec square = e - n * e.Dot(n);
  return square.Magnitude() > 1e-12 ? square.Normalized() : gp_Vec(1.0, 0.0, 0.0);
}

// The label's middle a little off a line's middle, on the side `normal`
// points to (screen-facing).
QPointF besideLine(const QPointF& middle, const QPointF& normal, double textPixels) {
  return middle + normal * (kBesideShare * textPixels);
}

// A unit normal of a picture's line that points up the picture (or left
// where the line is upright).
QPointF upwardNormal(const QPointF& along) {
  const QPointF dir = unitOf(along);
  QPointF n(-dir.y(), dir.x());
  if (n.y() > 1e-9 || (std::abs(n.y()) <= 1e-9 && n.x() > 0.0)) {
    n = -n;
  }
  return n;
}

void layoutLength(const MeasurementDisplay& m, const MeasurementProjection& project, double scale,
                  MeasurementShape& shape) {
  if (m.points.size() != 2) {
    return;
  }
  const std::optional<QPointF> p = project(m.points[0]);
  const std::optional<QPointF> q = project(m.points[1]);
  if (!p || !q) {
    return;
  }
  const double h = shape.textPixels;
  const QPointF along = *q - *p;
  const double length = lengthOf(along);
  shape.valid = true;
  if (length < 1.0) {
    // Seen end on: the label alone.
    shape.labelAt = besideLine(*p, QPointF(0.0, -1.0), h);
    return;
  }
  const QPointF n = upwardNormal(along);
  const double off = kOffsetShare * h;
  const QPointF a = *p + n * off;
  const QPointF b = *q + n * off;
  shape.lines.emplace_back(*p + n * (kExtensionGap * h), *p + n * (off + kBeyondShare * h));
  shape.lines.emplace_back(*q + n * (kExtensionGap * h), *q + n * (off + kBeyondShare * h));
  shape.lines.emplace_back(a, b);
  const QPointF dir = unitOf(b - a);
  const double arrow = arrowLength(h, scale);
  if (length >= 2.5 * arrow) {
    shape.arrows.emplace_back(a, -dir);
    shape.arrows.emplace_back(b, dir);
  } else {
    // Short: the arrows outside, pointing in.
    const QPointF tail = dir * (2.0 * arrow);
    shape.lines.emplace_back(a - tail, a);
    shape.lines.emplace_back(b, b + tail);
    shape.arrows.emplace_back(a, dir);
    shape.arrows.emplace_back(b, -dir);
  }
  shape.labelAt = besideLine((a + b) / 2.0, n, h);
}

void layoutCircle(const MeasurementDisplay& m, const MeasurementProjection& project, double scale,
                  MeasurementShape& shape) {
  gp_Pnt centre = m.center;
  gp_Pnt rim;
  bool found = false;
  const double h = shape.textPixels;
  const std::optional<QPointF> c = m.circle ? project(m.center) : std::nullopt;
  if (m.circle && c && m.radius > 0.0) {
    // The point of the circle farthest from its centre on the picture: the
    // radius is drawn where it shows longest.
    const gp_Vec u = squareTo(m.axis);
    const gp_Vec v = m.axis.Magnitude() > 1e-12 ? m.axis.Normalized().Crossed(u) : gp_Vec(0.0, 1.0, 0.0);
    double best = -1.0;
    for (int i = 0; i < kCircleSamples; ++i) {
      const double t = 2.0 * kPi * i / kCircleSamples;
      const gp_Pnt point = m.center.Translated((u * std::cos(t) + v * std::sin(t)) * m.radius);
      const std::optional<QPointF> at = project(point);
      if (at && lengthOf(*at - *c) > best + 1e-9) {
        best = lengthOf(*at - *c);
        rim = point;
        found = true;
      }
    }
  } else if (m.points.size() == 2) {
    centre = m.kind == QLatin1String("diameter")
                 ? gp_Pnt((m.points[0].XYZ() + m.points[1].XYZ()) / 2.0)
                 : m.points[0];
    rim = m.points[1];
    found = true;
  }
  if (!found) {
    return;
  }
  const std::optional<QPointF> from = project(centre);
  const std::optional<QPointF> to = project(rim);
  if (!from || !to) {
    return;
  }
  shape.valid = true;
  const QPointF along = *to - *from;
  if (lengthOf(along) < 1.0) {
    shape.labelAt = besideLine(*from, QPointF(0.0, -1.0), h);
    return;
  }
  const QPointF dir = unitOf(along);
  const double mark = std::max(kCentreLeast * scale, kMarkShare * h);
  shape.lines.emplace_back(*from - QPointF(mark, 0.0), *from + QPointF(mark, 0.0));
  shape.lines.emplace_back(*from - QPointF(0.0, mark), *from + QPointF(0.0, mark));
  if (m.kind == QLatin1String("diameter")) {
    const std::optional<QPointF> opposite = project(centre.Translated(gp_Vec(rim, centre)));
    if (!opposite) {
      shape.valid = false;
      return;
    }
    shape.lines.emplace_back(*opposite, *to);
    shape.arrows.emplace_back(*to, dir);
    shape.arrows.emplace_back(*opposite, -dir);
    shape.labelAt = besideLine(*from, upwardNormal(along), h);
  } else {
    shape.lines.emplace_back(*from, *to);
    shape.arrows.emplace_back(*to, dir);
    shape.labelAt = *to + dir * (kOffsetShare * h) + upwardNormal(along) * (0.5 * h);
  }
}

void layoutAngle(const MeasurementDisplay& m, const MeasurementProjection& project, MeasurementShape& shape) {
  const std::optional<QPointF> corner = project(m.corner);
  if (!m.arc || !corner) {
    return;
  }
  const double h = shape.textPixels;
  shape.valid = true;
  shape.labelAt = besideLine(*corner, QPointF(0.0, -1.0), h);
  if (m.from.Magnitude() < 1e-12 || m.to.Magnitude() < 1e-12) {
    return;
  }
  const gp_Vec f = m.from.Normalized();
  const gp_Vec t = m.to.Normalized();
  const double angle = std::acos(std::clamp(f.Dot(t), -1.0, 1.0));
  if (angle < 1e-6) {
    return; // parallel: no sector
  }
  gp_Vec w = t - f * f.Dot(t);
  w = w.Magnitude() > 1e-9 ? w.Normalized() : squareTo(f);
  // The arc's radius in the design: kArcShare text heights at the corner.
  double perMillimetre = 0.0;
  for (const gp_Vec& d : {f, w}) {
    if (const std::optional<QPointF> one = project(m.corner.Translated(d))) {
      perMillimetre = std::max(perMillimetre, lengthOf(*one - *corner));
    }
  }
  if (!(perMillimetre > 1e-9) || !std::isfinite(perMillimetre)) {
    return;
  }
  const double r = kArcShare * h / perMillimetre;
  const int steps = std::max(8, static_cast<int>(std::ceil(angle / (kPi / 36.0))));
  QPolygonF arc;
  for (int i = 0; i <= steps; ++i) {
    const double s = angle * i / steps;
    if (const std::optional<QPointF> at = project(m.corner.Translated((f * std::cos(s) + w * std::sin(s)) * r))) {
      arc << *at;
    }
  }
  if (arc.size() < 2) {
    return;
  }
  // The rays a little past the arc, as extension lines.
  for (const gp_Vec& ray : {f, t}) {
    if (const std::optional<QPointF> end = project(m.corner.Translated(ray * (r * 1.2)))) {
      shape.lines.emplace_back(*corner, *end);
    }
  }
  shape.arcs.push_back(arc);
  const qsizetype last = arc.size() - 1;
  shape.arrows.emplace_back(arc[0], arc[0] - arc[1]);
  shape.arrows.emplace_back(arc[last], arc[last] - arc[last - 1]);
  const gp_Vec middle = f * std::cos(angle / 2.0) + w * std::sin(angle / 2.0);
  if (const std::optional<QPointF> at = project(m.corner.Translated(middle * (r * 1.45)))) {
    shape.labelAt = *at;
  }
}

// The picture's direction of a design direction at a point: its pixels per
// millimetre along it. None where the point cannot be drawn.
std::optional<QPointF> pictureDirection(const MeasurementProjection& project, const gp_Pnt& at, const gp_Vec& along,
                                        const QPointF& here) {
  if (along.Magnitude() < 1e-12) {
    return std::nullopt;
  }
  // A small step: a perspective's projection is not linear.
  constexpr double kStep = 1e-3;
  const std::optional<QPointF> there = project(at.Translated(along.Normalized() * kStep));
  if (!there) {
    return std::nullopt;
  }
  return (*there - here) / kStep;
}

// What ends a leader or a dimension line.
enum class Terminator { Arrow, Datum, Dot };

// A terminator laid out in the annotation's plane (mitcad#902: as the text
// lies, foreshortened with it): its tip at `tip`, pointing along the unit
// `dir`, `length` millimetres long, its width along the plane's
// `normal` x `dir`; none where a point of it cannot be drawn or the plane
// is not known.
QPolygonF planeTerminator(const MeasurementProjection& project, const gp_Pnt& tip, const gp_Vec& dir,
                          const gp_Vec& normal, double length, Terminator kind, double dotRadius) {
  if (normal.Magnitude() < 1e-12 || dir.Magnitude() < 1e-12 || !(length > 0.0)) {
    return {};
  }
  gp_Vec side = normal.Crossed(dir);
  if (side.Magnitude() < 1e-9) {
    return {};
  }
  side.Normalize();
  std::vector<gp_Pnt> points;
  switch (kind) {
  case Terminator::Arrow: {
    const gp_Pnt back = tip.Translated(dir * -length);
    points = {tip, back.Translated(side * (length * kArrowTan)), back.Translated(side * (-length * kArrowTan))};
    break;
  }
  case Terminator::Datum: {
    const double half = length / std::sqrt(3.0);
    points = {tip.Translated(dir * -length), tip.Translated(side * half), tip.Translated(side * -half)};
    break;
  }
  case Terminator::Dot:
    for (int i = 0; i < kDotSamples; ++i) {
      const double t = 2.0 * kPi * i / kDotSamples;
      points.push_back(tip.Translated((dir * std::cos(t) + side * std::sin(t)) * dotRadius));
    }
    break;
  }
  QPolygonF polygon;
  for (const gp_Pnt& point : points) {
    const std::optional<QPointF> at = project(point);
    if (!at) {
      return {};
    }
    polygon << *at;
  }
  // An arrowhead seen obliquely: at least kLeastWidth of its full width
  // across its axis on the picture, so that it does not dwindle to a
  // stroke.
  if (kind == Terminator::Arrow && polygon.size() == 3) {
    const QPointF base = (polygon[1] + polygon[2]) / 2.0;
    const QPointF axis = base - polygon[0];
    const double least = kLeastWidth * lengthOf(axis) * kArrowTan;
    const QPointF across(-unitOf(axis).y(), unitOf(axis).x());
    const double half = std::abs(dotOf(polygon[1] - base, across));
    if (lengthOf(axis) > 1e-9 && half < least) {
      polygon[1] = base + across * least;
      polygon[2] = base - across * least;
    }
  }
  return polygon;
}

// The label's sizes in its own coordinates (x along it, y down, 0 at its
// anchor): the value's line, a stack of deviations, the text after; a
// note's lines; a frame's cells.
struct LabelBox {
  QRectF rect;        // with a little room round the text (clicks), a frame's cells
  double mainWidth = 0.0;
  double stackWidth = 0.0;
  double bracket = 0.0; // a bracket's width
  double baseline = 0.0;
  double upperBaseline = 0.0; // the stacked deviations'
  double lowerBaseline = 0.0;
  double lineHeight = 0.0; // a note's lines, baseline to baseline
  std::vector<double> cells; // a feature control frame's cells' widths
};

QFont stackFont(const QFont& font) {
  QFont reduced = font;
  reduced.setPixelSize(std::max(1, static_cast<int>(std::lround(font.pixelSize() * kStackShare))));
  return reduced;
}

// A feature control frame's cell as written: the tolerance without its
// material condition's circled letter, which is drawn (circledWidth).
QString cellWords(const MeasurementShape& shape, std::size_t i) {
  QString words = shape.frameCells.at(static_cast<qsizetype>(i));
  // " Ⓜ" or " Ⓛ": a circled capital (U+24B6 to U+24CF) after a space.
  if (i == 1 && !shape.modifier.isEmpty() && words.size() >= 2 && words.back().unicode() >= 0x24B6 &&
      words.back().unicode() <= 0x24CF) {
    words.chop(2);
  }
  return words;
}

// The circle of a material condition (Ⓜ, Ⓛ) and the space before it.
double circledWidth(const MeasurementShape& shape, std::size_t i, const QFontMetricsF& metrics, double unit) {
  if (cellWords(shape, i).size() == shape.frameCells.at(static_cast<qsizetype>(i)).size()) {
    return 0.0; // none, or written as it is
  }
  return metrics.horizontalAdvance(QLatin1Char(' ')) + 1.3 * unit;
}

// An ISO 1101 characteristic's symbol in lines (fonts often lack them),
// about `r` from `c` each way, in the painter's pen; arrowheads filled in
// its colour.
void paintCharacteristic(QPainter& painter, const QString& key, const QPointF& c, double r) {
  const auto at = [&c, r](double x, double y) { return c + QPointF(x * r, y * r); };
  const auto line = [&painter, &at](double x1, double y1, double x2, double y2) {
    painter.drawLine(at(x1, y1), at(x2, y2));
  };
  const auto circle = [&painter, &c](double radius) {
    painter.setBrush(Qt::NoBrush);
    painter.drawEllipse(c, radius, radius);
  };
  // A line from (x1, y1) to an arrowhead at (x2, y2).
  const auto arrow = [&painter, &at, &line, r](double x1, double y1, double x2, double y2) {
    line(x1, y1, x2, y2);
    const QPointF tip = at(x2, y2);
    const QPointF along = unitOf(at(x2, y2) - at(x1, y1));
    const QPointF side(-along.y(), along.x());
    const double length = 0.55 * r;
    QPolygonF head;
    head << tip << tip - along * length + side * (length * 0.4) << tip - along * length - side * (length * 0.4);
    painter.setBrush(painter.pen().color());
    painter.drawPolygon(head);
    painter.setBrush(Qt::NoBrush);
  };
  // 60° slopes, as ISO 1101 draws the inclined strokes.
  const double dx = 0.5;
  const double dy = 0.866;
  if (key == QLatin1String("straightness")) {
    line(-1.0, 0.0, 1.0, 0.0);
  } else if (key == QLatin1String("flatness")) {
    painter.setBrush(Qt::NoBrush);
    painter.drawPolygon(QPolygonF{at(-1.0, 0.45), at(0.55, 0.45), at(1.0, -0.45), at(-0.55, -0.45)});
  } else if (key == QLatin1String("circularity")) {
    circle(0.8 * r);
  } else if (key == QLatin1String("cylindricity")) {
    circle(0.55 * r);
    for (const double side : {-1.0, 1.0}) {
      const double ox = side * 0.55 * dy;
      const double oy = side * 0.55 * dx;
      line(ox - dx, oy + dy, ox + dx, oy - dy);
    }
  } else if (key == QLatin1String("line_profile") || key == QLatin1String("surface_profile")) {
    painter.setBrush(Qt::NoBrush);
    const QRectF bounds(at(-0.9, -0.5), at(0.9, 1.3));
    painter.drawArc(bounds, 0, 180 * 16);
    if (key == QLatin1String("surface_profile")) {
      line(-0.9, 0.4, 0.9, 0.4);
    }
  } else if (key == QLatin1String("parallelism")) {
    for (const double ox : {-0.35, 0.35}) {
      line(ox - dx, dy, ox + dx, -dy);
    }
  } else if (key == QLatin1String("perpendicularity")) {
    line(0.0, -1.0, 0.0, 0.9);
    line(-0.9, 0.9, 0.9, 0.9);
  } else if (key == QLatin1String("angularity")) {
    line(-0.9, 0.9, 0.9, 0.9);
    line(-0.9, 0.9, 0.9, 0.9 - 1.8 * 0.577);
  } else if (key == QLatin1String("position")) {
    circle(0.6 * r);
    line(-1.0, 0.0, 1.0, 0.0);
    line(0.0, -1.0, 0.0, 1.0);
  } else if (key == QLatin1String("concentricity")) {
    circle(0.85 * r);
    circle(0.45 * r);
  } else if (key == QLatin1String("symmetry")) {
    line(-1.0, 0.0, 1.0, 0.0);
    line(-0.6, -0.5, 0.6, -0.5);
    line(-0.6, 0.5, 0.6, 0.5);
  } else if (key == QLatin1String("circular_runout")) {
    arrow(-0.6, 0.8, 0.5, -0.9);
  } else if (key == QLatin1String("total_runout")) {
    arrow(-0.8, 0.8, 0.1, -0.9);
    arrow(0.0, 0.8, 0.9, -0.9);
    line(-1.0, 0.8, 0.2, 0.8);
  }
}

// Whether a characteristic's symbol is drawn in lines (one of ISO 1101's
// fourteen) rather than written.
bool drawnSymbol(const QString& key) {
  static const char* const kKeys[] = {"straightness",  "flatness",         "circularity", "cylindricity",
                                      "line_profile",  "surface_profile",  "parallelism", "perpendicularity",
                                      "angularity",    "position",         "concentricity", "symmetry",
                                      "circular_runout", "total_runout"};
  return std::any_of(std::begin(kKeys), std::end(kKeys), [&key](const char* own) { return key == QLatin1String(own); });
}

// The height of a shape's text on the picture: the model's in its plane,
// else a screen-facing label's (the floor).
double textPixelsOf(const MeasurementShape& shape, double scale) {
  return shape.textPixels > 0.0 ? shape.textPixels : kTextFloor * scale;
}

// A feature control frame's cells' widths (ISO 1101: the symbol's cell a
// square two text heights `unit` high, the others round their words).
std::vector<double> frameCellWidths(const MeasurementShape& frame, const QFontMetricsF& metrics, double unit) {
  const double height = std::round(kCellShare * unit);
  const double cellPad = kCellPad * unit;
  std::vector<double> cells;
  for (std::size_t i = 0; i < static_cast<std::size_t>(frame.frameCells.size()); ++i) {
    const double words = metrics.horizontalAdvance(cellWords(frame, i)) + circledWidth(frame, i, metrics, unit);
    cells.push_back(i == 0 && drawnSymbol(frame.characteristic) ? height
                                                                 : std::max(height, std::ceil(words + 2.0 * cellPad)));
  }
  return cells;
}

// The widest of a leader's lines of text.
double widestLine(const QStringList& lines, const QFontMetricsF& metrics) {
  double width = 0.0;
  for (const QString& line : lines) {
    width = std::max(width, metrics.horizontalAdvance(line));
  }
  return width;
}

// The width a leader's text and the frames under it share (mitcad#902:
// its lines and the frames set flush left).
double leaderBlockWidth(const QStringList& lines, const std::vector<MeasurementDisplay::FrameUnder>& frames,
                        const QFontMetricsF& metrics, double unit) {
  double width = widestLine(lines, metrics);
  for (const MeasurementDisplay::FrameUnder& under : frames) {
    MeasurementShape frame;
    frame.frameCells = under.cells;
    frame.characteristic = under.characteristic;
    frame.modifier = under.modifier;
    double cells = 0.0;
    for (const double cell : frameCellWidths(frame, metrics, unit)) {
      cells += cell;
    }
    width = std::max(width, cells);
  }
  return width;
}

LabelBox labelBox(const MeasurementShape& shape, const QFont& font, double scale) {
  const QFontMetricsF metrics(font);
  const double unit = textPixelsOf(shape, scale);
  const double pad = kPadShare * unit;
  const double gap = kGapShare * unit;
  // A leader's text from its end (a note's, a callout's, a frame's under
  // a callout).
  const double away = gap + pad;
  LabelBox box;
  box.lineHeight = kPitchShare * unit;
  if (!shape.frameCells.isEmpty() || !shape.datumLetter.isEmpty()) {
    // A feature control frame's cells in a row, a datum label's letter in
    // a square (ISO 1101, ISO 5459: two text heights high): the box is the
    // frame drawn round them, in whole pixels. At the end of the leader,
    // its middle on it; under a dimension's line, centred there; under a
    // hole callout, below its lines on the leader's side (mitcad#902). The
    // symbol's cell is a square.
    const double height = std::round(kCellShare * unit);
    const double cellPad = kCellPad * unit;
    double width = 0.0;
    if (!shape.frameCells.isEmpty()) {
      box.cells = frameCellWidths(shape, metrics, unit);
      for (const double cell : box.cells) {
        width += cell;
      }
    } else {
      width = std::max(height, std::ceil(metrics.horizontalAdvance(shape.datumLetter) + 2.0 * cellPad));
    }
    double left = shape.centred ? -width / 2.0 : shape.rightAligned ? -width : 0.0;
    double top = -height / 2.0;
    if (shape.attached && shape.underLines > 0) {
      // Below the leader's lines (the first level with the landing), flush
      // with their left edge (mitcad#902).
      top = (shape.underLines - 0.5) * box.lineHeight + gap;
      const double block = std::max(width, widestLine(shape.overLines, metrics));
      left = shape.rightAligned ? -away - block : away;
    } else if (shape.attached) {
      left = -width / 2.0;
      top = gap;
    }
    box.mainWidth = width;
    box.rect = QRectF(left, top, width, height);
    // The capitals in the middle of the cell.
    box.baseline = top + (height + unit) / 2.0;
    return box;
  }
  if (shape.note) {
    // Set flush left with the frames under it, its first line level with
    // the landing and the others below (mitcad#902, as a drawing's).
    const double width = leaderBlockWidth(shape.noteLines, shape.framesUnder, metrics, unit);
    const double height = box.lineHeight * static_cast<double>(std::max<qsizetype>(1, shape.noteLines.size()));
    const double left = shape.rightAligned ? -away - width : away;
    const double top = -box.lineHeight / 2.0;
    box.mainWidth = width;
    box.rect = QRectF(left - pad, top - pad, width + 2.0 * pad, height + 2.0 * pad);
    // Each line's capitals in the middle of its pitch.
    box.baseline = top + (box.lineHeight + unit) / 2.0;
    return box;
  }
  const QFontMetricsF stacked(stackFont(font));
  const QString main = shape.main.isEmpty() && shape.upper.isEmpty() ? shape.label : shape.main;
  box.mainWidth = metrics.horizontalAdvance(main);
  double width = box.mainWidth;
  double height = unit;
  double stack = 0.0;
  if (!shape.upper.isEmpty() || !shape.lower.isEmpty()) {
    box.stackWidth = std::max(stacked.horizontalAdvance(shape.upper), stacked.horizontalAdvance(shape.lower));
    box.bracket = shape.brackets ? metrics.horizontalAdvance(QStringLiteral("(")) : 0.0;
    width += (box.mainWidth > 0.0 ? metrics.horizontalAdvance(QLatin1Char(' ')) : 0.0) + box.stackWidth +
             2.0 * box.bracket;
    stack = (2.0 * kStackShare + kStackGap) * unit;
    height = std::max(height, stack);
  }
  if (!shape.after.isEmpty()) {
    width += metrics.horizontalAdvance(QLatin1Char(' ') + shape.after);
  }
  // Centred on the anchor, a quarter of the text's height above it (the
  // dimension line, ISO 129-1): the value's capitals in the middle of the
  // block, the deviations stacked in it.
  const double bottom = -kAboveShare * unit;
  box.rect = QRectF(-width / 2.0 - pad, bottom - height - pad, width + 2.0 * pad, height + 2.0 * pad);
  box.baseline = bottom - (height - unit) / 2.0;
  const double stackTop = bottom - height + (height - stack) / 2.0;
  box.upperBaseline = stackTop + kStackShare * unit;
  box.lowerBaseline = stackTop + stack;
  return box;
}

void layoutPlanar(const MeasurementDisplay& m, const MeasurementProjection& project, double scale,
                  MeasurementShape& shape) {
  shape.planar = true;
  // A diameter the model put on a leader (its text's end given, mitcad#902)
  // is written as a note's line.
  const bool ledDiameter = m.kind == QLatin1String("diameter") &&
                           (m.align == QLatin1String("start") || m.align == QLatin1String("end"));
  shape.note = m.kind == QLatin1String("note") || m.kind == QLatin1String("hole_callout") || ledDiameter;
  shape.leader = shape.note || m.kind == QLatin1String("datum") || m.kind == QLatin1String("frame");
  shape.datumTerminator = m.datumTerminator;
  shape.dotTerminator = m.dotTerminator && !m.datumTerminator;
  shape.attached = !m.attached.isEmpty();
  shape.underLines = m.underLines;
  shape.overLines = m.overLines;
  shape.framesUnder = m.framesUnder;
  shape.centred = m.align == QLatin1String("middle");
  if (!m.frame.isEmpty()) {
    shape.frameCells = m.frame;
    shape.characteristic = m.characteristic;
    shape.modifier = m.modifier;
  } else if (!m.datum.isEmpty()) {
    shape.datumLetter = m.datum;
  } else if (shape.leader) {
    shape.noteLines = m.lines.isEmpty() ? QStringList{m.label} : m.lines;
  } else {
    shape.main = m.main;
    shape.upper = m.upper;
    shape.lower = m.lower;
    shape.brackets = m.brackets;
    shape.after = m.after;
  }
  const std::optional<QPointF> at = project(m.textAt);
  if (!at) {
    return;
  }
  // The text's axes on the picture, per millimetre.
  const std::optional<QPointF> ex = pictureDirection(project, m.textAt, m.textDirection, *at);
  const std::optional<QPointF> ey = pictureDirection(project, m.textAt, m.textUp, *at);
  if (!ex || !ey) {
    return;
  }
  // How face on the plane is: the smaller of the projection's two singular
  // values against the larger.
  const double xx = dotOf(*ex, *ex);
  const double yy = dotOf(*ey, *ey);
  const double xy = dotOf(*ex, *ey);
  const double det = ex->x() * ey->y() - ex->y() * ey->x();
  // The eigenvalue gap of the Gram matrix without subtracting nearly
  // equal fourth powers: face-on projections have equal singular values.
  const double larger = std::sqrt(std::max(0.0, (xx + yy + std::hypot(xx - yy, 2.0 * xy)) / 2.0));
  if (!(larger > 1e-12) || !std::isfinite(larger)) {
    return;
  }
  const double faceOn = std::abs(det) / (larger * larger);
  shape.opacity = std::clamp((faceOn - kFadeFrom) / (kFadeFull - kFadeFrom), 0.0, 1.0);
  shape.valid = true;
  // The text the model's height in its plane (mitcad#894; `larger` is the
  // plane's pixels per millimetre at the text), its capitals as seen on
  // screen (foreshortened across the text as the plane turns away) never
  // under the floor nor over the ceiling (mitcad#902).
  const double across = std::max(kLeastAcross, faceOn * larger / std::max(lengthOf(*ex), 1e-12));
  shape.textPixels =
      std::clamp(m.textHeight * larger * across, measurementTextFloor(scale), measurementTextCeiling(scale)) / across;
  // The text's axes, a pixel per pixel face on, and readable (ISO 129-1:
  // from the bottom or from the right): never mirrored (the picture's y
  // runs down), running left to right, and one that would then run down
  // steeply (within 30 degrees of straight down) runs up instead, so that
  // vertical and near-vertical texts read bottom to top from any side.
  QPointF ux = *ex / larger;
  QPointF uy = *ey / larger;
  if (ux.x() * uy.y() - ux.y() * uy.x() > 0.0) {
    uy = -uy;
  }
  if (ux.x() < 0.0) {
    ux = -ux;
    uy = -uy;
  }
  if (ux.y() > kSteep * lengthOf(ux)) {
    ux = -ux;
    uy = -uy;
  }
  shape.textTransform = QTransform(ux.x(), ux.y(), -uy.x(), -uy.y(), at->x(), at->y());
  // A leader's text runs away from its arrow: the model says which end the
  // leader meets along its text's direction (mitcad#902), the other one
  // where the text was turned round to read; else from the arrow's side.
  const bool turned = dotOf(ux, *ex) < 0.0;
  if (m.align == QLatin1String("end")) {
    shape.rightAligned = !turned;
  } else if (m.align == QLatin1String("start")) {
    shape.rightAligned = turned;
  } else if (shape.leader && !m.arrows.empty()) {
    if (const std::optional<QPointF> tip = project(m.arrows.front().tip)) {
      shape.rightAligned = dotOf(*tip - *at, ux) > 0.0;
    }
  }
  // The plane's axes per millimetre at the text, for drags.
  if (const std::optional<QPointF> px = pictureDirection(project, m.textAt, m.planeX, *at)) {
    shape.xPerMillimetre = *px;
  }
  if (const std::optional<QPointF> py = pictureDirection(project, m.textAt, m.planeY, *at)) {
    shape.yPerMillimetre = *py;
  }
  // Millimetres of the plane per text height on the picture.
  const double millimetres = 1.0 / larger;
  gp_Vec normal = m.planeNormal;
  if (normal.Magnitude() < 1e-12) {
    normal = m.textDirection.Crossed(m.textUp);
  }
  if (normal.Magnitude() > 1e-12) {
    normal.Normalize();
  }

  for (const MeasurementDisplay::Segment& segment : m.extensionLines) {
    const std::optional<QPointF> p = project(segment.from);
    const std::optional<QPointF> q = project(segment.to);
    if (!p || !q || lengthOf(*q - *p) < 1e-6) {
      continue;
    }
    // A gap of a quarter of the text's height at the measured point, half
    // its height past the dimension line (mitcad#894).
    const QPointF u = unitOf(*q - *p);
    const double gap = std::min(kExtensionGap * shape.textPixels, lengthOf(*q - *p) / 2.0);
    shape.lines.emplace_back(*p + u * gap, *q + u * (kBeyondShare * shape.textPixels));
  }
  const bool length = !shape.leader && m.kind != QLatin1String("angle") && m.kind != QLatin1String("radius") &&
                      m.kind != QLatin1String("diameter");
  bool first = true;
  for (const MeasurementDisplay::Segment& segment : m.dimensionLines) {
    const std::optional<QPointF> p = project(segment.from);
    const std::optional<QPointF> q = project(segment.to);
    if (!p || !q) {
      continue;
    }
    shape.lines.emplace_back(*p, *q);
    // A length's dimension line itself (the first), for a drag.
    if (length && first) {
      shape.grips.push_back(QPolygonF{*p, *q});
    }
    first = false;
  }
  for (const MeasurementDisplay::Arc& arc : m.arcs) {
    if (arc.from.Magnitude() < 1e-12 || arc.to.Magnitude() < 1e-12 || arc.normal.Magnitude() < 1e-12) {
      continue;
    }
    const gp_Vec f = arc.from.Normalized();
    const gp_Vec w = arc.normal.Normalized().Crossed(f);
    const double angle = std::acos(std::clamp(f.Dot(arc.to.Normalized()), -1.0, 1.0));
    const int steps = std::max(8, static_cast<int>(std::ceil(angle / (kPi / 36.0))));
    QPolygonF points;
    for (int i = 0; i <= steps; ++i) {
      const double s = angle * i / steps;
      if (const std::optional<QPointF> p =
              project(arc.center.Translated((f * std::cos(s) + w * std::sin(s)) * arc.radius))) {
        points << *p;
      }
    }
    if (points.size() >= 2) {
      shape.arcs.push_back(points);
      shape.grips.push_back(points);
    }
  }
  // The arrows in the design: a length too short on the picture for its
  // arrows inside has them outside, pointing in, with a tail each.
  std::vector<std::pair<gp_Pnt, gp_Vec>> arrows;
  for (const MeasurementDisplay::Arrow& arrow : m.arrows) {
    if (arrow.direction.Magnitude() > 1e-12) {
      arrows.emplace_back(arrow.tip, arrow.direction.Normalized());
    }
  }
  const double arrowPixels = arrowLength(shape.textPixels, scale);
  if (length && arrows.size() == 2 && !shape.grips.empty()) {
    const QPointF from = shape.grips.front().first();
    const QPointF to = shape.grips.front().last();
    if (lengthOf(to - from) < 2.5 * arrowPixels && lengthOf(to - from) > 1e-6) {
      const QPointF tail = unitOf(to - from) * (2.0 * arrowPixels);
      shape.lines.emplace_back(from - tail, from);
      shape.lines.emplace_back(to, to + tail);
      for (auto& arrow : arrows) {
        arrow.second.Reverse();
      }
    }
  }
  // Each arrowhead (a datum's triangle, a dot) in the plane, sized by the
  // text on the picture (mitcad#902).
  const Terminator kind = shape.datumTerminator ? Terminator::Datum
                          : shape.dotTerminator ? Terminator::Dot
                                                : Terminator::Arrow;
  for (const auto& [tip3, dir3] : arrows) {
    const std::optional<QPointF> tip = project(tip3);
    if (!tip) {
      continue;
    }
    const std::optional<QPointF> along = pictureDirection(project, tip3, dir3, *tip);
    if (!along || lengthOf(*along) <= 1e-9) {
      continue;
    }
    shape.arrows.emplace_back(*tip, *along);
    shape.heads.push_back(planeTerminator(project, tip3, dir3, normal, arrowPixels * millimetres, kind,
                                          kDotShare * shape.textPixels * millimetres));
  }
  // A circle's centre mark: a cross in its plane.
  if (m.centerMark) {
    const double mark = std::max(kCentreLeast * scale, kMarkShare * shape.textPixels) * millimetres;
    std::vector<gp_Vec> arms;
    if (m.planeX.Magnitude() > 1e-12 && m.planeY.Magnitude() > 1e-12) {
      arms.push_back(m.planeX.Normalized());
      arms.push_back(m.planeY.Normalized());
    }
    bool drawn = !arms.empty();
    std::vector<QLineF> cross;
    for (const gp_Vec& arm : arms) {
      const std::optional<QPointF> p = project(m.centerMark->Translated(arm * -mark));
      const std::optional<QPointF> q = project(m.centerMark->Translated(arm * mark));
      if (!p || !q) {
        drawn = false;
        break;
      }
      cross.emplace_back(*p, *q);
    }
    if (drawn) {
      shape.lines.insert(shape.lines.end(), cross.begin(), cross.end());
    } else if (const std::optional<QPointF> c = project(*m.centerMark)) {
      const double half = mark / millimetres;
      shape.lines.emplace_back(*c - QPointF(half, 0.0), *c + QPointF(half, 0.0));
      shape.lines.emplace_back(*c - QPointF(0.0, half), *c + QPointF(0.0, half));
    }
  }
  // Crisp: the lines on the pixel grid (mitcad#902).
  const double width = measurementLineWidth(scale);
  for (QLineF& line : shape.lines) {
    line = measurementSnapped(line, width);
  }
  // A datum label's square on its stem (mitcad#902): the middle of its
  // side, not its own middle, at the stem's end, as large as the square is
  // on the picture.
  if (shape.centred && !m.dimensionLines.empty()) {
    const std::optional<QPointF> end = project(m.dimensionLines.front().from);
    const std::optional<QPointF> foot = project(m.dimensionLines.front().to);
    const QRectF square = labelBox(shape, measurementTextFont(QFont(), shape, scale), scale).rect;
    bool invertible = false;
    const QTransform inverse = shape.textTransform.inverted(&invertible);
    if (end && foot && invertible && lengthOf(*end - *foot) > 1e-6 && square.width() > 0.0 &&
        square.height() > 0.0) {
      // The stem meets the middle of the square's side it comes to.
      const QPointF along = inverse.map(*end + unitOf(*end - *foot)) - inverse.map(*end);
      const bool upright = std::abs(along.y()) / square.height() >= std::abs(along.x()) / square.width();
      const QPointF reach = upright ? QPointF(0.0, std::copysign(square.height() / 2.0, along.y()))
                                    : QPointF(std::copysign(square.width() / 2.0, along.x()), 0.0);
      const QPointF middle = *end + (shape.textTransform.map(reach) - shape.textTransform.map(QPointF()));
      shape.textTransform = QTransform(shape.textTransform.m11(), shape.textTransform.m12(), shape.textTransform.m21(),
                                       shape.textTransform.m22(), middle.x(), middle.y());
    }
  }
  // A leader's text set flush left with its frames, ending at the leader:
  // the landing runs on to its first line's end (mitcad#902).
  if (shape.note && shape.rightAligned && !shape.noteLines.isEmpty() && !m.dimensionLines.empty()) {
    const QFontMetricsF metrics(measurementTextFont(QFont(), shape, scale));
    const double unit = textPixelsOf(shape, scale);
    const double block = leaderBlockWidth(shape.noteLines, shape.framesUnder, metrics, unit);
    const double firstLine = metrics.horizontalAdvance(shape.noteLines.front());
    if (block - firstLine > 0.5) {
      const QLineF more(shape.textTransform.map(QPointF(0.0, 0.0)),
                        shape.textTransform.map(QPointF(firstLine - block, 0.0)));
      shape.lines.push_back(measurementSnapped(more, measurementLineWidth(scale)));
    }
  }
  // Where the text is, for clicks and the tests' log; the label's middle.
  const QRectF box = labelBox(shape, measurementTextFont(QFont(), shape, scale), scale).rect;
  shape.textArea = shape.textTransform.map(QPolygonF(box));
  shape.labelAt = shape.textTransform.map(box.center());
}

// The deviation on the picture of a drag `delta` across a shape's line
// (along its plane's y axis): the part `b` along y of the drag split into
// the plane's axes (a x + b y), as the drag is committed
// (measurementDragMillimetres); in an oblique view the axes are not at
// right angles on the picture, so not the drag's perpendicular projection
// on y (mitcad#759). None where the plane is seen edge on.
QPointF acrossLine(const MeasurementShape& shape, const QPointF& delta) {
  const std::optional<std::array<double, 2>> along = measurementDragMillimetres(shape, delta);
  if (!along) {
    return {};
  }
  return shape.yPerMillimetre * (*along)[1];
}

// The painter's transform for a shape's text, moved by under half a pixel
// so that a frame's lines lie on the pixel grid where the text is upright
// and face on (its cells' sides then crisp).
QTransform snappedTextTransform(const MeasurementShape& shape, const QRectF& rect, double width) {
  const QTransform& t = shape.textTransform;
  const bool upright = std::abs(t.m12()) < 1e-6 && std::abs(t.m21()) < 1e-6 && std::abs(std::abs(t.m11()) - 1.0) < 1e-6 &&
                       std::abs(std::abs(t.m22()) - 1.0) < 1e-6;
  if (!upright) {
    return t;
  }
  const QPointF corner = t.map(rect.topLeft());
  const QPointF shift = measurementSnapped(corner, width) - corner;
  return t * QTransform::fromTranslate(shift.x(), shift.y());
}

// A label in its plane, in `ink` (mitcad#894, mitcad#902): a dimension's
// value and a note's lines in the lettering without a card; a feature
// control frame's cells and a datum label's square outlined in thin
// lines, not filled.
void paintPlanarLabel(QPainter& painter, const MeasurementShape& shape, const QFont& font, double scale,
                      const QColor& ink, const QColor& halo, double lineWidth) {
  const LabelBox box = labelBox(shape, font, scale);
  const double unit = textPixelsOf(shape, scale);
  painter.save();
  painter.setTransform(snappedTextTransform(shape, box.rect, lineWidth), true);
  painter.setFont(font);
  if (!box.cells.empty() || !shape.datumLetter.isEmpty()) {
    // The frame's outline and the lines between its cells, each cell's
    // text centred in it; a datum label's letter in its square.
    QPen pen(ink, lineWidth, Qt::SolidLine, Qt::FlatCap, Qt::MiterJoin);
    pen.setCosmetic(true);
    painter.setPen(pen);
    painter.setBrush(Qt::NoBrush);
    painter.drawRect(box.rect);
    const QFontMetricsF metrics(font);
    if (box.cells.empty()) {
      const double width = metrics.horizontalAdvance(shape.datumLetter);
      lettering(painter, QPointF(box.rect.center().x() - width / 2.0, box.baseline), shape.datumLetter, ink, halo,
                scale);
      painter.restore();
      return;
    }
    double x = box.rect.left();
    const double middle = box.rect.center().y();
    for (std::size_t i = 0; i < box.cells.size(); ++i) {
      if (i > 0) {
        painter.drawLine(QPointF(x, box.rect.top()), QPointF(x, box.rect.bottom()));
      }
      if (i == 0 && drawnSymbol(shape.characteristic)) {
        // The symbol in lines, in the middle of its square cell.
        paintCharacteristic(painter, shape.characteristic, QPointF(x + box.cells[i] / 2.0, middle),
                            kSymbolShare * unit);
        x += box.cells[i];
        continue;
      }
      const QString words = cellWords(shape, i);
      const double circled = circledWidth(shape, i, metrics, unit);
      const double width = metrics.horizontalAdvance(words) + circled;
      const double left = x + (box.cells[i] - width) / 2.0;
      lettering(painter, QPointF(left, box.baseline), words, ink, halo, scale);
      if (circled > 0.0) {
        // Ⓜ or Ⓛ: the letter in a circle after the tolerance.
        const double diameter = 1.3 * unit;
        const QPointF centre(left + width - diameter / 2.0, box.baseline - unit / 2.0);
        painter.setBrush(Qt::NoBrush);
        painter.drawEllipse(centre, diameter / 2.0, diameter / 2.0);
        const QFont reduced = stackFont(font);
        const QFontMetricsF reducedMetrics(reduced);
        painter.setFont(reduced);
        const double letter = reducedMetrics.horizontalAdvance(shape.modifier);
        lettering(painter, QPointF(centre.x() - letter / 2.0, centre.y() + kStackShare * unit / 2.0),
                  shape.modifier, ink, halo, scale);
        painter.setFont(font);
      }
      x += box.cells[i];
    }
    painter.restore();
    return;
  }
  const double pad = kPadShare * unit;
  if (shape.note) {
    double y = box.baseline;
    // Flush left on either side of the leader (mitcad#902).
    for (const QString& line : shape.noteLines) {
      lettering(painter, QPointF(box.rect.left() + pad, y), line, ink, halo, scale);
      y += box.lineHeight;
    }
    painter.restore();
    return;
  }
  const QFontMetricsF metrics(font);
  double x = box.rect.left() + pad;
  const QString main = shape.main.isEmpty() && shape.upper.isEmpty() ? shape.label : shape.main;
  lettering(painter, QPointF(x, box.baseline), main, ink, halo, scale);
  x += box.mainWidth;
  if (!shape.upper.isEmpty() || !shape.lower.isEmpty()) {
    if (box.mainWidth > 0.0) {
      x += metrics.horizontalAdvance(QLatin1Char(' '));
    }
    if (shape.brackets) {
      lettering(painter, QPointF(x, box.baseline), QStringLiteral("("), ink, halo, scale);
      x += box.bracket;
    }
    painter.setFont(stackFont(font));
    // The upper deviation above the lower, both beside the value.
    lettering(painter, QPointF(x, box.upperBaseline), shape.upper, ink, halo, scale);
    lettering(painter, QPointF(x, box.lowerBaseline), shape.lower, ink, halo, scale);
    painter.setFont(font);
    x += box.stackWidth;
    if (shape.brackets) {
      lettering(painter, QPointF(x, box.baseline), QStringLiteral(")"), ink, halo, scale);
      x += box.bracket;
    }
  }
  if (!shape.after.isEmpty()) {
    lettering(painter, QPointF(x, box.baseline), QLatin1Char(' ') + shape.after, ink, halo, scale);
  }
  painter.restore();
}

// A number with up to three decimals, without trailing zeros.
QString trimmed(double value) {
  QString text = QString::number(value, 'f', 3);
  while (text.contains(QLatin1Char('.')) && (text.endsWith(QLatin1Char('0')) || text.endsWith(QLatin1Char('.')))) {
    text.chop(1);
  }
  return text == QStringLiteral("-0") ? QStringLiteral("0") : text;
}

double distanceToPolyline(const QPolygonF& line, const QPointF& at) {
  double best = std::numeric_limits<double>::infinity();
  for (qsizetype i = 0; i + 1 < line.size(); ++i) {
    const QPointF a = line[i];
    const QPointF b = line[i + 1];
    const QPointF ab = b - a;
    const double length = dotOf(ab, ab);
    const double t = length > 1e-12 ? std::clamp(dotOf(at - a, ab) / length, 0.0, 1.0) : 0.0;
    best = std::min(best, lengthOf(at - (a + ab * t)));
  }
  return best;
}

} // namespace

double lengthUnitMillimetres(const QString& unit) {
  if (unit == QLatin1String("cm")) {
    return 10.0;
  }
  if (unit == QLatin1String("m")) {
    return 1000.0;
  }
  if (unit == QLatin1String("in")) {
    return 25.4;
  }
  if (unit == QLatin1String("ft")) {
    return 304.8;
  }
  return 1.0;
}

QString measurementValueText(const QString& kind, double value, const QString& unit, double unitMillimetres) {
  if (kind == QLatin1String("angle")) {
    return trimmed(value * 180.0 / kPi) + QStringLiteral("°");
  }
  const double per = unitMillimetres > 0.0 ? unitMillimetres : 1.0;
  return QStringLiteral("%1 %2").arg(trimmed(value / per), unit.isEmpty() ? QStringLiteral("mm") : unit);
}

QIcon measurementSwatch(const QColor& color) {
  QPixmap pixmap(16, 16);
  pixmap.fill(Qt::transparent);
  QPainter painter(&pixmap);
  painter.setRenderHint(QPainter::Antialiasing);
  // A dimension: a line with an arrowhead at each end, in the colour (Auto:
  // a neutral grey that reads in light and dark lists).
  const QColor ink = color.isValid() ? color : QColor(0x86, 0x8e, 0x96);
  painter.setPen(QPen(ink, 1.0));
  painter.drawLine(QPointF(2.5, 8.5), QPointF(13.5, 8.5));
  painter.drawLine(QPointF(1.5, 4.0), QPointF(1.5, 14.0));
  painter.drawLine(QPointF(14.5, 4.0), QPointF(14.5, 14.0));
  painter.setPen(Qt::NoPen);
  painter.setBrush(ink);
  painter.drawPolygon(measurementArrowHead(QPointF(2.0, 8.5), QPointF(-1.0, 0.0), 6.0, 0.5));
  painter.drawPolygon(measurementArrowHead(QPointF(14.0, 8.5), QPointF(1.0, 0.0), 6.0, 0.5));
  painter.end();
  return QIcon(pixmap);
}

QString measurementFontFamily() {
  static const QString family = [] {
    const QDir executable(QCoreApplication::applicationDirPath());
    // Beside the executable (a build tree, the Windows installer), in an
    // installed tree's share folder (the AppImage), in a macOS bundle's
    // resources.
    const QStringList folders{executable.filePath(QStringLiteral("fonts")),
                              executable.filePath(QStringLiteral("../share/mitcad/fonts")),
                              executable.filePath(QStringLiteral("../Resources/fonts"))};
    for (const QString& folder : folders) {
      const QString path = QDir::cleanPath(QDir(folder).filePath(QLatin1String(kLetteringFile)));
      if (!QFileInfo::exists(path)) {
        continue;
      }
      const int id = QFontDatabase::addApplicationFont(path);
      const QStringList families = id >= 0 ? QFontDatabase::applicationFontFamilies(id) : QStringList();
      if (!families.isEmpty()) {
        qDebug().noquote() << QStringLiteral("Annotation lettering: %1 from %2").arg(families.first(), path);
        return families.first();
      }
    }
    qWarning().noquote() << QStringLiteral("Annotation lettering: %1 not found beside %2; the UI's font instead")
                                .arg(QLatin1String(kLetteringFile), executable.path());
    return QString();
  }();
  return family;
}

double measurementTextFloor(double scale) { return kTextFloor * scale; }

double measurementTextCeiling(double scale) { return kTextCeiling * scale; }

QPointF measurementSnapped(const QPointF& point, double width) {
  // An odd number of pixels wide: centred on a pixel; an even one: on the
  // corner between pixels.
  const long whole = std::max(1L, std::lround(width));
  const double offset = whole % 2 == 1 ? 0.5 : 0.0;
  return QPointF(std::round(point.x() - offset) + offset, std::round(point.y() - offset) + offset);
}

QLineF measurementSnapped(const QLineF& line, double width) {
  return QLineF(measurementSnapped(line.p1(), width), measurementSnapped(line.p2(), width));
}

double measurementLineWidth(double scale) { return std::max(1.0, std::round(kLineWidth * scale)); }

QColor measurementSelectionColor() { return QColor(0x1f, 0x7a, 0xff); }

QColor measurementHoverColor() { return QColor(0x2f, 0x9b, 0xff); }

QColor measurementPictureTone(const QImage& image) {
  if (image.isNull() || image.width() < 1 || image.height() < 1) {
    return QColor();
  }
  double red = 0.0;
  double green = 0.0;
  double blue = 0.0;
  int count = 0;
  const auto take = [&image, &red, &green, &blue, &count](int x, int y) {
    const QColor at = image.pixelColor(x, y);
    if (at.alpha() < 128) {
      return;
    }
    red += at.redF();
    green += at.greenF();
    blue += at.blueF();
    ++count;
  };
  // Its border, every few pixels.
  const int stepX = std::max(1, image.width() / 64);
  const int stepY = std::max(1, image.height() / 64);
  for (int x = 0; x < image.width(); x += stepX) {
    take(x, 0);
    take(x, image.height() - 1);
  }
  for (int y = 0; y < image.height(); y += stepY) {
    take(0, y);
    take(image.width() - 1, y);
  }
  if (count == 0) {
    return QColor();
  }
  return QColor::fromRgbF(static_cast<float>(red / count), static_cast<float>(green / count),
                          static_cast<float>(blue / count));
}

QColor measurementAutoColor(const QColor& halo) {
  // Near-black on a light background, a light grey on a dark one
  // (mitcad#902: not chalk-white); over what lies behind, its lines piece
  // by piece and its text in one ink (measurementAutoInk).
  return halo.lightnessF() >= 0.5 ? kDarkInk : kLightInk;
}

QColor measurementAutoInk(const MeasurementBackdrop* backdrop, const QPointF& at, const QColor& halo) {
  if (backdrop == nullptr || backdrop->image.isNull()) {
    return measurementAutoColor(halo);
  }
  const QPoint pixel = (at * backdrop->pixels).toPoint();
  if (!backdrop->image.rect().contains(pixel)) {
    return measurementAutoColor(halo);
  }
  return qGray(backdrop->image.pixel(pixel)) >= kLightBehind ? kDarkInk : kLightInk;
}

QColor measurementAutoInk(const MeasurementBackdrop* backdrop, const MeasurementShape& shape, const QColor& halo) {
  if (backdrop == nullptr || backdrop->image.isNull()) {
    return measurementAutoColor(halo);
  }
  // A vote of points over its text (and frames); without one, along its
  // lines and arcs (one each kInkPiece pixels) and over its arrowheads.
  int light = 0;
  int seen = 0;
  const auto vote = [&](const QPointF& at) {
    const QPoint pixel = (at * backdrop->pixels).toPoint();
    if (!backdrop->image.rect().contains(pixel)) {
      return;
    }
    ++seen;
    light += qGray(backdrop->image.pixel(pixel)) >= kLightBehind ? 1 : 0;
  };
  const auto along = [&vote](const QLineF& line) {
    const int pieces = std::max(1, static_cast<int>(std::ceil(line.length() / kInkPiece)));
    for (int k = 0; k < pieces; ++k) {
      vote(line.pointAt((k + 0.5) / pieces));
    }
  };
  if (shape.textArea.isEmpty()) {
    for (const QLineF& line : shape.lines) {
      along(line);
    }
    for (const QPolygonF& arc : shape.arcs) {
      for (qsizetype k = 0; k + 1 < arc.size(); ++k) {
        along(QLineF(arc[k], arc[k + 1]));
      }
    }
    for (const QPolygonF& head : shape.heads) {
      if (!head.isEmpty()) {
        vote(head.boundingRect().center());
      }
    }
  } else {
    const QRectF bounds = shape.textArea.boundingRect();
    const int across = std::max(1, static_cast<int>(std::ceil(bounds.width() / kInkPiece)));
    const int down = std::max(1, static_cast<int>(std::ceil(bounds.height() / kInkPiece)));
    for (int i = 0; i < across; ++i) {
      for (int j = 0; j < down; ++j) {
        const QPointF at(bounds.left() + bounds.width() * (i + 0.5) / across,
                         bounds.top() + bounds.height() * (j + 0.5) / down);
        if (shape.textArea.containsPoint(at, Qt::OddEvenFill)) {
          vote(at);
        }
      }
    }
  }
  if (seen == 0) {
    return measurementAutoColor(halo);
  }
  return 2 * light >= seen ? kDarkInk : kLightInk;
}

QPolygonF measurementArrowHead(const QPointF& tip, const QPointF& along, double textPixels, double scale) {
  const QPointF dir = unitOf(along);
  const QPointF side(-dir.y(), dir.x());
  const double length = arrowLength(textPixels, scale);
  const double half = length * kArrowTan;
  QPolygonF head;
  head << tip << tip - dir * length + side * half << tip - dir * length - side * half;
  return head;
}

std::vector<MeasurementDisplay> measurementDisplays(const QJsonArray& measurements,
                                                    const MeasurementValueText& valueText) {
  std::vector<MeasurementDisplay> shown;
  for (const QJsonValue& entry : measurements) {
    const QJsonObject m = entry.toObject();
    const QJsonObject drawing = m.value(QStringLiteral("drawing")).toObject();
    const QString kindOf = m.value(QStringLiteral("kind")).toString();
    // A leader (a note, datum label, feature control frame, hole callout)
    // has no value.
    const bool leader = kindOf == QLatin1String("note") || kindOf == QLatin1String("datum") ||
                        kindOf == QLatin1String("frame") || kindOf == QLatin1String("hole_callout");
    if (!m.value(QStringLiteral("visible")).toBool(true) || m.contains(QStringLiteral("error")) ||
        (!m.value(QStringLiteral("value")).isDouble() && !(leader && !drawing.isEmpty()))) {
      continue;
    }
    MeasurementDisplay d;
    d.name = m.value(QStringLiteral("name")).toString();
    d.kind = m.value(QStringLiteral("kind")).toString();
    d.group = m.value(QStringLiteral("group")).toString();
    // Auto (mitcad#894), or none known: the annotation colour of the
    // background tone, chosen when painted.
    const QString colour = m.value(QStringLiteral("color")).toString();
    d.color = QColor(colour);
    d.autoColor = colour == QLatin1String("auto") || !d.color.isValid();
    if (d.autoColor) {
      d.color = QColor();
    }
    const double value = m.value(QStringLiteral("value")).toDouble();
    const QJsonObject label = m.value(QStringLiteral("label")).toObject();
    if (label.contains(QStringLiteral("text"))) {
      d.label = label.value(QStringLiteral("text")).toString();
      d.main = label.value(QStringLiteral("main")).toString();
      d.upper = label.value(QStringLiteral("upper")).toString();
      d.lower = label.value(QStringLiteral("lower")).toString();
      d.brackets = label.value(QStringLiteral("brackets")).toBool();
      d.after = label.value(QStringLiteral("after")).toString();
      for (const QJsonValue& line : label.value(QStringLiteral("lines")).toArray()) {
        d.lines << line.toString();
      }
      for (const QJsonValue& cell : label.value(QStringLiteral("frame")).toArray()) {
        d.frame << cell.toString();
      }
      d.datum = label.value(QStringLiteral("datum")).toString();
      const QJsonObject frameOf = m.value(QStringLiteral("frame")).toObject();
      d.characteristic = frameOf.value(QStringLiteral("characteristic")).toString();
      d.modifier = frameOf.value(QStringLiteral("modifier")).toString();
    } else {
      d.label = QStringLiteral("%1 = %2").arg(d.name, valueText ? valueText(d.kind, value) : QString::number(value));
    }
    const QJsonArray points = m.value(QStringLiteral("points")).toArray();
    if (points.size() == 2 && isPoint(points.at(0)) && isPoint(points.at(1))) {
      d.points = {pointIn(points.at(0)), pointIn(points.at(1))};
    }
    if (isPoint(m.value(QStringLiteral("center"))) && m.value(QStringLiteral("radius")).isDouble()) {
      d.circle = true;
      d.center = pointIn(m.value(QStringLiteral("center")));
      d.axis = vectorIn(m.value(QStringLiteral("axis")));
      d.radius = m.value(QStringLiteral("radius")).toDouble();
    }
    const QJsonObject arc = m.value(QStringLiteral("arc")).toObject();
    if (isPoint(arc.value(QStringLiteral("center")))) {
      d.arc = true;
      d.corner = pointIn(arc.value(QStringLiteral("center")));
      d.from = vectorIn(arc.value(QStringLiteral("from")));
      d.to = vectorIn(arc.value(QStringLiteral("to")));
    }
    // The model's drawing in the annotation's plane (mitcad#702).
    const QJsonObject text = drawing.value(QStringLiteral("text")).toObject();
    if (isPoint(text.value(QStringLiteral("at")))) {
      d.planar = true;
      const QJsonObject plane = drawing.value(QStringLiteral("plane")).toObject();
      d.planeX = vectorIn(plane.value(QStringLiteral("x")));
      d.planeY = vectorIn(plane.value(QStringLiteral("y")));
      d.planeNormal = vectorIn(plane.value(QStringLiteral("normal")));
      const auto segments = [&drawing](const char* key) {
        std::vector<MeasurementDisplay::Segment> out;
        for (const QJsonValue& entryOf : drawing.value(QLatin1String(key)).toArray()) {
          const QJsonArray pair = entryOf.toArray();
          if (pair.size() == 2 && isPoint(pair.at(0)) && isPoint(pair.at(1))) {
            out.push_back({pointIn(pair.at(0)), pointIn(pair.at(1))});
          }
        }
        return out;
      };
      d.extensionLines = segments("extension_lines");
      d.dimensionLines = segments("dimension_lines");
      for (const QJsonValue& arcEntry : drawing.value(QStringLiteral("arcs")).toArray()) {
        const QJsonObject a = arcEntry.toObject();
        d.arcs.push_back({pointIn(a.value(QStringLiteral("center"))), a.value(QStringLiteral("radius")).toDouble(),
                          vectorIn(a.value(QStringLiteral("from"))), vectorIn(a.value(QStringLiteral("to"))),
                          vectorIn(a.value(QStringLiteral("normal")))});
      }
      for (const QJsonValue& arrowEntry : drawing.value(QStringLiteral("arrows")).toArray()) {
        const QJsonObject a = arrowEntry.toObject();
        d.arrows.push_back({pointIn(a.value(QStringLiteral("tip"))), vectorIn(a.value(QStringLiteral("direction")))});
      }
      if (isPoint(drawing.value(QStringLiteral("center")))) {
        d.centerMark = pointIn(drawing.value(QStringLiteral("center")));
      }
      d.textAt = pointIn(text.value(QStringLiteral("at")));
      d.textDirection = vectorIn(text.value(QStringLiteral("direction")));
      d.textUp = vectorIn(text.value(QStringLiteral("up")));
      d.lineOffset = drawing.value(QStringLiteral("line_offset")).toDouble();
      const QJsonArray offset = drawing.value(QStringLiteral("text_offset")).toArray();
      d.textOffset = {offset.at(0).toDouble(), offset.at(1).toDouble()};
      const QString terminator = drawing.value(QStringLiteral("terminator")).toString();
      d.datumTerminator = terminator == QLatin1String("datum");
      d.dotTerminator = terminator == QLatin1String("dot");
      d.align = drawing.value(QStringLiteral("align")).toString();
      d.attached = drawing.value(QStringLiteral("attached")).toString();
      d.underLines = drawing.value(QStringLiteral("under_lines")).toInt();
      d.textHeight = drawing.value(QStringLiteral("text_height")).toDouble();
    }
    shown.push_back(d);
  }
  // A frame under a leader's text and that text share their left edge
  // (mitcad#902): each knows the other's words.
  for (MeasurementDisplay& frame : shown) {
    if (frame.attached.isEmpty() || frame.underLines <= 0) {
      continue;
    }
    for (MeasurementDisplay& over : shown) {
      if (over.name != frame.attached) {
        continue;
      }
      frame.overLines = over.lines.isEmpty() ? QStringList{over.label} : over.lines;
      over.framesUnder.push_back({frame.frame, frame.characteristic, frame.modifier});
    }
  }
  return shown;
}

MeasurementShape layoutMeasurement(const MeasurementDisplay& measurement, const MeasurementProjection& project,
                                   double scale) {
  MeasurementShape shape;
  shape.name = measurement.name;
  shape.color = measurement.color;
  shape.autoColor = measurement.autoColor;
  shape.label = measurement.label;
  // A screen-facing label's text the floor; one in its plane takes the
  // model's height.
  shape.textPixels = measurementTextFloor(scale);
  if (!project) {
    return shape;
  }
  if (measurement.planar) {
    layoutPlanar(measurement, project, scale, shape);
    return shape;
  }
  if (measurement.kind == QLatin1String("angle")) {
    layoutAngle(measurement, project, shape);
  } else if (measurement.kind == QLatin1String("radius") || measurement.kind == QLatin1String("diameter")) {
    layoutCircle(measurement, project, scale, shape);
  } else {
    layoutLength(measurement, project, scale, shape);
  }
  return shape;
}

QFont measurementFont(const QFont& base, double scale) { return letteringFont(base, measurementTextFloor(scale)); }

QFont measurementTextFont(const QFont& base, const MeasurementShape& shape, double scale) {
  return letteringFont(base, textPixelsOf(shape, scale));
}

QRectF measurementLabelRect(const MeasurementShape& shape, const QFont& font, double scale) {
  if (shape.planar) {
    return labelBox(shape, measurementTextFont(font, shape, scale), scale).rect;
  }
  const QFontMetricsF metrics(font);
  const double pad = kPadShare * textPixelsOf(shape, scale);
  const double w = metrics.horizontalAdvance(shape.label) + 2.0 * pad;
  const double h = metrics.height() + pad;
  return QRectF(shape.labelAt.x() - w / 2.0, shape.labelAt.y() - h / 2.0, w, h);
}

void paintMeasurements(QPainter& painter, const std::vector<MeasurementShape>& shapes, const QColor& halo,
                       const QFont& font, double scale, const MeasurementBackdrop* backdrop) {
  painter.save();
  painter.setRenderHint(QPainter::Antialiasing);
  painter.setFont(font);
  const double thin = measurementLineWidth(scale);
  for (const MeasurementShape& shape : shapes) {
    if (!shape.valid || !(shape.opacity > 0.0)) {
      continue;
    }
    painter.setOpacity(shape.opacity);
    // A selected or hovered one: the same thin drawing in the selection's
    // or the hover's colour (mitcad#902: no band, no box).
    QColor ink = shape.autoColor || !shape.color.isValid() ? measurementAutoColor(halo) : shape.color;
    double width = thin;
    if (shape.highlighted) {
      ink = measurementSelectionColor();
      width = std::min(kSelectedWidth * scale, thin + 0.5 * scale);
    } else if (shape.hovered) {
      ink = measurementHoverColor();
      width = std::min(kSelectedWidth * scale, thin + 0.5 * scale);
    }
    // Auto (mitcad#902): its lines and terminators piece by piece in the
    // ink that reads over what lies behind them, so that a leader stays
    // readable over a body; its text and frames in one ink, by what lies
    // behind most of them. Single plain lines.
    const bool chosen = !shape.highlighted && !shape.hovered && (shape.autoColor || !shape.color.isValid()) &&
                        backdrop != nullptr && !backdrop->image.isNull();
    const QColor textInk = chosen ? measurementAutoInk(backdrop, shape, halo) : ink;
    // Thin lines (ISO 128), crisp: on the pixel grid, flat at their ends.
    QPen pen(ink, width, Qt::SolidLine, Qt::FlatCap, Qt::MiterJoin);
    painter.setPen(pen);
    painter.setBrush(Qt::NoBrush);
    const auto stroke = [&painter, &pen, chosen, backdrop, &halo](const QLineF& line) {
      if (!chosen) {
        painter.setPen(pen);
        painter.drawLine(line);
        return;
      }
      // Runs of pieces of one ink, each run one line.
      const int pieces = std::max(1, static_cast<int>(std::ceil(line.length() / kInkPiece)));
      int from = 0;
      QColor run = measurementAutoInk(backdrop, line.pointAt(0.5 / pieces), halo);
      for (int k = 1; k <= pieces; ++k) {
        const QColor next = k < pieces ? measurementAutoInk(backdrop, line.pointAt((k + 0.5) / pieces), halo) : QColor();
        if (k < pieces && next == run) {
          continue;
        }
        QPen piece = pen;
        piece.setColor(run);
        painter.setPen(piece);
        painter.drawLine(QLineF(line.pointAt(static_cast<double>(from) / pieces),
                                line.pointAt(static_cast<double>(k) / pieces)));
        from = k;
        run = next;
      }
    };
    for (const QLineF& line : shape.lines) {
      stroke(measurementSnapped(line, width));
    }
    for (const QPolygonF& arc : shape.arcs) {
      for (qsizetype k = 0; k + 1 < arc.size(); ++k) {
        stroke(QLineF(arc[k], arc[k + 1]));
      }
    }
    // Closed, filled arrowheads (a datum label's triangle, a dot), in the
    // annotation's plane, without an outline.
    painter.setPen(Qt::NoPen);
    for (std::size_t i = 0; i < shape.arrows.size(); ++i) {
      const QPolygonF head = terminatorOf(shape, i, scale);
      painter.setBrush(chosen ? measurementAutoInk(backdrop, head.boundingRect().center(), halo) : ink);
      painter.drawPolygon(head);
    }
    if (shape.planar) {
      paintPlanarLabel(painter, shape, measurementTextFont(font, shape, scale), scale, textInk, halo, width);
      continue;
    }
    // Screen-facing: the label on its own in the middle of its place.
    const QRectF rect = measurementLabelRect(shape, font, scale);
    const QFontMetricsF metrics(font);
    const QPointF start(rect.center().x() - metrics.horizontalAdvance(shape.label) / 2.0,
                        rect.center().y() + (metrics.ascent() - metrics.descent()) / 2.0);
    lettering(painter, start, shape.label, ink, halo, scale);
  }
  painter.restore();
}

std::optional<std::array<double, 2>> measurementDragMillimetres(const MeasurementShape& shape, const QPointF& delta) {
  // delta = a x + b y with x and y the plane's axes per millimetre.
  const QPointF x = shape.xPerMillimetre;
  const QPointF y = shape.yPerMillimetre;
  const double det = x.x() * y.y() - x.y() * y.x();
  // A frame under a dimension stands where that dimension's text is.
  if (!shape.planar || shape.attached || !(std::abs(det) > 1e-12) || !std::isfinite(det)) {
    return std::nullopt;
  }
  const double a = (delta.x() * y.y() - delta.y() * y.x()) / det;
  const double b = (x.x() * delta.y() - x.y() * delta.x()) / det;
  return std::array<double, 2>{a, b};
}

MeasurementShape draggedMeasurement(const MeasurementShape& shape, bool line, const QPointF& delta) {
  MeasurementShape moved = shape;
  if (shape.attached) {
    return moved;
  }
  if (!line) {
    moved.textTransform = shape.textTransform * QTransform::fromTranslate(delta.x(), delta.y());
    moved.textArea = shape.textArea.translated(delta);
    moved.labelAt = shape.labelAt + delta;
    return moved;
  }
  // Across the line only: the dimension line, its arrows and the text move,
  // the extension lines' far ends with them.
  const QPointF shift = acrossLine(shape, delta);
  for (QPolygonF& grip : moved.grips) {
    grip.translate(shift);
  }
  for (QPolygonF& arc : moved.arcs) {
    arc.translate(shift);
  }
  for (auto& arrow : moved.arrows) {
    arrow.first += shift;
  }
  for (QPolygonF& head : moved.heads) {
    head.translate(shift);
  }
  for (QLineF& drawnLine : moved.lines) {
    // Lines that touch the dimension line follow at that end.
    for (const QPolygonF& grip : shape.grips) {
      if (distanceToPolyline(grip, drawnLine.p2()) < 1.0) {
        drawnLine.setP2(drawnLine.p2() + shift);
      }
      if (distanceToPolyline(grip, drawnLine.p1()) < 1.0) {
        drawnLine.setP1(drawnLine.p1() + shift);
      }
    }
  }
  moved.textTransform = shape.textTransform * QTransform::fromTranslate(shift.x(), shift.y());
  moved.textArea = shape.textArea.translated(shift);
  moved.labelAt = shape.labelAt + shift;
  return moved;
}

int measurementHit(const MeasurementShape& shape, const QPointF& at, double reach) {
  // Nothing of an annotation seen nearly edge on takes clicks.
  if (!shape.valid || !shape.planar || shape.opacity < 0.5) {
    return 0;
  }
  if (shape.textArea.containsPoint(at, Qt::OddEvenFill)) {
    return 1;
  }
  for (const QPolygonF& grip : shape.grips) {
    if (distanceToPolyline(grip, at) <= reach) {
      return 2;
    }
  }
  return 0;
}

MeasurementProjection renderViewProjection(const QJsonObject& view) {
  const gp_Pnt eye = pointIn(view.value(QStringLiteral("eye")));
  const gp_Pnt target = pointIn(view.value(QStringLiteral("target")));
  const gp_Vec up = vectorIn(view.value(QStringLiteral("up")));
  const double half = view.value(QStringLiteral("half_height")).toDouble();
  const QJsonArray size = view.value(QStringLiteral("size")).toArray();
  const double width = size.at(0).toDouble();
  const double height = size.at(1).toDouble();
  const bool perspective = view.value(QStringLiteral("projection")).toString() == QLatin1String("perspective");
  gp_Vec forward(eye, target);
  const double distance = forward.Magnitude();
  if (!(distance > 1e-12) || !(half > 0.0) || !(height > 0.0)) {
    return [](const gp_Pnt&) -> std::optional<QPointF> { return std::nullopt; };
  }
  forward /= distance;
  gp_Vec right = forward.Crossed(up);
  if (right.Magnitude() < 1e-12) {
    return [](const gp_Pnt&) -> std::optional<QPointF> { return std::nullopt; };
  }
  right.Normalize();
  const gp_Vec trueUp = right.Crossed(forward);
  const double pixels = height / (2.0 * half); // per mm at the target
  return [=](const gp_Pnt& point) -> std::optional<QPointF> {
    const gp_Vec d(eye, point);
    double x = d.Dot(right);
    double y = d.Dot(trueUp);
    if (perspective) {
      const double depth = d.Dot(forward);
      if (depth <= 1e-9) {
        return std::nullopt;
      }
      x *= distance / depth;
      y *= distance / depth;
    }
    return QPointF(width / 2.0 + x * pixels, height / 2.0 - y * pixels);
  };
}

} // namespace mitcad
