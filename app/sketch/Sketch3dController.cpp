// SPDX-License-Identifier: MIT
#include "Sketch3dController.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <utility>
#include <vector>

#include <QCoreApplication>
#include <QEvent>
#include <QFontMetricsF>
#include <QHash>
#include <QJsonArray>
#include <QMouseEvent>
#include <QPaintEvent>
#include <QPainter>
#include <QPainterPath>
#include <QPen>
#include <QPolygonF>
#include <QRectF>
#include <QResizeEvent>
#include <QSizeF>
#include <QStringList>
#include <QWidget>
#include <QtLogging>

#include "../OcctViewer.hpp"
#include "../framework/NoticePlaces.hpp"
#include "../framework/Theme.hpp"
#include "../framework/UiSizes.hpp"
#include "SketchController.hpp"

namespace mitcad::sketch {
namespace {

constexpr double kAxisPixels = 48.0;   // the frame's axes on the screen

double length(const QPointF& v) { return std::hypot(v.x(), v.y()); }

QString windowPlace(const QPointF& at, const QPoint& window) {
  return QStringLiteral("%1,%2").arg(qRound(at.x() + window.x())).arg(qRound(at.y() + window.y()));
}

// A box in the window, "x0,y0 to x1,y1", for UI tests.
QString windowBox(const QRectF& box, const QPoint& window) {
  return QStringLiteral("%1 to %2").arg(windowPlace(box.topLeft(), window), windowPlace(box.bottomRight(), window));
}

// The X, Y and Z axes' roles (0, 1, 2), as the view's triad.
QColor axisColor(int axis, bool dark) {
  static const ColorRole roles[] = {ColorRole::AxisX, ColorRole::AxisY, ColorRole::AxisZ};
  return viewColor(roles[axis], dark);
}

} // namespace

// What the controller draws over the view; it takes no mouse input.
class Sketch3dOverlay : public QWidget {
public:
  Sketch3dOverlay(Sketch3dController& controller, QWidget* view) : QWidget(view), m_c(controller) {
    setObjectName(QStringLiteral("sketch3dOverlay"));
    setAttribute(Qt::WA_TransparentForMouseEvents);
    setAttribute(Qt::WA_NoSystemBackground);
    setFocusPolicy(Qt::NoFocus);
    QFont small = font();
    small.setPointSizeF(9.0 * uiTextScale()); // at Text size (mitcad#1556)
    setFont(small);
  }

  struct Axis {
    QString name;
    QPointF from;
    QPointF to;
    bool endOn = false; // seen along it: a dot
  };
  // One `In plane` badge per shape (mitcad#1592): the relations and
  // dimensions it marks ("k1,k2,k5"), its box and the shape's on the screen.
  struct Badge {
    QString id;
    QPointF at; // the middle of its box
    QRectF box;
    QRectF shape;
  };
  struct Handle {
    QString spline;
    QString point;
    QPointF from;
    QPointF to;
  };

  QPointF onScreen(const Xyz& local) const { return m_c.viewer().toWidgetF(m_c.designPoint(local)); }

  // The frame's origin and its X, Y and Z axes, as long on the screen
  // whatever the zoom.
  std::vector<Axis> axes() const {
    std::vector<Axis> out;
    const QPointF origin = m_c.viewer().toWidgetF(m_c.designOrigin());
    const struct {
      const char* name;
      Xyz along;
    } frame[] = {{"X", {1.0, 0.0, 0.0}}, {"Y", {0.0, 1.0, 0.0}}, {"Z", {0.0, 0.0, 1.0}}};
    for (const auto& axis : frame) {
      const QPointF tip = m_c.viewer().toWidgetF(m_c.designAlong(axis.along, 10.0));
      const QPointF along = tip - origin;
      const double pixels = length(along);
      Axis drawn;
      drawn.name = QString::fromLatin1(axis.name);
      drawn.from = origin;
      drawn.endOn = pixels < 1e-3;
      drawn.to = drawn.endOn ? origin : origin + along * (kAxisPixels / pixels);
      out.push_back(drawn);
    }
    return out;
  }

  // The points that define a point or a curve.
  void entityPoints(const QString& id, QStringList& out) const {
    const Sketch3dModel& model = m_c.model();
    if (model.point(id) != nullptr) {
      out << id;
    } else if (const Sketch3dCurve* curve = model.curve(id)) {
      for (const QString& p : curve->points) {
        entityPoints(p, out);
      }
    }
  }

  // The part of the view a badge of `size` keeps inside: what no panel
  // floats over, or the whole view when that is too small for it.
  QRectF badgeArea(const QSizeF& size) const {
    const QRectF whole = QRectF(rect()).adjusted(2.0, 2.0, -2.0, -2.0);
    const QRectF free = m_c.viewer().uncoveredArea().adjusted(2.0, 2.0, -2.0, -2.0).intersected(whole);
    return free.width() >= size.width() && free.height() >= size.height() ? free : whole;
  }

  static QRectF keptInside(QRectF box, const QRectF& area) {
    if (box.right() > area.right()) {
      box.moveRight(area.right());
    }
    if (box.left() < area.left()) {
      box.moveLeft(area.left());
    }
    if (box.bottom() > area.bottom()) {
      box.moveBottom(area.bottom());
    }
    if (box.top() < area.top()) {
      box.moveTop(area.top());
    }
    return box;
  }

  // A badge's box beside the place `at` (a snap, a drag's target, the
  // pointer): `offset` right of and below it, or on its left or above it
  // where the view or a panel over it would cut it (mitcad#1592: On Face at
  // the view's right edge), and always inside the view.
  QRectF badgeBox(const QPointF& at, const QString& text, const QPointF& offset = QPointF(14.0, 12.0)) const {
    const QFontMetricsF metrics(font());
    QRectF box = metrics.boundingRect(QRectF(0.0, 0.0, 360.0, 200.0), Qt::TextWordWrap, text);
    box.adjust(-5.0, -3.0, 5.0, 3.0);
    box.moveTopLeft(at + offset);
    const QRectF area = badgeArea(box.size());
    if (box.right() > area.right()) {
      box.moveRight(at.x() - offset.x());
    }
    if (box.bottom() > area.bottom()) {
      box.moveBottom(at.y() - offset.y());
    }
    return keptInside(box, area);
  }

  // The `In plane` badges of the relations and dimensions kept in the
  // sketch plane (a converted sketch's, a plane-bound tool's), one per
  // shape (mitcad#1592: a small polygon or a text's frame stacked one per
  // relation over each other and over the text): the points joined by the
  // curves and by those relations are one shape. Each badge is above its
  // shape, else below, left or right of it, clear of the badges placed
  // before it, inside the view.
  std::vector<Badge> badges() const {
    const Sketch3dModel& model = m_c.model();
    QHash<QString, QString> parent;
    const auto root = [&parent](QString id) {
      while (parent.contains(id)) {
        id = parent.value(id);
      }
      return id;
    };
    const auto join = [&parent, &root](const QStringList& points) {
      for (qsizetype i = 1; i < points.size(); ++i) {
        const QString a = root(points.front());
        const QString b = root(points.at(i));
        if (a != b) {
          parent.insert(b, a);
        }
      }
    };
    for (const Sketch3dCurve& curve : model.curves) {
      QStringList points;
      entityPoints(curve.id, points);
      join(points);
    }
    // The badged items by their shape, in the order the list gives them.
    QStringList shapes;
    QHash<QString, QStringList> itemsOf;
    for (const Sketch3dItem& item : model.items) {
      if (!item.inPlane) {
        continue;
      }
      QStringList points;
      for (const QString& entity : item.entities) {
        entityPoints(entity, points);
      }
      if (points.isEmpty()) {
        continue;
      }
      join(points);
      const QString shape = root(points.front());
      if (!itemsOf.contains(shape)) {
        shapes << shape;
      }
      itemsOf[shape] << item.id;
    }
    // Joined later by another item: one badge for the shape they make.
    QStringList merged;
    QHash<QString, QStringList> ids;
    for (const QString& shape : std::as_const(shapes)) {
      const QString at = root(shape);
      if (!ids.contains(at)) {
        merged << at;
      }
      ids[at] << itemsOf.value(shape);
    }
    const QString text = QCoreApplication::translate("Sketch3d", "In plane");
    const QFontMetricsF metrics(font());
    const QSizeF size = metrics.boundingRect(text).adjusted(-4.0, -2.0, 4.0, 2.0).size();
    const QRectF area = badgeArea(size);
    constexpr double kGap = 6.0;
    std::vector<Badge> out;
    for (const QString& shape : std::as_const(merged)) {
      QRectF bounds;
      for (const Sketch3dPoint& point : model.points) {
        if (root(point.id) == shape) {
          // The point's mark with it.
          bounds = bounds.united(QRectF(onScreen(point.at) - QPointF(4.0, 4.0), QSizeF(8.0, 8.0)));
        }
      }
      if (bounds.isNull()) {
        continue;
      }
      const QPointF middle = bounds.center();
      const QRectF candidates[] = {
          QRectF(QPointF(middle.x() - size.width() / 2.0, bounds.top() - kGap - size.height()), size),
          QRectF(QPointF(middle.x() - size.width() / 2.0, bounds.bottom() + kGap), size),
          QRectF(QPointF(bounds.left() - kGap - size.width(), middle.y() - size.height() / 2.0), size),
          QRectF(QPointF(bounds.right() + kGap, middle.y() - size.height() / 2.0), size)};
      const auto clear = [&out, &bounds](const QRectF& box) {
        if (box.intersects(bounds)) {
          return false;
        }
        return std::none_of(out.begin(), out.end(), [&box](const Badge& placed) {
          return placed.box.adjusted(-2.0, -2.0, 2.0, 2.0).intersects(box);
        });
      };
      QRectF chosen;
      for (const QRectF& candidate : candidates) {
        const QRectF box = keptInside(candidate, area);
        if (clear(box)) {
          chosen = box;
          break;
        }
      }
      // Every side taken: further above the shape or below it, past the
      // badges in the way.
      const double step = size.height() + 2.0;
      for (int n = 1; n <= 40 && chosen.isNull(); ++n) {
        const QRectF further[] = {candidates[0].translated(0.0, -n * step), candidates[1].translated(0.0, n * step)};
        for (const QRectF& candidate : further) {
          const QRectF box = keptInside(candidate, area);
          if (clear(box)) {
            chosen = box;
            break;
          }
        }
      }
      if (chosen.isNull()) {
        chosen = keptInside(candidates[0], area);
      }
      out.push_back({ids.value(shape).join(QLatin1Char(',')), chosen.center(), chosen, bounds});
    }
    return out;
  }

  // The control polygons of the control-point splines.
  std::vector<QPolygonF> controlPolygons() const {
    std::vector<QPolygonF> out;
    const Sketch3dModel& model = m_c.model();
    for (const Sketch3dCurve& curve : model.curves) {
      if (curve.type != QStringLiteral("spline")) {
        continue;
      }
      QPolygonF polygon;
      for (const QJsonValue& id : curve.json.value(QStringLiteral("control")).toArray()) {
        if (const Sketch3dPoint* point = model.point(id.toString())) {
          polygon << onScreen(point->at);
        }
      }
      if (polygon.size() >= 2) {
        out.push_back(polygon);
      }
    }
    return out;
  }

  // The end tangent handles of the open fit-point splines, at an end that
  // is free or held tangent by a relation: from the end to its tip, a
  // third of the end tangent C'(t) along the curve's direction there
  // (mitcad#1526: its direction and length are what a drag of the tip
  // changes).
  std::vector<Handle> handles() const {
    std::vector<Handle> out;
    for (const TangentHandle& handle : tangentHandles(m_c.model())) {
      out.push_back({handle.curve, handle.point, onScreen(handle.at), onScreen(handle.tip)});
    }
    return out;
  }

protected:
  void paintEvent(QPaintEvent* /*event*/) override {
    if (!m_c.isActive()) {
      return;
    }
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QColor halo = m_c.viewer().sketchHaloColor();
    const bool dark = halo.lightnessF() < 0.5;
    // The roles of the 2D sketch's overlay (mitcad#1041): its dimensions'
    // ink, by the view's background.
    const QColor ink = viewColor(ColorRole::SketchDimension, dark);
    // The frame: X red, Y green, Z blue, as the view's triad.
    if (m_c.model().hasFrame) {
      const QColor colours[] = {axisColor(0, dark), axisColor(1, dark), axisColor(2, dark)};
      int index = 0;
      for (const Axis& axis : axes()) {
        const QColor colour = colours[index++];
        painter.setPen(QPen(colour, 2.0));
        if (axis.endOn) {
          painter.setBrush(colour);
          painter.drawEllipse(axis.from, 3.0, 3.0);
        } else {
          painter.drawLine(axis.from, axis.to);
          const QPointF label = axis.to + (axis.to - axis.from) * (8.0 / kAxisPixels) - QPointF(4.0, -4.0);
          painter.drawText(label, axis.name);
        }
      }
      painter.setBrush(Qt::NoBrush);
    }
    // Control polygons, dashed.
    painter.setPen(QPen(ink, 1.0, Qt::DashLine));
    for (const QPolygonF& polygon : controlPolygons()) {
      painter.drawPolyline(polygon);
    }
    // End tangent handles: a line with a round grip.
    painter.setPen(QPen(ink, 1.5));
    for (const Handle& handle : handles()) {
      painter.drawLine(handle.from, handle.to);
      painter.drawEllipse(handle.to, 3.5, 3.5);
    }
    // Points On Sketch Plane keeps: a square around each.
    for (const Sketch3dPoint& point : m_c.model().points) {
      if (point.onSketchPlane) {
        painter.drawRect(QRectF(onScreen(point.at) - QPointF(6.0, 6.0), QSizeF(12.0, 12.0)));
      }
    }
    // `In plane` badges, one per shape.
    const QString inPlane = QCoreApplication::translate("Sketch3d", "In plane");
    for (const Badge& badge : badges()) {
      painter.setPen(QPen(ink, 1.0));
      painter.setBrush(QColor(halo.red(), halo.green(), halo.blue(), 220));
      painter.drawRoundedRect(badge.box, 4.0, 4.0);
      painter.drawText(badge.box, Qt::AlignCenter, inPlane);
    }
    // A drawing tool's preview (mitcad#1525).
    if (m_c.toolActive()) {
      paintTool(painter, ink, halo);
      return;
    }
    // A drag's preview and a source pick (mitcad#1526).
    if (m_c.dragging()) {
      paintDrag(painter, ink, halo);
      return;
    }
    if (m_c.sourcePick() != Sketch3dController::SourcePick::None) {
      paintSource(painter, ink, halo);
      return;
    }
    // What is under the pointer, beside it.
    if (!m_c.hoverText().isEmpty() && m_c.pointerInView()) {
      const QRectF box = badgeBox(m_c.pointer(), m_c.hoverText(), QPointF(16.0, 18.0));
      painter.setPen(QPen(ink, 1.0));
      painter.setBrush(QColor(halo.red(), halo.green(), halo.blue(), 230));
      painter.drawRoundedRect(box, 4.0, 4.0);
      painter.drawText(box.adjusted(5.0, 3.0, -5.0, -3.0), Qt::TextWordWrap, m_c.hoverText());
    }
  }

private:
  // A text box beside a place, as the tools' badge.
  void paintBadge(QPainter& painter, const QColor& ink, const QColor& halo, const QPointF& at, const QString& text,
                  const QColor& border) {
    if (text.isEmpty()) {
      return;
    }
    const QRectF box = badgeBox(at, text);
    painter.setPen(QPen(border.isValid() ? border : ink, 1.0));
    painter.setBrush(QColor(halo.red(), halo.green(), halo.blue(), 230));
    painter.drawRoundedRect(box, 4.0, 4.0);
    painter.setPen(QPen(ink, 1.0));
    painter.drawText(box.adjusted(5.0, 3.0, -5.0, -3.0), Qt::TextWordWrap, text);
  }

  // The curves as the drag would leave them, the points pulled, and a
  // dashed line from the place reached to the pointer's target when the
  // relations stop it short (mitcad#1526).
  void paintDrag(QPainter& painter, const QColor& ink, const QColor& halo) {
    const Sketch3dController::DragPreview preview = m_c.dragPreview();
    const bool dark = halo.lightnessF() < 0.5;
    const QColor drawColour = viewColor(ColorRole::SketchPreview, dark);
    painter.setPen(QPen(drawColour, 2.0));
    painter.setBrush(Qt::NoBrush);
    for (const std::vector<Xyz>& curve : preview.curves) {
      QPolygonF line;
      for (const Xyz& p : curve) {
        line << onScreen(p);
      }
      painter.drawPolyline(line);
    }
    painter.setBrush(drawColour);
    for (const Xyz& p : preview.points) {
      painter.drawEllipse(onScreen(p), 3.5, 3.5);
    }
    painter.setBrush(Qt::NoBrush);
    if (!preview.target) {
      return;
    }
    const QPointF target = onScreen(*preview.target);
    const QColor mark =
        preview.refused ? viewColor(ColorRole::ErrorBorder, dark) : viewColor(ColorRole::Handle, dark);
    if (preview.reached && preview.shortOf) {
      painter.setPen(QPen(ink, 1.0, Qt::DashLine));
      painter.drawLine(onScreen(*preview.reached), target);
    }
    painter.setPen(QPen(mark, 1.5));
    painter.drawEllipse(target, 4.0, 4.0);
    paintBadge(painter, ink, halo, target, m_c.dragText(), preview.refused ? mark : QColor());
  }

  // The source a click would take, with its badge.
  void paintSource(QPainter& painter, const QColor& ink, const QColor& halo) {
    const std::optional<SnapTarget> snap = m_c.sourceSnap();
    if (!snap) {
      return;
    }
    // A snap, as the 2D sketch marks one.
    const QColor snapColour = viewColor(ColorRole::Handle, halo.lightnessF() < 0.5);
    const QPointF at = onScreen(snap->at);
    painter.setPen(QPen(snapColour, 2.0));
    painter.setBrush(Qt::NoBrush);
    painter.drawRect(QRectF(at - QPointF(5.0, 5.0), QSizeF(10.0, 10.0)));
    paintBadge(painter, ink, halo, at, snapBadge(*snap), QColor());
  }

  // The temporary plane and axes through the anchor, the curve the next
  // click makes, the pending points, the next point with its snap badge
  // and a drop line to the plane for depth.
  void paintTool(QPainter& painter, const QColor& ink, const QColor& halo) {
    const Sketch3dPreview preview = m_c.preview();
    const bool dark = halo.lightnessF() < 0.5;
    const QColor axisColours[] = {axisColor(0, dark), axisColor(1, dark), axisColor(2, dark)};
    const QColor drawColour = viewColor(ColorRole::SketchPreview, dark);
    const QColor constructionColour = viewColor(ColorRole::SketchConstruction, dark);
    const auto polygon = [this](const std::vector<Xyz>& points) {
      QPolygonF out;
      for (const Xyz& p : points) {
        out << onScreen(p);
      }
      return out;
    };
    for (const Sketch3dPreview::Path& path : preview.paths) {
      const QPolygonF line = polygon(path.points);
      if (line.size() < 2) {
        continue;
      }
      const QColor axis = path.axis >= 0 && path.axis < 3 ? axisColours[path.axis] : ink;
      switch (path.style) {
      case Sketch3dPreview::Style::Plane:
        painter.setPen(QPen(QColor(axis.red(), axis.green(), axis.blue(), 120), 1.0));
        painter.setBrush(QColor(axis.red(), axis.green(), axis.blue(), 28));
        painter.drawPolygon(line);
        painter.setBrush(Qt::NoBrush);
        break;
      case Sketch3dPreview::Style::Axis:
        painter.setPen(QPen(QColor(axis.red(), axis.green(), axis.blue(), path.active ? 255 : 140),
                            path.active ? 2.0 : 1.0, path.active ? Qt::SolidLine : Qt::DashLine));
        painter.drawPolyline(line);
        break;
      case Sketch3dPreview::Style::Ring:
        // A rotation handle (mitcad#1661): strong under the pointer or
        // while it is dragged.
        painter.setPen(QPen(QColor(axis.red(), axis.green(), axis.blue(), path.active ? 255 : 170),
                            path.active ? 3.0 : 1.5));
        painter.drawPolyline(line);
        break;
      case Sketch3dPreview::Style::Guide:
        painter.setPen(QPen(ink, 1.0, Qt::DotLine));
        painter.drawPolyline(line);
        break;
      case Sketch3dPreview::Style::Construction:
        painter.setPen(QPen(constructionColour, 1.5, Qt::DashLine));
        path.closed ? painter.drawPolygon(line) : painter.drawPolyline(line);
        break;
      case Sketch3dPreview::Style::Draw:
        painter.setPen(QPen(drawColour, 2.0));
        path.closed ? painter.drawPolygon(line) : painter.drawPolyline(line);
        break;
      }
    }
    painter.setPen(QPen(drawColour, 1.5));
    painter.setBrush(drawColour);
    for (const Xyz& p : preview.placed) {
      painter.drawEllipse(onScreen(p), 3.5, 3.5);
    }
    painter.setBrush(Qt::NoBrush);
    if (!preview.hover) {
      return;
    }
    const QPointF at = onScreen(*preview.hover);
    const QColor mark =
        preview.conflict ? viewColor(ColorRole::ErrorBorder, dark) : viewColor(ColorRole::Handle, dark);
    painter.setPen(QPen(mark, 2.0));
    if (preview.snapped) {
      painter.drawRect(QRectF(at - QPointF(5.0, 5.0), QSizeF(10.0, 10.0)));
    } else {
      painter.drawEllipse(at, 4.0, 4.0);
    }
    QString text = preview.badge;
    if (!preview.unstable.isEmpty()) {
      text = text.isEmpty() ? preview.unstable : text + QLatin1Char('\n') + preview.unstable;
    }
    if (text.isEmpty()) {
      return;
    }
    const QRectF box = badgeBox(at, text);
    painter.setPen(QPen(preview.conflict ? mark : ink, 1.0));
    painter.setBrush(QColor(halo.red(), halo.green(), halo.blue(), 230));
    painter.drawRoundedRect(box, 4.0, 4.0);
    painter.drawText(box.adjusted(5.0, 3.0, -5.0, -3.0), Qt::TextWordWrap, text);
  }

  Sketch3dController& m_c;
};

Sketch3dController::Sketch3dController(SketchHost& host, OcctViewer& viewer, QObject* parent)
    : QObject(parent), m_host(host), m_viewer(viewer) {
  m_overlay = new Sketch3dOverlay(*this, &viewer);
  m_overlay->hide();
  viewer.installEventFilter(this);
  // The overlay follows the camera; the places are logged once it rests.
  connect(&viewer, &OcctViewer::viewChanged, m_overlay, [this] {
    if (isActive()) {
      m_overlay->update();
    }
  });
  // Over a panel floating over the view, or out of it, a tool's pointer
  // snaps to nothing: typed values go from the place it left.
  connect(&viewer, &OcctViewer::pointerLeft, this, [this] {
    m_pointerInView = false;
    if (isActive() && toolActive()) {
      updateHover();
    }
  });
  connect(&viewer, &OcctViewer::cameraRested, this, [this] {
    if (isActive()) {
      logPlaces();
      // A tool's place follows the turned view with the same plane or axis
      // (mitcad#1525); nothing is committed by the navigation. The rotation
      // handles' places are logged again (mitcad#1661).
      if (toolActive()) {
        updateHover();
        logHandles();
      }
    }
  });
}

Sketch3dController::~Sketch3dController() = default;

void Sketch3dController::enter(const QString& uid) {
  m_uid = uid;
  m_loggedStatus.clear();
  m_loggedPlaces.clear();
  m_loggedHover.clear();
  m_hoverText.clear();
  // The manipulator starts at the origin with XY, each time (mitcad#1525):
  // it is not part of the design.
  m_place = Placement(); // not rotated (mitcad#1661)
  m_rotation = RotationDrag();
  m_ringHover.reset();
  m_anchor = Xyz{};
  m_anchorPoint.clear();
  m_placed.reset();
  reload();
  if (m_model.hasFrame) {
    const auto text = [](const gp_Pnt& p) {
      const auto number = [](double v) { return QString::number(std::abs(v) < 5e-7 ? 0.0 : v, 'f', 6); };
      return QStringLiteral("(%1, %2, %3)").arg(number(p.X()), number(p.Y()), number(p.Z()));
    };
    const gp_Pnt origin = designOrigin();
    const gp_Pnt x = designAlong({1.0, 0.0, 0.0}, 1.0);
    const gp_Pnt n = designAlong({0.0, 0.0, 1.0}, 1.0);
    qDebug().noquote() << QStringLiteral("3D sketch frame in the design: origin %1, x axis %2, normal %3")
                              .arg(text(origin),
                                   text(gp_Pnt(x.XYZ() - origin.XYZ())),
                                   text(gp_Pnt(n.XYZ() - origin.XYZ())));
  } else {
    qDebug().noquote() << QStringLiteral("3D sketch frame in the design: none (%1)").arg(m_model.failureDetail);
  }
  m_overlay->resize(m_viewer.size());
  m_overlay->show();
  m_overlay->raise();
  refreshView();
}

void Sketch3dController::leave() {
  stopTool();
  cancelDrag();
  cancelSourcePick();
  m_pendingDelete.clear();
  m_pendingDependents.clear();
  m_editNotice.clear();
  setPaletteHover(QString()); // what a row painted goes with the sketch
  paintSnap(SnapTarget());    // and a snap's source (mitcad#1653)
  m_overlay->hide();
  m_uid.clear();
  m_model = Sketch3dModel();
  m_hoverText.clear();
  m_pointerInView = false;
  emit changed();
}

void Sketch3dController::reload() {
  if (!isActive()) {
    return;
  }
  try {
    m_model = Sketch3dModel::read(
        m_host.queryObject({{QStringLiteral("query"), QStringLiteral("sketch3d")}, {QStringLiteral("uid"), m_uid}}));
  } catch (const std::exception&) {
    // The sketch was just taken back; the main window leaves sketch mode.
    m_model = Sketch3dModel();
    m_model.uid = m_uid;
  }
  // Where the view shows its component (mitcad#505): applied once, to the
  // frame; the sketch's own coordinates stay.
  m_placement = m_host.sketchPlacement(m_uid);
  const QString status = statusLog(m_model);
  if (status != m_loggedStatus) {
    m_loggedStatus = status;
    qDebug().noquote() << status;
  }
  // What the snaps sampled may have moved (mitcad#1525).
  m_polylines.clear();
  m_bodyEdges.clear();
  // A delete waiting for its dependents, or a record being replaced, that
  // went with the change (mitcad#1526).
  if (std::any_of(m_pendingDelete.begin(), m_pendingDelete.end(),
                  [this](const QString& id) { return m_model.point(id) == nullptr && m_model.curve(id) == nullptr; })) {
    m_pendingDelete.clear();
    m_pendingDependents.clear();
    m_editNotice.clear();
  }
  if (m_sourcePick == SourcePick::Replace && m_model.reference(m_sourcePickItem) == nullptr) {
    cancelSourcePick();
  }
  // The palette's row under the pointer paints what it is now, or nothing
  // when it went (mitcad#1652).
  if (!m_paletteHover.isEmpty()) {
    setPaletteHover(m_paletteHover);
  }
  // No anchor, chain or pending point stays on a point that went (an
  // undo, an agent's command); the anchor follows its point's solve.
  if (!m_anchorPoint.isEmpty()) {
    if (const Sketch3dPoint* anchor = m_model.point(m_anchorPoint)) {
      m_anchor = anchor->at;
    } else {
      m_anchorPoint.clear();
      m_anchor = Xyz{};
    }
  }
  if (toolActive()) {
    const bool chainGone = !m_chainPoint.isEmpty() && m_model.point(m_chainPoint) == nullptr;
    const bool pendingGone = std::any_of(m_pending.begin(), m_pending.end(), [this](const PlacedPoint& p) {
      return !p.snap.point.isEmpty() && m_model.point(p.snap.point) == nullptr;
    });
    if (chainGone || pendingGone) {
      m_pending.clear();
      m_chainPoint.clear();
      m_chainCurve.clear();
      toolLog(QStringLiteral("3D sketch tool: its pending points went with the model's change"));
    }
    // Fillet's picked corner as the sketch has it now, or gone with it
    // (mitcad#1548).
    if (m_filletCorner) {
      m_filletCorner = m_filletCorner->byLines
                           ? filletCornerOfLines(m_model, m_filletCorner->first, m_filletCorner->second)
                           : filletCornerAt(m_model, m_filletCorner->point);
    }
    if (!m_filletLine.isEmpty() && m_model.curve(m_filletLine) == nullptr) {
      m_filletLine.clear();
    }
    updateHover();
  }
  emit changed();
  refreshView();
}

void Sketch3dController::setHover(const SelectionItem& item) {
  QString text;
  QString detail;
  if (isActive() && item.isValid() && item.owner == m_uid &&
      (item.kind == SelectKind::SketchPoint || item.kind == SelectKind::SketchCurve)) {
    // In words; the ids and coordinates are the log's (mitcad#1652).
    text = describeEntity(m_model, item.name, m_hooks.names);
    detail = entityDetail(m_model, item.name, unit(), millimetres());
  }
  if (text == m_hoverText) {
    return;
  }
  m_hoverText = text;
  const QString logged = QStringLiteral("%1 | shown: %2").arg(detail, text);
  if (logged != m_loggedHover && !text.isEmpty()) {
    m_loggedHover = logged;
    qDebug().noquote() << QStringLiteral("3D sketch hover: %1").arg(logged);
  }
  m_overlay->update();
  emit changed();
}

QStringList Sketch3dController::selectionTexts() const {
  QStringList texts;
  if (!isActive()) {
    return texts;
  }
  for (const SelectionItem& item : m_host.selection()) {
    if (item.owner != m_uid) {
      continue;
    }
    const QString text = describeEntity(m_model, item.name, m_hooks.names);
    if (!text.isEmpty()) {
      texts << text;
    }
  }
  return texts;
}

QStringList Sketch3dController::selectionDetails() const {
  QStringList texts;
  if (!isActive()) {
    return texts;
  }
  for (const SelectionItem& item : m_host.selection()) {
    if (item.owner != m_uid) {
      continue;
    }
    const QString text = entityDetail(m_model, item.name, unit(), millimetres());
    if (!text.isEmpty()) {
      texts << text;
    }
  }
  return texts;
}

void Sketch3dController::setPaletteHover(const QString& id) {
  // Its places as the sketch has them now: an edit, an undo or an agent's
  // command may have moved the source or taken the row away.
  const QString hovered = isActive() ? id : QString();
  const QJsonArray places = hovered.isEmpty() ? QJsonArray() : entityPlaces(m_model, hovered);
  const QString shown = places.isEmpty() ? QString() : hovered;
  if (shown == m_paletteHover && places == m_paletteHoverPlaces) {
    return;
  }
  m_paletteHover = shown;
  m_paletteHoverPlaces = places;
  qDebug().noquote() << QStringLiteral("3D sketch palette hover: %1")
                            .arg(shown.isEmpty() ? QStringLiteral("none")
                                                 : QStringLiteral("%1: %2").arg(shown, placesText(places)));
  if (m_hooks.paintPlaces) {
    m_hooks.paintPlaces(QStringLiteral("3D Sketch Palette"), places);
  }
}

void Sketch3dController::paintSnap(const SnapTarget& snap) {
  const QJsonArray places = isActive() ? snapPlaces(snap) : QJsonArray();
  if (places == m_snapPlaces) {
    return;
  }
  m_snapPlaces = places;
  qDebug().noquote() << QStringLiteral("3D sketch snap places: %1").arg(placesText(places));
  if (m_hooks.paintPlaces) {
    m_hooks.paintPlaces(QStringLiteral("3D sketch snap"), places);
  }
}

void Sketch3dController::refreshView() {
  if (!isActive()) {
    return;
  }
  m_overlay->update();
  logPlaces();
}

QString Sketch3dController::unit() const { return m_host.lengthUnit(); }

double Sketch3dController::millimetres() const { return m_host.unitMillimetres(); }

gp_Pnt Sketch3dController::designPoint(const Xyz& local) const {
  const Sketch3dModel& m = m_model;
  gp_Pnt point(m.origin[0] + m.xAxis[0] * local[0] + m.yAxis[0] * local[1] + m.normal[0] * local[2],
               m.origin[1] + m.xAxis[1] * local[0] + m.yAxis[1] * local[1] + m.normal[1] * local[2],
               m.origin[2] + m.xAxis[2] * local[0] + m.yAxis[2] * local[1] + m.normal[2] * local[2]);
  point.Transform(m_placement);
  return point;
}

gp_Pnt Sketch3dController::designOrigin() const { return designPoint({0.0, 0.0, 0.0}); }

gp_Pnt Sketch3dController::designAlong(const Xyz& local, double size) const {
  return designPoint({local[0] * size, local[1] * size, local[2] * size});
}

bool Sketch3dController::eventFilter(QObject* watched, QEvent* event) {
  if (watched == &m_viewer) {
    switch (event->type()) {
    case QEvent::Resize:
      m_overlay->resize(static_cast<QResizeEvent*>(event)->size());
      break;
    case QEvent::MouseMove:
      // Where the hover's text goes; the view keeps the event.
      m_pointer = static_cast<QMouseEvent*>(event)->position();
      // Over a card floating over the view the pointer is not in the view
      // (mitcad#1525: a tool snaps nothing there).
      m_pointerInView = !m_viewer.isCovered(m_pointer);
      m_hasPointer = true;
      if (isActive() && !m_hoverText.isEmpty()) {
        m_overlay->update();
      }
      break;
    case QEvent::Leave:
      m_pointerInView = false;
      if (isActive()) {
        if (toolActive()) {
          updateHover(); // no snap where the pointer left
        }
        m_overlay->update();
      }
      break;
    default:
      break;
    }
    // While a tool draws, the left button is its own (mitcad#1525); with
    // none, a press on a point, a curve or a tangent handle drags it, and
    // a source pick takes the click (mitcad#1526).
    if (isActive() && toolActive() && toolMouse(event)) {
      return true;
    }
    if (isActive() && !toolActive() && editMouse(event)) {
      return true;
    }
  }
  return QObject::eventFilter(watched, event);
}

void Sketch3dController::updateOverlay() { m_overlay->update(); }

QString Sketch3dController::badgePlace(const Xyz& local, const QString& text) const {
  const QPoint window = m_viewer.mapTo(m_viewer.window(), QPoint(0, 0));
  const QRectF box = m_overlay->badgeBox(m_overlay->onScreen(local), text);
  const QRectF view = QRectF(m_overlay->rect());
  return QStringLiteral("at %1, %2 the view %3")
      .arg(windowBox(box, window), view.contains(box) ? QStringLiteral("inside") : QStringLiteral("cut by"),
           windowBox(view, window));
}

void Sketch3dController::logPlaces() {
  // The frame's axes and the badges and handles the overlay draws, in the
  // window, for UI tests (also where the view never paints).
  const QPoint window = m_viewer.mapTo(m_viewer.window(), QPoint(0, 0));
  QStringList lines;
  if (m_model.hasFrame) {
    QStringList axes;
    for (const Sketch3dOverlay::Axis& axis : m_overlay->axes()) {
      axes << QStringLiteral("%1 %2").arg(axis.name, axis.endOn ? QStringLiteral("end on") : windowPlace(axis.to, window));
    }
    lines << QStringLiteral("3D sketch frame at %1; %2")
                 .arg(windowPlace(m_overlay->axes().front().from, window), axes.join(QStringLiteral("; ")));
  }
  // One per shape (mitcad#1592): its box, the shape's and the part of the
  // view badges keep inside, for checks that none covers its shape or
  // another badge and none is cut.
  const std::vector<Sketch3dOverlay::Badge> badges = m_overlay->badges();
  for (const Sketch3dOverlay::Badge& badge : badges) {
    lines << QStringLiteral("3D sketch badge In plane %1 at %2; box %3; shape %4; view %5")
                 .arg(badge.id, windowPlace(badge.at, window), windowBox(badge.box, window),
                      windowBox(badge.shape, window), windowBox(m_overlay->badgeArea(badge.box.size()), window));
  }
  for (const Sketch3dOverlay::Handle& handle : m_overlay->handles()) {
    lines << QStringLiteral("3D sketch tangent handle %1/%2 at %3")
                 .arg(handle.spline, handle.point, windowPlace(handle.to, window));
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_loggedPlaces) {
    m_loggedPlaces = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

} // namespace mitcad::sketch
