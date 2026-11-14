// SPDX-License-Identifier: MIT
// Test steps for the orbit's center (mitcad#1791): the Orbit center
// preference in this test process (view-orbit-center), the fixed center
// and the last orbit's point (capture-view-orbit-center,
// expect-view-orbit-center), a model point's place in the view
// (capture-view-point, expect-view-point, capture-view-place) and input at
// it (click-view-point, drag-view-point); see TestDriver.hpp.
#include "TestDriver.hpp"

#include <cmath>
#include <memory>
#include <optional>

#include <QPoint>
#include <QPointF>
#include <QRectF>
#include <QStringList>
#include <QtLogging>

#include <gp_Pnt.hxx>

#include "../OcctViewer.hpp"
#include "../view/Navigation.hpp"

namespace mitcad {
namespace {

// "x,y,z" in millimetres.
std::optional<gp_Pnt> pointArg(const QString& text) {
  const QStringList parts = text.split(QLatin1Char(','));
  if (parts.size() != 3) {
    return std::nullopt;
  }
  double xyz[3] = {0.0, 0.0, 0.0};
  for (int i = 0; i < 3; ++i) {
    bool ok = false;
    xyz[i] = parts.at(i).trimmed().toDouble(&ok);
    if (!ok || !std::isfinite(xyz[i])) {
      return std::nullopt;
    }
  }
  return gp_Pnt(xyz[0], xyz[1], xyz[2]);
}

// "x,y" in the view's logical pixels.
std::optional<QPointF> placeArg(const QString& text) {
  const QStringList parts = text.split(QLatin1Char(','));
  if (parts.size() != 2) {
    return std::nullopt;
  }
  bool okX = false;
  bool okY = false;
  const QPointF place(parts.at(0).trimmed().toDouble(&okX), parts.at(1).trimmed().toDouble(&okY));
  if (!okX || !okY) {
    return std::nullopt;
  }
  return place;
}

QString pointText(const gp_Pnt& at) {
  return QStringLiteral("%1,%2,%3").arg(at.X(), 0, 'g', 17).arg(at.Y(), 0, 'g', 17).arg(at.Z(), 0, 'g', 17);
}

QString placeText(const QPointF& at) {
  return QStringLiteral("%1,%2").arg(at.x(), 0, 'f', 3).arg(at.y(), 0, 'f', 3);
}

// [button] [mods] after `from`: left, right or middle, and shift, ctrl,
// alt, meta joined by '+'.
bool mouseArgs(const QStringList& a, int from, Qt::MouseButton& button, Qt::KeyboardModifiers& modifiers) {
  button = Qt::LeftButton;
  modifiers = Qt::NoModifier;
  for (int i = from; i < a.size(); ++i) {
    const QString word = a.at(i).toLower();
    if (word == QLatin1String("left")) {
      button = Qt::LeftButton;
    } else if (word == QLatin1String("right")) {
      button = Qt::RightButton;
    } else if (word == QLatin1String("middle")) {
      button = Qt::MiddleButton;
    } else {
      for (const QString& key : word.split(QLatin1Char('+'))) {
        if (key == QLatin1String("shift")) {
          modifiers |= Qt::ShiftModifier;
        } else if (key == QLatin1String("ctrl")) {
          modifiers |= Qt::ControlModifier;
        } else if (key == QLatin1String("alt")) {
          modifiers |= Qt::AltModifier;
        } else if (key == QLatin1String("meta")) {
          modifiers |= Qt::MetaModifier;
        } else {
          return false;
        }
      }
    }
  }
  return true;
}

} // namespace

bool TestDriver::runOrbitCenterStep(const QString& name, const QStringList& a, int line) {
  const bool matches = name == QLatin1String("view-orbit-center") ||
                       name == QLatin1String("capture-view-orbit-center") ||
                       name == QLatin1String("expect-view-orbit-center") ||
                       name == QLatin1String("capture-view-point") || name == QLatin1String("expect-view-point") ||
                       name == QLatin1String("capture-view-place") || name == QLatin1String("click-view-point") ||
                       name == QLatin1String("drag-view-point");
  if (!matches) {
    return false;
  }
  auto* view = qobject_cast<OcctViewer*>(viewer());
  if (view == nullptr || !view->isValid()) {
    fail(line, QStringLiteral("%1 requires an initialized 3D view").arg(name));
    return true;
  }

  if (name == QLatin1String("view-orbit-center")) {
    // view-orbit-center model|cursor: Preferences › Navigation's Orbit
    // center in this test process only, not saved.
    if (a.at(0) != QLatin1String("model") && a.at(0) != QLatin1String("cursor")) {
      fail(line, QStringLiteral("view-orbit-center model|cursor"));
      return true;
    }
    NavigationSettings settings = view->navigation();
    settings.orbitCenter = orbitCenterFromId(a.at(0));
    view->setNavigation(settings);
    return true;
  }

  if (name == QLatin1String("capture-view-orbit-center")) {
    // capture-view-orbit-center <var> fixed|last: the fixed center, or the
    // point the last orbit or turn turned about, as x,y,z.
    const bool fixed = a.at(1) == QLatin1String("fixed");
    if (!fixed && a.at(1) != QLatin1String("last")) {
      fail(line, QStringLiteral("capture-view-orbit-center <var> fixed|last"));
      return true;
    }
    const QString variable = a.at(0);
    const auto found = std::make_shared<QString>();
    whenReady(
        [this, fixed, found] {
          auto* shown = qobject_cast<OcctViewer*>(viewer());
          std::optional<gp_Pnt> center;
          if (shown != nullptr) {
            center = fixed ? shown->orbitCenter() : shown->lastOrbitPivot();
          }
          *found = center ? pointText(*center) : QString();
          m_waitDetail = center ? *found : QStringLiteral("none");
          return center.has_value();
        },
        6, fixed ? QStringLiteral("no orbit center is set") : QStringLiteral("no orbit has turned"),
        [this, variable, found] {
          m_vars.insert(variable, *found);
          qDebug().noquote() << QStringLiteral("TestDriver: orbit center %1").arg(*found);
        });
    return true;
  }

  if (name == QLatin1String("expect-view-orbit-center")) {
    // expect-view-orbit-center none|<x,y,z>: no fixed center, or one at
    // that point (within 1e-6 mm).
    const bool none = a.at(0) == QLatin1String("none");
    const std::optional<gp_Pnt> wanted = none ? std::nullopt : pointArg(a.at(0));
    if (!none && !wanted) {
      fail(line, QStringLiteral("expect-view-orbit-center none|<x,y,z>"));
      return true;
    }
    whenReady(
        [this, wanted] {
          auto* shown = qobject_cast<OcctViewer*>(viewer());
          if (shown == nullptr) {
            return false;
          }
          const std::optional<gp_Pnt>& center = shown->orbitCenter();
          m_waitDetail = center ? pointText(*center) : QStringLiteral("none");
          return wanted ? center && center->Distance(*wanted) <= 1e-6 : !center;
        },
        6, QStringLiteral("the orbit center is not %1").arg(a.at(0)));
    return true;
  }

  if (name == QLatin1String("capture-view-point")) {
    // capture-view-point <var> <x,y,z>: the model point's place in the
    // view (logical pixels), as x,y.
    const std::optional<gp_Pnt> point = pointArg(a.at(1));
    if (!point) {
      fail(line, QStringLiteral("capture-view-point <var> <x,y,z>"));
      return true;
    }
    view->settleCamera();
    m_vars.insert(a.at(0), placeText(view->toWidgetF(*point)));
    return true;
  }

  if (name == QLatin1String("capture-view-place")) {
    // capture-view-place <var> <x%> <y%>: that place of the view as x,y,
    // as click and drag take it.
    bool okX = false;
    bool okY = false;
    const double x = a.at(1).toDouble(&okX);
    const double y = a.at(2).toDouble(&okY);
    if (!okX || !okY) {
      fail(line, QStringLiteral("capture-view-place <var> <x%> <y%>"));
      return true;
    }
    m_vars.insert(a.at(0), placeText(QPointF(qRound(view->width() * x / 100.0), qRound(view->height() * y / 100.0))));
    return true;
  }

  if (name == QLatin1String("expect-view-point")) {
    // expect-view-point <x,y,z> <x,y> same|moved <pixels>: the model
    // point's place in the view at most (same) or more than (moved) that
    // far from a place captured before.
    const std::optional<gp_Pnt> point = pointArg(a.at(0));
    const std::optional<QPointF> before = placeArg(a.at(1));
    const bool same = a.at(2) == QLatin1String("same");
    bool okTolerance = false;
    const double tolerance = a.at(3).toDouble(&okTolerance);
    if (!point || !before || (!same && a.at(2) != QLatin1String("moved")) || !okTolerance || tolerance < 0.0) {
      fail(line, QStringLiteral("expect-view-point <x,y,z> <x,y> same|moved <pixels>"));
      return true;
    }
    whenReady(
        [this, point, before, same, tolerance] {
          auto* shown = qobject_cast<OcctViewer*>(viewer());
          if (shown == nullptr) {
            return false;
          }
          shown->settleCamera();
          const QPointF now = shown->toWidgetF(*point);
          const double distance = std::hypot(now.x() - before->x(), now.y() - before->y());
          m_waitDetail = QStringLiteral("at %1, %2 px from %3").arg(placeText(now)).arg(distance, 0, 'f', 3)
                             .arg(placeText(*before));
          return std::isfinite(distance) && (same ? distance <= tolerance : distance > tolerance);
        },
        6,
        QStringLiteral("the place of %1 did not stay %2 within %3 px").arg(a.at(0), a.at(2), a.at(3)),
        [this] { qDebug().noquote() << QStringLiteral("TestDriver: view point %1").arg(m_waitDetail); });
    return true;
  }

  // click-view-point <x,y,z> [button] [mods], drag-view-point <x,y,z> <dx>
  // <dy> [button] [mods]: at the model point's place in the view.
  const bool drag = name == QLatin1String("drag-view-point");
  const std::optional<gp_Pnt> point = pointArg(a.at(0));
  double dx = 0.0;
  double dy = 0.0;
  bool okDx = true;
  bool okDy = true;
  if (drag && a.size() >= 3) {
    dx = a.at(1).toDouble(&okDx);
    dy = a.at(2).toDouble(&okDy);
  }
  Qt::MouseButton button = Qt::LeftButton;
  Qt::KeyboardModifiers modifiers;
  if (!point || (drag && (a.size() < 3 || !okDx || !okDy)) || !mouseArgs(a, drag ? 3 : 1, button, modifiers)) {
    fail(line, drag ? QStringLiteral("drag-view-point <x,y,z> <dx> <dy> [button] [mods]")
                    : QStringLiteral("click-view-point <x,y,z> [button] [mods]"));
    return true;
  }
  view->settleCamera();
  const QPointF place = view->toWidgetF(*point);
  if (!QRectF(view->rect()).contains(place)) {
    fail(line, QStringLiteral("%1 is outside the view (at %2)").arg(a.at(0), placeText(place)));
    return true;
  }
  const QPoint at = view->mapTo(topWindow(), place.toPoint());
  qDebug().noquote() << QStringLiteral("TestDriver: %1 at %2").arg(name, placeText(place));
  if (drag) {
    queueDrag(at, at + QPoint(qRound(dx), qRound(dy)), button, modifiers);
  } else {
    queueClick(at, button, modifiers, 1);
  }
  return true;
}

} // namespace mitcad
