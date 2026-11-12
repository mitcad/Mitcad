// SPDX-License-Identifier: MIT
#include "DrawingSheetView.hpp"
#include "framework/ModelMessages.hpp"

#include <algorithm>
#include <cmath>
#include <limits>
#include <utility>

#include <QContextMenuEvent>
#include <QEnterEvent>
#include <QHideEvent>
#include <QShowEvent>
#include <QFontMetrics>
#include <QFontMetricsF>
#include <QGuiApplication>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <QPolygonF>
#include <QSvgRenderer>
#include <QWheelEvent>
#include <QtLogging>

#include "../framework/Theme.hpp"
#include "TitleBlockEditor.hpp"

namespace mitcad::drawings {
namespace {

// The paper's grey under the view (a dark one on a dark theme, mitcad#1008:
// a light grey made the sheet a window in the dark window), the
// selection's blue.
const QColor kBackground(0xd9, 0xdc, 0xe0);
const QColor kDarkBackground(0x2c, 0x2f, 0x34);
// Selection and hover as in the 3D view (mitcad#1041: one role each).
const QColor kSelected = themeColor(ColorRole::Select, false);
const QColor kHovered = themeColor(ColorRole::Hover, false);
// A broken annotation is a warning (mitcad#1040): the warning colour on the
// sheet's paper, with the warning sign.
const QColor kBroken = themeColor(ColorRole::WarningText, false);
// Helvetica's cap height per em (core/model-drawing/src/sheet.rs).
constexpr double kCapHeight = 0.718;
constexpr double kPi = 3.14159265358979323846;
// A right press and release further apart than this (widget px) is a
// drag, not a click: the 3D view's and the sketch's rule (OcctViewer,
// SketchController; mitcad#1085, mitcad#1404).
constexpr double kRightClickPixels = 4.0;
// How opaque the hover's and the picks' marks are (of 255): the line they
// mark shows under them (mitcad#1618).
constexpr int kMarkAlpha = 160;

QPointF pointOf(const QJsonValue& value) {
  const QJsonArray a = value.toArray();
  return {a.at(0).toDouble(), a.at(1).toDouble()};
}

QPolygonF polygonOf(const QJsonValue& value) {
  QPolygonF out;
  for (const QJsonValue& p : value.toArray()) {
    out.append(pointOf(p));
  }
  return out;
}

// A pen by its name from the table of widths (mm) and dashes (mm) the
// model's `drawing` query gives with the groups (`pens`, mitcad#1112: the
// writers' own table, ISO 128's 0.35 series); a narrow continuous line
// for a pen it does not list.
QPen penOf(const QJsonObject& pens, const QString& name) {
  const QJsonObject entry = pens.value(name).toObject();
  const double width = entry.value(QStringLiteral("width")).toDouble(0.18);
  QVector<qreal> dashes;
  for (const QJsonValue& d : entry.value(QStringLiteral("dashes")).toArray()) {
    dashes.append(d.toDouble());
  }
  QPen pen(Qt::black, width, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin);
  if (!dashes.isEmpty() && width > 0.0) {
    // In pen widths.
    for (qreal& d : dashes) {
      d /= width;
    }
    pen.setDashPattern(dashes);
  }
  return pen;
}

double distance(const QPointF& a, const QPointF& b) { return std::hypot(a.x() - b.x(), a.y() - b.y()); }

// A dimension's snap (mitcad#1173): its hint ("12 mm × 2") on a light box
// beside where it goes (`at`, widget px).
void paintSnapHint(QPainter& painter, const QPointF& at, const QString& hint) {
  if (hint.isEmpty()) {
    return;
  }
  painter.save();
  const QFontMetricsF metrics(painter.font());
  const QRectF box(at.x() + 10.0, at.y() - 10.0 - metrics.height(), metrics.horizontalAdvance(hint) + 8.0,
                   metrics.height() + 4.0);
  // The light chrome surface, a little see-through, under the selection's
  // blue (a UI color, not the paper's ink).
  QColor fill = themeColor(ColorRole::Surface, false);
  fill.setAlpha(220);
  painter.setPen(Qt::NoPen);
  painter.setBrush(fill);
  painter.drawRoundedRect(box, 3.0, 3.0);
  painter.setPen(kSelected);
  painter.drawText(box, Qt::AlignCenter, hint);
  painter.restore();
}

// A key pressed without Ctrl, Alt, Shift or Meta (the keypad's flag aside).
bool unmodified(const QKeyEvent* key) {
  return !(key->modifiers() & (Qt::ShiftModifier | Qt::ControlModifier | Qt::AltModifier | Qt::MetaModifier));
}

// How far a point is from a segment.
double segmentDistance(const QPointF& p, const QLineF& line) {
  const QPointF d = line.p2() - line.p1();
  const double length = d.x() * d.x() + d.y() * d.y();
  double t = 0.0;
  if (length > 1e-24) {
    const QPointF w = p - line.p1();
    t = std::clamp((w.x() * d.x() + w.y() * d.y()) / length, 0.0, 1.0);
  }
  return distance(p, line.p1() + d * t);
}

// The middle of a datum's triangle or a frame's arrow: its corners'
// average (mitcad#1295).
QPointF tipCenter(const QPolygonF& tip) {
  QPointF sum;
  for (const QPointF& p : tip) {
    sum += p;
  }
  return tip.isEmpty() ? sum : sum / static_cast<double>(tip.size());
}

// Whether a sheet point is on a datum's triangle or a frame's arrow, or
// within `reach` (sheet mm) of its outline.
bool onTip(const QPolygonF& tip, const QPointF& at, double reach) {
  if (tip.size() < 3) {
    return false;
  }
  if (tip.containsPoint(at, Qt::OddEvenFill)) {
    return true;
  }
  for (qsizetype i = 0; i < tip.size(); ++i) {
    if (segmentDistance(at, QLineF(tip[i], tip[(i + 1) % tip.size()])) <= reach) {
      return true;
    }
  }
  return false;
}

// A text item's area on the sheet: from its baseline up to its height,
// as wide as Helvetica writes it at that height (as it is painted).
QPolygonF textArea(const QJsonObject& item) {
  const QString text = item.value(QStringLiteral("text")).toString();
  const double height = item.value(QStringLiteral("height")).toDouble();
  const double angle = item.value(QStringLiteral("angle")).toDouble();
  const QString anchor = item.value(QStringLiteral("anchor")).toString();
  QFont face(QStringLiteral("Helvetica"));
  face.setStyleHint(QFont::SansSerif);
  face.setPixelSize(100);
  const double width = QFontMetricsF(face).horizontalAdvance(text) / 100.0 * height / kCapHeight;
  double start = 0.0;
  if (anchor == QStringLiteral("middle")) {
    start = -width / 2.0;
  } else if (anchor == QStringLiteral("end")) {
    start = -width;
  }
  const QPointF at = pointOf(item.value(QStringLiteral("at")));
  const QPointF along(std::cos(angle), std::sin(angle));
  const QPointF up(-std::sin(angle), std::cos(angle));
  return QPolygonF({at + along * start, at + along * (start + width), at + along * (start + width) + up * height,
                    at + along * start + up * height});
}

} // namespace

DrawingSheetView::DrawingSheetView(QWidget* parent) : QWidget(parent) {
  setObjectName(QStringLiteral("drawingSheet"));
  setMouseTracking(true);
  setFocusPolicy(Qt::StrongFocus);
  setAutoFillBackground(false);
}

void DrawingSheetView::setTitleBlockEditor(TitleBlockEditor* editor) {
  if (editor == m_blockEditor) {
    return;
  }
  m_blockEditor = editor;
  m_drag = Drag::None;
  // The model's title block hidden under the editor's, or shown again.
  sheetMoved();
}

void DrawingSheetView::setSheet(const QJsonObject& sheet) {
  const double w = sheet.value(QStringLiteral("width")).toDouble(297.0);
  const double h = sheet.value(QStringLiteral("height")).toDouble(210.0);
  const bool resized = w != m_width || h != m_height;
  m_width = w;
  m_height = h;
  m_views.clear();
  for (const QJsonValue& value : sheet.value(QStringLiteral("views")).toArray()) {
    const QJsonObject v = value.toObject();
    SheetView entry;
    entry.id = v.value(QStringLiteral("id")).toInt();
    entry.type = v.value(QStringLiteral("type")).toString();
    entry.parent = v.value(QStringLiteral("parent")).toInt();
    entry.label = v.value(QStringLiteral("label")).toString();
    entry.error = mitcad::modelMessage(v.value(QStringLiteral("error")).toString());
    const QJsonArray box = v.value(QStringLiteral("box")).toArray();
    if (box.size() == 2) {
      entry.box = QRectF(pointOf(box.at(0)), pointOf(box.at(1))).normalized();
    }
    entry.json = v;
    m_views.append(entry);
  }
  m_annotations.clear();
  for (const QJsonValue& value : sheet.value(QStringLiteral("annotations")).toArray()) {
    const QJsonObject a = value.toObject();
    SheetAnnotation entry;
    entry.id = a.value(QStringLiteral("id")).toInt();
    entry.view = a.value(QStringLiteral("view")).toInt();
    entry.type = a.value(QStringLiteral("type")).toString();
    entry.label = a.value(QStringLiteral("label")).toString();
    entry.broken = a.value(QStringLiteral("broken")).toBool();
    entry.error = mitcad::modelMessage(a.value(QStringLiteral("error")).toString());
    const QJsonArray box = a.value(QStringLiteral("box")).toArray();
    if (box.size() == 2) {
      entry.box = QRectF(pointOf(box.at(0)), pointOf(box.at(1))).normalized();
    }
    entry.json = a;
    m_annotations.append(entry);
  }
  m_groups = sheet.value(QStringLiteral("groups")).toArray();
  m_pens = sheet.value(QStringLiteral("pens")).toObject();
  // Each annotation's lines and texts, from its group (`A<id>`).
  QVector<int> hasText;
  for (const QJsonValue& groupValue : std::as_const(m_groups)) {
    const QJsonObject group = groupValue.toObject();
    const QString groupId = group.value(QStringLiteral("id")).toString();
    bool numbered = false;
    const int owner = groupId.startsWith(QLatin1Char('A')) ? groupId.mid(1).toInt(&numbered) : 0;
    SheetAnnotation* entry = nullptr;
    for (SheetAnnotation& a : m_annotations) {
      if (numbered && a.id == owner) {
        entry = &a;
      }
    }
    if (entry == nullptr) {
      continue;
    }
    for (const QJsonValue& itemValue : group.value(QStringLiteral("items")).toArray()) {
      const QJsonObject item = itemValue.toObject();
      if (item.contains(QStringLiteral("line"))) {
        const QJsonArray line = item.value(QStringLiteral("line")).toArray();
        entry->lines.append(QLineF(pointOf(line.at(0)), pointOf(line.at(1))));
      } else if (item.contains(QStringLiteral("arc"))) {
        const QJsonObject arc = item.value(QStringLiteral("arc")).toObject();
        const QPointF c = pointOf(arc.value(QStringLiteral("center")));
        const double r = arc.value(QStringLiteral("radius")).toDouble();
        const double start = arc.value(QStringLiteral("start")).toDouble();
        const double end = arc.value(QStringLiteral("end")).toDouble();
        constexpr int kSteps = 24;
        QPointF last(c.x() + r * std::cos(start), c.y() + r * std::sin(start));
        for (int i = 1; i <= kSteps; ++i) {
          const double a = start + (end - start) * i / kSteps;
          const QPointF next(c.x() + r * std::cos(a), c.y() + r * std::sin(a));
          entry->lines.append(QLineF(last, next));
          last = next;
        }
      } else if (item.contains(QStringLiteral("polyline"))) {
        const QPolygonF points = polygonOf(item.value(QStringLiteral("polyline")));
        for (qsizetype i = 1; i < points.size(); ++i) {
          entry->lines.append(QLineF(points[i - 1], points[i]));
        }
        if (item.value(QStringLiteral("closed")).toBool() && points.size() > 2) {
          entry->lines.append(QLineF(points.last(), points.first()));
        }
      } else if (item.contains(QStringLiteral("fill"))) {
        const QPolygonF fill = polygonOf(item.value(QStringLiteral("fill")));
        entry->areas.append(fill);
        // A datum's triangle, a frame's arrow: its tip (mitcad#1295).
        const bool tipped = entry->type == QStringLiteral("datum") || entry->type == QStringLiteral("frame");
        if (tipped && entry->tip.isEmpty()) {
          entry->tip = fill;
        }
      } else if (item.contains(QStringLiteral("text"))) {
        const QPolygonF text = textArea(item);
        if (!hasText.contains(entry->id)) {
          hasText.append(entry->id);
          entry->handle = text.boundingRect().center();
        }
        entry->areas.append(text);
      }
    }
  }
  for (SheetAnnotation& a : m_annotations) {
    if (!hasText.contains(a.id)) {
      a.handle = a.lines.isEmpty() ? a.box.center() : a.lines.first().center();
    }
  }
  if (view(m_selected) == nullptr) {
    m_selected = 0;
  }
  if (annotation(m_selectedAnnotation) == nullptr) {
    m_selectedAnnotation = 0;
  }
  m_moreViews.removeIf([this](int id) { return view(id) == nullptr; });
  m_moreAnnotations.removeIf([this](int id) { return annotation(id) == nullptr; });
  if (m_selected == 0 && m_selectedAnnotation == 0) {
    if (!m_moreViews.isEmpty()) {
      m_selected = m_moreViews.takeFirst();
    } else if (!m_moreAnnotations.isEmpty()) {
      m_selectedAnnotation = m_moreAnnotations.takeFirst();
    }
  }
  if (view(m_parentView) == nullptr) {
    m_parentView = 0;
  }
  // The zoom and the pan kept for the model's next answer (mitcad#751):
  // fitted the first time and for another size only.
  if (resized || !m_everFitted) {
    fit();
  }
  sheetMoved();
  int items = 0;
  int marks = 0;
  for (const QJsonValue& group : std::as_const(m_groups)) {
    const QJsonObject g = group.toObject();
    items += static_cast<int>(g.value(QStringLiteral("items")).toArray().size());
    if (g.value(QStringLiteral("class")).toString() == QStringLiteral("center-mark")) {
      ++marks;
    }
  }
  qDebug().noquote() << QStringLiteral("Drawing sheet drawn: %1 views, %2 items, %3 center marks")
                            .arg(m_views.size())
                            .arg(items)
                            .arg(marks);
  logAnnotations();
  logPlaces();
}

void DrawingSheetView::sheetMoved() {
  m_sheetPainted = false;
  update();
}

bool DrawingSheetView::followsPointer() const {
  // A line's or a circle's rubber band, the line to where a view or an
  // annotation goes and its preview, a projected view's box beside its
  // parent.
  return m_placing || !m_picks.points.isEmpty() || m_pick == SheetPick::OutOfView;
}

void DrawingSheetView::logAnnotations() {
  QStringList parts;
  for (const SheetAnnotation& a : std::as_const(m_annotations)) {
    if (a.broken) {
      parts << QStringLiteral("%1 broken").arg(a.id);
    } else {
      parts << QStringLiteral("%1 = %2").arg(a.id).arg(a.label.isEmpty() ? a.type : a.label);
    }
  }
  const QString text = parts.join(QStringLiteral(" | "));
  if (text == m_loggedAnnotations) {
    return;
  }
  m_loggedAnnotations = text;
  qDebug().noquote() << QStringLiteral("Drawing annotations drawn: %1").arg(text.isEmpty() ? QStringLiteral("none") : text);
}

void DrawingSheetView::selectAnnotation(int id) {
  const int chosen = annotation(id) != nullptr ? id : 0;
  // One item selected: the others Ctrl+click added go.
  const bool several = !m_moreViews.isEmpty() || !m_moreAnnotations.isEmpty();
  m_moreViews.clear();
  m_moreAnnotations.clear();
  if (chosen == m_selectedAnnotation && !several) {
    return;
  }
  m_selectedAnnotation = chosen;
  if (chosen != 0) {
    // One selection: the view's goes.
    m_selected = 0;
    qDebug().noquote() << QStringLiteral("Drawing annotation %1 selected").arg(chosen);
  }
  emit annotationSelected(chosen);
  update();
}

QList<int> DrawingSheetView::selectedViews() const {
  QList<int> out;
  if (m_selected != 0) {
    out << m_selected;
  }
  return out + m_moreViews;
}

QList<int> DrawingSheetView::selectedAnnotations() const {
  QList<int> out;
  if (m_selectedAnnotation != 0) {
    out << m_selectedAnnotation;
  }
  return out + m_moreAnnotations;
}

int DrawingSheetView::selectionSize() const {
  return static_cast<int>(selectedViews().size() + selectedAnnotations().size());
}

bool DrawingSheetView::isSelectedView(int id) const { return id != 0 && (id == m_selected || m_moreViews.contains(id)); }

bool DrawingSheetView::isSelectedAnnotation(int id) const {
  return id != 0 && (id == m_selectedAnnotation || m_moreAnnotations.contains(id));
}

void DrawingSheetView::clearSelection() {
  if (selectionSize() == 0) {
    return;
  }
  m_moreViews.clear();
  m_moreAnnotations.clear();
  m_selected = 0;
  m_selectedAnnotation = 0;
  emit selectionChanged();
  update();
}

void DrawingSheetView::toggleSelected(int viewId, int note) {
  if (note != 0) {
    if (note == m_selectedAnnotation) {
      m_selectedAnnotation = 0;
    } else if (!m_moreAnnotations.removeOne(note)) {
      if (selectionSize() == 0) {
        m_selectedAnnotation = note;
      } else {
        m_moreAnnotations.append(note);
      }
    }
  } else if (viewId != 0) {
    if (viewId == m_selected) {
      m_selected = 0;
    } else if (!m_moreViews.removeOne(viewId)) {
      if (selectionSize() == 0) {
        m_selected = viewId;
      } else {
        m_moreViews.append(viewId);
      }
    }
  } else {
    return;
  }
  // The first one taken out: another is the first (the panel's).
  if (m_selected == 0 && m_selectedAnnotation == 0) {
    if (!m_moreViews.isEmpty()) {
      m_selected = m_moreViews.takeFirst();
    } else if (!m_moreAnnotations.isEmpty()) {
      m_selectedAnnotation = m_moreAnnotations.takeFirst();
    }
  }
  logSelection();
  emit selectionChanged();
  update();
}

void DrawingSheetView::logSelection() {
  QStringList items;
  for (const int id : selectedViews()) {
    items << QStringLiteral("view %1").arg(id);
  }
  for (const int id : selectedAnnotations()) {
    items << QStringLiteral("annotation %1").arg(id);
  }
  qDebug().noquote() << QStringLiteral("Drawing selection: %1")
                            .arg(items.isEmpty() ? QStringLiteral("nothing") : items.join(QStringLiteral(", ")));
}

const SheetAnnotation* DrawingSheetView::annotation(int id) const {
  for (const SheetAnnotation& a : m_annotations) {
    if (a.id == id && id != 0) {
      return &a;
    }
  }
  return nullptr;
}

int DrawingSheetView::annotationAt(const QPointF& sheet) const {
  // Near its lines or texts: 4 pixels, 0.8 mm of paper at least.
  const double reach = std::max(0.8, 4.0 / m_zoom);
  int best = 0;
  double nearest = 0.0;
  double area = 0.0;
  for (const SheetAnnotation& a : m_annotations) {
    double gap = -1.0;
    if (a.lines.isEmpty() && a.areas.isEmpty()) {
      // Nothing drawn (a broken one): its box.
      if (!a.box.isNull() && a.box.adjusted(-1.0, -1.0, 1.0, 1.0).contains(sheet)) {
        gap = 0.0;
      }
    } else {
      double d = std::numeric_limits<double>::infinity();
      for (const QLineF& line : a.lines) {
        d = std::min(d, segmentDistance(sheet, line));
      }
      for (const QPolygonF& shape : a.areas) {
        if (shape.containsPoint(sheet, Qt::OddEvenFill)) {
          d = 0.0;
        } else {
          for (qsizetype i = 0; i < shape.size(); ++i) {
            d = std::min(d, segmentDistance(sheet, QLineF(shape[i], shape[(i + 1) % shape.size()])));
          }
        }
      }
      if (d <= reach) {
        gap = d;
      }
    }
    if (gap < 0.0) {
      continue;
    }
    const double size = a.box.width() * a.box.height();
    if (best == 0 || gap < nearest - 1e-9 || (gap <= nearest + 1e-9 && size < area)) {
      best = a.id;
      nearest = gap;
      area = size;
    }
  }
  return best;
}

double DrawingSheetView::markWidth(const QJsonObject& item, bool picked) const {
  const double least = picked ? kPickMarkWidth : kHoverMarkWidth;
  const QString pen = item.value(QStringLiteral("pen")).toString();
  if (pen.isEmpty()) {
    return least;
  }
  // As paintItems draws the line: at least a pixel wide.
  const double drawn = std::max(penOf(m_pens, pen).widthF() * m_zoom, 1.0);
  return std::max(least, drawn + 2.0 * kMarkMargin);
}

void DrawingSheetView::setPickMarks(const QJsonArray& items) {
  m_pickMarks = items;
  update();
}

void DrawingSheetView::setHoverMarks(const QJsonArray& items) {
  if (items == m_hoverMarks) {
    return;
  }
  m_hoverMarks = items;
  // The corner (or centre) under the pointer, marked (mitcad#1338).
  QString marker;
  for (const QJsonValue& value : items) {
    const QJsonObject item = value.toObject();
    if (item.contains(QStringLiteral("point"))) {
      const QPointF at = pointOf(item.value(QStringLiteral("point")));
      marker = QStringLiteral("Drawing corner marker at %1,%2, radius %3 px, filled")
                   .arg(at.x(), 0, 'f', 1)
                   .arg(at.y(), 0, 'f', 1)
                   .arg(kCornerMarkerRadius, 0, 'f', 1);
      break;
    }
  }
  if (marker != m_loggedMarker) {
    m_loggedMarker = marker;
    if (!marker.isEmpty()) {
      qDebug().noquote() << marker;
    }
  }
  update();
}

void DrawingSheetView::setCornerDots(const QVector<QPointF>& points, double radius) {
  if (points == m_cornerDots && (points.isEmpty() || radius == m_cornerDotRadius)) {
    return;
  }
  m_cornerDots = points;
  m_cornerDotRadius = radius;
  qDebug().noquote() << (points.isEmpty() ? QStringLiteral("Drawing corner dots: none")
                                          : QStringLiteral("Drawing corner dots: %1, radius %2 px")
                                                .arg(points.size())
                                                .arg(radius, 0, 'f', 1));
  update();
}

void DrawingSheetView::setPlacingPreview(const QString& kind, const QVector<QPointF>& points,
                                         const QPointF& direction) {
  m_previewKind = kind;
  m_previewPoints = points;
  m_previewDirection = direction;
  if (!kind.isEmpty()) {
    logPreview(kind);
  }
  update();
}

void DrawingSheetView::setPlacingLabel(const QString& label) {
  if (label == m_previewLabel) {
    return;
  }
  m_previewLabel = label;
  if (!label.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Drawing preview value: %1").arg(label);
  }
  update();
}

void DrawingSheetView::setHoverWords(const QString& words) {
  if (words == m_hoverWords) {
    return;
  }
  m_hoverWords = words;
  qDebug().noquote() << QStringLiteral("Drawing hover words: %1")
                            .arg(words.isEmpty() ? QStringLiteral("none") : words);
  update();
}

void DrawingSheetView::setPicksWhilePlacing(bool on) { m_picksWhilePlacing = on; }

void DrawingSheetView::setPointSnap(std::function<std::optional<QPointF>(int view, const QPointF& at)> snap) {
  m_pointSnap = std::move(snap);
}

bool DrawingSheetView::asksHover() const {
  // An annotation tool's picks (and, mitcad#1272, while one pick of the
  // Dimension tool's Auto is placed); a line's or an arc's snapping
  // clicks; a section's line's or a break's (mitcad#1362), until it is
  // placed.
  return (m_pick == SheetPick::Picks && (!m_placing || m_picksWhilePlacing)) || m_pick == SheetPick::Segment ||
         m_pick == SheetPick::ArcPoints || (m_pick == SheetPick::Line && !m_placing);
}

QPointF DrawingSheetView::lineEnd(const QPointF& at, const std::optional<QPointF>& snap, bool freely,
                                  double* angle) const {
  if (angle != nullptr) {
    *angle = -1.0;
  }
  if (freely) {
    return at;
  }
  // A point first (mitcad#1362): it wins over the angle.
  if (snap) {
    return *snap;
  }
  // A section's second click (a break's sides stand alone).
  if (m_pick == SheetPick::Line && m_place && m_picks.points.size() == 1) {
    if (const std::optional<LineAngleSnap> turned = snapLineAngle(m_picks.points.first(), at)) {
      if (angle != nullptr) {
        *angle = turned->angle;
      }
      return turned->to;
    }
  }
  return at;
}

std::optional<QPointF> DrawingSheetView::hoverPoint() const {
  for (const QJsonValue& value : m_hoverMarks) {
    const QJsonObject item = value.toObject();
    if (item.contains(QStringLiteral("point"))) {
      return pointOf(item.value(QStringLiteral("point")));
    }
  }
  return std::nullopt;
}

void DrawingSheetView::setPlacingSnap(bool snapped, const QPointF& at, const QString& hint) {
  if (snapped == m_snapped && at == m_snapAt && hint == m_snapHint) {
    return;
  }
  m_snapped = snapped;
  m_snapAt = at;
  m_snapHint = snapped ? hint : QString();
  update();
}

void DrawingSheetView::setDragSnap(bool snapped, const QPointF& delta, const QString& hint) {
  if (snapped == m_dragSnapped && delta == m_dragSnap && hint == m_snapHint) {
    return;
  }
  m_dragSnapped = snapped;
  m_dragSnap = delta;
  m_snapHint = snapped ? hint : QString();
  update();
}

bool DrawingSheetView::draggedAlone() const {
  // A tip slid along its edge (mitcad#1295) does not snap.
  return m_drag == Drag::Annotation && !m_dragSelection && !m_dragTip && m_dragMoved;
}

void DrawingSheetView::askSnap(bool freely) {
  if (draggedAlone()) {
    emit annotationDragged(m_dragView, m_dragOffset, freely);
  } else if (m_placing && m_pick == SheetPick::Picks && m_pointerIn) {
    emit placingMoved(m_pointer, freely);
  }
}

void DrawingSheetView::logPreview(const QString& what) {
  if (what == m_loggedPreview) {
    return;
  }
  m_loggedPreview = what;
  qDebug().noquote() << QStringLiteral("Drawing preview: %1").arg(what);
}

QRectF DrawingSheetView::previewBox() const {
  QSizeF extent;
  if (m_pick == SheetPick::Line && m_place && m_picks.points.size() > 1) {
    // A section: about its parent's size.
    if (const SheetView* cut = view(m_picks.view)) {
      extent = cut->box.size();
    }
  } else if (m_pick == SheetPick::Circle && m_picks.radius > 0.0) {
    // A detail: its circle at twice its parent's scale.
    extent = QSizeF(4.0 * m_picks.radius, 4.0 * m_picks.radius);
  } else if (m_pick == SheetPick::OutOfView) {
    const int source = m_drag == Drag::OutOfView ? m_dragView : m_parentView;
    if (const SheetView* from = view(source)) {
      extent = from->box.size();
    }
  }
  if (!(extent.width() > 0.0) || !(extent.height() > 0.0)) {
    return {};
  }
  return QRectF(m_pointer.x() - extent.width() / 2.0, m_pointer.y() - extent.height() / 2.0, extent.width(),
                extent.height());
}

void DrawingSheetView::paintPreview(QPainter& painter) const {
  if (!m_pointerIn) {
    return;
  }
  painter.save();
  QPen pen(kSelected, 1.0, Qt::DashLine);
  pen.setCosmetic(true);
  painter.setPen(pen);
  painter.setBrush(Qt::NoBrush);
  // Where a dimension goes: the pointer, or the place it snaps to
  // (mitcad#1173).
  const bool snapped = m_snapped && m_placing && m_pick == SheetPick::Picks;
  const QPointF pointer = snapped ? m_snapAt : m_pointer;
  const QPointF now = toWidget(pointer);
  if (m_placing && m_pick == SheetPick::Picks && !m_previewPoints.isEmpty()) {
    if (m_previewKind.startsWith(QStringLiteral("dimension")) && m_previewPoints.size() >= 2) {
      // As the model lays a linear dimension out: along the direction it
      // measures along (the model's, mitcad#1272; else the axis its picks
      // lie further apart along), its line through the placing click.
      const QPointF a = m_previewPoints[0];
      const QPointF b = m_previewPoints[1];
      QPointF d = m_previewDirection;
      if (d.isNull()) {
        d = std::abs(a.x() - b.x()) >= std::abs(a.y() - b.y()) ? QPointF(1.0, 0.0) : QPointF(0.0, 1.0);
      }
      // A point's foot on the line through the pointer along `d`.
      const auto foot = [&pointer, &d](const QPointF& p) {
        const QPointF v = p - pointer;
        return pointer + d * (v.x() * d.x() + v.y() * d.y());
      };
      const QPointF a2 = foot(a);
      const QPointF b2 = foot(b);
      painter.drawLine(toWidget(a), toWidget(a2));
      painter.drawLine(toWidget(b), toWidget(b2));
      QPen line(kSelected, 1.5);
      line.setCosmetic(true);
      painter.setPen(line);
      painter.drawLine(toWidget(a2), toWidget(b2));
      // Its value over the middle of its line (mitcad#1691), as the
      // model writes it.
      if (!m_previewLabel.isEmpty()) {
        const QFontMetricsF metrics(painter.font());
        const QPointF middle = (toWidget(a2) + toWidget(b2)) / 2.0;
        const double w = metrics.horizontalAdvance(m_previewLabel) + 8.0;
        const double h = metrics.height() + 2.0;
        const QRectF box(middle.x() - w / 2.0, middle.y() - h - 3.0, w, h);
        painter.fillRect(box, QColor(255, 255, 255, 200));
        painter.drawText(box, Qt::AlignCenter, m_previewLabel);
      }
    } else {
      // A leader from the pick to where its text or symbol goes, its
      // value in the box where the model says it (mitcad#1691).
      painter.drawLine(toWidget(m_previewPoints.last()), now);
      const QFontMetricsF metrics(painter.font());
      const double w =
          m_previewLabel.isEmpty() ? 36.0 : std::max(36.0, metrics.horizontalAdvance(m_previewLabel) + 10.0);
      const QRectF box(now.x() - w / 2.0, now.y() - 9.0, w, 18.0);
      painter.drawRect(box);
      if (!m_previewLabel.isEmpty()) {
        painter.drawText(box, Qt::AlignCenter, m_previewLabel);
      }
    }
    if (snapped) {
      paintSnapHint(painter, now, m_snapHint);
    }
  } else if ((m_placing && (m_pick == SheetPick::Line || m_pick == SheetPick::Circle)) ||
             (m_pick == SheetPick::OutOfView && (m_drag == Drag::OutOfView ? m_dragMoved : viewAt(m_pointer) == 0))) {
    const QRectF box = previewBox();
    if (!box.isNull()) {
      painter.drawRect(QRectF(toWidget(box.bottomLeft()), toWidget(box.topRight())).normalized());
    }
  }
  painter.restore();
}

void DrawingSheetView::setHint(const QString& text) {
  if (text == m_hint) {
    return;
  }
  m_hint = text;
  if (!text.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Drawing hint: %1").arg(text);
  }
  update();
}

void DrawingSheetView::setPick(SheetPick pick, bool place) {
  m_pick = pick;
  m_place = place && (pick == SheetPick::Line || pick == SheetPick::Circle || pick == SheetPick::Picks ||
                      pick == SheetPick::ViewThenPlace);
  resetPicks();
  if (pick != SheetPick::OutOfView) {
    m_parentView = 0;
  }
  setCursor(pick == SheetPick::None ? Qt::ArrowCursor : Qt::CrossCursor);
  update();
}

void DrawingSheetView::setParentView(int id) {
  m_parentView = view(id) != nullptr ? id : 0;
  update();
}

void DrawingSheetView::selectView(int id) {
  const int chosen = view(id) != nullptr ? id : 0;
  const bool several = !m_moreViews.isEmpty() || !m_moreAnnotations.isEmpty();
  m_moreViews.clear();
  m_moreAnnotations.clear();
  if (chosen == m_selected && !several) {
    return;
  }
  m_selected = chosen;
  if (chosen != 0) {
    // One selection: the annotation's goes.
    m_selectedAnnotation = 0;
    qDebug().noquote() << QStringLiteral("Drawing view %1 selected").arg(chosen);
  }
  emit viewSelected(chosen);
  update();
}

const SheetView* DrawingSheetView::view(int id) const {
  for (const SheetView& v : m_views) {
    if (v.id == id && id != 0) {
      return &v;
    }
  }
  return nullptr;
}

bool DrawingSheetView::detailCircle(int id, QPointF* center, double* radius) const {
  const SheetView* v = view(id);
  if (v == nullptr || v->type != QStringLiteral("detail")) {
    return false;
  }
  const QJsonObject circle = v->json.value(QStringLiteral("circle")).toObject();
  if (circle.isEmpty()) {
    return false;
  }
  *center = pointOf(circle.value(QStringLiteral("center")));
  *radius = circle.value(QStringLiteral("radius")).toDouble();
  return *radius > 0.0;
}

int DrawingSheetView::viewAt(const QPointF& sheet) const {
  int best = 0;
  double area = 0.0;
  for (const SheetView& v : m_views) {
    // A little room around thin views.
    const QRectF box = v.box.adjusted(-2.0, -2.0, 2.0, 2.0);
    if (v.box.width() > 0.0 || v.box.height() > 0.0) {
      if (box.contains(sheet)) {
        const double a = box.width() * box.height();
        if (best == 0 || a < area) {
          best = v.id;
          area = a;
        }
      }
    }
  }
  return best;
}

void DrawingSheetView::fit() {
  const double margin = 24.0;
  // Beside what covers it, unless that leaves too little.
  QRect room = rect().marginsRemoved(m_fitMargins);
  if (room.width() < width() / 2 || room.height() < height() / 2) {
    room = rect();
  }
  const double w = std::max(1.0, room.width() - 2.0 * margin);
  const double h = std::max(1.0, room.height() - 2.0 * margin);
  m_zoom = std::max(0.05, std::min(w / m_width, h / m_height));
  m_origin = QPointF(room.left() + (room.width() - m_width * m_zoom) / 2.0,
                     room.top() + (room.height() + m_height * m_zoom) / 2.0);
  m_fitted = true;
  m_everFitted = true;
  logPlaces();
  sheetMoved();
}

void DrawingSheetView::setFitMargins(const QMargins& margins) {
  if (margins == m_fitMargins) {
    return;
  }
  m_fitMargins = margins;
  if (m_fitted) {
    fit();
    update();
  }
}

QPointF DrawingSheetView::toSheet(const QPointF& widget) const {
  return {(widget.x() - m_origin.x()) / m_zoom, (m_origin.y() - widget.y()) / m_zoom};
}

QPointF DrawingSheetView::toWidget(const QPointF& sheet) const {
  return {m_origin.x() + sheet.x() * m_zoom, m_origin.y() - sheet.y() * m_zoom};
}

void DrawingSheetView::logPlaces() {
  if (!isVisible()) {
    return;
  }
  const auto inWindow = [this](const QPointF& sheet) {
    const QPoint at = mapTo(window(), toWidget(sheet).toPoint());
    return QStringLiteral("%1,%2").arg(at.x()).arg(at.y());
  };
  QStringList lines;
  lines << QStringLiteral("Drawing sheet view %1 %2 %3")
               .arg(inWindow({0.0, 0.0}), inWindow({100.0, 0.0}), inWindow({0.0, 100.0}));
  for (const SheetView& v : std::as_const(m_views)) {
    if (v.box.width() > 0.0 || v.box.height() > 0.0) {
      lines << QStringLiteral("Drawing view %1 at %2").arg(v.id).arg(inWindow(v.box.center()));
    }
    QPointF center;
    double radius = 0.0;
    if (detailCircle(v.id, &center, &radius)) {
      lines << QStringLiteral("Drawing detail %1 circle at %2").arg(v.id).arg(inWindow(center));
    }
  }
  for (const SheetAnnotation& a : std::as_const(m_annotations)) {
    if (!a.box.isNull()) {
      // A point that picks it: on its text or a line, not its box's middle.
      lines << QStringLiteral("Drawing annotation %1 at %2").arg(a.id).arg(inWindow(a.handle));
    }
    // Its triangle or arrow, which a drag slides along its edge
    // (mitcad#1295).
    if (!a.tip.isEmpty()) {
      // A datum's triangle or dot (mitcad#1296); a frame's arrow is a
      // triangle too.
      const QString kind = a.tip.size() == 3 ? QStringLiteral("triangle") : QStringLiteral("dot");
      lines << QStringLiteral("Drawing annotation %1 tip at %2 (%3)").arg(a.id).arg(inWindow(tipCenter(a.tip)), kind);
    }
  }
  const QString text = lines.join(QLatin1Char('\n'));
  if (text == m_loggedPlaces) {
    return;
  }
  m_loggedPlaces = text;
  for (const QString& line : std::as_const(lines)) {
    qDebug().noquote() << line;
  }
}

void DrawingSheetView::startPlacing() {
  m_placing = true;
  qDebug().noquote() << QStringLiteral("Drawing placing view");
  emit placing();
  update();
}

void DrawingSheetView::resetPicks() {
  m_picks = ToolPicks();
  m_pickMarks = QJsonArray();
  m_placing = false;
  m_hoverMarks = QJsonArray();
  m_previewKind.clear();
  m_previewPoints.clear();
  m_previewDirection = QPointF();
  m_previewLabel.clear();
  m_hoverWords.clear();
  m_picksWhilePlacing = false;
  m_loggedPreview.clear();
  m_snapped = false;
  m_snapHint.clear();
  if (m_drag == Drag::OutOfView) {
    m_drag = Drag::None;
  }
}

void DrawingSheetView::paintSheet(const QSize& pixels, qreal ratio) {
  m_sheetPicture = QPixmap(pixels.expandedTo(QSize(1, 1)));
  m_sheetPicture.setDevicePixelRatio(ratio);
  m_sheetPicture.fill(isDarkPalette(palette()) ? kDarkBackground : kBackground);
  QPainter painter(&m_sheetPicture);
  painter.setRenderHint(QPainter::Antialiasing);
  painter.setFont(font());
  // The paper with a shadow.
  const QRectF paper(toWidget({0.0, m_height}), toWidget({m_width, 0.0}));
  painter.fillRect(paper.translated(3.0, 3.0), QColor(0, 0, 0, 40));
  painter.fillRect(paper, Qt::white);
  paintItems(painter);
  m_sheetPainted = true;
  ++m_sheetPaints;
}

void DrawingSheetView::paintEvent(QPaintEvent* /*event*/) {
  // The sheet from its picture, painted again only when it changed or
  // moved (mitcad#759: a shaded view's triangles and every text are not
  // walked again for a move of the pointer); what is selected, hovered and
  // placed over it.
  const qreal ratio = devicePixelRatioF();
  const QSize pixels = (QSizeF(size()) * ratio).toSize().expandedTo(QSize(1, 1));
  if (!m_sheetPainted || m_sheetPicture.size() != pixels || m_sheetPicture.devicePixelRatio() != ratio) {
    paintSheet(pixels, ratio);
  }
  QPainter painter(this);
  painter.drawPixmap(QPointF(0.0, 0.0), m_sheetPicture);
  painter.setRenderHint(QPainter::Antialiasing);

  // The views: the selected one, the one under the pointer, a tool's
  // parent.
  const auto outline = [&](const SheetView& v, const QColor& colour, Qt::PenStyle style, const QPointF& offset) {
    if (v.box.width() <= 0.0 && v.box.height() <= 0.0) {
      return;
    }
    const QRectF box = v.box.adjusted(-2.0, -2.0, 2.0, 2.0).translated(offset);
    QPen pen(colour, 1.5, style);
    pen.setCosmetic(true);
    painter.setPen(pen);
    painter.setBrush(Qt::NoBrush);
    painter.drawRect(QRectF(toWidget(box.bottomLeft()), toWidget(box.topRight())).normalized());
  };
  // Views the model could not draw: a red box at their place, with why.
  for (const SheetView& v : std::as_const(m_views)) {
    if (v.error.isEmpty() || (v.box.width() <= 0.0 && v.box.height() <= 0.0)) {
      continue;
    }
    const QRectF box(toWidget(v.box.bottomLeft()), toWidget(v.box.topRight()));
    QPen pen(kBroken, 1.5, Qt::DashLine);
    pen.setCosmetic(true);
    painter.setPen(pen);
    painter.setBrush(QColor(kBroken.red(), kBroken.green(), kBroken.blue(), 24));
    painter.drawRect(box.normalized());
    painter.drawText(box.normalized().adjusted(4.0, 4.0, -4.0, -4.0), Qt::AlignCenter | Qt::TextWordWrap,
                     tr("View %1: %2").arg(v.id).arg(v.error));
  }
  for (const SheetView& v : std::as_const(m_views)) {
    if (isSelectedView(v.id)) {
      outline(v, kSelected, Qt::SolidLine, {});
    } else if (v.id == m_parentView) {
      outline(v, kSelected, Qt::DashLine, {});
    } else if (v.id == m_hovered) {
      outline(v, kHovered, Qt::SolidLine, {});
    }
  }
  // A view being dragged: where it goes; several selected go together.
  if ((m_drag == Drag::View || m_drag == Drag::Annotation) && m_dragMoved) {
    for (const SheetView& v : std::as_const(m_views)) {
      if ((m_drag == Drag::View && v.id == m_dragView) || (m_dragSelection && isSelectedView(v.id))) {
        outline(v, kSelected, Qt::DashLine, m_dragOffset);
      }
    }
  }
  // Every corner the views show while Always show corners is on
  // (mitcad#1338): on the screen only, under what is hovered and picked.
  if (!m_cornerDots.isEmpty()) {
    painter.save();
    painter.setPen(Qt::NoPen);
    QColor dot = kHovered;
    dot.setAlpha(210);
    painter.setBrush(dot);
    for (const QPointF& p : std::as_const(m_cornerDots)) {
      painter.drawEllipse(toWidget(p), m_cornerDotRadius, m_cornerDotRadius);
    }
    painter.restore();
  }
  paintAnnotations(painter);
  // The title block editor's block (mitcad#1625).
  if (m_blockEditor != nullptr) {
    paintBlockEditor(painter);
  }
  // A selected detail's circle on its parent, which a drag moves.
  QPointF circleCenter;
  double circleRadius = 0.0;
  if (detailCircle(m_selected, &circleCenter, &circleRadius)) {
    const QPointF offset = m_drag == Drag::DetailCircle ? m_dragOffset : QPointF();
    QPen ring(kSelected, 1.5, m_drag == Drag::DetailCircle ? Qt::DashLine : Qt::SolidLine);
    ring.setCosmetic(true);
    painter.setPen(ring);
    painter.setBrush(Qt::NoBrush);
    const double r = circleRadius * m_zoom;
    painter.drawEllipse(toWidget(circleCenter + offset), r, r);
  }
  // The rubber band of a tool: a line, a circle, a projected view's place.
  QPen band(kSelected, 1.5, Qt::DashLine);
  band.setCosmetic(true);
  painter.setPen(band);
  painter.setBrush(Qt::NoBrush);
  if (m_placing && !m_picks.points.isEmpty()) {
    // The line or circle drawn, and a line from it to where the view goes.
    const QPointF first = toWidget(m_picks.points.first());
    QPointF from = first;
    if (m_pick == SheetPick::Line && m_picks.points.size() > 1) {
      const QPointF second = toWidget(m_picks.points[1]);
      painter.drawLine(first, second);
      from = (first + second) / 2.0;
    } else if (m_pick == SheetPick::Circle) {
      const double r = m_picks.radius * m_zoom;
      painter.drawEllipse(first, r, r);
    }
    if (m_pointerIn) {
      QPen thin(kSelected, 1.0, Qt::DotLine);
      thin.setCosmetic(true);
      painter.setPen(thin);
      painter.drawLine(from, toWidget(m_pointer));
    }
  } else if (m_pointerIn && !m_picks.points.isEmpty()) {
    const QPointF first = toWidget(m_picks.points.first());
    const QPointF now = toWidget(m_pointer);
    if (m_pick == SheetPick::Line) {
      // To where the click would go (mitcad#1362): the point it snaps to,
      // or turned onto a multiple of 45 degrees with its angle beside it.
      double angle = -1.0;
      const QPointF end = toWidget(
          lineEnd(m_pointer, hoverPoint(), dragsFreely(QGuiApplication::keyboardModifiers()), &angle));
      painter.drawLine(first, end);
      if (angle >= 0.0) {
        paintSnapHint(painter, end, tr("%1°").arg(QString::number(angle, 'g', 4)));
      }
    } else if (m_pick == SheetPick::Segment) {
      painter.drawLine(first, now);
    } else if (m_pick == SheetPick::Circle) {
      const double r = distance(first, now);
      painter.drawEllipse(first, r, r);
    } else if (m_pick == SheetPick::ArcPoints) {
      // The arc's centre, then its circle through the start (mitcad#729).
      painter.drawLine(first, now);
      if (m_picks.points.size() > 1) {
        const double r = distance(first, toWidget(m_picks.points[1]));
        painter.drawEllipse(first, r, r);
      }
    }
  }
  if (m_drag == Drag::OutOfView && m_dragMoved) {
    if (const SheetView* v = view(m_dragView)) {
      painter.drawLine(toWidget(v->box.center()), toWidget(m_pointer));
    }
  }
  // What is placed, under the pointer (mitcad#725).
  paintPreview(painter);
  // The tool's hint over the top of the sheet, as the 3D view shows one.
  if (!m_hint.isEmpty()) {
    const QFontMetrics metrics(font());
    const int w = metrics.horizontalAdvance(m_hint) + 24;
    const int h = metrics.height() + 10;
    const QRectF pill((width() - w) / 2.0, 8.0, w, h);
    painter.setPen(Qt::NoPen);
    painter.setBrush(QColor(0, 0, 0, 160));
    painter.drawRoundedRect(pill, h / 2.0, h / 2.0);
    painter.setPen(Qt::white);
    painter.drawText(pill, Qt::AlignCenter, m_hint);
  }
  // What a click would pick under the pointer, in words (mitcad#1691), at
  // the foot of the sheet: the status of the pointer, never kept in the
  // message history.
  if (!m_hoverWords.isEmpty()) {
    const QFontMetrics metrics(font());
    const int w = metrics.horizontalAdvance(m_hoverWords) + 24;
    const int h = metrics.height() + 10;
    const QRectF pill((width() - w) / 2.0, height() - h - 8.0, w, h);
    painter.setPen(Qt::NoPen);
    painter.setBrush(QColor(0, 0, 0, 160));
    painter.drawRoundedRect(pill, h / 2.0, h / 2.0);
    painter.setPen(Qt::white);
    painter.drawText(pill, Qt::AlignCenter, m_hoverWords);
  }
}

void DrawingSheetView::paintAnnotations(QPainter& painter) const {
  const auto widgetBox = [this](const QRectF& box) {
    return QRectF(toWidget(box.bottomLeft()), toWidget(box.topRight())).normalized();
  };
  painter.save();
  // Annotations whose picks are gone: a warning's box beside their view, with
  // their id and why.
  for (const SheetAnnotation& a : m_annotations) {
    if (!a.broken || a.box.isNull()) {
      continue;
    }
    QPen pen(kBroken, 1.5, Qt::DashLine);
    pen.setCosmetic(true);
    painter.setPen(pen);
    painter.setBrush(QColor(kBroken.red(), kBroken.green(), kBroken.blue(), 24));
    const QRectF box = widgetBox(a.box);
    painter.drawRect(box);
    painter.drawText(box.adjusted(2.0, 0.0, -2.0, 0.0), Qt::AlignCenter, QStringLiteral("%1 ⚠").arg(a.id));
  }
  // The selected one, the one under the pointer, the one dragged.
  for (const SheetAnnotation& a : m_annotations) {
    const bool selected = isSelectedAnnotation(a.id);
    if (a.box.isNull() || (!selected && a.id != m_hoveredAnnotation)) {
      continue;
    }
    // Dragged, alone or with the others selected.
    const bool dragged = m_dragMoved && ((m_drag == Drag::Annotation && a.id == m_dragView) ||
                                         ((m_drag == Drag::Annotation || m_drag == Drag::View) &&
                                          m_dragSelection && selected));
    QPen pen(selected ? kSelected : kHovered, 1.5, dragged ? Qt::DashLine : Qt::SolidLine);
    pen.setCosmetic(true);
    painter.setPen(pen);
    painter.setBrush(Qt::NoBrush);
    // A dimension dragged alone by its snapped drag (mitcad#1173).
    const bool snappedDrag = dragged && m_dragSnapped && draggedAlone();
    const QPointF offset = dragged ? (snappedDrag ? m_dragSnap : m_dragOffset) : QPointF();
    const QRectF drawn = widgetBox(a.box.adjusted(-1.0, -1.0, 1.0, 1.0).translated(offset));
    painter.drawRect(drawn);
    if (snappedDrag) {
      paintSnapHint(painter, drawn.topRight(), m_snapHint);
    }
  }
  // What a click would pick under the pointer, light (mitcad#725), and
  // what an annotation tool picked so far, in the handles' orange
  // (mitcad#1041: what is under the pointer in the hover's blue). Each
  // line, circle or arc as wide as the line it marks and a little more,
  // see-through so that the line shows under it (mitcad#1618).
  QColor hoverMark = themeColor(ColorRole::Hover, false);
  hoverMark.setAlpha(kMarkAlpha);
  QColor pickMark = themeColor(ColorRole::Handle, false);
  pickMark.setAlpha(kMarkAlpha);
  QPen mark(hoverMark, kHoverMarkWidth, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin);
  mark.setCosmetic(true);
  painter.setBrush(Qt::NoBrush);
  QJsonArray marks = m_hoverMarks;
  const qsizetype hovered = marks.size();
  for (const QJsonValue& value : m_pickMarks) {
    marks.append(value);
  }
  for (qsizetype i = 0; i < marks.size(); ++i) {
    const bool picked = i >= hovered;
    const QJsonObject item = marks.at(i).toObject();
    mark.setColor(picked ? pickMark : hoverMark);
    mark.setWidthF(markWidth(item, picked));
    painter.setPen(mark);
    if (item.contains(QStringLiteral("outline"))) {
      // A line of the body the hover's pick is on (mitcad#1691), under
      // the pick's own mark and narrower.
      QPen outline(hoverMark, kOutlineMarkWidth, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin);
      outline.setCosmetic(true);
      painter.setPen(outline);
      QPolygonF points;
      for (const QJsonValue& p : item.value(QStringLiteral("outline")).toArray()) {
        points.append(toWidget(pointOf(p)));
      }
      painter.drawPolyline(points);
    } else if (item.contains(QStringLiteral("line"))) {
      const QJsonArray line = item.value(QStringLiteral("line")).toArray();
      painter.drawLine(toWidget(pointOf(line.at(0))), toWidget(pointOf(line.at(1))));
    } else if (item.contains(QStringLiteral("circle"))) {
      const QJsonObject circle = item.value(QStringLiteral("circle")).toObject();
      const double r = circle.value(QStringLiteral("radius")).toDouble() * m_zoom;
      painter.drawEllipse(toWidget(pointOf(circle.value(QStringLiteral("center")))), r, r);
    } else if (item.contains(QStringLiteral("arc"))) {
      // An arc picked (mitcad#717: not its whole circle).
      const QJsonObject arc = item.value(QStringLiteral("arc")).toObject();
      const QPointF c = pointOf(arc.value(QStringLiteral("center")));
      const double r = arc.value(QStringLiteral("radius")).toDouble();
      const double start = arc.value(QStringLiteral("start")).toDouble();
      const double end = arc.value(QStringLiteral("end")).toDouble();
      QPolygonF points;
      for (int step = 0; step <= 32; ++step) {
        const double a = start + (end - start) * step / 32.0;
        points.append(toWidget(QPointF(c.x() + r * std::cos(a), c.y() + r * std::sin(a))));
      }
      painter.drawPolyline(points);
    } else if (item.contains(QStringLiteral("point"))) {
      // A corner or a centre: filled, ringed in the paper's white so that
      // it shows on the lines that end there (mitcad#1338: an unfilled
      // circle of 4 pixels went unnoticed on a high-DPI screen).
      painter.save();
      QPen ring(Qt::white, 1.5);
      ring.setCosmetic(true);
      painter.setPen(ring);
      QColor fill = mark.color();
      fill.setAlpha(230);
      painter.setBrush(fill);
      painter.drawEllipse(toWidget(pointOf(item.value(QStringLiteral("point")))), kCornerMarkerRadius,
                          kCornerMarkerRadius);
      painter.restore();
    }
  }
  painter.restore();
}

bool DrawingSheetView::paintImage(QPainter& painter, const QString& base64, const QString& format,
                                  const QRectF& box) const {
  // The box on the sheet (y up) as a rectangle in the widget (y down).
  const QRectF target = QRectF(toWidget(QPointF(box.left(), box.top() + box.height())),
                               toWidget(QPointF(box.left() + box.width(), box.top())))
                            .normalized();
  if (target.width() < 1.0 || target.height() < 1.0) {
    return true;
  }
  const auto fitted = [&target](double aspect) {
    if (!(aspect > 0.0)) {
      return target;
    }
    double w = target.width();
    double h = target.height();
    if (w / h > aspect) {
      w = h * aspect;
    } else {
      h = w / aspect;
    }
    return QRectF(target.center().x() - w / 2.0, target.center().y() - h / 2.0, w, h);
  };
  if (format == QStringLiteral("svg")) {
    QSvgRenderer renderer(QByteArray::fromBase64(base64.toLatin1()));
    if (!renderer.isValid()) {
      return false;
    }
    const QSizeF size = renderer.viewBoxF().size();
    renderer.render(&painter, fitted(size.height() > 0.0 ? size.width() / size.height() : 0.0));
    return true;
  }
  auto cached = m_imageCache.constFind(base64);
  if (cached == m_imageCache.constEnd()) {
    // A few images at most: the cache is cleared rather than grown on.
    if (m_imageCache.size() > 16) {
      m_imageCache.clear();
    }
    cached = m_imageCache.insert(base64, QImage::fromData(QByteArray::fromBase64(base64.toLatin1())));
  }
  const QImage& image = cached.value();
  if (image.isNull()) {
    return false;
  }
  painter.save();
  painter.setRenderHint(QPainter::SmoothPixmapTransform);
  painter.drawImage(fitted(static_cast<double>(image.width()) / image.height()), image);
  painter.restore();
  return true;
}

void DrawingSheetView::paintBlockEditor(QPainter& painter) const {
  const TitleBlockEditor& editor = *m_blockEditor;
  const QPointF origin = editor.origin();
  const auto at = [this, &origin](const QPointF& block) { return toWidget(origin + block); };
  // A box of the block (y up) as a widget rectangle.
  const auto rectOf = [&at](const QRectF& box) {
    return QRectF(at(QPointF(box.left(), box.top() + box.height())), at(QPointF(box.left() + box.width(), box.top())))
        .normalized();
  };
  painter.save();
  const QRectF block = rectOf(QRectF(0.0, 0.0, editor.width(), editor.height()));
  painter.fillRect(block, Qt::white);
  // The grid, where its points are apart enough to see.
  const double step = editor.grid() * m_zoom;
  if (step >= 6.0) {
    painter.setPen(QPen(QColor(0, 0, 0, 70), 1.0));
    for (double x = 0.0; x <= editor.width() + 1e-9; x += editor.grid()) {
      for (double y = 0.0; y <= editor.height() + 1e-9; y += editor.grid()) {
        painter.drawPoint(at(QPointF(x, y)));
      }
    }
  }
  // The images, then the cells, the lines and the outline.
  const auto selected = [&editor](TitleBlockEditor::Kind kind, int index) {
    return editor.selectedKind() == kind && editor.selectedIndex() == index;
  };
  for (int i = 0; i < static_cast<int>(editor.images().size()); ++i) {
    const BlockImage& image = editor.images().at(i);
    const QRectF box(origin + image.box.topLeft(), image.box.size());
    paintImage(painter, image.data, image.format, box);
    QPen dashed(selected(TitleBlockEditor::Kind::Image, i) ? kSelected : kHovered, 1.0, Qt::DashLine);
    dashed.setCosmetic(true);
    painter.setPen(dashed);
    painter.setBrush(Qt::NoBrush);
    painter.drawRect(rectOf(image.box));
  }
  const auto face = [this](double capHeight) {
    QFont font(QStringLiteral("Helvetica"));
    font.setStyleHint(QFont::SansSerif);
    font.setPixelSize(std::max(1, static_cast<int>(std::lround(capHeight / kCapHeight * m_zoom))));
    return font;
  };
  for (int i = 0; i < static_cast<int>(editor.fields().size()); ++i) {
    const BlockField& field = editor.fields().at(i);
    const QRectF cell = rectOf(field.cell);
    const bool on = selected(TitleBlockEditor::Kind::Field, i);
    QPen pen(on ? kSelected : kHovered, on ? 2.0 : 1.0, on ? Qt::SolidLine : Qt::DashLine);
    pen.setCosmetic(true);
    painter.setPen(pen);
    painter.setBrush(on ? QColor(kSelected.red(), kSelected.green(), kSelected.blue(), 30) : QColor(Qt::transparent));
    painter.drawRect(cell);
    painter.setPen(Qt::black);
    // As the model writes them: the label small in the upper left corner,
    // the text (its names in braces as typed) at the bottom.
    if (!field.label.isEmpty() && 2.0 * m_zoom >= 3.0) {
      painter.setFont(face(2.0));
      painter.drawText(at(QPointF(field.cell.left() + 1.5, field.cell.top() + field.cell.height() - 3.5)), field.label);
    }
    const double letters = field.height > 0.0 ? field.height : 3.5;
    if (!field.text.isEmpty() && letters * m_zoom >= 3.0) {
      painter.setFont(face(letters));
      painter.drawText(at(QPointF(field.cell.left() + 1.5, field.cell.top() + 2.0)), field.text);
    }
  }
  for (int i = 0; i < static_cast<int>(editor.lines().size()); ++i) {
    const QLineF& line = editor.lines().at(i);
    const bool on = selected(TitleBlockEditor::Kind::Line, i);
    QPen pen(on ? kSelected : Qt::black, on ? 3.0 : std::max(1.0, 0.18 * m_zoom));
    pen.setCosmetic(true);
    painter.setPen(pen);
    painter.drawLine(at(line.p1()), at(line.p2()));
  }
  QPen outline(Qt::black, std::max(1.0, 0.5 * m_zoom));
  outline.setCosmetic(true);
  painter.setPen(outline);
  painter.setBrush(Qt::NoBrush);
  painter.drawRect(block);
  // A tool's first click: its line or box to the pointer; the point a
  // click would take.
  QPen band(kSelected, 1.5, Qt::DashLine);
  band.setCosmetic(true);
  painter.setPen(band);
  if (const std::optional<QPointF> first = editor.firstPoint()) {
    if (editor.tool() == TitleBlockEditor::Tool::Line) {
      painter.drawLine(at(*first), at(editor.pointer()));
    } else {
      painter.drawRect(QRectF(at(*first), at(editor.pointer())).normalized());
    }
  }
  if (editor.tool() != TitleBlockEditor::Tool::Select && m_pointerIn) {
    painter.setBrush(kSelected);
    painter.drawEllipse(at(editor.pointer()), 3.0, 3.0);
  }
  painter.restore();
}

void DrawingSheetView::paintItems(QPainter& painter) const {
  // Lines in sheet millimetres: the painter scaled, y up.
  painter.save();
  painter.translate(m_origin);
  painter.scale(m_zoom, -m_zoom);
  const auto withPen = [&](const QString& name) {
    QPen pen = penOf(m_pens, name);
    // At least a pixel wide on the screen.
    if (pen.widthF() * m_zoom < 1.0) {
      pen.setWidthF(1.0 / m_zoom);
    }
    painter.setPen(pen);
    painter.setBrush(Qt::NoBrush);
  };
  struct Label {
    QPointF at;
    double height;
    double angle;
    QString anchor;
    QString text;
  };
  QVector<Label> labels;
  // The title block editor's block is painted over the sheet instead of the
  // model's (mitcad#1625).
  const auto shown = [this](const QJsonValue& group) {
    return m_blockEditor == nullptr ||
           group.toObject().value(QStringLiteral("class")).toString() != QStringLiteral("title-block");
  };
  // A title block's images first, under its lines (mitcad#1625), in the
  // widget's own coordinates: upright.
  painter.save();
  painter.resetTransform();
  for (const QJsonValue& groupValue : m_groups) {
    if (!shown(groupValue)) {
      continue;
    }
    for (const QJsonValue& itemValue : groupValue.toObject().value(QStringLiteral("items")).toArray()) {
      const QJsonObject item = itemValue.toObject();
      if (item.contains(QStringLiteral("image"))) {
        const QPointF at = pointOf(item.value(QStringLiteral("at")));
        const QPointF size = pointOf(item.value(QStringLiteral("size")));
        paintImage(painter, item.value(QStringLiteral("image")).toString(),
                   item.value(QStringLiteral("format")).toString(), QRectF(at, QSizeF(size.x(), size.y())));
      }
    }
  }
  painter.restore();
  for (const QJsonValue& groupValue : m_groups) {
    if (!shown(groupValue)) {
      continue;
    }
    for (const QJsonValue& itemValue : groupValue.toObject().value(QStringLiteral("items")).toArray()) {
      const QJsonObject item = itemValue.toObject();
      const QString pen = item.value(QStringLiteral("pen")).toString();
      if (item.contains(QStringLiteral("line"))) {
        const QJsonArray line = item.value(QStringLiteral("line")).toArray();
        withPen(pen);
        painter.drawLine(pointOf(line.at(0)), pointOf(line.at(1)));
      } else if (item.contains(QStringLiteral("arc"))) {
        const QJsonObject arc = item.value(QStringLiteral("arc")).toObject();
        const QPointF c = pointOf(arc.value(QStringLiteral("center")));
        const double r = arc.value(QStringLiteral("radius")).toDouble();
        const double start = arc.value(QStringLiteral("start")).toDouble();
        const double end = arc.value(QStringLiteral("end")).toDouble();
        withPen(pen);
        QPainterPath path;
        path.moveTo(c.x() + r * std::cos(start), c.y() + r * std::sin(start));
        // Counter-clockwise on the sheet, whose y runs up here as in the
        // painter's turned coordinates: arcTo's angles run the other way.
        path.arcTo(QRectF(c.x() - r, c.y() - r, 2.0 * r, 2.0 * r), -start * 180.0 / kPi,
                   -(end - start) * 180.0 / kPi);
        painter.drawPath(path);
      } else if (item.contains(QStringLiteral("polyline"))) {
        withPen(pen);
        const QPolygonF points = polygonOf(item.value(QStringLiteral("polyline")));
        if (item.value(QStringLiteral("closed")).toBool()) {
          painter.drawPolygon(points);
        } else {
          painter.drawPolyline(points);
        }
      } else if (item.contains(QStringLiteral("fill"))) {
        painter.setPen(Qt::NoPen);
        painter.setBrush(Qt::black);
        painter.drawPolygon(polygonOf(item.value(QStringLiteral("fill"))));
      } else if (item.contains(QStringLiteral("shade"))) {
        const int level = std::clamp(static_cast<int>(std::lround(item.value(QStringLiteral("grey")).toDouble() * 255.0)), 0, 255);
        painter.setPen(Qt::NoPen);
        painter.setBrush(QColor(level, level, level));
        painter.drawPolygon(polygonOf(item.value(QStringLiteral("shade"))));
      } else if (item.contains(QStringLiteral("text"))) {
        labels.append({pointOf(item.value(QStringLiteral("at"))), item.value(QStringLiteral("height")).toDouble(),
                       item.value(QStringLiteral("angle")).toDouble(), item.value(QStringLiteral("anchor")).toString(),
                       item.value(QStringLiteral("text")).toString()});
      }
    }
  }
  painter.restore();
  // Texts upright in the widget's own coordinates, at their size on the
  // paper.
  painter.setPen(Qt::black);
  for (const Label& label : std::as_const(labels)) {
    const double pixels = label.height / kCapHeight * m_zoom;
    if (pixels < 2.0) {
      continue; // too small to read
    }
    QFont face(QStringLiteral("Helvetica"));
    face.setStyleHint(QFont::SansSerif);
    face.setPixelSize(std::max(1, static_cast<int>(std::lround(pixels))));
    painter.setFont(face);
    const double advance = QFontMetricsF(face).horizontalAdvance(label.text);
    double x = 0.0;
    if (label.anchor == QStringLiteral("middle")) {
      x = -advance / 2.0;
    } else if (label.anchor == QStringLiteral("end")) {
      x = -advance;
    }
    painter.save();
    painter.translate(toWidget(label.at));
    painter.rotate(-label.angle * 180.0 / kPi);
    painter.drawText(QPointF(x, 0.0), label.text);
    painter.restore();
  }
}

// A sheet closed and opened again says where it is again, even in the
// same place: the tests wait for its place after opening a drawing.
void DrawingSheetView::hideEvent(QHideEvent* event) {
  QWidget::hideEvent(event);
  m_loggedPlaces.clear();
}

void DrawingSheetView::showEvent(QShowEvent* event) {
  QWidget::showEvent(event);
  logPlaces();
}

void DrawingSheetView::resizeEvent(QResizeEvent* event) {
  QWidget::resizeEvent(event);
  if (m_fitted) {
    fit();
  } else {
    logPlaces();
  }
}

void DrawingSheetView::mousePressEvent(QMouseEvent* event) {
  setFocus(Qt::MouseFocusReason);
  if (event->button() == Qt::RightButton) {
    // While a tool picks, a right click is its Esc on the release, as in a
    // sketch (mitcad#1404); with no tool, contextMenuEvent opens the menu.
    // A left drag under way keeps its state.
    m_rightPressTool = m_pick != SheetPick::None;
    m_rightPressAt = event->position();
    return;
  }
  const QPointF at = toSheet(event->position());
  m_dragMoved = false;
  m_dragSnapped = false;
  m_dragTip = false;
  if (event->button() == Qt::MiddleButton) {
    m_drag = Drag::Pan;
    m_dragFrom = event->position();
    return;
  }
  if (event->button() != Qt::LeftButton) {
    return;
  }
  // The title block editor's clicks and drags (mitcad#1625).
  if (m_blockEditor != nullptr) {
    const QString what = m_blockEditor->press(at, std::max(0.5, 5.0 / m_zoom));
    if (!what.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Title block %1").arg(what);
    }
    emit titleBlockEdited();
    update();
    return;
  }
  const int under = viewAt(at);
  switch (m_pick) {
  case SheetPick::None: {
    // An annotation before the view it is on: it is the smaller.
    const int note = annotationAt(at);
    m_dragSelection = false;
    // Ctrl+click: into the selection, or out of it (mitcad#725).
    if (event->modifiers().testFlag(Qt::ControlModifier)) {
      toggleSelected(note != 0 ? 0 : under, note);
      break;
    }
    // A selected detail's circle on its parent is dragged to move it.
    QPointF center;
    double radius = 0.0;
    if (selectionSize() == 1 && detailCircle(m_selected, &center, &radius) && distance(at, center) <= radius) {
      m_drag = Drag::DetailCircle;
      m_dragView = m_selected;
      m_dragFrom = at;
      m_dragOffset = QPointF();
      break;
    }
    // One of several selected: a drag moves them all.
    const bool ofSeveral = selectionSize() > 1 && (note != 0 ? isSelectedAnnotation(note) : isSelectedView(under));
    if (note != 0) {
      if (ofSeveral) {
        m_dragSelection = true;
      } else {
        selectAnnotation(note);
      }
      m_drag = Drag::Annotation;
      m_dragView = note;
      m_dragFrom = at;
      m_dragOffset = QPointF();
      // On a datum's triangle or a frame's arrow, alone: the drag slides
      // its tip along its edge (mitcad#1295).
      const SheetAnnotation* pressed = annotation(note);
      m_dragTip =
          !ofSeveral && pressed != nullptr && onTip(pressed->tip, at, std::max(0.8, 4.0 / m_zoom));
      break;
    }
    if (ofSeveral) {
      m_dragSelection = true;
    } else {
      selectAnnotation(0);
      selectView(under);
    }
    if (under != 0) {
      m_drag = Drag::View;
      m_dragView = under;
      m_dragFrom = at;
      m_dragOffset = QPointF();
    }
    break;
  }
  case SheetPick::OutOfView:
    if (under != 0) {
      m_drag = Drag::OutOfView;
      m_dragView = under;
      m_dragFrom = at;
    }
    break;
  default:
    break; // the clicks of the other tools come on release
  }
}

void DrawingSheetView::mouseMoveEvent(QMouseEvent* event) {
  const QPointF at = toSheet(event->position());
  const bool wasIn = m_pointerIn;
  m_pointer = at;
  m_pointerIn = true;
  switch (m_drag) {
  case Drag::Pan: {
    const QPointF delta = event->position() - m_dragFrom;
    m_dragFrom = event->position();
    m_origin += delta;
    m_fitted = false;
    sheetMoved();
    return;
  }
  case Drag::View:
  case Drag::DetailCircle:
  case Drag::Annotation:
    m_dragOffset = at - m_dragFrom;
    m_dragMoved = m_dragMoved || distance(m_dragOffset, QPointF()) * m_zoom > 3.0;
    // Where a dimension dragged alone snaps (mitcad#1173), or a leader's
    // text (mitcad#1294); Shift or Alt held: where it is dropped.
    if (draggedAlone()) {
      emit annotationDragged(m_dragView, m_dragOffset, dragsFreely(event->modifiers()));
    }
    update();
    return;
  case Drag::OutOfView:
    m_dragMoved = m_dragMoved || distance(at, m_dragFrom) * m_zoom > 3.0;
    if (m_dragMoved && !previewBox().isNull()) {
      logPreview(QStringLiteral("view box"));
    }
    update();
    return;
  case Drag::None:
    break;
  }
  // The title block editor follows the pointer (mitcad#1625).
  if (m_blockEditor != nullptr) {
    m_blockEditor->moveTo(at);
    update();
    return;
  }
  const int under = viewAt(at);
  const int note = m_pick == SheetPick::None ? annotationAt(at) : 0;
  const bool hoverChanged = !wasIn || under != m_hovered || note != m_hoveredAnnotation;
  m_hovered = under;
  m_hoveredAnnotation = note;
  // A projected view's box beside its parent, a section's or a detail's
  // where it goes (mitcad#725).
  if (((m_placing && (m_pick == SheetPick::Line || m_pick == SheetPick::Circle)) ||
       (m_pick == SheetPick::OutOfView && under == 0)) &&
      !previewBox().isNull()) {
    logPreview(QStringLiteral("view box"));
  }
  // What a click would pick: the workspace asks the model (mitcad#725).
  // A section's line's or a break's second click snaps on the view of its
  // first (mitcad#1362), wherever it is.
  if (asksHover()) {
    emit hoverRequested(m_pick == SheetPick::Line && !m_picks.points.isEmpty() ? m_picks.view : under, at);
  }
  // Where a dimension placed snaps (mitcad#1173); Shift or Alt held:
  // where it is clicked (mitcad#1362).
  if (m_pick == SheetPick::Picks && m_placing) {
    emit placingMoved(at, dragsFreely(event->modifiers()));
  }
  // Painted again only when what shows changed (mitcad#759): the outline
  // under the pointer, or what follows it; the model's hover marks paint
  // when they come (setHoverMarks).
  if (hoverChanged || followsPointer()) {
    update();
  }
}

void DrawingSheetView::enterEvent(QEnterEvent* event) {
  QWidget::enterEvent(event);
  // The pointer over the sheet again without a move, as when a job's
  // progress dialog closes (its showing took the pointer away, leaveEvent):
  // what a click there would pick is asked again (mitcad#753).
  if (asksHover() && m_drag == Drag::None) {
    const QPointF at = toSheet(event->position());
    emit hoverRequested(m_pick == SheetPick::Line && !m_picks.points.isEmpty() ? m_picks.view : viewAt(at), at);
  }
}

void DrawingSheetView::leaveEvent(QEvent* event) {
  QWidget::leaveEvent(event);
  m_pointerIn = false;
  m_hovered = 0;
  m_hoveredAnnotation = 0;
  if (m_pick == SheetPick::Picks || m_pick == SheetPick::Segment || m_pick == SheetPick::ArcPoints ||
      m_pick == SheetPick::Line) {
    emit hoverRequested(-1, QPointF());
  }
  update();
}

void DrawingSheetView::mouseDoubleClickEvent(QMouseEvent* event) {
  if (event->button() != Qt::LeftButton || m_pick != SheetPick::None || m_blockEditor != nullptr) {
    // As a press: a tool takes its clicks as they come.
    mousePressEvent(event);
    return;
  }
  const int note = annotationAt(toSheet(event->position()));
  if (note != 0) {
    qDebug().noquote() << QStringLiteral("Drawing annotation %1 double-clicked").arg(note);
    emit annotationDoubleClicked(note);
    return;
  }
  mousePressEvent(event);
}

void DrawingSheetView::contextMenuEvent(QContextMenuEvent* event) {
  event->accept();
  // The title block editor has no menu: its panel has its tools.
  if (m_pick != SheetPick::None || m_blockEditor != nullptr) {
    return; // a tool's clicks; the right one is its Esc (mouseReleaseEvent)
  }
  // The right click that ended a tool opens no menu (mitcad#1404): where
  // the menu comes after the release (Windows), the tool is gone by then.
  if (event->reason() == QContextMenuEvent::Mouse && m_rightPressTool) {
    return;
  }
  const QPointF at = toSheet(QPointF(event->pos()));
  const int note = annotationAt(at);
  const int under = note == 0 ? viewAt(at) : 0;
  // The menu is for what is under the pointer: selected first, unless it is
  // one of several selected (the menu is then theirs).
  if (note != 0 && !isSelectedAnnotation(note)) {
    selectAnnotation(note);
  } else if (note == 0 && under != 0 && !isSelectedView(under)) {
    selectAnnotation(0);
    selectView(under);
  } else if (note == 0 && under == 0) {
    clearSelection();
  }
  emit contextMenuRequested(at, event->globalPos());
}

void DrawingSheetView::mouseReleaseEvent(QMouseEvent* event) {
  if (event->button() == Qt::RightButton) {
    // A right click while a tool picks is the tool's Esc, without a menu
    // (mitcad#1404, as a sketch's, mitcad#1085): the picks so far (a
    // line's first end, an annotation's picks, the placing click), else
    // the tool. A right drag is not a click. A middle pan or a left drag
    // under way goes on.
    if (m_rightPressTool && m_pick != SheetPick::None) {
      const QPointF moved = event->position() - m_rightPressAt;
      if (std::hypot(moved.x(), moved.y()) <= kRightClickPixels) {
        qDebug().noquote() << QStringLiteral("Drawing right-click: Esc of the tool");
        emit toolCancelled();
      } else {
        qDebug().noquote() << QStringLiteral("Drawing right drag: not a click");
      }
    }
    return;
  }
  const QPointF at = toSheet(event->position());
  // Shift or Alt with the click or at the drag's end: placed freely
  // (mitcad#1173, mitcad#1362).
  m_releaseModifiers = event->modifiers();
  m_dragSnapped = false;
  m_snapHint.clear();
  const Drag drag = m_drag;
  m_drag = Drag::None;
  if (drag == Drag::Pan) {
    logPlaces();
    return;
  }
  if (event->button() != Qt::LeftButton) {
    return;
  }
  // A move of the title block editor ends (mitcad#1625).
  if (m_blockEditor != nullptr) {
    const QString what = m_blockEditor->release(at);
    if (!what.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Title block %1").arg(what);
      emit titleBlockEdited();
    }
    update();
    return;
  }
  // Several selected items dragged together; a click on one of them
  // without a drag selects it alone (mitcad#725).
  if ((drag == Drag::View || drag == Drag::Annotation) && m_dragSelection) {
    m_dragSelection = false;
    if (m_dragMoved) {
      qDebug().noquote() << QStringLiteral("Drawing selection dragged by %1,%2")
                                .arg(m_dragOffset.x(), 0, 'f', 1)
                                .arg(m_dragOffset.y(), 0, 'f', 1);
      emit selectionMoved(m_dragOffset);
    } else if (drag == Drag::View) {
      selectView(m_dragView);
    } else {
      selectAnnotation(m_dragView);
    }
    m_dragMoved = false;
    update();
    return;
  }
  if (drag == Drag::View) {
    if (m_dragMoved) {
      qDebug().noquote() << QStringLiteral("Drawing view %1 dragged by %2,%3")
                                .arg(m_dragView)
                                .arg(m_dragOffset.x(), 0, 'f', 1)
                                .arg(m_dragOffset.y(), 0, 'f', 1);
      emit viewMoved(m_dragView, m_dragOffset);
    }
    m_dragMoved = false;
    update();
    return;
  }
  if (drag == Drag::DetailCircle) {
    QPointF center;
    double radius = 0.0;
    if (m_dragMoved && detailCircle(m_dragView, &center, &radius)) {
      qDebug().noquote() << QStringLiteral("Drawing detail %1 circle dragged by %2,%3")
                                .arg(m_dragView)
                                .arg(m_dragOffset.x(), 0, 'f', 1)
                                .arg(m_dragOffset.y(), 0, 'f', 1);
      emit detailCircleMoved(m_dragView, center + m_dragOffset);
    }
    m_dragMoved = false;
    update();
    return;
  }
  if (drag == Drag::Annotation && m_dragTip) {
    // A datum's triangle or a frame's arrow: its tip slides along its
    // edge, the square or the frame following (mitcad#1295).
    m_dragTip = false;
    if (m_dragMoved) {
      qDebug().noquote() << QStringLiteral("Drawing annotation %1 tip dragged by %2,%3")
                                .arg(m_dragView)
                                .arg(m_dragOffset.x(), 0, 'f', 1)
                                .arg(m_dragOffset.y(), 0, 'f', 1);
      emit annotationTipMoved(m_dragView, m_dragOffset);
    }
    m_dragMoved = false;
    update();
    return;
  }
  if (drag == Drag::Annotation) {
    if (m_dragMoved) {
      qDebug().noquote() << QStringLiteral("Drawing annotation %1 dragged by %2,%3")
                                .arg(m_dragView)
                                .arg(m_dragOffset.x(), 0, 'f', 1)
                                .arg(m_dragOffset.y(), 0, 'f', 1);
      emit annotationMoved(m_dragView, m_dragOffset);
    }
    m_dragMoved = false;
    update();
    return;
  }
  const int under = viewAt(at);
  if (m_placing) {
    // The click that places the view of the line or circle drawn.
    ToolPicks picks = m_picks;
    picks.points.append(at);
    resetPicks();
    emit picked(picks);
    m_dragMoved = false;
    update();
    return;
  }
  switch (m_pick) {
  case SheetPick::None:
    break;
  case SheetPick::Point:
    emit picked(ToolPicks{under, {at}, 0.0, {}, {}, {}});
    break;
  case SheetPick::OutOfView:
    if (drag == Drag::OutOfView) {
      const SheetView* from = view(m_dragView);
      const bool outside = from != nullptr && !from->box.adjusted(-2.0, -2.0, 2.0, 2.0).contains(at);
      if (m_dragMoved && outside) {
        // Dragged out of the view: one projected from it, there.
        m_parentView = m_dragView;
        emit picked(ToolPicks{m_dragView, {at}, 0.0, {}, {}, {}});
      } else {
        // A click on a view: the next views are projected from it.
        setParentView(m_dragView);
        qDebug().noquote() << QStringLiteral("Drawing projected views from view %1").arg(m_dragView);
      }
    } else if (m_parentView != 0 && under == 0) {
      emit picked(ToolPicks{m_parentView, {at}, 0.0, {}, {}, {}});
    }
    break;
  case SheetPick::Line: {
    const bool first = m_picks.points.isEmpty();
    if (first && under == 0) {
      break; // on a view only
    }
    // Each click snaps as a line's do, on the view of the first
    // (mitcad#1362); a section's second, near nothing, to a multiple of 45
    // degrees from its first; Shift (or Alt): where it is clicked.
    const int on = first ? under : m_picks.view;
    const bool freely = dragsFreely(m_releaseModifiers);
    std::optional<QPointF> snap;
    if (!freely && m_pointSnap) {
      snap = m_pointSnap(on, at);
    }
    double angle = -1.0;
    const QPointF point = lineEnd(at, snap, freely, &angle);
    if (angle >= 0.0) {
      qDebug().noquote() << QStringLiteral("Drawing snap: section line at %1 degrees").arg(angle, 0, 'f', 0);
    }
    if (first) {
      m_picks.view = under;
    }
    m_picks.points.append(point);
    m_hoverMarks = QJsonArray();
    qDebug().noquote() << QStringLiteral("Drawing point %1 at %2,%3%4")
                              .arg(m_picks.points.size())
                              .arg(point.x(), 0, 'f', 1)
                              .arg(point.y(), 0, 'f', 1)
                              .arg(snap ? QStringLiteral(" (snapped)")
                                   : angle >= 0.0
                                       ? QStringLiteral(" (at %1 degrees)").arg(angle, 0, 'f', 0)
                                       : QString());
    if (!first) {
      if (m_place) {
        startPlacing();
        break;
      }
      const ToolPicks picks = m_picks;
      resetPicks();
      emit picked(picks);
    }
    break;
  }
  case SheetPick::Circle:
    if (m_picks.points.isEmpty()) {
      if (under == 0) {
        break;
      }
      m_picks.view = under;
      m_picks.points.append(at);
    } else {
      const double radius = distance(m_picks.points.first(), at);
      if (!(radius > 0.0)) {
        break;
      }
      m_picks.radius = radius;
      if (m_place) {
        startPlacing();
        break;
      }
      const ToolPicks picks = m_picks;
      resetPicks();
      emit picked(picks);
    }
    break;
  case SheetPick::Picks:
    // The workspace asks the model what the click picks.
    emit pickRequested(under, at);
    break;
  case SheetPick::ViewThenPlace:
    // The view (a table's), then where it goes.
    if (under == 0) {
      break;
    }
    m_picks.view = under;
    m_picks.points.append(at);
    qDebug().noquote() << QStringLiteral("Drawing picked view %1").arg(under);
    startPlacing();
    break;
  case SheetPick::Segment:
  case SheetPick::ArcPoints: {
    // Anywhere on the sheet (mitcad#729): the view under the first click
    // is the one it may be drawn in.
    if (m_picks.points.isEmpty()) {
      m_picks.view = under;
    }
    // On a view it snaps to an edge's end or middle or a centre near it
    // (mitcad#1272); Shift, or Alt (mitcad#1362): where it is clicked.
    QPointF point = at;
    bool snapped = false;
    if (under != 0 && m_pointSnap && !dragsFreely(m_releaseModifiers)) {
      if (const std::optional<QPointF> to = m_pointSnap(under, at)) {
        point = *to;
        snapped = true;
      }
    }
    m_picks.points.append(point);
    m_hoverMarks = QJsonArray();
    qDebug().noquote() << QStringLiteral("Drawing point %1 at %2,%3%4")
                              .arg(m_picks.points.size())
                              .arg(point.x(), 0, 'f', 1)
                              .arg(point.y(), 0, 'f', 1)
                              .arg(snapped ? QStringLiteral(" (snapped)") : QString());
    const qsizetype needed = m_pick == SheetPick::Segment ? 2 : 3;
    if (m_picks.points.size() >= needed) {
      const ToolPicks picks = m_picks;
      resetPicks();
      emit picked(picks);
    }
    break;
  }
  }
  m_dragMoved = false;
  update();
}

void DrawingSheetView::wheelEvent(QWheelEvent* event) {
  const double steps = event->angleDelta().y() / 120.0;
  if (steps == 0.0) {
    return;
  }
  // About the point under the pointer.
  const QPointF under = toSheet(event->position());
  m_zoom = std::clamp(m_zoom * std::pow(1.15, steps), 0.05, 200.0);
  m_origin = QPointF(event->position().x() - under.x() * m_zoom, event->position().y() + under.y() * m_zoom);
  m_fitted = false;
  logPlaces();
  sheetMoved();
}

bool DrawingSheetView::event(QEvent* event) {
  // The sheet's keys before the window's shortcuts (Delete, Esc, Home),
  // without modifiers only: Ctrl+Backspace and the like stay the window's.
  if (event->type() == QEvent::ShortcutOverride) {
    const auto* key = static_cast<QKeyEvent*>(event);
    const int k = key->key();
    if (unmodified(key) &&
        (k == Qt::Key_Delete || k == Qt::Key_Backspace || k == Qt::Key_Escape || k == Qt::Key_Home)) {
      event->accept();
      return true;
    }
  }
  return QWidget::event(event);
}

bool DrawingSheetView::cancelPicks() {
  if (m_picks.points.isEmpty() && !m_placing && m_pickMarks.isEmpty()) {
    return false;
  }
  resetPicks();
  update();
  return true;
}

void DrawingSheetView::keyReleaseEvent(QKeyEvent* event) {
  // Alt or Shift let go: a dimension placed or dragged snaps again
  // (mitcad#1173, mitcad#1294; Shift for a placing click too since
  // mitcad#1362), a section's line follows its snap again.
  if ((event->key() == Qt::Key_Alt || event->key() == Qt::Key_Shift) && !event->isAutoRepeat()) {
    askSnap(false);
    if (followsPointer()) {
      update();
    }
  }
  QWidget::keyReleaseEvent(event);
}

void DrawingSheetView::keyPressEvent(QKeyEvent* event) {
  // Alt or Shift held: a dimension placed or dragged goes where the
  // pointer is (mitcad#1173, mitcad#1294, mitcad#1362), a section's line
  // runs to the pointer.
  if ((event->key() == Qt::Key_Alt || event->key() == Qt::Key_Shift) && !event->isAutoRepeat()) {
    askSnap(true);
    if (followsPointer()) {
      update();
    }
  }
  if (!unmodified(event)) {
    QWidget::keyPressEvent(event);
    return;
  }
  switch (event->key()) {
  case Qt::Key_Delete:
  case Qt::Key_Backspace:
    if (m_blockEditor != nullptr) {
      const QString gone = m_blockEditor->deleteSelected();
      if (!gone.isEmpty()) {
        qDebug().noquote() << QStringLiteral("Title block %1 deleted").arg(gone);
        emit titleBlockEdited();
        update();
      }
      return;
    }
    if (selectionSize() > 1 && m_pick == SheetPick::None) {
      emit deleteSelectionRequested();
      return;
    }
    if (m_selectedAnnotation != 0 && m_pick == SheetPick::None) {
      emit annotationDeleteRequested(m_selectedAnnotation);
      return;
    }
    if (m_selected != 0 && m_pick == SheetPick::None) {
      emit deleteRequested(m_selected);
      return;
    }
    break;
  case Qt::Key_Escape:
    // The workspace says what goes: the picks so far, the tool, the
    // selection or the drawing.
    emit toolCancelled();
    return;
  case Qt::Key_Home:
    fit();
    return;
  default:
    break;
  }
  QWidget::keyPressEvent(event);
}

} // namespace mitcad::drawings
