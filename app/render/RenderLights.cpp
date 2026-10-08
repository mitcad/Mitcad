// SPDX-License-Identifier: MIT
#include "render/RenderLights.hpp"

#include <algorithm>
#include <cmath>
#include <limits>

#include <QPainter>
#include <QPainterPath>
#include <QStringList>
#include <QtLogging>

#include <BRep_Tool.hxx>
#include <Graphic3d_Camera.hxx>
#include <Poly_Triangulation.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Trsf.hxx>

#include "OcctViewer.hpp"

namespace mitcad::render {

namespace {

constexpr double kPi = 3.14159265358979323846;

gp_XYZ xyzOf(const QJsonValue& value, const gp_XYZ& fallback) {
  const QJsonArray array = value.toArray();
  if (array.size() != 3) {
    return fallback;
  }
  return gp_XYZ(array.at(0).toDouble(), array.at(1).toDouble(), array.at(2).toDouble());
}

QJsonArray arrayOf(const gp_XYZ& v) {
  // Rounded a little: a light placed from the view need not carry the
  // last digits of a projection.
  const auto round = [](double x) { return std::round(x * 1e6) / 1e6; };
  return QJsonArray{round(v.X()), round(v.Y()), round(v.Z())};
}

// The camera's frame: to the right, up and toward the viewer.
struct Frame {
  gp_XYZ right;
  gp_XYZ up;
  gp_XYZ back;
};

Frame frameOf(const CameraState& camera) {
  gp_XYZ forward = camera.target.XYZ() - camera.eye.XYZ();
  if (forward.Modulus() < 1e-12) {
    forward = gp_XYZ(0, 1, 0);
  }
  forward.Normalize();
  gp_XYZ right = forward.Crossed(camera.up.XYZ());
  if (right.Modulus() < 1e-12) {
    right = gp_XYZ(1, 0, 0);
  }
  right.Normalize();
  return {right, right.Crossed(forward), forward.Reversed()};
}

gp_Dir directionOr(const gp_XYZ& v, const gp_Dir& fallback) {
  return v.Modulus() > 1e-12 ? gp_Dir(v) : fallback;
}

} // namespace

LightPose worldPose(const QJsonObject& light, const CameraState& camera) {
  const gp_XYZ position = xyzOf(light.value(QStringLiteral("position")), gp_XYZ(0, 0, 200));
  const gp_XYZ direction = xyzOf(light.value(QStringLiteral("direction")), gp_XYZ(0, 0, -1));
  if (light.value(QStringLiteral("space")).toString() != QLatin1String("camera")) {
    return {gp_Pnt(position), directionOr(direction, gp_Dir(0, 0, -1))};
  }
  const Frame f = frameOf(camera);
  const gp_XYZ at = camera.eye.XYZ() + f.right * position.X() + f.up * position.Y() + f.back * position.Z();
  const gp_XYZ along = f.right * direction.X() + f.up * direction.Y() + f.back * direction.Z();
  return {gp_Pnt(at), directionOr(along, gp_Dir(f.back.Reversed()))};
}

QJsonObject poseFields(const LightPose& pose, const QString& space, const CameraState& camera) {
  if (space != QLatin1String("camera")) {
    return {{QStringLiteral("position"), arrayOf(pose.position.XYZ())},
            {QStringLiteral("direction"), arrayOf(pose.direction.XYZ())}};
  }
  const Frame f = frameOf(camera);
  const gp_XYZ from = pose.position.XYZ() - camera.eye.XYZ();
  const gp_XYZ d = pose.direction.XYZ();
  return {{QStringLiteral("position"), arrayOf(gp_XYZ(from.Dot(f.right), from.Dot(f.up), from.Dot(f.back)))},
          {QStringLiteral("direction"), arrayOf(gp_XYZ(d.Dot(f.right), d.Dot(f.up), d.Dot(f.back)))}};
}

std::optional<std::pair<gp_Pnt, gp_Dir>> surfaceAt(OcctViewer& viewer, const QPointF& position) {
  if (viewer.width() <= 0 || viewer.height() <= 0) {
    return std::nullopt;
  }
  // The ray through the pixel, from the near to the far plane.
  const occ::handle<Graphic3d_Camera>& camera = viewer.viewCamera();
  const double x = 2.0 * position.x() / viewer.width() - 1.0;
  const double y = 1.0 - 2.0 * position.y() / viewer.height();
  const gp_XYZ from = camera->UnProject(gp_Pnt(x, y, -1.0)).XYZ();
  const gp_XYZ to = camera->UnProject(gp_Pnt(x, y, 1.0)).XYZ();
  const gp_XYZ ray = to - from;
  double nearest = std::numeric_limits<double>::infinity();
  gp_XYZ hitNormal;
  for (const BodyDisplay& body : viewer.shownBodies()) {
    if (!body.shape) {
      continue;
    }
    const gp_Trsf placement = body.placement.Transformation();
    for (TopExp_Explorer faces(body.shape->occt(), TopAbs_FACE); faces.More(); faces.Next()) {
      TopLoc_Location location;
      const occ::handle<Poly_Triangulation> mesh = BRep_Tool::Triangulation(TopoDS::Face(faces.Current()), location);
      if (mesh.IsNull()) {
        continue;
      }
      const gp_Trsf trsf = placement * location.Transformation();
      for (int t = 1; t <= mesh->NbTriangles(); ++t) {
        int a = 0;
        int b = 0;
        int c = 0;
        mesh->Triangle(t).Get(a, b, c);
        const gp_XYZ p0 = mesh->Node(a).Transformed(trsf).XYZ();
        const gp_XYZ e1 = mesh->Node(b).Transformed(trsf).XYZ() - p0;
        const gp_XYZ e2 = mesh->Node(c).Transformed(trsf).XYZ() - p0;
        // Moeller-Trumbore.
        const gp_XYZ p = ray.Crossed(e2);
        const double det = e1.Dot(p);
        if (std::abs(det) < 1e-18) {
          continue;
        }
        const gp_XYZ s = from - p0;
        const double u = s.Dot(p) / det;
        if (u < 0.0 || u > 1.0) {
          continue;
        }
        const gp_XYZ q = s.Crossed(e1);
        const double v = ray.Dot(q) / det;
        if (v < 0.0 || u + v > 1.0) {
          continue;
        }
        const double along = e2.Dot(q) / det;
        if (along >= 0.0 && along < nearest) {
          nearest = along;
          hitNormal = e1.Crossed(e2);
        }
      }
    }
  }
  if (!std::isfinite(nearest) || hitNormal.Modulus() < 1e-18) {
    return std::nullopt;
  }
  // Toward the viewer.
  if (hitNormal.Dot(ray) > 0.0) {
    hitNormal.Reverse();
  }
  return std::make_pair(gp_Pnt(from + ray * nearest), gp_Dir(hitNormal));
}

RenderLightsOverlay::RenderLightsOverlay(OcctViewer& viewer) : QWidget(&viewer), m_viewer(viewer) {
  setAttribute(Qt::WA_TransparentForMouseEvents);
  setAttribute(Qt::WA_NoSystemBackground);
  setObjectName(QStringLiteral("renderLights"));
  setGeometry(viewer.rect());
  connect(&viewer, &OcctViewer::viewChanged, this, &RenderLightsOverlay::follow);
}

void RenderLightsOverlay::setLights(const QJsonArray& lights, const QString& chosen) {
  m_lights = lights;
  m_chosen = chosen;
  follow();
}

void RenderLightsOverlay::follow() {
  if (geometry() != m_viewer.rect()) {
    setGeometry(m_viewer.rect());
  }
  update();
}

void RenderLightsOverlay::paintEvent(QPaintEvent*) {
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  const CameraState camera = m_viewer.camera();
  // A length in the design that shows as a short line in the view.
  const double scale = std::max(camera.height, 1e-6) * 0.08;
  QStringList logged;
  const QWidget* window = this->window();
  for (const QJsonValue& value : std::as_const(m_lights)) {
    const QJsonObject light = value.toObject();
    const QString id = light.value(QStringLiteral("id")).toString();
    const QString type = light.value(QStringLiteral("type")).toString();
    const bool chosen = id == m_chosen;
    const bool on = light.value(QStringLiteral("enabled")).toBool(true);
    QColor color = chosen ? QColor(240, 130, 20) : QColor(250, 210, 40);
    if (!on) {
      color = QColor(150, 150, 150);
    }
    const LightPose pose = worldPose(light, camera);
    const gp_XYZ along = pose.direction.XYZ();
    gp_Pnt at = pose.position;
    if (type == QLatin1String("sun")) {
      // A sun has no place: its arrow comes toward the middle of the view.
      at = gp_Pnt(camera.target.XYZ() - along * (scale * 3.0));
    }
    const QPointF centre = m_viewer.toWidgetF(at);
    const QPointF tip = m_viewer.toWidgetF(gp_Pnt(at.XYZ() + along * (scale * (type == QLatin1String("sun") ? 2.0 : 1.0))));
    const QPen outline(QColor(30, 30, 30, 200), 4.0, Qt::SolidLine, Qt::RoundCap);
    const QPen pen(color, 2.0, Qt::SolidLine, Qt::RoundCap);
    QPainterPath path;
    // Where it shines.
    path.moveTo(centre);
    path.lineTo(tip);
    if (type == QLatin1String("area")) {
      // Its outline in the plane across the direction (as the renderer
      // places it).
      const gp_XYZ z = along.Reversed();
      gp_XYZ x = std::abs(z.Z()) > 0.999 ? gp_XYZ(1, 0, 0) : gp_XYZ(0, 0, 1).Crossed(z);
      x.Normalize();
      const gp_XYZ y = z.Crossed(x);
      const double w = light.value(QStringLiteral("size")).toDouble(20.0) / 2.0;
      const bool disc = light.value(QStringLiteral("shape")).toString() == QLatin1String("disc");
      const double h = disc ? w : light.value(QStringLiteral("size_y")).toDouble(20.0) / 2.0;
      const int steps = disc ? 24 : 4;
      for (int i = 0; i <= steps; ++i) {
        gp_XYZ corner;
        if (disc) {
          const double angle = 2.0 * kPi * i / steps;
          corner = x * (w * std::cos(angle)) + y * (h * std::sin(angle));
        } else {
          const double sx = (i == 0 || i == 3 || i == 4) ? -w : w;
          const double sy = (i < 2 || i == 4) ? -h : h;
          corner = x * sx + y * sy;
        }
        const QPointF p = m_viewer.toWidgetF(gp_Pnt(at.XYZ() + corner));
        i == 0 ? path.moveTo(p) : path.lineTo(p);
      }
    } else if (type == QLatin1String("spot")) {
      // The cone's edges in the view.
      const QPointF d = tip - centre;
      const double length = std::hypot(d.x(), d.y());
      if (length > 1.0) {
        const double half = light.value(QStringLiteral("spot_angle")).toDouble(kPi / 4.0) / 2.0;
        const double base = std::atan2(d.y(), d.x());
        for (const double side : {-1.0, 1.0}) {
          const double angle = base + side * std::min(half, 1.4);
          path.moveTo(centre);
          path.lineTo(centre + QPointF(std::cos(angle), std::sin(angle)) * (length * 0.8));
        }
      }
    } else if (type != QLatin1String("sun")) {
      // A point light's rays.
      for (int i = 0; i < 8; ++i) {
        const double angle = kPi / 4.0 * i;
        const QPointF ray(std::cos(angle), std::sin(angle));
        path.moveTo(centre + ray * 8.0);
        path.lineTo(centre + ray * 12.0);
      }
    } else {
      // The sun's arrow head.
      const QPointF d = tip - centre;
      const double length = std::hypot(d.x(), d.y());
      if (length > 1.0) {
        const QPointF unit = d / length;
        const QPointF normal(-unit.y(), unit.x());
        path.moveTo(tip - unit * 9.0 + normal * 5.0);
        path.lineTo(tip);
        path.lineTo(tip - unit * 9.0 - normal * 5.0);
      }
    }
    painter.setBrush(Qt::NoBrush);
    painter.setPen(outline);
    painter.drawPath(path);
    painter.setPen(pen);
    painter.drawPath(path);
    painter.setPen(QPen(QColor(30, 30, 30, 220), 1.5));
    painter.setBrush(color);
    painter.drawEllipse(centre, 5.5, 5.5);
    const QString name = light.value(QStringLiteral("name")).toString();
    painter.setPen(QColor(30, 30, 30));
    painter.drawText(centre + QPointF(10, -8), name);
    painter.setPen(color.lighter(130));
    painter.drawText(centre + QPointF(9, -9), name);
    const QPoint inWindow = mapTo(window, centre.toPoint());
    logged << QStringLiteral("Render light glyph %1 at %2,%3").arg(id).arg(inWindow.x()).arg(inWindow.y());
  }
  const QString text = logged.join(QLatin1Char('\n'));
  if (text != m_logged) {
    m_logged = text;
    for (const QString& line : std::as_const(logged)) {
      qDebug().noquote() << line;
    }
  }
}

} // namespace mitcad::render
