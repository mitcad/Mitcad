// SPDX-License-Identifier: MIT
// Redefine Sketch Plane (mitcad#787, app/COMMANDS.md "Redefine Sketch
// Plane"): a panel that moves a sketch to another origin or construction
// plane or planar face and places it there by an offset along the normal, a
// move in the plane, an angle about the normal and a flip
// (`sketch.redefine_plane`, commands.md "Redefine Sketch Plane"). A sketch
// command (CommandDef::sketchCommand): the preview applies the change and
// takes it back, so the view shows the sketch and what is built on it where
// OK puts them; OK keeps it as one undo step. It starts outside sketches
// from the sketch's context menus (browser, timeline, view) with the
// sketch, and in the sketch being edited from the sketch's context menu,
// always on that sketch.
#include "Commands.hpp"

#include <exception>
#include <utility>

#include <QColor>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QObject>
#include <QString>
#include <QStringList>
#include <QTimer>

#include "../framework/CommandRegistry.hpp"
#include "../framework/LengthUnits.hpp"
#include "../sketch/SketchController.hpp"
#include "CommandSupport.hpp"

namespace mitcad {
namespace {

const QString kSketch = QStringLiteral("sketch");
const QString kPlane = QStringLiteral("plane");
const QString kFlip = QStringLiteral("flip");

// The placement's values: input id (the command's field) and its kind.
struct PlacementValue {
  const char* id;
  ValueKind kind;
};
constexpr PlacementValue kValues[] = {
    {"offset", ValueKind::Length},
    {"x", ValueKind::Length},
    {"y", ValueKind::Length},
    {"angle", ValueKind::Angle},
};

bool isOriginPlane(const SelectionItem& item) {
  return item.kind == SelectKind::Plane &&
         (item.owner == QStringLiteral("xy") || item.owner == QStringLiteral("xz") ||
          item.owner == QStringLiteral("yz"));
}

// The plane as the model names it: a datum's uid or a face of a body.
QJsonValue planeRef(const SelectionItem& item) {
  if (item.kind == SelectKind::Plane) {
    return item.owner;
  }
  return QJsonObject{{QStringLiteral("face"), item.name}, {QStringLiteral("body"), item.owner}};
}

// The sketch the panel moves.
QString movedSketch(const CommandState& s) {
  const Selection& items = s.items(kSketch);
  return items.isEmpty() ? QString() : items.first().owner;
}

// The sketch's definition (the `feature` query's), empty when it is gone.
QJsonObject sketchDef(const QString& uid, const CommandContext& context) {
  if (uid.isEmpty()) {
    return {};
  }
  try {
    return context
        .queryObject({{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), uid}})
        .value(QStringLiteral("def"))
        .toObject();
  } catch (const std::exception&) {
    return {};
  }
}

// The inputs filled from the sketch: Plane empty (the sketch keeps its
// plane until another is picked) and the placement's values as expressions
// (its own parameters' expressions, a borrowed parameter by name). The
// plane is not put in the input: shown as a pick, an origin plane's square
// in front of the faces would take the clicks meant for them.
void loadSketch(CommandState& s, const CommandContext& context) {
  const QString uid = movedSketch(s);
  const QJsonObject def = sketchDef(uid, context);
  if (def.isEmpty()) {
    return;
  }
  s.setItems(kPlane, {});
  const QJsonObject placement = def.value(QStringLiteral("placement")).toObject();
  for (const PlacementValue& value : kValues) {
    const QString id = QString::fromLatin1(value.id);
    if (placement.contains(id)) {
      s.setText(id, context.valueText(placement.value(id), uid));
    } else {
      // A length's 0 in the design's unit, as the inputs' defaults (mitcad#1165).
      s.setText(id, value.kind == ValueKind::Angle ? QStringLiteral("0 deg")
                                                   : defaultLengthText(QStringLiteral("0 mm"), context.lengthUnit()));
    }
  }
  s.setChecked(kFlip, placement.value(kFlip).toBool());
}

// "(x, y, z)" of a JSON vector.
QString vectorText(const QJsonValue& value) {
  const QJsonArray v = value.toArray();
  return QStringLiteral("(%1, %2, %3)")
      .arg(cmd::number(v.at(0).toDouble()), cmd::number(v.at(1).toDouble()), cmd::number(v.at(2).toDouble()));
}

} // namespace

void registerSketchPlaneCommands(CommandRegistry& registry, sketch::SketchController& c) {
  CommandDef def;
  def.id = QStringLiteral("sketch.redefine_plane");
  def.name = QObject::tr("Redefine Sketch Plane");
  def.icon = QStringLiteral("offset-plane");
  def.tooltip = QObject::tr("Moves a sketch to another plane or face, with an offset and an angle");
  def.keywords = {QStringLiteral("move sketch"), QStringLiteral("sketch plane"), QStringLiteral("change plane"),
                  QStringLiteral("offset sketch"), QStringLiteral("rotate sketch"), QStringLiteral("flip sketch")};
  def.kind = CommandDef::Kind::Feature;
  // Outside sketches (with a sketch picked) and in the sketch being edited.
  def.mode = CommandDef::Mode::Any;
  def.tab.clear(); // the sketch's context menus and the command search
  def.group = QStringLiteral("MODIFY");
  def.sketchCommand = true;
  // Only from the sketch's own menus: never a guess of the view's menus or
  // the selection card from what is selected.
  def.offeredFor = [](const Selection&) { return false; };
  // Empty, the sketch keeps its plane ("Same Plane"); a pick is the new one.
  InputDef planeInput = selectionInput(kPlane, QObject::tr("Plane"), cmd::kPlaneKinds, 0, 1);
  planeInput.withTooltip(QObject::tr("An origin or construction plane, or a planar face made before the sketch"))
      .withAccepts(cmd::isPlanar);
  planeInput.emptyShows = [](const CommandState&) { return std::make_pair(QObject::tr("Same Plane"), QColor()); };
  def.inputs = {
      // In the sketch being edited it is that sketch, not pickable (a
      // browser or timeline pick would move another one).
      selectionInput(kSketch, QObject::tr("Sketch"), SelectKind::Sketch, 1, 1)
          .withTooltip(QObject::tr("The sketch to move, in the browser or the timeline"))
          .withVisible([&c](const CommandState&) { return !c.isActive(); })
          .withOnPick([](const SelectionItem&, CommandState& s, const CommandContext& context) {
            loadSketch(s, context);
          }),
      planeInput,
      valueInput(QStringLiteral("offset"), QObject::tr("Offset"), ValueKind::Length, QStringLiteral("0 mm"))
          .withTooltip(QObject::tr("Along the plane's normal")),
      // Beside Offset: both act on the normal (as mitcad#656's placements).
      flipInput(kFlip, QObject::tr("Flip"))
          .withTooltip(QObject::tr("Turns the sketch over: its normal reversed"))
          .withBesidePrevious(),
      valueInput(QStringLiteral("x"), QObject::tr("Move X"), ValueKind::Length, QStringLiteral("0 mm"))
          .withTooltip(QObject::tr("Along the sketch's x axis on the plane")),
      valueInput(QStringLiteral("y"), QObject::tr("Move Y"), ValueKind::Length, QStringLiteral("0 mm"))
          .withTooltip(QObject::tr("Along the sketch's y axis on the plane")),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withTooltip(QObject::tr("About the plane's normal, from the sketch's x axis towards its y axis")),
  };
  // The sketch being edited (always that one), else the one selected before
  // (the context menus give it), with its plane and placement.
  def.init = [&c](CommandState& s, const CommandContext& context) {
    if (c.isActive()) {
      s.setItems(kSketch, {SelectionItem{SelectKind::Sketch, c.uid(), QString(), QString()}});
    }
    loadSketch(s, context);
  };
  def.build = [](const CommandState& s, const CommandContext&) -> Built {
    const QString uid = movedSketch(s);
    if (uid.isEmpty()) {
      return Built::failure(QObject::tr("Select the sketch to move."), kSketch);
    }
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("sketch.redefine_plane")}, {kSketch, uid}};
    // The plane only when one was picked: without it the sketch keeps its
    // plane, own frame and link (the model keeps them too for the plane the
    // sketch has).
    const Selection& planes = s.items(kPlane);
    if (!planes.isEmpty()) {
      const SelectionItem& plane = planes.first();
      command.insert(kPlane, planeRef(plane));
      // Where it was picked, as Create Sketch sends it: another
      // component's plane or face is linked (mitcad#100).
      if (!isOriginPlane(plane)) {
        command.insert(QStringLiteral("occurrence"), plane.occurrence);
      }
      // Upright on the new plane, as a new sketch is (mitcad#692).
      command.insert(QStringLiteral("upright"), true);
    }
    for (const PlacementValue& value : kValues) {
      const QString id = QString::fromLatin1(value.id);
      command.insert(id, s.expression(id));
    }
    command.insert(kFlip, s.checked(kFlip));
    return Built::of(command);
  };
  // "Redefined the plane of Sketch1: plane xz, offset 5 mm, x 0 mm, y 0 mm,
  // angle 30 deg; frame origin (...), x (...), y (...)" ("the same plane"
  // without a pick), for UI tests.
  def.describe = [](const CommandState& s, const CommandContext& context) {
    const QString uid = movedSketch(s);
    const Selection& planes = s.items(kPlane);
    QString text = QStringLiteral("Redefined the plane of %1: %2")
                       .arg(context.featureName(uid),
                            planes.isEmpty() ? QStringLiteral("the same plane") : planes.first().describe());
    QStringList values;
    for (const PlacementValue& value : kValues) {
      const QString id = QString::fromLatin1(value.id);
      values << QStringLiteral("%1 %2").arg(id, s.expression(id));
    }
    text += QStringLiteral(", ") + values.join(QStringLiteral(", "));
    if (s.checked(kFlip)) {
      text += QStringLiteral(", flipped");
    }
    try {
      const QJsonObject frame =
          context.queryObject({{QStringLiteral("query"), QStringLiteral("sketch")}, {QStringLiteral("uid"), uid}})
              .value(QStringLiteral("frame"))
              .toObject();
      if (!frame.isEmpty()) {
        text += QStringLiteral("; frame origin %1, x %2, y %3")
                    .arg(vectorText(frame.value(QStringLiteral("origin"))),
                         vectorText(frame.value(QStringLiteral("x_axis"))),
                         vectorText(frame.value(QStringLiteral("y_axis"))));
      }
    } catch (const std::exception&) {
      // A sketch that is gone has no frame to report.
    }
    return text;
  };
  // In the sketch being edited the view turns to the sketch where it is
  // now, as entering it does.
  def.ended = [&c](bool committed) {
    if (committed && c.isActive()) {
      QTimer::singleShot(0, &c, [&c] {
        if (c.isActive()) {
          c.lookAt();
        }
      });
    }
  };
  registry.add(def);
}

} // namespace mitcad
