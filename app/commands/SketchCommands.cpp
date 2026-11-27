// SPDX-License-Identifier: MIT
// Commands of the SKETCH tab (U2): the drawing tools, the sketch dimension,
// the constraints, and the modify commands. Drawing and picking tools are
// `Tool` commands run by sketch mode (app/sketch); Offset, Mirror, the
// patterns, Project, Fillet, Chamfer and Move/Copy are panels of the
// command framework whose definition is a `sketch.*` command
// (CommandDef::sketchCommand).
#include "Commands.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include <QDir>
#include <QFileInfo>
#include <QFontDatabase>
#include <QJsonArray>
#include <QKeySequence>
#include <QSet>
#include <gp_Pnt.hxx>

#include "../browser/DocumentHost.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/LengthUnits.hpp"
#include "../sketch/FontFiles.hpp"
#include "../sketch/SketchChains.hpp"
#include "../sketch/SketchController.hpp"
#include "../sketch/SketchModel.hpp"
#include "../sketch/SketchTool.hpp"
#include "../sketch/TextCommands.hpp"
#include "CommandSupport.hpp"
#include "PatternOptions.hpp"
#include "SketchConstraintKeys.hpp"

namespace mitcad {
namespace {

using cmd::flipApplies;
using cmd::negated;
using cmd::number;
using cmd::oneDirection;
using cmd::symmetricDirection;
using sketch::SketchController;

// Its place on the toolbar is the layout's (mitcad#1951).
CommandDef sketchCommand(const char* id, const QString& name, const char* icon, CommandDef::Kind kind) {
  CommandDef def;
  def.id = QString::fromLatin1(id);
  def.name = name;
  def.icon = QString::fromLatin1(icon);
  def.kind = kind;
  def.mode = CommandDef::Mode::Sketch;
  return def;
}

// The sketch being edited owns what sketch inputs take.
std::function<bool(const SelectionItem&)> ofSketch(SketchController& c) {
  // Not texts: they have their own edit panel and are not curves to copy.
  return [&c](const SelectionItem& item) { return item.owner == c.uid() && c.model().text(item.name) == nullptr; };
}

std::function<bool(const SelectionItem&)> linesOf(SketchController& c) {
  return [&c](const SelectionItem& item) {
    return item.owner == c.uid() && item.geometry == QStringLiteral("line");
  };
}

// A line input that also takes the sketch's x and y axes (mitcad#876:
// Mirror's Mirror Line; mitcad#1655: Rectangular Pattern's Direction): an
// axis picked shows its name in the field, a line its count.
InputDef lineOrAxisInput(const QString& id, const QString& label, int min, SketchController& c) {
  InputDef input = selectionInput(id, label, SelectKind::SketchCurve, min, 1)
                       .withAccepts(linesOf(c))
                       .withItemsText([&c](const Selection& items, const CommandState&, const CommandContext&) {
                         const QString axis = items.size() == 1 ? c.model().axisOf(items.first().name) : QString();
                         return axis == QStringLiteral("x")   ? QObject::tr("Sketch x axis")
                                : axis == QStringLiteral("y") ? QObject::tr("Sketch y axis")
                                                              : QString();
                       });
  input.sketchAxes = true;
  return input;
}

QJsonArray names(const Selection& items) {
  QJsonArray result;
  for (const SelectionItem& item : items) {
    result.append(item.name);
  }
  return result;
}

QJsonObject base(SketchController& c, const char* command) {
  return {{QStringLiteral("cmd"), QString::fromLatin1(command)}, {QStringLiteral("sketch"), c.uid()}};
}

// An offset's Corners (mitcad#1774): where its curves cross (Sharp), or an
// arc about the source's corner at each outer corner of lines and arcs
// (Round, mitcad#1786).
InputDef offsetCornersInput() {
  const QVector<QPair<QString, QString>> corners = {{QStringLiteral("sharp"), QObject::tr("Sharp")},
                                                    {QStringLiteral("round"), QObject::tr("Round")}};
  return choiceInput(QStringLiteral("corners"), QObject::tr("Corners"), corners, QStringLiteral("sharp"))
      .withTooltip(QObject::tr("Round: an arc about each outer corner, as far from it as the offset"));
}

void addOffset(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.offset", QObject::tr("Offset"), "offset",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.shortcut = QKeySequence(Qt::Key_O);
  def.tooltip = QObject::tr("A copy of a chain of curves at a distance, which stays at it");
  def.keywords = {QStringLiteral("parallel copy")};
  def.inputs = {
      selectionInput(QStringLiteral("curves"), QObject::tr("Curves"), SelectKind::SketchCurve, 1, 0)
          .withAccepts(ofSketch(c)),
      checkInput(QStringLiteral("chain"), QObject::tr("Chain Selection"), true),
      valueInput(QStringLiteral("distance"), QObject::tr("Offset Position"), ValueKind::Length,
                 QStringLiteral("5")),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip")),
      offsetCornersInput(),
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    QStringList curves;
    for (const SelectionItem& item : s.items(QStringLiteral("curves"))) {
      curves << item.name;
    }
    if (s.checked(QStringLiteral("chain"))) {
      curves = sketch::chainOf(c.model(), curves);
    }
    QString distance = s.expression(QStringLiteral("distance"));
    if (s.checked(QStringLiteral("flip"))) {
      distance = negated(distance);
    }
    QJsonObject cmd = base(c, "sketch.offset");
    cmd.insert(QStringLiteral("curves"), QJsonArray::fromStringList(curves));
    cmd.insert(QStringLiteral("distance"), distance);
    cmd.insert(QStringLiteral("corners"), s.choice(QStringLiteral("corners")));
    return Built::of(cmd);
  };
  def.describe = [](const CommandState& s, const CommandContext&) {
    return QStringLiteral("Offset %1 curve(s) by %2")
        .arg(s.items(QStringLiteral("curves")).size())
        .arg(s.expression(QStringLiteral("distance")));
  };
  registry.add(def);
}

void addMirror(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.mirror", QObject::tr("Mirror"), "mirror",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("Mirrored copies about a line or a sketch axis, kept symmetric");
  // The line, or the sketch's x or y axis (mitcad#876), which sketch mode
  // picks for this input only, as its pick id.
  const InputDef mirrorLine = lineOrAxisInput(QStringLiteral("axis"), QObject::tr("Mirror Line"), 1, c);
  def.inputs = {
      selectionInput(QStringLiteral("entities"), QObject::tr("Objects"),
                     SelectKind::SketchCurve | SelectKind::SketchPoint, 1, 0)
          .withAccepts(ofSketch(c)),
      mirrorLine,
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    const QString axis = s.items(QStringLiteral("axis")).first().name;
    QJsonArray entities;
    for (const SelectionItem& item : s.items(QStringLiteral("entities"))) {
      if (item.name != axis) {
        entities.append(item.name);
      }
    }
    if (entities.isEmpty()) {
      return Built::failure(QObject::tr("The mirror line cannot mirror itself."), QStringLiteral("entities"));
    }
    QJsonObject cmd = base(c, "sketch.mirror");
    cmd.insert(QStringLiteral("entities"), entities);
    // An axis by its name: the mirror makes its line in its own step when
    // the sketch has none.
    cmd.insert(QStringLiteral("axis"), sketch::isAxisPick(axis) ? sketch::axisPickName(axis) : axis);
    return Built::of(cmd);
  };
  def.describe = [&c](const CommandState& s, const CommandContext&) {
    const QString line = s.items(QStringLiteral("axis")).first().name;
    const QString axis = c.model().axisOf(line);
    return QStringLiteral("Mirrored %1 object(s) about %2")
        .arg(s.items(QStringLiteral("entities")).size())
        .arg(axis.isEmpty() ? line : QStringLiteral("the %1 axis").arg(axis));
  };
  registry.add(def);
}

void addCircularPattern(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.circular_pattern", QObject::tr("Circular Pattern"),
                                 "circular-pattern", CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("Copies around a center point (the origin when none is picked)");
  def.inputs = {
      selectionInput(QStringLiteral("entities"), QObject::tr("Objects"),
                     SelectKind::SketchCurve | SelectKind::SketchPoint, 1, 0)
          .withAccepts(ofSketch(c)),
      selectionInput(QStringLiteral("center"), QObject::tr("Center Point"), SelectKind::SketchPoint, 0, 1)
          .withAccepts(ofSketch(c)),
      valueInput(QStringLiteral("quantity"), QObject::tr("Quantity"), ValueKind::Unitless, QStringLiteral("3")).withStepMinimum(1),
      valueInput(QStringLiteral("angle"), QObject::tr("Total Angle"), ValueKind::Angle, QStringLiteral("360 deg")),
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    const int count = static_cast<int>(std::lround(s.value(QStringLiteral("quantity"))));
    if (count < 2) {
      return Built::failure(QObject::tr("The quantity must be at least 2."), QStringLiteral("quantity"));
    }
    QJsonObject cmd = base(c, "sketch.circular_pattern");
    QJsonArray entities = names(s.items(QStringLiteral("entities")));
    const Selection& center = s.items(QStringLiteral("center"));
    cmd.insert(QStringLiteral("entities"), entities);
    cmd.insert(QStringLiteral("center"),
               !center.isEmpty()                    ? QJsonValue(center.first().name)
               : !c.model().originPoint().isEmpty() ? QJsonValue(c.model().originPoint())
                                                    : QJsonValue(QJsonArray{0.0, 0.0}));
    cmd.insert(QStringLiteral("count"), count);
    cmd.insert(QStringLiteral("angle"), s.expression(QStringLiteral("angle")));
    return Built::of(cmd);
  };
  def.describe = [](const CommandState& s, const CommandContext&) {
    return QStringLiteral("Circular pattern of %1 object(s), %2 in all")
        .arg(s.items(QStringLiteral("entities")).size())
        .arg(std::lround(s.value(QStringLiteral("quantity"))));
  };
  registry.add(def);
}

// A rectangular pattern's spacings, signed (mitcad#447): a negative one
// goes against its direction. The panels' Flip negates the spacing that is
// built; Edit Pattern shows the stored signed spacings with Flip off. A
// symmetric direction (mitcad#545) hides Flip: its copies lie on both
// sides.
QJsonArray signedSpacings(const CommandState& s) {
  QJsonArray spacings;
  for (const char* n : {"1", "2"}) {
    const QString suffix = QString::fromLatin1(n);
    const QString spacing = s.expression(QStringLiteral("distance") + suffix);
    const bool flip = flipApplies(s.choice(QStringLiteral("direction_type") + suffix),
                                  s.checked(QStringLiteral("flip") + suffix));
    spacings.append(flip ? negated(spacing) : spacing);
  }
  return spacings;
}

// A direction's Direction Type (mitcad#545): One Direction, or Symmetric
// with the originals as the middle instance of an odd quantity.
InputDef sketchDirectionType(const QString& id, const QString& name) {
  return choiceInput(id, name,
                     {{oneDirection(), QStringLiteral("One Direction")}, {symmetricDirection(), QStringLiteral("Symmetric")}},
                     oneDirection())
      .withTooltip(QObject::tr("One Direction: copies along the direction; Symmetric: on both sides (an odd "
                               "quantity)"));
}

std::function<bool(const CommandState&)> notSymmetric(const QString& type) {
  return [type](const CommandState& s) { return s.choice(type) != symmetricDirection(); };
}

// The directions' Direction Types as the sketch pattern's `symmetric`
// ([bool, bool]).
std::array<bool, 2> symmetricDirections(const CommandState& s) {
  return {s.choice(QStringLiteral("direction_type1")) == symmetricDirection(),
          s.choice(QStringLiteral("direction_type2")) == symmetricDirection()};
}

QString pointPositions(const sketch::SketchModel& model, const QStringList& entities);

// A rectangular pattern's Direction: the picked line's, from its start to
// its end, or the picked sketch axis's (mitcad#1655), else the sketch's x
// axis.
sketch::V2 patternDirection(const CommandState& s, SketchController& c) {
  const Selection& line = s.items(QStringLiteral("direction"));
  if (!line.isEmpty()) {
    if (const auto along = c.model().lineDirection(line.first().name)) {
      return *along;
    }
  }
  return sketch::V2{1.0, 0.0};
}

// A rectangular pattern's Direction 2 (mitcad#1969): the picked line's or
// sketch axis's, as Direction's; none when nothing is picked, and the rows
// then run at right angles to Direction.
std::optional<sketch::V2> patternDirection2(const CommandState& s, SketchController& c) {
  const Selection& line = s.items(QStringLiteral("direction2"));
  return line.isEmpty() ? std::nullopt : c.model().lineDirection(line.first().name);
}

// Whether two pattern directions are parallel (the model refuses them: the
// copies would fall on one row).
bool parallelDirections(const sketch::V2& a, const sketch::V2& b) { return std::abs(sketch::cross(a, b)) < 1e-6; }

// The second direction's tooltip (mitcad#1640, mitcad#1969): along
// Direction 2, or at right angles to Direction when none is picked.
QString acrossDirection() {
  return QObject::tr("Along Direction 2, or at right angles to Direction when none is picked");
}

// Rectangular Pattern's handle (mitcad#1640): an arrow along Direction at
// the originals' first copy, which drags Spacing, and one along Direction 2
// (or at right angles to Direction without it, mitcad#1969) at the first
// copy of the second direction, which drags Spacing 2;
// either against its direction while its Flip is on. An arrow whose
// Quantity is 1 is faint, a hint of where those copies would go.
std::optional<SketchHandle> patternHandle(const CommandState& s, SketchController& c) {
  QStringList originals;
  for (const SelectionItem& item : s.items(QStringLiteral("entities"))) {
    originals << item.name;
  }
  const auto middle = sketch::middleOf(c.model(), originals);
  if (!middle) {
    return std::nullopt;
  }
  const sketch::V2 first = patternDirection(s, c);
  const std::array<sketch::V2, 2> directions{first, patternDirection2(s, c).value_or(sketch::perp(first))};
  SketchHandle handle;
  handle.at = {middle->x, middle->y};
  handle.x = QStringLiteral("distance1");
  handle.y = QStringLiteral("distance2");
  handle.moves = originals;
  handle.square = false;
  handle.snapsToGeometry = false;
  handle.dragHint = QObject::tr("Drag: the spacing follows in steps of the grid (Shift or Alt: freely).");
  std::array<std::array<double, 2>, 2> starts{};
  for (std::size_t k = 0; k < 2; ++k) {
    const QString n = QString::number(k + 1);
    const double value = s.value(QStringLiteral("distance") + n);
    const double spacing = std::isfinite(value) ? value : 0.0;
    const sketch::V2 way = flipApplies(s.choice(QStringLiteral("direction_type") + n),
                                       s.checked(QStringLiteral("flip") + n))
                               ? -directions[k]
                               : directions[k];
    const sketch::V2 start = *middle + way * spacing;
    handle.values[k] = spacing;
    handle.axes[k] = {way.x, way.y};
    starts[k] = {start.x, start.y};
    const double quantity = s.value(QStringLiteral("quantity") + n);
    handle.faint[k] = !(std::isfinite(quantity) && std::lround(quantity) > 1);
  }
  handle.starts = starts;
  return handle;
}

void addRectangularPattern(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.rectangular_pattern", QObject::tr("Rectangular Pattern"),
                                 "rectangular-pattern", CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("Copies in rows and columns along a line (the sketch x axis when none is picked)");
  def.inputs = {
      selectionInput(QStringLiteral("entities"), QObject::tr("Objects"),
                     SelectKind::SketchCurve | SelectKind::SketchPoint, 1, 0)
          .withAccepts(ofSketch(c)),
      // A line, or the sketch's x or y axis (mitcad#1655), so a sketch of
      // circles alone has a direction to pick.
      lineOrAxisInput(QStringLiteral("direction"), QObject::tr("Direction"), 0, c)
          .withTooltip(QObject::tr("A line, or the sketch's x or y axis; the x axis when none is picked")),
      sketchDirectionType(QStringLiteral("direction_type1"), QObject::tr("Direction Type")),
      valueInput(QStringLiteral("quantity1"), QObject::tr("Quantity"), ValueKind::Unitless, QStringLiteral("3")).withStepMinimum(1),
      valueInput(QStringLiteral("distance1"), QObject::tr("Spacing"), ValueKind::Length, QStringLiteral("10")),
      flipInput(QStringLiteral("flip1"), QObject::tr("Flip"))
          .withTooltip(QObject::tr("Copies against the direction"))
          .withVisible(notSymmetric(QStringLiteral("direction_type1"))),
      // The second direction (mitcad#1969): a line or an axis at any angle
      // to Direction but parallel, for rows of a honeycomb; none picked, it
      // runs across Direction, a quarter turn from it (mitcad#1640).
      // Quantity 2 starts at 2, so the preview shows a grid at once; one
      // row is a click on its step down.
      lineOrAxisInput(QStringLiteral("direction2"), QObject::tr("Direction 2"), 0, c)
          .withTooltip(QObject::tr("A line or a sketch axis; at right angles to Direction when none is picked")),
      sketchDirectionType(QStringLiteral("direction_type2"), QObject::tr("Direction Type 2"))
          .withTooltip(QObject::tr("Copies along the second direction; Symmetric: on both sides (an odd "
                                   "quantity)")),
      valueInput(QStringLiteral("quantity2"), QObject::tr("Quantity 2"), ValueKind::Unitless, QStringLiteral("2"))
          .withStepMinimum(1)
          .withTooltip(acrossDirection()),
      valueInput(QStringLiteral("distance2"), QObject::tr("Spacing 2"), ValueKind::Length, QStringLiteral("10"))
          .withTooltip(acrossDirection()),
      flipInput(QStringLiteral("flip2"), QObject::tr("Flip 2"))
          .withTooltip(QObject::tr("Copies against the second direction"))
          .withVisible(notSymmetric(QStringLiteral("direction_type2"))),
  };
  def.sketchHandle = [&c](const CommandState& s) { return patternHandle(s, c); };
  // Where the copies are once a preview is shown (the sketch read again),
  // for UI tests: "Preview Rectangular Pattern copies: p9 (-10.000,
  // -15.000), …", their points in the pattern's order. Not when a preview
  // is taken back or failed.
  const QString commandName = def.name;
  def.sketchPreviewShown = [&c, commandName](const QJsonObject& answer) {
    const std::vector<sketch::PatternData>& patterns = c.model().patterns;
    if (answer.isEmpty() || patterns.empty()) {
      return;
    }
    // The pattern the answer names (`constraints`), else the newest one.
    const QJsonArray made = answer.value(QStringLiteral("constraints")).toArray();
    const QString id = made.isEmpty() ? QString() : made.at(0).toString();
    const sketch::PatternData* shown = &patterns.back();
    for (const sketch::PatternData& p : patterns) {
      if (p.id == id) {
        shown = &p;
      }
    }
    qDebug().noquote() << QStringLiteral("Preview %1 copies: %2").arg(commandName, pointPositions(c.model(), shown->copies));
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    const int n1 = static_cast<int>(std::lround(s.value(QStringLiteral("quantity1"))));
    const int n2 = static_cast<int>(std::lround(s.value(QStringLiteral("quantity2"))));
    if (n1 < 1 || n2 < 1 || n1 * n2 < 2) {
      return Built::failure(QObject::tr("The quantities make no copies."), QStringLiteral("quantity1"));
    }
    // The originals are a symmetric direction's middle instance: an odd
    // quantity (commands.md, `patterns`).
    const std::array<bool, 2> symmetric = symmetricDirections(s);
    if ((symmetric[0] && n1 % 2 == 0) || (symmetric[1] && n2 % 2 == 0)) {
      return Built::failure(QObject::tr("A symmetric direction needs an odd quantity."),
                            symmetric[0] && n1 % 2 == 0 ? QStringLiteral("quantity1") : QStringLiteral("quantity2"));
    }
    const sketch::V2 u = patternDirection(s, c);
    const QJsonArray direction{u.x, u.y};
    const std::optional<sketch::V2> v = patternDirection2(s, c);
    if (v && parallelDirections(u, *v)) {
      return Built::failure(QObject::tr("Direction 2 runs along Direction: pick a line at an angle to it."),
                            QStringLiteral("direction2"));
    }
    QJsonObject cmd = base(c, "sketch.rectangular_pattern");
    cmd.insert(QStringLiteral("entities"), names(s.items(QStringLiteral("entities"))));
    cmd.insert(QStringLiteral("direction"), direction);
    if (v) {
      cmd.insert(QStringLiteral("direction2"), QJsonArray{v->x, v->y});
    }
    cmd.insert(QStringLiteral("count"), QJsonArray{n1, n2});
    cmd.insert(QStringLiteral("spacing"), signedSpacings(s));
    if (symmetric[0] || symmetric[1]) {
      cmd.insert(QStringLiteral("symmetric"), QJsonArray{symmetric[0], symmetric[1]});
    }
    return Built::of(cmd);
  };
  def.describe = [](const CommandState& s, const CommandContext&) {
    const std::array<bool, 2> symmetric = symmetricDirections(s);
    QStringList both;
    for (int k = 0; k < 2; ++k) {
      if (symmetric[static_cast<std::size_t>(k)]) {
        both << QStringLiteral("direction %1 symmetric").arg(k + 1);
      }
    }
    return QStringLiteral("Rectangular pattern of %1 object(s), %2 x %3%4")
        .arg(s.items(QStringLiteral("entities")).size())
        .arg(std::lround(s.value(QStringLiteral("quantity1"))))
        .arg(std::lround(s.value(QStringLiteral("quantity2"))))
        .arg(both.isEmpty() ? QString() : QStringLiteral(", ") + both.join(QStringLiteral(", ")));
  };
  registry.add(def);
}

// Project: edges, faces (their edges) and vertices of the bodies, and
// curves and points of earlier sketches (mitcad#904: lines, arcs, circles,
// ellipses, splines and points, construction and projected ones too; a
// whole sketch picked in the browser gives all its curves), each a
// `sketch.project`. Other sketches' curves and points are pickable only
// here, through a selection filter row of their own (SelectionScope);
// the edited sketch's own never. A later sketch's geometry fails the
// preview with the model's reason.
bool projectsSketchGeometry(const SelectionItem& item) {
  return item.kind == SelectKind::SketchCurve || item.kind == SelectKind::SketchPoint ||
         item.kind == SelectKind::Sketch;
}

// Project's and Intersect's picks (mitcad#1417): the curve of an
// Intersection Curve among features (picked in the view, on its Browser row
// or on the timeline, isFeatureCurve), never another feature; other kinds as
// the input's filter has them, but not the edited sketch's own geometry nor
// the sketch itself. Only Intersection Curves among features, so that a
// sketch clicked on the timeline still goes in as the whole sketch
// (BrowserController::featureItems, mitcad#1031).
bool projectionSourceAccepted(const SelectionItem& item, const QString& edited) {
  if (item.kind == SelectKind::Feature) {
    return isFeatureCurve(item);
  }
  return !projectsSketchGeometry(item) || item.owner != edited;
}

// "intersection curve F4" (logs), as other sketches' geometry is named.
QString projectedName(const SelectionItem& item) {
  return isFeatureCurve(item) ? QStringLiteral("intersection curve %1").arg(item.owner) : item.logDescription();
}

void addProject(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.project", QObject::tr("Project"), "project",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.shortcut = QKeySequence(Qt::Key_P);
  def.tooltip = QObject::tr("Body edges, faces and vertices, and other sketches' curves, onto the plane");
  def.keywords = {QStringLiteral("include"), QStringLiteral("reference")};
  def.inputs = {
      selectionInput(QStringLiteral("geometry"), QObject::tr("Geometry"),
                     SelectKind::Edge | SelectKind::Face | SelectKind::Vertex | SelectKind::SketchCurve |
                         SelectKind::SketchPoint | SelectKind::Sketch | SelectKind::Feature,
                     1, 0)
          .withAccepts([&c](const SelectionItem& item) { return projectionSourceAccepted(item, c.uid()); }),
      checkInput(QStringLiteral("linked"), QObject::tr("Projection Link"), true)
          .withTooltip(QObject::tr("The projection follows its source when the design changes")),
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    QJsonArray commands;
    for (const SelectionItem& item : s.items(QStringLiteral("geometry"))) {
      QJsonObject cmd = base(c, "sketch.project");
      if (isFeatureCurve(item)) {
        // An Intersection Curve's curve (mitcad#1417), its ends with it.
        cmd.insert(QStringLiteral("source"), QJsonObject{{QStringLiteral("feature"), item.owner}});
      } else if (projectsSketchGeometry(item)) {
        // {"sketch": "F5", "curve": "c3"}, {"sketch": "F5", "point":
        // "p2"} or a whole sketch, {"sketch": "F5"}.
        cmd.insert(QStringLiteral("source"), item.reference());
      } else {
        cmd.insert(QStringLiteral("source"), item.name);
        cmd.insert(QStringLiteral("body"), item.owner);
      }
      cmd.insert(QStringLiteral("linked"), s.checked(QStringLiteral("linked")));
      // Where it was picked: another component's geometry comes through
      // a link (mitcad#100).
      cmd.insert(QStringLiteral("occurrence"), item.occurrence);
      commands.append(cmd);
    }
    return Built::of(QJsonObject{{QStringLiteral("commands"), commands}});
  };
  def.describe = [](const CommandState& s, const CommandContext&) {
    const Selection& items = s.items(QStringLiteral("geometry"));
    // Other sketches' geometry and Intersection Curves by name (mitcad#904,
    // mitcad#1417).
    QStringList sketched;
    for (const SelectionItem& item : items) {
      if (projectsSketchGeometry(item) || isFeatureCurve(item)) {
        sketched << projectedName(item);
      }
    }
    return QStringLiteral("Projected %1 item(s)").arg(items.size()) +
           (sketched.isEmpty() ? QString() : QStringLiteral(": ") + sketched.join(QStringLiteral(", ")));
  };
  registry.add(def);
}

// Intersect (mitcad#736): where the sketch's plane cuts the picked faces and
// whole bodies (section curves), and the points where it crosses the picked
// edges: `sketch.project` with `section`, a body without a `source`. Linked
// by default, as Project; another component's geometry through its
// occurrence. A pick the plane does not reach fails the preview with the
// model's reason in the panel. Curves and points of earlier sketches, or a
// whole sketch picked in the browser (mitcad#1031), give the points where
// the plane crosses them (the model's section of a sketch's curves,
// mitcad#998), linked as well; they are pickable through the same
// selection filter row as Project's, and the edited sketch's own never.
void addIntersect(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.intersect", QObject::tr("Intersect"), "sketch-intersect", CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("Where the sketch plane cuts bodies, faces, edges and other sketches' curves");
  def.keywords = {QStringLiteral("section"), QStringLiteral("cut"), QStringLiteral("reference"),
                  QStringLiteral("project")};
  def.inputs = {
      selectionInput(QStringLiteral("geometry"), QObject::tr("Geometry"),
                     SelectKind::Body | SelectKind::Face | SelectKind::Edge | SelectKind::SketchCurve |
                         SelectKind::SketchPoint | SelectKind::Sketch | SelectKind::Feature,
                     1, 0)
          .withAccepts([&c](const SelectionItem& item) { return projectionSourceAccepted(item, c.uid()); }),
      checkInput(QStringLiteral("linked"), QObject::tr("Projection Link"), true)
          .withTooltip(QObject::tr("The curves follow the design when it changes")),
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    QJsonArray commands;
    for (const SelectionItem& item : s.items(QStringLiteral("geometry"))) {
      QJsonObject cmd = base(c, "sketch.project");
      if (isFeatureCurve(item)) {
        // Where the plane crosses an Intersection Curve (mitcad#1417).
        cmd.insert(QStringLiteral("source"), QJsonObject{{QStringLiteral("feature"), item.owner}});
      } else if (projectsSketchGeometry(item)) {
        // Another sketch's curve or point, or all its curves: where the
        // plane crosses them, no body.
        cmd.insert(QStringLiteral("source"), item.reference());
      } else {
        // A whole body: every face the plane cuts.
        if (item.kind != SelectKind::Body) {
          cmd.insert(QStringLiteral("source"), item.name);
        }
        cmd.insert(QStringLiteral("body"), item.owner);
      }
      cmd.insert(QStringLiteral("linked"), s.checked(QStringLiteral("linked")));
      cmd.insert(QStringLiteral("section"), true);
      cmd.insert(QStringLiteral("occurrence"), item.occurrence);
      commands.append(cmd);
    }
    return Built::of(QJsonObject{{QStringLiteral("commands"), commands}});
  };
  def.describe = [](const CommandState& s, const CommandContext&) {
    const Selection& items = s.items(QStringLiteral("geometry"));
    // Other sketches' geometry and Intersection Curves by name, as
    // Project's log (mitcad#1031, mitcad#1417).
    QStringList sketched;
    for (const SelectionItem& item : items) {
      if (projectsSketchGeometry(item) || isFeatureCurve(item)) {
        sketched << projectedName(item);
      }
    }
    return QStringLiteral("Intersected %1 item(s)").arg(items.size()) +
           (sketched.isEmpty() ? QString() : QStringLiteral(": ") + sketched.join(QStringLiteral(", ")));
  };
  registry.add(def);
}

// Clean Up Projections (mitcad#232): projected curves the same as another,
// and lines on or overlapping another, removed with what refers to them
// moved to the kept curve. The preview lists what it removes and what
// changes (the command's warnings); OK keeps it as one undo step.
void addCleanUpProjections(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.clean_up_projections", QObject::tr("Clean Up Projections"), "project", CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("Removes repeated projected curves and projected lines lying on another");
  def.keywords = {QStringLiteral("duplicate"), QStringLiteral("overlapping"), QStringLiteral("merge")};
  def.inputs = {
      checkInput(QStringLiteral("merge"), QObject::tr("Merge Overlapping Lines"), true)
          .withTooltip(QObject::tr("Also merges overlapping projected and unlinked lines")),
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    QJsonObject cmd = base(c, "sketch.clean_up_projections");
    cmd.insert(QStringLiteral("merge"), s.checked(QStringLiteral("merge")));
    return Built::of(cmd);
  };
  def.describe = [](const CommandState&, const CommandContext&) {
    return QStringLiteral("Cleaned up projections");
  };
  registry.add(def);
}

// Clean Up Stale Projections (mitcad#252): projected curves that no longer
// match their source removed when nothing uses them, stale lines trimmed to
// the regions later features use, and the projections that still do not
// follow unlinked. The preview shows the result with the removed curves and
// the pieces trimmed off drawn dashed in the remove colour and the trimmed
// lines highlighted, and lists what it removes, trims, unlinks and keeps
// (the command's warnings); OK keeps it as one undo step. The Sketch
// Palette offers it under a warning about such projections.
void addCleanUpStaleProjections(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.clean_up_stale_projections", QObject::tr("Clean Up Stale Projections"),
                                 "project", CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("Removes, trims or unlinks projected curves that no longer match their source");
  def.keywords = {QStringLiteral("stale"), QStringLiteral("outdated"), QStringLiteral("unlink")};
  def.build = [&c](const CommandState&, const CommandContext&) -> Built {
    return Built::of(base(c, "sketch.clean_up_stale_projections"));
  };
  // The view shows what goes and what is trimmed while the preview lasts.
  def.sketchPreviewShown = [&c](const QJsonObject& answer) { c.setStalePreview(answer); };
  def.describe = [](const CommandState&, const CommandContext&) {
    return QStringLiteral("Cleaned up stale projections");
  };
  registry.add(def);
}

// The edges that could take the place of the sketch's lost projections'
// sources (mitcad#1794), as Project Again lists them: kept in its state when
// it starts, since its preview applies the command and the sketch then has
// none.
const QString kSuccessors = QStringLiteral("successors");

QJsonArray successorRows(const CommandState& state) {
  return state.object(kSuccessors).value(QStringLiteral("rows")).toArray();
}

// One of them as a row in words (mitcad#1794): its number, its kind, its
// length and an arc's or circle's radius, in the design's unit; never its
// name, which is the model's.
QString successorRow(int index, const QJsonObject& row, const QString& unit) {
  const QString type = row.value(QStringLiteral("type")).toString();
  const QString kind = type == QStringLiteral("line")     ? QObject::tr("Line")
                       : type == QStringLiteral("arc")    ? QObject::tr("Arc")
                       : type == QStringLiteral("circle") ? QObject::tr("Circle")
                                                          : QObject::tr("Edge");
  const bool round = type == QStringLiteral("arc") || type == QStringLiteral("circle");
  return QStringList{QString::number(index + 1), kind,
                     lengthInUnit(row.value(QStringLiteral("length")).toDouble(), unit),
                     round ? lengthInUnit(row.value(QStringLiteral("radius")).toDouble(), unit) : QString()}
      .join(QLatin1Char('\t'));
}

// Project Again (mitcad#1303): the linked projections that no longer follow
// their sources (their curves kept their last geometry, or some found
// nothing like them) projected anew from their sources, the constraints and
// dimensions on the old curves carried over to the new ones where they map;
// what does not map stays on the old curves, kept as construction
// references, and the preview lists it (the command's warnings). OK keeps it
// as one undo step. The Sketch Palette offers it under the warning, beside
// Clean Up Stale Projections, and so do the sketch's context menus.
// A projection whose source edge is gone, with edges on its curve that could
// have taken its place (mitcad#1794): the panel lists them as rows in words,
// the edge painted while the pointer is over its row, the first chosen; the
// chosen one becomes the projection's source (`successor`).
void addProjectAgain(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.project_again", QObject::tr("Project Again"), "project", CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("Projects again what no longer follows, keeping its constraints");
  def.keywords = {QStringLiteral("reproject"), QStringLiteral("repair"), QStringLiteral("stale"),
                  QStringLiteral("follow")};
  const auto hasRows = [](const CommandState& state) { return !successorRows(state).isEmpty(); };
  def.inputs = {
      choiceInput(QStringLiteral("successor"), QObject::tr("Edge to Follow"), {}, QString())
          .withChoiceColumns({QObject::tr("Edge"), QObject::tr("Kind"), QObject::tr("Length"), QObject::tr("Radius")})
          .withChoices([&c](const CommandState& state) {
            QVector<QPair<QString, QString>> rows;
            const QJsonArray listed = successorRows(state);
            for (int i = 0; i < listed.size(); ++i) {
              rows.append({QString::number(i), successorRow(i, listed.at(i).toObject(), c.lengthUnit())});
            }
            return rows;
          })
          .withChoicePlaces([](const CommandState& state, const QString& value) {
            // The edge where the body has it (another component's where its
            // occurrence puts it).
            bool ok = false;
            const int index = value.toInt(&ok);
            const QJsonArray listed = successorRows(state);
            if (!ok || index < 0 || index >= listed.size()) {
              return QJsonArray();
            }
            const QJsonObject row = listed.at(index).toObject();
            QJsonObject place{{QStringLiteral("kind"), QStringLiteral("edge")},
                              {QStringLiteral("name"), row.value(QStringLiteral("edge"))}};
            for (const char* field : {"body", "occurrence"}) {
              const QString text = row.value(QLatin1String(field)).toString();
              if (!text.isEmpty()) {
                place.insert(QLatin1String(field), text);
              }
            }
            return QJsonArray{place};
          })
          .withVisible(hasRows)
          .withTooltip(
              QObject::tr("The edge on the curve that takes the place of the gone source; the pointer over "
                          "a row shows it")),
  };
  def.init = [&c](CommandState& s, const CommandContext&) {
    QJsonArray rows;
    for (const sketch::SuccessorData& found : c.model().successors()) {
      rows.append(QJsonObject{{QStringLiteral("edge"), found.edge},
                              {QStringLiteral("type"), found.type},
                              {QStringLiteral("length"), found.length},
                              {QStringLiteral("radius"), found.radius},
                              {QStringLiteral("body"), found.body},
                              {QStringLiteral("occurrence"), found.occurrence}});
    }
    s.setObject(kSuccessors, QJsonObject{{QStringLiteral("rows"), rows}});
    s.setChoice(QStringLiteral("successor"), rows.isEmpty() ? QString() : QStringLiteral("0"));
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    QJsonObject cmd = base(c, "sketch.project_again");
    // The edge chosen to take a lost source's place (mitcad#1794).
    bool ok = false;
    const int chosen = s.choice(QStringLiteral("successor")).toInt(&ok);
    const QJsonArray rows = successorRows(s);
    if (ok && chosen >= 0 && chosen < rows.size()) {
      cmd.insert(QStringLiteral("successor"), rows.at(chosen).toObject().value(QStringLiteral("edge")));
    }
    return Built::of(cmd);
  };
  def.describe = [](const CommandState&, const CommandContext&) { return QStringLiteral("Projected again"); };
  registry.add(def);
}

// Convert to Sketch Geometry (mitcad#257): the selected projected curves and
// points become ordinary free sketch geometry that no longer follows its
// source, at once, as one undo step. Enabled while the selection holds
// projected geometry (SketchController::selectionHasProjected); also in the
// context menu of projected geometry.
void addConvertProjected(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.convert_projected", QObject::tr("Convert to Sketch Geometry"),
                                 "convert-projected", CommandDef::Kind::Action);
  def.run = [&c] { c.convertProjected(); };
  def.enabled = [&c] { return c.selectionHasProjected(); };
  def.tooltip = QObject::tr("Turns the selected projected geometry into ordinary sketch geometry");
  def.keywords = {QStringLiteral("projected"), QStringLiteral("unlink"), QStringLiteral("reference"),
                  QStringLiteral("convert")};
  registry.add(def);
}

// Profile Boundary (mitcad#2551): the selected curves of the face's outline
// the sketch got when it was made bound profiles as drawn curves do (a
// boss's circle makes a hole in an inset of the outline), and back, as one
// undo step; they stay linked to the face's edges. Enabled while the
// selection holds such curves (SketchController::selectionHasAutomatic);
// also in their context menu.
void addProfileBoundary(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.profile_boundary", QObject::tr("Profile Boundary"), "profile-boundary",
                                 CommandDef::Kind::Action);
  def.run = [&c] { c.toggleProfileBoundary(); };
  def.enabled = [&c] { return c.selectionHasAutomatic(); };
  def.tooltip = QObject::tr("Lets the selected curves of the face's outline bound profiles and back");
  def.keywords = {QStringLiteral("hole"), QStringLiteral("profile"), QStringLiteral("outline"),
                  QStringLiteral("boundary")};
  registry.add(def);
}

bool filletCurve(const sketch::CurveData& curve) { return curve.isLine() || curve.isArc() || curve.isConic(); }

// Whether the end `point` of the curve `own` and an end of another fillet
// curve of the kind are one corner although they are two points (mitcad#1192,
// as a Trim made them): at the same place (within the model's join gap of
// 0.01 mm) and held together by a point-to-point coincident, or by a
// point-on-curve of one end on the other curve. The model merges them first.
bool heldCorner(const sketch::SketchModel& model, const QString& point, const sketch::CurveData& own) {
  const std::optional<sketch::V2> at = model.pointAt(point);
  if (!at) {
    return false;
  }
  const auto holds = [&model](const QString& a, const QString& b) {
    return std::any_of(model.constraints.cbegin(), model.constraints.cend(),
                       [&a, &b](const sketch::ConstraintData& k) {
                         return k.active && k.type == QStringLiteral("coincident") && k.refs.size() == 2 &&
                                k.refs.at(0) == a && k.refs.at(1) == b;
                       });
  };
  for (const sketch::CurveData& other : model.curves) {
    if (other.id == own.id || other.construction != own.construction || !filletCurve(other)) {
      continue;
    }
    for (const QString& end : {other.start, other.end}) {
      const std::optional<sketch::V2> there = model.pointAt(end);
      if (end.isEmpty() || end == point || !there || sketch::dist(*at, *there) > 0.01) {
        continue;
      }
      if (holds(point, end) || holds(end, point) || holds(point, other.id) || holds(end, own.id)) {
        return true;
      }
    }
  }
  return false;
}

// A point of the sketch where exactly two lines, arcs or conics of one kind
// end (the drawn ones, else the construction ones), or one of them whose end
// is held to another's (heldCorner): a corner Sketch Fillet rounds
// (mitcad#867, arcs mitcad#1192; the model's `sketch.fillet` with `points`).
bool filletCorner(const sketch::SketchModel& model, const QString& point) {
  for (const bool construction : {false, true}) {
    int lines = 0;
    int others = 0;
    const sketch::CurveData* ending = nullptr;
    for (const sketch::CurveData& curve : model.curves) {
      if (curve.construction == construction && (curve.start == point || curve.end == point)) {
        const bool rounds = filletCurve(curve);
        ++(rounds ? lines : others);
        if (rounds) {
          ending = &curve;
        }
      }
    }
    if (lines + others > 0) {
      if (lines == 1 && others == 0 && ending != nullptr) {
        return heldCorner(model, point, *ending);
      }
      return lines == 2 && others == 0;
    }
  }
  return false;
}

// Sketch Fillet (mitcad#867): two lines, a closed chain of lines (a whole
// rectangle), lines and corner points, several corners at once with one
// radius dimension and the arcs Equal; one undo step. Circular arcs and
// conics can be rounded where they end with another curve.
void addFillet(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.fillet", QObject::tr("Sketch Fillet"), "sketch-fillet",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("Rounds corners of lines, arcs and conics: two curves, a chain or corner points");
  def.keywords = {QStringLiteral("round corners"), QStringLiteral("rounded rectangle")};
  def.inputs = {
      selectionInput(QStringLiteral("corners"), QObject::tr("Curves or Corners"),
                     SelectKind::SketchCurve | SelectKind::SketchPoint, 1, 0)
          .withAccepts([&c](const SelectionItem& item) {
            if (item.owner != c.uid()) {
              return false;
            }
            if (item.geometry == QStringLiteral("line") || item.geometry == QStringLiteral("arc") ||
                item.geometry == QStringLiteral("conic")) {
              return true;
            }
            return c.model().pointAt(item.name).has_value() && filletCorner(c.model(), item.name);
          })
          .withTooltip(QObject::tr("Two curves: their corner; more: each corner where two end; a point: its corner")),
      valueInput(QStringLiteral("radius"), QObject::tr("Radius"), ValueKind::Length, QStringLiteral("2")).withStepMinimum(0.0, true),
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    QJsonArray curves;
    QJsonArray points;
    for (const SelectionItem& item : s.items(QStringLiteral("corners"))) {
      (c.model().pointAt(item.name) ? points : curves).append(item.name);
    }
    if (points.isEmpty() && curves.size() < 2) {
      return Built::failure(QObject::tr("Pick two lines, arcs or conics, a chain of them or corner points."),
                            QStringLiteral("corners"));
    }
    QJsonObject cmd = base(c, "sketch.fillet");
    if (!curves.isEmpty()) {
      cmd.insert(QStringLiteral("curves"), curves);
    }
    if (!points.isEmpty()) {
      cmd.insert(QStringLiteral("points"), points);
    }
    cmd.insert(QStringLiteral("radius"), s.expression(QStringLiteral("radius")));
    return Built::of(cmd);
  };
  def.describe = [](const CommandState& s, const CommandContext&) {
    const Selection& picked = s.items(QStringLiteral("corners"));
    QStringList ids;
    for (const SelectionItem& item : picked) {
      ids << item.name;
    }
    if (ids.size() == 2) {
      return QStringLiteral("Sketch fillet of %1 and %2, radius %3")
          .arg(ids[0], ids[1], s.expression(QStringLiteral("radius")));
    }
    return QStringLiteral("Sketch fillet of %1, radius %2")
        .arg(ids.join(QStringLiteral(", ")), s.expression(QStringLiteral("radius")));
  };
  registry.add(def);
}

void addChamfer(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.chamfer", QObject::tr("Sketch Chamfer"), "sketch-chamfer",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("Bevels the corner of two lines");
  const QVector<QPair<QString, QString>> types = {
      {QStringLiteral("equal_distance"), QObject::tr("Equal Distance")},
      {QStringLiteral("two_distances"), QObject::tr("Distance and Distance")},
      {QStringLiteral("distance_angle"), QObject::tr("Distance and Angle")}};
  def.inputs = {
      selectionInput(QStringLiteral("a"), QObject::tr("First Line"), SelectKind::SketchCurve, 1, 1)
          .withAccepts(linesOf(c)),
      selectionInput(QStringLiteral("b"), QObject::tr("Second Line"), SelectKind::SketchCurve, 1, 1)
          .withAccepts(linesOf(c)),
      choiceInput(QStringLiteral("type"), QObject::tr("Chamfer Type"), types, QStringLiteral("equal_distance")),
      valueInput(QStringLiteral("distance"), QObject::tr("Distance"), ValueKind::Length, QStringLiteral("2")).withStepMinimum(0.0, true),
      valueInput(QStringLiteral("distance2"), QObject::tr("Distance 2"), ValueKind::Length, QStringLiteral("2")).withStepMinimum(0.0, true)
          .withVisible([](const CommandState& s) { return s.choice(QStringLiteral("type")) == QStringLiteral("two_distances"); }),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("45 deg")).withStepMinimum(0.0, true)
          .withVisible([](const CommandState& s) { return s.choice(QStringLiteral("type")) == QStringLiteral("distance_angle"); }),
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    QJsonObject cmd = base(c, "sketch.chamfer");
    cmd.insert(QStringLiteral("a"), s.items(QStringLiteral("a")).first().name);
    cmd.insert(QStringLiteral("b"), s.items(QStringLiteral("b")).first().name);
    cmd.insert(QStringLiteral("distance"), s.expression(QStringLiteral("distance")));
    const QString type = s.choice(QStringLiteral("type"));
    if (type == QStringLiteral("two_distances")) {
      cmd.insert(QStringLiteral("distance2"), s.expression(QStringLiteral("distance2")));
    } else if (type == QStringLiteral("distance_angle")) {
      cmd.insert(QStringLiteral("angle"), s.expression(QStringLiteral("angle")));
    }
    return Built::of(cmd);
  };
  registry.add(def);
}

// Blend Curve (mitcad#2107): the curves it joins are open lines, arcs,
// elliptical arcs, splines and conics (mitcad#2236); a circle, an ellipse
// or a closed spline has no end it can join.
bool blendable(const sketch::CurveData& curve) {
  const bool open = curve.isLine() || curve.isArc() || curve.isSpline() || curve.isConic() ||
                    curve.type == QStringLiteral("elliptical_arc");
  return open && !curve.closed && !curve.start.isEmpty() &&
         !curve.end.isEmpty() && curve.start != curve.end;
}

// Where a Blend Curve input's curve was picked, in sketch coordinates: the
// end nearest it is joined. Where the pointer clicked on the sketch plane;
// the view's pick place, off by up to its pick tolerance (about a
// millimetre at 5 px per mm, which near a quarter arc's middle chose the
// wrong end), only when the click's place is not known. None for a pick
// without its place (a selection made by an agent).
std::optional<sketch::V2> blendPick(SketchController& c, const SelectionItem& item) {
  if (!item.at) {
    return std::nullopt;
  }
  gp_Pnt at((*item.at)[0], (*item.at)[1], (*item.at)[2]);
  // The pick is in its component's coordinates, the sketch's frame in the
  // design's (mitcad#505).
  at.Transform(c.host().sketchPlacement(c.uid()));
  const sketch::V2 picked = sketch::toSketch(c.model().frame, at);
  return c.clickPlace(item.name, picked).value_or(picked);
}

// The end of a Blend Curve input's curve nearest its pick, for the panel's
// marker; none before both are there.
std::optional<sketch::V2> blendEnd(SketchController& c, const Selection& items) {
  if (items.size() != 1) {
    return std::nullopt;
  }
  const sketch::CurveData* curve = c.model().curve(items.first().name);
  const auto pick = blendPick(c, items.first());
  if (curve == nullptr || !pick) {
    return std::nullopt;
  }
  const sketch::V2 start = curve->curve.startPoint();
  const sketch::V2 end = curve->curve.endPoint();
  return sketch::dist(*pick, end) < sketch::dist(*pick, start) ? end : start;
}

// Blend Curve: a spline that joins the ends of two curves with Tangent (G1)
// or Curvature (G2) continuity and stays attached to them. Each curve is
// picked near the end to join, the end marked in the view; one curve
// picked twice joins its two ends across the gap.
void addBlendCurve(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.blend_curve", QObject::tr("Blend Curve"), "blend-curve",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("Joins the ends of two curves with a smooth spline that stays attached to them");
  def.keywords = {QStringLiteral("blend"), QStringLiteral("connect"), QStringLiteral("smooth"),
                  QStringLiteral("curvature continuous")};
  const auto curveInput = [&c](const char* id, const QString& label) {
    return selectionInput(QString::fromLatin1(id), label, SelectKind::SketchCurve, 1, 1)
        .withAccepts([&c](const SelectionItem& item) {
          const sketch::CurveData* curve = item.owner == c.uid() ? c.model().curve(item.name) : nullptr;
          return curve != nullptr && blendable(*curve);
        })
        .withRefusedHint([](const SelectionItem&, const CommandState&) {
          return QObject::tr("A closed curve has no end to join: pick a line, an arc or an open spline.");
        })
        .withTooltip(QObject::tr("Pick the curve near the end to join"));
  };
  const QVector<QPair<QString, QString>> continuity = {{QStringLiteral("tangent"), QObject::tr("Tangent")},
                                                       {QStringLiteral("curvature"), QObject::tr("Curvature")}};
  def.inputs = {
      curveInput("curve1", QObject::tr("Curve 1")),
      curveInput("curve2", QObject::tr("Curve 2")),
      choiceInput(QStringLiteral("continuity"), QObject::tr("Continuity"), continuity, QStringLiteral("tangent"))
          .withTooltip(QObject::tr("Tangent: the blend leaves each curve in its direction; Curvature: with its "
                                   "curvature as well")),
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    const Selection& first = s.items(QStringLiteral("curve1"));
    const Selection& second = s.items(QStringLiteral("curve2"));
    if (first.size() != 1) {
      return Built::failure(QObject::tr("Pick the first curve near the end to join."), QStringLiteral("curve1"));
    }
    if (second.size() != 1) {
      return Built::failure(QObject::tr("Pick the second curve near the end to join."), QStringLiteral("curve2"));
    }
    const auto at1 = blendPick(c, first.first());
    const auto at2 = blendPick(c, second.first());
    if (!at1) {
      return Built::failure(QObject::tr("Pick the first curve again, near the end to join."), QStringLiteral("curve1"));
    }
    if (!at2) {
      return Built::failure(QObject::tr("Pick the second curve again, near the end to join."), QStringLiteral("curve2"));
    }
    QJsonObject cmd = base(c, "sketch.blend_curve");
    cmd.insert(QStringLiteral("curve1"), first.first().name);
    cmd.insert(QStringLiteral("at1"), QJsonArray{at1->x, at1->y});
    cmd.insert(QStringLiteral("curve2"), second.first().name);
    cmd.insert(QStringLiteral("at2"), QJsonArray{at2->x, at2->y});
    cmd.insert(QStringLiteral("continuity"), s.choice(QStringLiteral("continuity")));
    return Built::of(cmd);
  };
  // The ends joined, marked while the panel is open.
  def.previewMarkersIn = [&c](const QJsonObject&, const CommandState& s) {
    std::vector<PreviewMarker> markers;
    const std::array<std::pair<const char*, QString>, 2> inputs = {
        std::pair<const char*, QString>{"curve1", QObject::tr("End 1")},
        std::pair<const char*, QString>{"curve2", QObject::tr("End 2")}};
    for (const auto& [id, label] : inputs) {
      if (const auto end = blendEnd(c, s.items(QString::fromLatin1(id)))) {
        markers.push_back({label, c.model().place(*end)});
      }
    }
    return markers;
  };
  def.describe = [&c](const CommandState& s, const CommandContext&) {
    QStringList ends;
    for (const char* id : {"curve1", "curve2"}) {
      const Selection& items = s.items(QString::fromLatin1(id));
      const sketch::CurveData* curve = items.size() == 1 ? c.model().curve(items.first().name) : nullptr;
      const auto end = blendEnd(c, items);
      if (curve == nullptr || !end) {
        ends << QStringLiteral("?");
        continue;
      }
      const bool atEnd = sketch::dist(*end, curve->curve.endPoint()) < sketch::dist(*end, curve->curve.startPoint());
      ends << QStringLiteral("%1 %2").arg(curve->id, atEnd ? QStringLiteral("end") : QStringLiteral("start"));
    }
    return QStringLiteral("Blend curve from %1 to %2, %3")
        .arg(ends.value(0), ends.value(1), s.choice(QStringLiteral("continuity")));
  };
  registry.add(def);
}

// Where the points of sketch entities are, each point once, in the order
// the entities name them: "p2 (-20.000, 0.000), p3 (20.000, 10.000)"; a
// text by its anchor, "t5 (10.000, 0.000)" (mitcad#1911); "none" without
// points.
QString pointPositions(const sketch::SketchModel& model, const QStringList& entities) {
  // No "-0.000" for a coordinate the solver left a hair below zero.
  const auto coordinate = [](double v) { return QString::number(std::abs(v) < 5e-4 ? 0.0 : v, 'f', 3); };
  QStringList seen;
  QStringList shown;
  const auto add = [&model, &coordinate, &seen, &shown](const QString& id) {
    if (seen.contains(id)) {
      return;
    }
    seen << id;
    if (const sketch::PointData* p = model.point(id)) {
      shown << QStringLiteral("%1 (%2, %3)").arg(id, coordinate(p->at.x), coordinate(p->at.y));
    } else if (const auto anchor = sketch::textAnchor(model, id)) {
      shown << QStringLiteral("%1 (%2, %3)").arg(id, coordinate(anchor->x), coordinate(anchor->y));
    }
  };
  for (const QString& id : entities) {
    if (const sketch::CurveData* curve = model.curve(id)) {
      for (const QString& point : curve->points) {
        add(point);
      }
    } else {
      add(id);
    }
  }
  return shown.isEmpty() ? QStringLiteral("none") : shown.join(QStringLiteral(", "));
}

// A point input of a sketch command picked with the sketch's snaps
// (mitcad#1563): an existing point by its id, a place where the snap found
// no point (sketch::placePick) as [x, y].
QJsonValue snapPickInput(const SelectionItem& item) {
  if (const auto at = sketch::pickedPlace(item.name)) {
    return QJsonArray{at->x, at->y};
  }
  return item.name;
}

// Move/Copy starts at 0 (mitcad#1163): X and Y Distance 0 mm, so the
// preview that runs as the panel opens leaves the objects where they are
// until a distance is typed. Rotate's Angle keeps its 90 deg (a separate
// type the user chooses). Point to Point (mitcad#1563) moves by the Target
// Point less the Origin Point, both picked with the sketch's snaps; the
// model reads where they are before the move, so a point of the moved
// objects is where the preview has not moved it yet. Translate shows a
// handle at the objects' middle that drags X and Y Distance, and Rotate
// turns about that middle when no Pivot Point is picked.
void addMove(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.move", QObject::tr("Move/Copy"), "move",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.shortcut = QKeySequence(Qt::Key_M);
  def.tooltip = QObject::tr("Moves or copies sketch geometry and texts; a move keeps its constraints");
  const QVector<QPair<QString, QString>> types = {{QStringLiteral("translate"), QObject::tr("Translate")},
                                                   {QStringLiteral("rotate"), QObject::tr("Rotate")},
                                                   {QStringLiteral("point_to_point"), QObject::tr("Point to Point")}};
  const auto typeIs = [](const char* type) {
    const QString wanted = QString::fromLatin1(type);
    return [wanted](const CommandState& s) {
      // Translate also before the choice has a value.
      const QString chosen = s.choice(QStringLiteral("type"));
      return chosen == wanted || (chosen.isEmpty() && wanted == QStringLiteral("translate"));
    };
  };
  // The points of Point to Point: a point of the sketch, or a place the
  // snaps found (a line's middle, a quadrant, a crossing, the grid).
  const auto snapPick = [&c](const QString& id, const QString& label, const QString& tip) {
    InputDef input = selectionInput(id, label, SelectKind::SketchPoint, 1, 1)
                         .withAccepts([&c](const SelectionItem& item) {
                           return item.owner == c.uid() &&
                                  (c.model().point(item.name) != nullptr || sketch::pickedPlace(item.name));
                         })
                         .withTooltip(tip);
    input.sketchSnaps = true;
    return input;
  };
  def.inputs = {
      // Texts and QR codes too (mitcad#1911): the model moves a text's
      // anchor, or the rectangle of a text in a frame.
      selectionInput(QStringLiteral("entities"), QObject::tr("Objects"),
                     SelectKind::SketchCurve | SelectKind::SketchPoint, 1, 0)
          .withAccepts([&c](const SelectionItem& item) { return item.owner == c.uid(); }),
      choiceInput(QStringLiteral("type"), QObject::tr("Move Type"), types, QStringLiteral("translate")),
      valueInput(QStringLiteral("dx"), QObject::tr("X Distance"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(typeIs("translate")),
      valueInput(QStringLiteral("dy"), QObject::tr("Y Distance"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(typeIs("translate")),
      snapPick(QStringLiteral("from"), QObject::tr("Origin Point"),
               QObject::tr("Where the objects move from: a point, an end, a middle or a center the pointer snaps to"))
          .withVisible(typeIs("point_to_point")),
      snapPick(QStringLiteral("to"), QObject::tr("Target Point"),
               QObject::tr("Where the Origin Point moves to: a point, an end, a middle or a center"))
          .withVisible(typeIs("point_to_point")),
      selectionInput(QStringLiteral("center"), QObject::tr("Pivot Point"), SelectKind::SketchPoint, 0, 1)
          .withAccepts(ofSketch(c))
          .withTooltip(QObject::tr("The point to turn about; none: the middle of the objects"))
          .withVisible(typeIs("rotate")),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("90 deg"))
          .withVisible(typeIs("rotate")),
      checkInput(QStringLiteral("copy"), QObject::tr("Create Copy"), false),
  };
  // The objects of the last definition built, for the preview's log line.
  const auto objects = std::make_shared<QStringList>();
  def.build = [&c, objects](const CommandState& s, const CommandContext&) -> Built {
    QJsonObject cmd = base(c, "sketch.move");
    const Selection& entities = s.items(QStringLiteral("entities"));
    objects->clear();
    for (const SelectionItem& item : entities) {
      *objects << item.name;
    }
    // A text on a path follows its curve (mitcad#1911): the model refuses
    // it, the panel says so on the Objects.
    if (!sketch::textOnPath(c.model(), *objects).isEmpty()) {
      return Built::failure(QObject::tr("A text on a path follows its curve: move the curve instead."),
                            QStringLiteral("entities"));
    }
    const sketch::MoveObjects split = sketch::moveObjects(c.model(), *objects);
    cmd.insert(QStringLiteral("entities"), QJsonArray::fromStringList(split.entities));
    if (!split.texts.isEmpty()) {
      cmd.insert(QStringLiteral("texts"), QJsonArray::fromStringList(split.texts));
    }
    const QString type = s.choice(QStringLiteral("type"));
    if (type == QStringLiteral("rotate")) {
      // No Pivot Point: the model turns about the objects' middle
      // (mitcad#1563; the sketch's origin before, which could turn them
      // out of sight).
      QJsonObject rotate{{QStringLiteral("angle"), s.value(QStringLiteral("angle"))}};
      const Selection& center = s.items(QStringLiteral("center"));
      if (!center.isEmpty()) {
        rotate.insert(QStringLiteral("center"), center.first().name);
      }
      cmd.insert(QStringLiteral("rotate"), rotate);
    } else if (type == QStringLiteral("point_to_point")) {
      const Selection& from = s.items(QStringLiteral("from"));
      const Selection& to = s.items(QStringLiteral("to"));
      if (from.isEmpty() || to.isEmpty()) {
        return Built::failure(QObject::tr("Pick the Origin Point and the Target Point."),
                              from.isEmpty() ? QStringLiteral("from") : QStringLiteral("to"));
      }
      cmd.insert(QStringLiteral("from"), snapPickInput(from.first()));
      cmd.insert(QStringLiteral("to"), snapPickInput(to.first()));
    } else {
      cmd.insert(QStringLiteral("by"), QJsonArray{s.value(QStringLiteral("dx")), s.value(QStringLiteral("dy"))});
    }
    // Held where it was dropped on a point or a curve (mitcad#1645): the
    // Origin Point where the Target Point snapped, or the objects' point
    // at the handle's place where its drag dropped it; the copy's point
    // with Create Copy (the model's `hold`).
    std::vector<QJsonObject> hold;
    if (type == QStringLiteral("point_to_point")) {
      hold = c.pickHold(*objects, s.items(QStringLiteral("from")).first().name,
                        s.items(QStringLiteral("to")).first().name);
    } else if (type != QStringLiteral("rotate")) {
      hold = c.handleHold(*objects, s.value(QStringLiteral("dx")), s.value(QStringLiteral("dy")));
    }
    if (!hold.empty()) {
      QJsonArray holds;
      for (const QJsonObject& k : hold) {
        holds.append(k);
      }
      cmd.insert(QStringLiteral("hold"), holds);
    }
    cmd.insert(QStringLiteral("copy"), s.checked(QStringLiteral("copy")));
    return Built::of(cmd);
  };
  // Translate's handle (mitcad#1563): an arrow pair at the objects' middle
  // where the preview shows them, which drags X and Y Distance.
  def.sketchHandle = [&c](const CommandState& s) -> std::optional<SketchHandle> {
    const QString type = s.choice(QStringLiteral("type"));
    if (!type.isEmpty() && type != QStringLiteral("translate")) {
      return std::nullopt;
    }
    QStringList moved;
    for (const SelectionItem& item : s.items(QStringLiteral("entities"))) {
      moved << item.name;
    }
    const auto middle = sketch::middleOf(c.model(), moved);
    if (!middle) {
      return std::nullopt;
    }
    const auto valueOf = [&s](const char* key) {
      const double value = s.value(QString::fromLatin1(key));
      return std::isfinite(value) ? value : 0.0;
    };
    SketchHandle handle;
    handle.at = {middle->x, middle->y};
    handle.x = QStringLiteral("dx");
    handle.y = QStringLiteral("dy");
    handle.values = {valueOf("dx"), valueOf("dy")};
    handle.moves = moved;
    return handle;
  };
  // Where the objects' points are once a preview is shown (the sketch read
  // again), for UI tests: "Preview Move/Copy positions: p2 (-20.000,
  // 0.000), p3 (20.000, 10.000)". Not when a preview is taken back or
  // failed (an empty answer).
  const QString commandName = def.name;
  def.sketchPreviewShown = [&c, objects, commandName](const QJsonObject& answer) {
    if (answer.isEmpty()) {
      return;
    }
    qDebug().noquote() << QStringLiteral("Preview %1 positions: %2").arg(commandName, pointPositions(c.model(), *objects));
  };
  registry.add(def);
}

// The sizes of sketch curves for UI tests (mitcad#2109): "c3 radius 7.500,
// c5 length 15.000, k8 15.000", a circle's or an arc's radius, a line's
// length, then what the dimensions on them or their points measure.
QString curveSizes(const sketch::SketchModel& model, const QStringList& ids) {
  const auto fixed3 = [](double v) { return QString::number(v, 'f', 3); };
  QStringList shown;
  QSet<QString> refs;
  for (const QString& id : ids) {
    refs.insert(id);
    const sketch::CurveData* curve = model.curve(id);
    if (curve == nullptr) {
      continue;
    }
    for (const QString& point : curve->points) {
      refs.insert(point);
    }
    if (curve->isCircular()) {
      shown << QStringLiteral("%1 radius %2").arg(id, fixed3(curve->curve.radius));
    } else if (curve->isLine()) {
      const double length = std::hypot(curve->curve.b.x - curve->curve.a.x, curve->curve.b.y - curve->curve.a.y);
      shown << QStringLiteral("%1 length %2").arg(id, fixed3(length));
    }
  }
  for (const sketch::DimensionData& dimension : model.dimensions) {
    const bool on = std::any_of(dimension.refs.begin(), dimension.refs.end(),
                                [&refs](const QString& ref) { return refs.contains(ref); });
    if (on) {
      shown << QStringLiteral("%1 %2").arg(dimension.id, fixed3(dimension.measured));
    }
  }
  return shown.isEmpty() ? QStringLiteral("none") : shown.join(QStringLiteral(", "));
}

// Sketch Scale (mitcad#2109): the Objects (curves, points and texts, the
// selection taken when the command starts) grow or shrink by the Scale
// Factor about the Base Point, a point of the sketch or a place the snaps
// found, the sketch origin when none is picked. The factor is read once
// (an expression gives its value now). Dimensions: Scale with Geometry
// multiplies the driving lengths among the objects, Keep Values keeps them
// and the model solves the scaled geometry with them. Create Copy scales
// copies and leaves the objects. The model's `sketch.scale` with
// `entities`; what tied the objects to the rest and no longer holds is
// removed, which the preview's notes say.
void addScale(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.scale", QObject::tr("Sketch Scale"), "sketch-scale",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("Scales sketch geometry and texts about a base point");
  def.keywords = {QStringLiteral("resize"), QStringLiteral("enlarge"), QStringLiteral("shrink")};
  const QVector<QPair<QString, QString>> dimensionChoices = {
      {QStringLiteral("scale"), QObject::tr("Scale with Geometry")},
      {QStringLiteral("keep"), QObject::tr("Keep Values")}};
  InputDef basePoint = selectionInput(QStringLiteral("center"), QObject::tr("Base Point"), SelectKind::SketchPoint, 0, 1)
                           .withAccepts([&c](const SelectionItem& item) {
                             return item.owner == c.uid() &&
                                    (c.model().point(item.name) != nullptr || sketch::pickedPlace(item.name));
                           })
                           .withTooltip(QObject::tr("The point that stays: a point, an end, a middle or a center; "
                                                    "none: the sketch origin"));
  basePoint.sketchSnaps = true;
  def.inputs = {
      selectionInput(QStringLiteral("entities"), QObject::tr("Objects"),
                     SelectKind::SketchCurve | SelectKind::SketchPoint, 1, 0)
          .withAccepts([&c](const SelectionItem& item) { return item.owner == c.uid(); }),
      basePoint,
      valueInput(QStringLiteral("factor"), QObject::tr("Scale Factor"), ValueKind::Unitless, QStringLiteral("1"))
          .withTooltip(QObject::tr("Greater than 1 enlarges, less than 1 shrinks; an expression is read once")),
      choiceInput(QStringLiteral("dimensions"), QObject::tr("Dimensions"), dimensionChoices, QStringLiteral("scale")),
      checkInput(QStringLiteral("copy"), QObject::tr("Create Copy"), false),
  };
  // The objects of the last definition built, for the preview's log lines.
  const auto objects = std::make_shared<QStringList>();
  def.build = [&c, objects](const CommandState& s, const CommandContext&) -> Built {
    QJsonObject cmd = base(c, "sketch.scale");
    objects->clear();
    for (const SelectionItem& item : s.items(QStringLiteral("entities"))) {
      *objects << item.name;
    }
    const double factor = s.value(QStringLiteral("factor"));
    if (!std::isfinite(factor) || factor <= 0.0) {
      return Built::failure(QObject::tr("The scale factor must be greater than zero."), QStringLiteral("factor"));
    }
    cmd.insert(QStringLiteral("entities"), QJsonArray::fromStringList(*objects));
    cmd.insert(QStringLiteral("factor"), factor);
    const Selection& center = s.items(QStringLiteral("center"));
    if (!center.isEmpty()) {
      cmd.insert(QStringLiteral("center"), snapPickInput(center.first()));
    }
    const QString dimensions = s.choice(QStringLiteral("dimensions"));
    cmd.insert(QStringLiteral("dimensions"), dimensions.isEmpty() ? QStringLiteral("scale") : dimensions);
    cmd.insert(QStringLiteral("copy"), s.checked(QStringLiteral("copy")));
    return Built::of(cmd);
  };
  // For UI tests, once a preview is shown: "Preview Sketch Scale
  // positions: p4 (-10.000, 0.000), ...", "Preview Sketch Scale sizes: c3
  // radius 7.500, k8 15.000" and, with Create Copy, "Preview Sketch Scale
  // copies: c12 radius 7.500". The notes of the answer (the ties to the
  // rest removed) are shown as every sketch command's are.
  const QString commandName = def.name;
  def.sketchPreviewShown = [&c, objects, commandName](const QJsonObject& answer) {
    if (answer.isEmpty()) {
      return;
    }
    qDebug().noquote() << QStringLiteral("Preview %1 positions: %2").arg(commandName, pointPositions(c.model(), *objects));
    qDebug().noquote() << QStringLiteral("Preview %1 sizes: %2").arg(commandName, curveSizes(c.model(), *objects));
    QStringList made;
    for (const QJsonValue& id : answer.value(QStringLiteral("made")).toArray()) {
      made << id.toString();
    }
    if (!made.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Preview %1 copies: %2").arg(commandName, curveSizes(c.model(), made));
    }
  };
  registry.add(def);
}

// --- Patterns, offsets and texts edited afterwards (P3, P4)

// The item of the selection an edit panel works on, by the model's lookup.
template <class Lookup>
std::function<bool(const SelectionItem&)> editable(SketchController& c, Lookup lookup) {
  return [&c, lookup](const SelectionItem& item) {
    return item.owner == c.uid() && lookup(c.model(), item.name) != nullptr;
  };
}

// A text's input also takes a region its contours bound, selected before
// the command (the context menu's or a double-click's Edit Text inside a
// letter or its counter, mitcad#1369), as the text's curve; clicks in the
// panel still pick its outline.
InputDef textInputFor(InputDef input, SketchController& c) {
  return input.withAlsoTakes(SelectKind::Profile).withConvert([&c](const SelectionItem& item) {
    return sketch::textSelectionItem(c.model(), item);
  });
}

void addEditPattern(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.edit_pattern", QObject::tr("Edit Pattern"), "circular-pattern",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("The quantity, angle or spacing of a sketch pattern, picked by one of its curves");
  def.keywords = {QStringLiteral("pattern quantity")};
  const auto lookup = [](const sketch::SketchModel& m, const QString& id) { return m.patternOf(id); };
  const auto circular = [](const CommandState& s) { return s.choice(QStringLiteral("kind")) == QStringLiteral("circular"); };
  const auto rectangular = [](const CommandState& s) {
    return s.choice(QStringLiteral("kind")) == QStringLiteral("rectangular");
  };
  // The pattern's second direction as the panel opened (mitcad#1969):
  // kept in the state, as the sketch shown during a preview already has the
  // previewed one (none after At Right Angles).
  const auto storedDirection2 = [](const CommandState& s) -> std::optional<sketch::V2> {
    const QJsonArray v = s.object(QStringLiteral("stored")).value(QStringLiteral("direction2")).toArray();
    if (v.size() != 2) {
      return std::nullopt;
    }
    return sketch::V2{v.at(0).toDouble(), v.at(1).toDouble()};
  };
  InputDef direction2 = lineOrAxisInput(QStringLiteral("direction2"), QObject::tr("Direction 2"), 0, c)
                            .withTooltip(QObject::tr("A line or a sketch axis; none picked: the pattern's own"))
                            .withVisible([rectangular](const CommandState& s) {
                              return rectangular(s) && !s.checked(QStringLiteral("square2"));
                            });
  // Loaded from the pattern (mitcad#1969): an empty row says the stored
  // second direction stays.
  direction2.emptyShows = [storedDirection2](const CommandState& s) {
    return std::make_pair(storedDirection2(s) ? QObject::tr("As Stored") : QString(), QColor());
  };
  def.inputs = {
      selectionInput(QStringLiteral("entity"), QObject::tr("Pattern"), SelectKind::SketchCurve | SelectKind::SketchPoint,
                     1, 1)
          .withAccepts(editable(c, lookup)),
      valueInput(QStringLiteral("quantity"), QObject::tr("Quantity"), ValueKind::Unitless, QStringLiteral("3")).withStepMinimum(1),
      valueInput(QStringLiteral("angle"), QObject::tr("Total Angle"), ValueKind::Angle, QStringLiteral("360 deg"))
          .withVisible(circular),
      valueInput(QStringLiteral("distance1"), QObject::tr("Spacing"), ValueKind::Length, QStringLiteral("10"))
          .withVisible(rectangular),
      flipInput(QStringLiteral("flip1"), QObject::tr("Flip"))
          .withTooltip(QObject::tr("Copies against the direction"))
          .withVisible(rectangular),
      // The second direction (mitcad#1969): at right angles to the
      // pattern's direction, or along a picked line or axis; with none
      // picked, the stored one is kept.
      checkInput(QStringLiteral("square2"), QObject::tr("At Right Angles"), true)
          .withTooltip(QObject::tr("The second direction at right angles to the first; off: along Direction 2"))
          .withVisible(rectangular),
      direction2,
      valueInput(QStringLiteral("quantity2"), QObject::tr("Quantity 2"), ValueKind::Unitless, QStringLiteral("1")).withStepMinimum(1)
          .withTooltip(acrossDirection())
          .withVisible(rectangular),
      valueInput(QStringLiteral("distance2"), QObject::tr("Spacing 2"), ValueKind::Length, QStringLiteral("10"))
          .withTooltip(acrossDirection())
          .withVisible(rectangular),
      flipInput(QStringLiteral("flip2"), QObject::tr("Flip 2"))
          .withTooltip(QObject::tr("Copies against the second direction"))
          .withVisible(rectangular),
  };
  def.init = [&c](CommandState& s, const CommandContext&) {
    const Selection& picked = s.items(QStringLiteral("entity"));
    const sketch::PatternData* p = picked.isEmpty() ? nullptr : c.model().patternOf(picked.first().name);
    if (p == nullptr) {
      return;
    }
    s.setChoice(QStringLiteral("kind"), p->type);
    s.setText(QStringLiteral("quantity"), QString::number(p->count));
    if (p->type == QStringLiteral("circular")) {
      s.setText(QStringLiteral("angle"), p->values.value(0));
    } else {
      s.setText(QStringLiteral("quantity2"), QString::number(p->count2));
      s.setChecked(QStringLiteral("square2"), !p->direction2.has_value());
      s.setItems(QStringLiteral("direction2"), {});
      QJsonObject stored{{QStringLiteral("direction"), QJsonArray{p->direction.x, p->direction.y}}};
      if (p->direction2) {
        stored.insert(QStringLiteral("direction2"), QJsonArray{p->direction2->x, p->direction2->y});
      }
      s.setObject(QStringLiteral("stored"), stored);
      s.setText(QStringLiteral("distance1"), p->values.value(0));
      s.setText(QStringLiteral("distance2"), p->values.value(1));
      s.setChecked(QStringLiteral("flip1"), false);
      s.setChecked(QStringLiteral("flip2"), false);
    }
  };
  def.build = [&c, storedDirection2](const CommandState& s, const CommandContext&) -> Built {
    const Selection& picked = s.items(QStringLiteral("entity"));
    const sketch::PatternData* p = picked.isEmpty() ? nullptr : c.model().patternOf(picked.first().name);
    if (p == nullptr) {
      return Built::failure(QObject::tr("Pick a curve of a pattern."), QStringLiteral("entity"));
    }
    const std::optional<sketch::V2> stored2 = storedDirection2(s);
    const int n1 = static_cast<int>(std::lround(s.value(QStringLiteral("quantity"))));
    QJsonObject cmd = base(c, "sketch.edit_pattern");
    cmd.insert(QStringLiteral("pattern"), p->id);
    if (p->type == QStringLiteral("circular")) {
      if (n1 < 2) {
        return Built::failure(QObject::tr("The quantity must be at least 2."), QStringLiteral("quantity"));
      }
      cmd.insert(QStringLiteral("count"), n1);
      cmd.insert(QStringLiteral("angle"), s.expression(QStringLiteral("angle")));
    } else {
      const int n2 = static_cast<int>(std::lround(s.value(QStringLiteral("quantity2"))));
      if (n1 < 1 || n2 < 1 || n1 * n2 < 2) {
        return Built::failure(QObject::tr("The quantities make no copies."), QStringLiteral("quantity"));
      }
      cmd.insert(QStringLiteral("count"), QJsonArray{n1, n2});
      cmd.insert(QStringLiteral("spacing"), signedSpacings(s));
      if (s.checked(QStringLiteral("square2"))) {
        if (stored2) {
          cmd.insert(QStringLiteral("direction2"), QJsonValue::Null);
        }
      } else if (const std::optional<sketch::V2> v = patternDirection2(s, c)) {
        if (parallelDirections(p->direction, *v)) {
          return Built::failure(QObject::tr("Direction 2 runs along the pattern's direction: pick a line at an angle "
                                            "to it."),
                                QStringLiteral("direction2"));
        }
        cmd.insert(QStringLiteral("direction2"), QJsonArray{v->x, v->y});
      } else if (stored2) {
        // As Stored: sent again, as a preview of At Right Angles may have
        // taken it away from the sketch shown.
        cmd.insert(QStringLiteral("direction2"), QJsonArray{stored2->x, stored2->y});
      } else {
        return Built::failure(QObject::tr("Pick a line or a sketch axis for Direction 2."), QStringLiteral("direction2"));
      }
    }
    return Built::of(cmd);
  };
  def.describe = [&c](const CommandState& s, const CommandContext&) {
    const Selection& picked = s.items(QStringLiteral("entity"));
    const sketch::PatternData* p = picked.isEmpty() ? nullptr : c.model().patternOf(picked.first().name);
    return QStringLiteral("Edited pattern %1: %2 in all")
        .arg(p != nullptr ? p->id : QString())
        .arg(std::lround(s.value(QStringLiteral("quantity"))) *
             (s.choice(QStringLiteral("kind")) == QStringLiteral("rectangular")
                  ? std::lround(s.value(QStringLiteral("quantity2")))
                  : 1));
  };
  // Where the copies are once a preview is shown, as Rectangular Pattern
  // logs them (mitcad#1969: the stored second direction kept): "Preview
  // Edit Pattern copies: p9 (-10.000, -15.000), …". The pattern is the one
  // whose copies the answer lists (`made`).
  const QString commandName = def.name;
  def.sketchPreviewShown = [&c, commandName](const QJsonObject& answer) {
    const QJsonArray made = answer.value(QStringLiteral("made")).toArray();
    const sketch::PatternData* p = made.isEmpty() ? nullptr : c.model().patternOf(made.at(0).toString());
    if (p == nullptr) {
      return;
    }
    qDebug().noquote() << QStringLiteral("Preview %1 copies: %2").arg(commandName, pointPositions(c.model(), p->copies));
  };
  registry.add(def);
}

void addEditOffset(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.edit_offset", QObject::tr("Edit Offset"), "offset",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("The distance, side or corners of an offset, picked by one of the curves it made");
  const auto lookup = [](const sketch::SketchModel& m, const QString& id) { return m.offsetOf(id); };
  def.inputs = {
      selectionInput(QStringLiteral("curve"), QObject::tr("Offset"), SelectKind::SketchCurve, 1, 1)
          .withAccepts(editable(c, lookup)),
      valueInput(QStringLiteral("distance"), QObject::tr("Offset Position"), ValueKind::Length, QStringLiteral("5")),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip")),
      offsetCornersInput(),
  };
  def.init = [&c](CommandState& s, const CommandContext&) {
    const Selection& picked = s.items(QStringLiteral("curve"));
    const sketch::OffsetData* o = picked.isEmpty() ? nullptr : c.model().offsetOf(picked.first().name);
    if (o != nullptr) {
      s.setText(QStringLiteral("distance"), o->expression);
      s.setChoice(QStringLiteral("corners"), o->round ? QStringLiteral("round") : QStringLiteral("sharp"));
    }
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    const Selection& picked = s.items(QStringLiteral("curve"));
    const sketch::OffsetData* o = picked.isEmpty() ? nullptr : c.model().offsetOf(picked.first().name);
    if (o == nullptr) {
      return Built::failure(QObject::tr("Pick a curve an offset made."), QStringLiteral("curve"));
    }
    QJsonObject cmd = base(c, "sketch.edit_offset");
    cmd.insert(QStringLiteral("offset"), o->id);
    cmd.insert(QStringLiteral("distance"), s.expression(QStringLiteral("distance")));
    cmd.insert(QStringLiteral("flip"), s.checked(QStringLiteral("flip")));
    cmd.insert(QStringLiteral("corners"), s.choice(QStringLiteral("corners")));
    return Built::of(cmd);
  };
  def.describe = [&c](const CommandState& s, const CommandContext&) {
    const Selection& picked = s.items(QStringLiteral("curve"));
    const sketch::OffsetData* o = picked.isEmpty() ? nullptr : c.model().offsetOf(picked.first().name);
    return QStringLiteral("Edited offset %1: %2%3%4")
        .arg(o != nullptr ? o->id : QString(), s.expression(QStringLiteral("distance")),
             s.checked(QStringLiteral("flip")) ? QStringLiteral(", flipped") : QString(),
             s.choice(QStringLiteral("corners")) == QStringLiteral("round") ? QStringLiteral(", round corners")
                                                                              : QString());
  };
  registry.add(def);
}

// Edit Rho (mitcad#2236): a conic's Rho, from its right-click menu or a
// double-click on it.
void addEditRho(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.edit_rho", QObject::tr("Edit Rho"), "conic", CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("The Rho of a conic: 0.5 a parabola, less an ellipse, more a hyperbola");
  def.keywords = {QStringLiteral("conic rho shoulder")};
  const auto lookup = [](const sketch::SketchModel& m, const QString& id) -> const sketch::CurveData* {
    const sketch::CurveData* curve = m.curve(id);
    return curve != nullptr && curve->isConic() && !curve->reference ? curve : nullptr;
  };
  def.inputs = {
      selectionInput(QStringLiteral("curve"), QObject::tr("Conic"), SelectKind::SketchCurve, 1, 1)
          .withAccepts(editable(c, lookup)),
      valueInput(QStringLiteral("rho"), QObject::tr("Rho"), ValueKind::Unitless, QStringLiteral("0.5"))
          .withTooltip(QObject::tr("Between 0 and 1: a number, a parameter's name or an expression")),
  };
  def.init = [&c, lookup](CommandState& s, const CommandContext&) {
    const Selection& picked = s.items(QStringLiteral("curve"));
    const sketch::CurveData* conic = picked.isEmpty() ? nullptr : lookup(c.model(), picked.first().name);
    if (conic != nullptr) {
      s.setText(QStringLiteral("rho"), conic->rhoExpression.isEmpty() ? QString::number(conic->rho, 'g', 10)
                                                                      : conic->rhoExpression);
    }
  };
  def.build = [&c, lookup](const CommandState& s, const CommandContext&) -> Built {
    const Selection& picked = s.items(QStringLiteral("curve"));
    const sketch::CurveData* conic = picked.isEmpty() ? nullptr : lookup(c.model(), picked.first().name);
    if (conic == nullptr) {
      return Built::failure(QObject::tr("Pick a conic."), QStringLiteral("curve"));
    }
    QJsonObject cmd = base(c, "sketch.set_rho");
    cmd.insert(QStringLiteral("curve"), conic->id);
    cmd.insert(QStringLiteral("rho"), sketch::conicRhoValue(s.expression(QStringLiteral("rho"))));
    return Built::of(cmd);
  };
  // "Edited conic c5: Rho 0.7", for the UI tests.
  def.describe = [&c, lookup](const CommandState& s, const CommandContext&) {
    const Selection& picked = s.items(QStringLiteral("curve"));
    const sketch::CurveData* conic = picked.isEmpty() ? nullptr : lookup(c.model(), picked.first().name);
    return QStringLiteral("Edited conic %1: Rho %2")
        .arg(conic != nullptr ? conic->id : QString(), s.expression(QStringLiteral("rho")));
  };
  registry.add(def);
}

// The font families: the bundled one first, then the system's, then the
// files of Preferences' font folders (mitcad#473) and a file `chosen` now
// that is in none of them.
QVector<QPair<QString, QString>> fontChoices(const QString& chosen = QString()) {
  QVector<QPair<QString, QString>> choices = {{QStringLiteral("Droid Sans"), QStringLiteral("Droid Sans")}};
  for (const QString& family : sketch::installedFontFamilies()) {
    if (family != QStringLiteral("Droid Sans")) {
      choices.append({family, family});
    }
  }
  bool listed = sketch::fontFileOfChoice(chosen).isEmpty();
  for (const sketch::FolderFont& font : sketch::savedFolderFonts()) {
    const QString value = sketch::fontFileChoice(font.path);
    choices.append({value, QStringLiteral("%1 (%2)").arg(font.family, QFileInfo(font.path).fileName())});
    listed = listed || value == chosen;
  }
  if (!listed) {
    choices.append({chosen, sketch::fontFileLabel(sketch::fontFileOfChoice(chosen))});
  }
  return choices;
}

// The font chooser of Edit Text: installed fonts and font files, each with
// a preview, and a button that chooses a font file (mitcad#473).
InputDef fontInput(SketchController& c) {
  InputDef input = choiceInput(QStringLiteral("font"), QObject::tr("Font"), {}, QStringLiteral("Droid Sans"));
  input.choicesFor = [](const CommandState& s) { return fontChoices(s.choice(QStringLiteral("font"))); };
  input.choiceImage = [](const QString& value) { return sketch::fontPreview(value); };
  input.buttonText = QObject::tr("File…");
  input.buttonTooltip = QObject::tr("Chooses a font file (.ttf, .otf, .woff, .woff2); its outlines are kept in the "
                                    "design");
  input.onButton = [&c](const CommandState& s, const CommandContext&) {
    auto* window = dynamic_cast<QWidget*>(&c.host());
    const QString current = sketch::fontFileOfChoice(s.choice(QStringLiteral("font")));
    const QString path = dialogOpenFileName(
        window, QObject::tr("Font File"), current.isEmpty() ? QString() : QFileInfo(current).absolutePath(),
        QObject::tr("Fonts (*.ttf *.otf *.ttc *.woff *.woff2);;All files (*)"));
    if (path.isEmpty()) {
      return;
    }
    qDebug().noquote() << QStringLiteral("Edit Text font file %1").arg(path);
    auto* host = dynamic_cast<DocumentHost*>(&c.host());
    if (host == nullptr || !host->chooseInCommand(QStringLiteral("sketch.edit_text"), QStringLiteral("font"),
                                                  sketch::fontFileChoice(QDir::cleanPath(path)))) {
      qInfo().noquote() << QStringLiteral("Edit Text: the font file was not chosen");
    }
  };
  return input;
}

// QR codes (mitcad#473): the error correction levels.
const QVector<QPair<QString, QString>> kCodeLevels = {
    {QStringLiteral("L"), QStringLiteral("L (7 %)")},
    {QStringLiteral("M"), QStringLiteral("M (15 %)")},
    {QStringLiteral("Q"), QStringLiteral("Q (25 %)")},
    {QStringLiteral("H"), QStringLiteral("H (30 %)")},
};

// QR Code (mitcad#473): a QR code of a content in the sketch, its modules
// a size, its lower left corner at a point (`sketch.add_code`).
void addQrCode(CommandRegistry& registry, SketchController& c) {
  CommandDef def =
      sketchCommand("sketch.add_code", QObject::tr("QR Code"), "text", CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("A QR code of a text or an address; its dark modules are profiles to extrude");
  def.keywords = {QStringLiteral("qr"), QStringLiteral("code"), QStringLiteral("barcode"), QStringLiteral("url")};
  def.inputs = {
      textInput(QStringLiteral("content"), QObject::tr("Content")),
      valueInput(QStringLiteral("module"), QObject::tr("Module Size"), ValueKind::Length, QStringLiteral("1 mm"))
          .withTooltip(QObject::tr("The side of one square of the code")),
      choiceInput(QStringLiteral("ecc"), QObject::tr("Error Correction"), kCodeLevels, QStringLiteral("M"))
          .withTooltip(QObject::tr("How much of the code may be lost and it still reads")),
      valueInput(QStringLiteral("x"), QObject::tr("Corner X"), ValueKind::Length, QStringLiteral("0 mm")),
      valueInput(QStringLiteral("y"), QObject::tr("Corner Y"), ValueKind::Length, QStringLiteral("0 mm")),
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    const QString content = s.text(QStringLiteral("content"));
    if (content.isEmpty()) {
      return Built::failure(QObject::tr("Type the code's content."), QStringLiteral("content"));
    }
    const double module = s.value(QStringLiteral("module"));
    if (!(module > 0.0)) {
      return Built::failure(QObject::tr("The module size must be greater than zero."), QStringLiteral("module"));
    }
    QJsonObject cmd = base(c, "sketch.add_code");
    cmd.insert(QStringLiteral("text"), content);
    cmd.insert(QStringLiteral("module"), module);
    cmd.insert(QStringLiteral("ecc"), s.choice(QStringLiteral("ecc")));
    const double x = s.value(QStringLiteral("x"));
    const double y = s.value(QStringLiteral("y"));
    cmd.insert(QStringLiteral("at"), QJsonArray{std::isnan(x) ? 0.0 : x, std::isnan(y) ? 0.0 : y});
    return Built::of(cmd);
  };
  def.describe = [](const CommandState& s, const CommandContext&) {
    return QStringLiteral("Added QR code \"%1\", module %2, level %3")
        .arg(s.text(QStringLiteral("content")), number(s.value(QStringLiteral("module"))),
             s.choice(QStringLiteral("ecc")));
  };
  registry.add(def);
}

// Edit Text's and Edit QR Code's Position X and Y (mitcad#1911): where a
// text without a frame or a path is (its anchor, the model's `at`), a
// code's lower left corner; a changed one is sent as `at`.
InputDef positionInput(const QString& id, const QString& label) {
  return valueInput(id, label, ValueKind::Length, QStringLiteral("0 mm"))
      .withTooltip(QObject::tr("Where the text's anchor is in the sketch; a QR code's lower left corner"));
}

void setPosition(CommandState& s, const sketch::TextData& text, const CommandContext& context) {
  if (const auto at = sketch::freeTextPosition(text)) {
    const double unit = context.unitMillimetres();
    s.setText(QStringLiteral("x"), QString::number(at->x / unit, 'g', 12));
    s.setText(QStringLiteral("y"), QString::number(at->y / unit, 'g', 12));
  }
}

void insertChangedPosition(QJsonObject& cmd, const CommandState& s, const sketch::TextData& text) {
  if (const auto at = sketch::changedTextPosition(text, s.value(QStringLiteral("x")), s.value(QStringLiteral("y")))) {
    cmd.insert(QStringLiteral("at"), QJsonArray{at->x, at->y});
  }
}

// Edit QR Code (mitcad#473): a code's content, module size and error
// correction (`sketch.edit_code`); a double-click on the code opens it.
void addEditCode(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.edit_code", QObject::tr("Edit QR Code"), "text",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("A QR code's content, module size and error correction");
  const auto lookup = [](const sketch::SketchModel& m, const QString& id) -> const sketch::TextData* {
    const sketch::TextData* t = m.text(id);
    return t != nullptr && t->code ? t : nullptr;
  };
  def.inputs = {
      textInputFor(selectionInput(QStringLiteral("code"), QObject::tr("QR Code"), SelectKind::SketchCurve, 1, 1)
                       .withAccepts(editable(c, lookup)),
                   c),
      textInput(QStringLiteral("content"), QObject::tr("Content")),
      valueInput(QStringLiteral("module"), QObject::tr("Module Size"), ValueKind::Length, QStringLiteral("1 mm")),
      choiceInput(QStringLiteral("ecc"), QObject::tr("Error Correction"), kCodeLevels, QStringLiteral("M")),
      // Where the code's lower left corner is (mitcad#1911).
      positionInput(QStringLiteral("x"), QObject::tr("Position X")),
      positionInput(QStringLiteral("y"), QObject::tr("Position Y")),
  };
  def.init = [&c, lookup](CommandState& s, const CommandContext& context) {
    const Selection& picked = s.items(QStringLiteral("code"));
    const sketch::TextData* t = picked.isEmpty() ? nullptr : lookup(c.model(), picked.first().name);
    if (t == nullptr) {
      return;
    }
    s.setText(QStringLiteral("content"), t->text);
    s.setText(QStringLiteral("module"), QString::number(t->height / context.unitMillimetres(), 'g', 12));
    s.setChoice(QStringLiteral("ecc"), t->ecc);
    setPosition(s, *t, context);
  };
  def.build = [&c, lookup](const CommandState& s, const CommandContext&) -> Built {
    const Selection& picked = s.items(QStringLiteral("code"));
    const sketch::TextData* code = picked.isEmpty() ? nullptr : lookup(c.model(), picked.first().name);
    if (code == nullptr) {
      return Built::failure(QObject::tr("Pick a QR code."), QStringLiteral("code"));
    }
    const QString content = s.text(QStringLiteral("content"));
    if (content.isEmpty()) {
      return Built::failure(QObject::tr("Type the code's content."), QStringLiteral("content"));
    }
    QJsonObject cmd = base(c, "sketch.edit_code");
    cmd.insert(QStringLiteral("id"), picked.first().name);
    cmd.insert(QStringLiteral("text"), content);
    cmd.insert(QStringLiteral("module"), s.value(QStringLiteral("module")));
    cmd.insert(QStringLiteral("ecc"), s.choice(QStringLiteral("ecc")));
    insertChangedPosition(cmd, s, *code);
    return Built::of(cmd);
  };
  def.describe = [](const CommandState& s, const CommandContext&) {
    const Selection& picked = s.items(QStringLiteral("code"));
    return QStringLiteral("Edited QR code %1: \"%2\", module %3, level %4")
        .arg(picked.isEmpty() ? QString() : picked.first().name, s.text(QStringLiteral("content")),
             number(s.value(QStringLiteral("module"))), s.choice(QStringLiteral("ecc")));
  };
  registry.add(def);
}

// --- Equation curves (mitcad#2237)

// Whether the panel's Mode is `mode`.
std::function<bool(const CommandState&)> equationModeIs(const char* mode) {
  return [mode](const CommandState& s) { return s.choice(QStringLiteral("mode")) == QLatin1String(mode); };
}

// The suffix of the range inputs the panel's Mode and Variable use
// (mitcad#2264): `_x` for a length range (an explicit curve's, a
// parametric curve's length t), `_a` for an angle range (a polar
// curve's, a parametric curve's angle t), `_t` for a parametric curve's
// plain t.
QString equationRange(const CommandState& s) {
  const QString mode = s.choice(QStringLiteral("mode"));
  const QString unit = s.choice(QStringLiteral("variable_unit"));
  if (mode == QLatin1String("polar")) {
    return QStringLiteral("_a");
  }
  if (mode == QLatin1String("parametric")) {
    if (unit == QLatin1String("length")) {
      return QStringLiteral("_x");
    }
    return unit == QLatin1String("angle") ? QStringLiteral("_a") : QStringLiteral("_t");
  }
  return QStringLiteral("_x");
}

// Whether the range inputs of `suffix` are the ones shown.
std::function<bool(const CommandState&)> equationRangeIs(const char* suffix) {
  return [suffix](const CommandState& s) { return equationRange(s) == QLatin1String(suffix); };
}

// The inputs Equation Curve and Edit Equation Curve share: Mode, Variable
// (what the variable is, mitcad#2264), the mode's formulas (their
// variable a known name in Edit Formula), the range in the variable's
// units (a length for an explicit curve's x or a length t, an angle for a
// polar curve's t or an angle t, else a plain number), Tolerance and
// Construction.
QVector<InputDef> equationInputs() {
  const QVector<QPair<QString, QString>> modes = {
      {QStringLiteral("explicit"), QObject::tr("Explicit: y(x)")},
      {QStringLiteral("parametric"), QObject::tr("Parametric: x(t), y(t)")},
      {QStringLiteral("polar"), QObject::tr("Polar: r(t)")}};
  const QVector<QPair<QString, QString>> units = {{QStringLiteral("none"), QObject::tr("Plain number")},
                                                  {QStringLiteral("length"), QObject::tr("Length")},
                                                  {QStringLiteral("angle"), QObject::tr("Angle")}};
  const QString unit = QStringLiteral("variable_unit");
  const QString ofX = QObject::tr("A formula of x (in the length unit) and the parameters, such as x^2 / 20");
  const QString ofT = QObject::tr("A formula of t and the parameters, such as 10 * cos(t); angles in radians");
  return {
      choiceInput(QStringLiteral("mode"), QObject::tr("Mode"), modes, QStringLiteral("explicit"))
          .withTooltip(QObject::tr("Explicit: y of x; Parametric: x and y of t; Polar: the radius r of the angle t")),
      choiceInput(unit, QObject::tr("Variable"), units, QStringLiteral("none"))
          .withTooltip(QObject::tr("What x or t is in the formulas and the range: a plain number, a length or an angle")),
      textInput(QStringLiteral("y"), QObject::tr("y(x)"), QStringLiteral("x^2/20"))
          .withFormulaVariable(QStringLiteral("x"), unit)
          .withVisible(equationModeIs("explicit"))
          .withTooltip(ofX),
      textInput(QStringLiteral("xt"), QObject::tr("x(t)"), QStringLiteral("10*cos(t)"))
          .withFormulaVariable(QStringLiteral("t"), unit)
          .withVisible(equationModeIs("parametric"))
          .withTooltip(ofT),
      textInput(QStringLiteral("yt"), QObject::tr("y(t)"), QStringLiteral("10*sin(t)"))
          .withFormulaVariable(QStringLiteral("t"), unit)
          .withVisible(equationModeIs("parametric"))
          .withTooltip(ofT),
      textInput(QStringLiteral("r"), QObject::tr("r(t)"), QStringLiteral("2 + t"))
          .withFormulaVariable(QStringLiteral("t"), unit)
          .withVisible(equationModeIs("polar"))
          .withTooltip(QObject::tr("The radius as a formula of the angle t, in radians, and the parameters")),
      valueInput(QStringLiteral("start_x"), QObject::tr("Start"), ValueKind::Length, QStringLiteral("-10 mm"))
          .withVisible(equationRangeIs("_x"))
          .withTooltip(QObject::tr("Where x starts")),
      valueInput(QStringLiteral("end_x"), QObject::tr("End"), ValueKind::Length, QStringLiteral("10 mm"))
          .withVisible(equationRangeIs("_x"))
          .withTooltip(QObject::tr("Where x ends, beyond its start")),
      valueInput(QStringLiteral("start_t"), QObject::tr("Start"), ValueKind::Unitless, QStringLiteral("0"))
          .withVisible(equationRangeIs("_t"))
          .withTooltip(QObject::tr("Where t starts, a plain number")),
      valueInput(QStringLiteral("end_t"), QObject::tr("End"), ValueKind::Unitless, QStringLiteral("2*PI"))
          .withVisible(equationRangeIs("_t"))
          .withTooltip(QObject::tr("Where t ends, beyond its start")),
      valueInput(QStringLiteral("start_a"), QObject::tr("Start"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withVisible(equationRangeIs("_a"))
          .withTooltip(QObject::tr("The angle t starts at")),
      valueInput(QStringLiteral("end_a"), QObject::tr("End"), ValueKind::Angle, QStringLiteral("720 deg"))
          .withVisible(equationRangeIs("_a"))
          .withTooltip(QObject::tr("The angle t ends at, beyond its start")),
      valueInput(QStringLiteral("tolerance"), QObject::tr("Tolerance"), ValueKind::Length, QStringLiteral("0.01 mm"))
          .withExactDefault()
          .withTooltip(QObject::tr("How far the curve may be from its formulas; a smaller tolerance places more "
                                   "points")),
      checkInput(QStringLiteral("construction"), QObject::tr("Construction"), false)
          .withTooltip(QObject::tr("Construction geometry, left out of profiles")),
  };
}

// The fields of `sketch.equation_curve` and `sketch.edit_equation_curve`
// added to `cmd` from the panel, or what the panel still needs.
Built equationCommand(QJsonObject cmd, const CommandState& s) {
  const QString mode = s.choice(QStringLiteral("mode"));
  std::vector<std::pair<QString, QString>> formulas; // input, field
  const QString range = equationRange(s);            // the range inputs' suffix
  if (mode == QLatin1String("parametric")) {
    formulas = {{QStringLiteral("xt"), QStringLiteral("x")}, {QStringLiteral("yt"), QStringLiteral("y")}};
  } else if (mode == QLatin1String("polar")) {
    formulas = {{QStringLiteral("r"), QStringLiteral("r")}};
  } else {
    formulas = {{QStringLiteral("y"), QStringLiteral("y")}};
  }
  cmd.insert(QStringLiteral("mode"), mode.isEmpty() ? QStringLiteral("explicit") : mode);
  const QString unit = s.choice(QStringLiteral("variable_unit"));
  cmd.insert(QStringLiteral("variable_unit"), unit.isEmpty() ? QStringLiteral("none") : unit);
  for (const auto& [input, field] : formulas) {
    const QString text = s.text(input).trimmed();
    if (text.isEmpty()) {
      return Built::failure(QObject::tr("Type the formula %1.").arg(field), input);
    }
    cmd.insert(field, text);
  }
  const QString start = s.expression(QStringLiteral("start") + range).trimmed();
  if (start.isEmpty()) {
    return Built::failure(QObject::tr("Give the start of the range."), QStringLiteral("start") + range);
  }
  const QString end = s.expression(QStringLiteral("end") + range).trimmed();
  if (end.isEmpty()) {
    return Built::failure(QObject::tr("Give the end of the range."), QStringLiteral("end") + range);
  }
  cmd.insert(QStringLiteral("start"), start);
  cmd.insert(QStringLiteral("end"), end);
  const double tolerance = s.value(QStringLiteral("tolerance"));
  if (!(tolerance > 0.0)) {
    return Built::failure(QObject::tr("The tolerance must be greater than zero."), QStringLiteral("tolerance"));
  }
  cmd.insert(QStringLiteral("tolerance"), tolerance);
  cmd.insert(QStringLiteral("construction"), s.checked(QStringLiteral("construction")));
  return Built::of(cmd);
}

// An equation curve as the log shows it, for UI tests: "c12, 10 points from
// p4 (3.000, 0.000) to p13 (4.207, 5.225), open" (a closed one "from p4
// (10.000, 0.000), closed").
QString equationCurveText(const sketch::SketchModel& model, const QString& curve, const QStringList& points,
                          bool closed) {
  if (points.isEmpty()) {
    return QStringLiteral("%1, no points").arg(curve);
  }
  if (closed) {
    return QStringLiteral("%1, %2 points from %3, closed")
        .arg(curve)
        .arg(points.size())
        .arg(pointPositions(model, {points.first()}));
  }
  return QStringLiteral("%1, %2 points from %3 to %4, open")
      .arg(curve)
      .arg(points.size())
      .arg(pointPositions(model, {points.first()}), pointPositions(model, {points.last()}));
}

// What a preview of an equation curve made, for UI tests (the model's
// answer: its curve, points, closed and deviation).
std::function<void(const QJsonObject&)> equationPreviewLog(SketchController& c, const QString& name) {
  return [&c, name](const QJsonObject& answer) {
    if (answer.isEmpty()) {
      return;
    }
    QStringList points;
    for (const QJsonValue& p : answer.value(QStringLiteral("points")).toArray()) {
      points << p.toString();
    }
    qDebug().noquote() << QStringLiteral("Preview %1 curve: %2, deviation %3 mm")
                              .arg(name,
                                   equationCurveText(c.model(), answer.value(QStringLiteral("curve")).toString(),
                                                     points, answer.value(QStringLiteral("closed")).toBool()),
                                   QString::number(answer.value(QStringLiteral("deviation")).toDouble(), 'f', 4));
  };
}

// The panel's description of an equation curve: its mode, formulas and
// range ("parametric x = 10*cos(t), y = 10*sin(t), t from 0 to 2*PI"; a
// length or angle t "…, t a length from 0 mm to 10 mm", mitcad#2264).
QString equationDescription(const CommandState& s) {
  const QString mode = s.choice(QStringLiteral("mode"));
  const QString range = equationRange(s);
  const QString unit = s.choice(QStringLiteral("variable_unit"));
  if (mode == QLatin1String("parametric")) {
    const QString variable = unit == QLatin1String("length")  ? QStringLiteral("t a length")
                             : unit == QLatin1String("angle") ? QStringLiteral("t an angle")
                                                              : QStringLiteral("t");
    return QStringLiteral("parametric x = %1, y = %2, %3 from %4 to %5")
        .arg(s.text(QStringLiteral("xt")), s.text(QStringLiteral("yt")), variable,
             s.expression(QStringLiteral("start") + range), s.expression(QStringLiteral("end") + range));
  }
  if (mode == QLatin1String("polar")) {
    return QStringLiteral("polar r = %1, t from %2 to %3")
        .arg(s.text(QStringLiteral("r")), s.expression(QStringLiteral("start_a")), s.expression(QStringLiteral("end_a")));
  }
  return QStringLiteral("explicit y = %1, x from %2 to %3")
      .arg(s.text(QStringLiteral("y")), s.expression(QStringLiteral("start_x")), s.expression(QStringLiteral("end_x")));
}

// Equation Curve (mitcad#2237): a curve given by formulas of the design's
// parameters, a fit point spline through the points they place, within
// the tolerance; it follows the parameters (`sketch.equation_curve`).
void addEquationCurve(CommandRegistry& registry, SketchController& c) {
  CommandDef def =
      sketchCommand("sketch.equation_curve", QObject::tr("Equation Curve"), "equation-curve", CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("A curve from formulas of the design's parameters, which it follows");
  def.keywords = {QStringLiteral("formula"), QStringLiteral("equation"), QStringLiteral("involute"),
                  QStringLiteral("spiral"), QStringLiteral("parametric"), QStringLiteral("polar"),
                  QStringLiteral("function")};
  def.inputs = equationInputs();
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    return equationCommand(base(c, "sketch.equation_curve"), s);
  };
  def.sketchPreviewShown = equationPreviewLog(c, def.name);
  def.describe = [](const CommandState& s, const CommandContext&) {
    return QStringLiteral("Added equation curve, %1").arg(equationDescription(s));
  };
  registry.add(def);
}

// Edit Equation Curve (mitcad#2237): an equation curve's mode, formulas,
// range, tolerance and construction (`sketch.edit_equation_curve`); a
// double-click on the curve and its right-click menu open it. Its start
// point, and an open curve's end point, keep their ids.
void addEditEquationCurve(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.edit_equation_curve", QObject::tr("Edit Equation Curve"), "equation-curve",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("An equation curve's formulas, range and tolerance");
  const auto lookup = [](const sketch::SketchModel& m, const QString& id) { return m.equationOf(id); };
  def.inputs = {selectionInput(QStringLiteral("curve"), QObject::tr("Equation Curve"), SelectKind::SketchCurve, 1, 1)
                    .withAccepts(editable(c, lookup))
                    .withTooltip(QObject::tr("The equation curve to change"))};
  for (const InputDef& input : equationInputs()) {
    def.inputs.push_back(input);
  }
  def.init = [&c](CommandState& s, const CommandContext& context) {
    const Selection& picked = s.items(QStringLiteral("curve"));
    const sketch::EquationData* e = picked.isEmpty() ? nullptr : c.model().equationOf(picked.first().name);
    const sketch::CurveData* curve = picked.isEmpty() ? nullptr : c.model().curve(picked.first().name);
    if (e == nullptr || curve == nullptr) {
      return;
    }
    s.setChoice(QStringLiteral("mode"), e->mode);
    s.setChoice(QStringLiteral("variable_unit"), e->variableUnit.isEmpty() ? QStringLiteral("none") : e->variableUnit);
    if (e->mode == QLatin1String("parametric")) {
      s.setText(QStringLiteral("xt"), e->x);
      s.setText(QStringLiteral("yt"), e->y);
    } else if (e->mode == QLatin1String("polar")) {
      s.setText(QStringLiteral("r"), e->r);
    } else {
      s.setText(QStringLiteral("y"), e->y);
    }
    const QString range = equationRange(s);
    s.setText(QStringLiteral("start") + range, e->start);
    s.setText(QStringLiteral("end") + range, e->end);
    s.setText(QStringLiteral("tolerance"), QString::number(e->tolerance / context.unitMillimetres(), 'g', 12));
    s.setChecked(QStringLiteral("construction"), curve->construction);
    // For UI tests: the curve where the sketch has it now.
    qDebug().noquote() << QStringLiteral("Edit Equation Curve opened on %1: %2")
                              .arg(e->mode, equationCurveText(c.model(), curve->id, curve->points, curve->closed));
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    const Selection& picked = s.items(QStringLiteral("curve"));
    if (picked.isEmpty() || c.model().equationOf(picked.first().name) == nullptr) {
      return Built::failure(QObject::tr("Pick an equation curve."), QStringLiteral("curve"));
    }
    QJsonObject cmd = base(c, "sketch.edit_equation_curve");
    cmd.insert(QStringLiteral("curve"), picked.first().name);
    return equationCommand(cmd, s);
  };
  def.sketchPreviewShown = equationPreviewLog(c, def.name);
  def.describe = [](const CommandState& s, const CommandContext&) {
    const Selection& picked = s.items(QStringLiteral("curve"));
    return QStringLiteral("Edited equation curve %1, %2")
        .arg(picked.isEmpty() ? QString() : picked.first().name, equationDescription(s));
  };
  registry.add(def);
}

void addEditText(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.edit_text", QObject::tr("Edit Text"), "text",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("A sketch text's content, size, font, style, alignment and path");
  // A QR code has Edit QR Code (mitcad#473).
  const auto lookup = [](const sketch::SketchModel& m, const QString& id) -> const sketch::TextData* {
    const sketch::TextData* t = m.text(id);
    return t != nullptr && !t->code ? t : nullptr;
  };
  const auto free = [&c](const CommandState& s) {
    const Selection& picked = s.items(QStringLiteral("text"));
    const sketch::TextData* t = picked.isEmpty() ? nullptr : c.model().text(picked.first().name);
    return s.items(QStringLiteral("path")).isEmpty() && (t == nullptr || t->frame.isEmpty());
  };
  const auto onPath = [](const CommandState& s) { return !s.items(QStringLiteral("path")).isEmpty(); };
  // Wrap in Frame, for a text in a frame that is not on a path (mitcad#2029).
  const auto wrapShown = [&c, onPath](const CommandState& s) {
    const Selection& picked = s.items(QStringLiteral("text"));
    const sketch::TextData* t = picked.isEmpty() ? nullptr : c.model().text(picked.first().name);
    return t != nullptr && sketch::wrapInFrameShown(*t, onPath(s));
  };
  def.inputs = {
      textInputFor(selectionInput(QStringLiteral("text"), QObject::tr("Text"), SelectKind::SketchCurve, 1, 1)
                       .withAccepts(editable(c, lookup)),
                   c),
      // A box of several lines: Enter starts a new line (mitcad#2029).
      textInput(QStringLiteral("content"), QObject::tr("Text"))
          .withMultiLine()
          .withTooltip(QObject::tr("The text; Enter starts a new line, Ctrl+Enter is OK")),
      // The text from a text parameter or expression (mitcad#664).
      textInput(QStringLiteral("value"), QObject::tr("Text Parameter"))
          .withTooltip(QObject::tr(
              "A text parameter or text expression, such as 'Box ' & Width, for the text; empty for the text above")),
      valueInput(QStringLiteral("height"), QObject::tr("Height"), ValueKind::Length, QStringLiteral("5")),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withVisible(free),
      // Where a text without a frame or a path is (mitcad#1911).
      positionInput(QStringLiteral("x"), QObject::tr("Position X")).withVisible(free),
      positionInput(QStringLiteral("y"), QObject::tr("Position Y")).withVisible(free),
      fontInput(c),
      checkInput(QStringLiteral("bold"), QObject::tr("Bold")),
      checkInput(QStringLiteral("italic"), QObject::tr("Italic")),
      choiceInput(QStringLiteral("align"), QObject::tr("Horizontal Alignment"),
                  {{QStringLiteral("left"), QObject::tr("Left")},
                   {QStringLiteral("center"), QObject::tr("Center")},
                   {QStringLiteral("right"), QObject::tr("Right")}},
                  QStringLiteral("left")),
      checkInput(QStringLiteral("wrap"), QObject::tr("Wrap in Frame"))
          .withTooltip(QObject::tr("Breaks the lines at spaces to fit the frame's width; a word wider than the "
                                   "frame stays whole"))
          .withVisible(wrapShown),
      choiceInput(QStringLiteral("valign"), QObject::tr("Vertical Alignment"),
                  {{QStringLiteral("baseline"), QObject::tr("Baseline")},
                   {QStringLiteral("top"), QObject::tr("Top")},
                   {QStringLiteral("middle"), QObject::tr("Middle")},
                   {QStringLiteral("bottom"), QObject::tr("Bottom")}},
                  QStringLiteral("baseline"))
          .withVisible([onPath](const CommandState& s) { return !onPath(s); }),
      valueInput(QStringLiteral("spacing"), QObject::tr("Character Spacing (%)"), ValueKind::Unitless,
                 QStringLiteral("0")),
      checkInput(QStringLiteral("flip_x"), QObject::tr("Flip Horizontal")),
      checkInput(QStringLiteral("flip_y"), QObject::tr("Flip Vertical")),
      selectionInput(QStringLiteral("path"), QObject::tr("Path"), SelectKind::SketchCurve, 0, 1)
          .withAccepts([&c](const SelectionItem& item) {
            return item.owner == c.uid() && c.model().curve(item.name) != nullptr;
          })
          .withTooltip(QObject::tr("A curve the text runs along")),
      checkInput(QStringLiteral("above"), QObject::tr("Above Path"), true).withVisible(onPath),
      checkInput(QStringLiteral("fit"), QObject::tr("Fit on Path")).withVisible(onPath),
  };
  def.init = [&c](CommandState& s, const CommandContext& context) {
    // The font folders' files as they are now (mitcad#473).
    sketch::forgetFolderFonts();
    const Selection& picked = s.items(QStringLiteral("text"));
    const sketch::TextData* t = picked.isEmpty() ? nullptr : c.model().text(picked.first().name);
    if (t == nullptr) {
      return;
    }
    s.setText(QStringLiteral("content"), t->text);
    s.setText(QStringLiteral("value"), t->value);
    s.setText(QStringLiteral("height"), QString::number(t->height / context.unitMillimetres(), 'g', 12));
    s.setText(QStringLiteral("angle"), QStringLiteral("%1 deg").arg(t->angle * 180.0 / 3.14159265358979323846, 0, 'g', 12));
    setPosition(s, *t, context);
    if (!t->fontFile.isEmpty()) {
      // The file where it is now: where it was read, or one of the same
      // name in the font folders.
      s.setChoice(QStringLiteral("font"),
                  sketch::fontFileChoice(sketch::findFontFile(t->fontFile, sketch::savedFontFolders())));
    } else {
      s.setChoice(QStringLiteral("font"), t->font.isEmpty() ? QStringLiteral("Droid Sans") : t->font);
    }
    s.setChecked(QStringLiteral("bold"), t->bold);
    s.setChecked(QStringLiteral("italic"), t->italic);
    s.setChoice(QStringLiteral("align"), t->align);
    s.setChecked(QStringLiteral("wrap"), t->wrap);
    // The wrap it had when the command started: a preview changes the
    // sketch's text, and `wrap` is sent only when it differs from this
    // (mitcad#2029).
    s.setText(QStringLiteral("wrap_was"), t->wrap ? QStringLiteral("on") : QStringLiteral("off"));
    s.setChoice(QStringLiteral("valign"), t->valign);
    s.setText(QStringLiteral("spacing"), QString::number(t->spacing));
    s.setChecked(QStringLiteral("flip_x"), t->flipX);
    s.setChecked(QStringLiteral("flip_y"), t->flipY);
    if (!t->path.isEmpty()) {
      s.setItems(QStringLiteral("path"), {{SelectKind::SketchCurve, c.uid(), t->path, QStringLiteral("curve")}});
      s.setChecked(QStringLiteral("above"), t->above);
      s.setChecked(QStringLiteral("fit"), t->fit);
    }
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    const Selection& picked = s.items(QStringLiteral("text"));
    if (picked.isEmpty() || c.model().text(picked.first().name) == nullptr) {
      return Built::failure(QObject::tr("Pick a text."), QStringLiteral("text"));
    }
    // Its line breaks as the model's `\n` (mitcad#2029).
    const QString content = sketch::textWithLineBreaks(s.text(QStringLiteral("content")));
    const QString value = s.text(QStringLiteral("value")).trimmed();
    if (content.trimmed().isEmpty() && value.isEmpty()) {
      return Built::failure(QObject::tr("The text is empty."), QStringLiteral("content"));
    }
    QJsonObject cmd = base(c, "sketch.edit_text");
    cmd.insert(QStringLiteral("id"), picked.first().name);
    if (!content.trimmed().isEmpty()) {
      cmd.insert(QStringLiteral("text"), content);
    }
    // The text parameter it follows (mitcad#664); empty stops it following
    // the one it followed.
    const sketch::TextData* edited = c.model().text(picked.first().name);
    if (!value.isEmpty() || (edited != nullptr && !edited->value.isEmpty())) {
      cmd.insert(QStringLiteral("value"), value);
    }
    cmd.insert(QStringLiteral("height"), s.value(QStringLiteral("height")));
    if (!std::isnan(s.value(QStringLiteral("angle")))) {
      cmd.insert(QStringLiteral("angle"), s.value(QStringLiteral("angle")));
    }
    // A changed Position of a text without a frame or a path (mitcad#1911).
    if (edited != nullptr && s.items(QStringLiteral("path")).isEmpty()) {
      insertChangedPosition(cmd, s, *edited);
    }
    const QString fontFile = sketch::fontFileOfChoice(s.choice(QStringLiteral("font")));
    if (fontFile.isEmpty()) {
      cmd.insert(QStringLiteral("font"), s.choice(QStringLiteral("font")));
    } else {
      cmd.insert(QStringLiteral("font_file"), fontFile);
    }
    cmd.insert(QStringLiteral("bold"), s.checked(QStringLiteral("bold")));
    cmd.insert(QStringLiteral("italic"), s.checked(QStringLiteral("italic")));
    cmd.insert(QStringLiteral("align"), s.choice(QStringLiteral("align")));
    // Wrap in Frame only when it was changed: an imported text keeps its
    // own (mitcad#2029).
    if (edited != nullptr) {
      // Against the text as it was before the previews (which change the
      // sketch's text), else as it is now.
      sketch::TextData before = *edited;
      if (const QString was = s.text(QStringLiteral("wrap_was")); !was.isEmpty()) {
        before.wrap = was == QLatin1String("on");
      }
      if (const auto wrap = sketch::changedTextWrap(before, !s.items(QStringLiteral("path")).isEmpty(),
                                                    s.checked(QStringLiteral("wrap")))) {
        cmd.insert(QStringLiteral("wrap"), *wrap);
      }
    }
    if (s.items(QStringLiteral("path")).isEmpty()) {
      cmd.insert(QStringLiteral("valign"), s.choice(QStringLiteral("valign")));
    }
    if (!std::isnan(s.value(QStringLiteral("spacing")))) {
      cmd.insert(QStringLiteral("spacing"), s.value(QStringLiteral("spacing")));
    }
    cmd.insert(QStringLiteral("flip_x"), s.checked(QStringLiteral("flip_x")));
    cmd.insert(QStringLiteral("flip_y"), s.checked(QStringLiteral("flip_y")));
    const Selection& path = s.items(QStringLiteral("path"));
    if (!path.isEmpty()) {
      cmd.insert(QStringLiteral("path"), QJsonObject{{QStringLiteral("curve"), path.first().name},
                                                     {QStringLiteral("above"), s.checked(QStringLiteral("above"))},
                                                     {QStringLiteral("fit"), s.checked(QStringLiteral("fit"))}});
    }
    return Built::of(cmd);
  };
  def.describe = [](const CommandState& s, const CommandContext&) {
    const Selection& picked = s.items(QStringLiteral("text"));
    const QString font = s.choice(QStringLiteral("font"));
    const QString file = sketch::fontFileOfChoice(font);
    // One log line: its line breaks as \n.
    const QString content =
        sketch::textWithLineBreaks(s.text(QStringLiteral("content"))).replace(QLatin1Char('\n'), QStringLiteral("\\n"));
    return QStringLiteral("Edited text %1: \"%2\", %3, %4%5")
        .arg(picked.isEmpty() ? QString() : picked.first().name, content,
             file.isEmpty() ? font : sketch::fontFileLabel(file), s.choice(QStringLiteral("align")),
             s.checked(QStringLiteral("wrap")) ? QStringLiteral(", wrapped") : QString());
  };
  registry.add(def);
}

} // namespace

QStringList sketchConstraintCommands() {
  return {QStringLiteral("sketch.constraint.horizontal_vertical"), QStringLiteral("sketch.constraint.coincident"),
          QStringLiteral("sketch.constraint.tangent"),            QStringLiteral("sketch.constraint.equal"),
          QStringLiteral("sketch.constraint.parallel"),           QStringLiteral("sketch.constraint.perpendicular"),
          QStringLiteral("sketch.constraint.fix"),                QStringLiteral("sketch.constraint.midpoint"),
          QStringLiteral("sketch.constraint.concentric"),         QStringLiteral("sketch.constraint.collinear"),
          QStringLiteral("sketch.constraint.symmetric"),          QStringLiteral("sketch.constraint.smooth"),
          // A panel, not a tool (mitcad#221).
          QStringLiteral("sketch.equal_spacing")};
}

void registerSketchCommands(CommandRegistry& registry, SketchController& c) {
  // Tools and actions: what they run, and how the toolbar shows them.
  struct Spec {
    const char* id;
    QString name;
    const char* icon;
    std::function<void()> run;
    QKeySequence shortcut;
    QString tooltip;
  };
  const auto add = [&registry, &c](const Spec& spec, CommandDef::Kind kind = CommandDef::Kind::Tool,
                                   const QList<QKeySequence>& alternates = {}) {
    CommandDef def = sketchCommand(spec.id, spec.name, spec.icon, kind);
    def.run = spec.run;
    def.shortcut = spec.shortcut;
    def.alternates = alternates;
    def.tooltip = spec.tooltip;
    if (kind == CommandDef::Kind::Tool) {
      c.setToolIcon(def.name, def.icon); // the tool's cursor shows it
    }
    registry.add(def);
  };
  const auto draw = [&c](std::unique_ptr<sketch::SketchTool> (*make)(SketchController&, const QString&),
                         const char* mode) {
    return [&c, make, mode] { c.startTool(make(c, QString::fromLatin1(mode))); };
  };
  const QKeySequence none;

  // --- CREATE, in submenus (mitcad#222): Lines, Point (an entry of its
  // own, mitcad#1545), Rectangles, Circles, Arcs, Other Geometry, Text,
  // Patterns, References; a pinned command is its submenu's split button
  // default. Offset is in MODIFY.
  add({"sketch.line", QObject::tr("Line"), "line", [&c] { c.startTool(sketch::lineTool(c)); },
       QKeySequence(Qt::Key_L),
       QObject::tr("Lines end to end; drag from an end for an arc, tangent along the curve or normal across it")});
  // Lines is a split button with Line its default (mitcad#1393).
  add({"sketch.line_midpoint", QObject::tr("Midpoint Line"), "line-midpoint",
       [&c] { c.startTool(sketch::midpointLineTool(c)); }, none,
       QObject::tr("A line from its midpoint and an end, symmetric about the midpoint")});
  // Point on its own after Lines, in the menu and as a button (mitcad#1545:
  // users did not find it at the end of Other Geometry). No shortcut.
  add({"sketch.point", QObject::tr("Point"), "point", [&c] { c.startTool(sketch::pointTool(c)); }, none, QObject::tr("A sketch point")});
  add({"sketch.rectangle", QObject::tr("2-Point Rectangle"), "rectangle",
       draw(sketch::rectangleTool, "two_point"), QKeySequence(Qt::Key_R),
       QObject::tr("A rectangle from two opposite corners")});
  add({"sketch.rectangle_3pt", QObject::tr("3-Point Rectangle"), "rectangle-3pt",
       draw(sketch::rectangleTool, "three_point"), none,
       QObject::tr("A rectangle at any angle: a side, then the opposite side")});
  add({"sketch.rectangle_center", QObject::tr("Center Rectangle"), "rectangle-center",
       draw(sketch::rectangleTool, "center"), none, QObject::tr("A rectangle from its center and a corner")});
  // Rounded corners (mitcad#867): four tangent arcs, Equal, one radius.
  add({"sketch.rectangle_rounded", QObject::tr("2-Point Rounded Rectangle"), "rectangle-rounded", draw(sketch::roundedRectangleTool, "two_point"), none,
       QObject::tr("A rectangle with rounded corners from two opposite corners and a corner radius")});
  add({"sketch.rectangle_rounded_center", QObject::tr("Center Rounded Rectangle"), "rectangle-rounded-center", draw(sketch::roundedRectangleTool, "center"), none,
       QObject::tr("A rectangle with rounded corners from its center, a corner and a corner radius")});
  add({"sketch.circle", QObject::tr("Center Diameter Circle"), "circle",
       draw(sketch::circleTool, "center"), QKeySequence(Qt::Key_C),
       QObject::tr("A circle from its center and a point on it")});
  add({"sketch.circle_2pt", QObject::tr("2-Point Circle"), "circle-2pt",
       draw(sketch::circleTool, "two_point"), none, QObject::tr("A circle from the ends of a diameter")});
  add({"sketch.circle_3pt", QObject::tr("3-Point Circle"), "circle-3pt",
       draw(sketch::circleTool, "three_point"), none, QObject::tr("A circle through three points")});
  add({"sketch.circle_2tangent", QObject::tr("2-Tangent Circle"), "circle-tangent",
       draw(sketch::circleTool, "two_tangent"), none, QObject::tr("A circle tangent to two lines")});
  add({"sketch.circle_3tangent", QObject::tr("3-Tangent Circle"), "circle-tangent",
       draw(sketch::circleTool, "three_tangent"), none, QObject::tr("A circle tangent to three lines")});
  add({"sketch.arc", QObject::tr("3-Point Arc"), "arc-3pt", draw(sketch::arcTool, "three_point"),
       none, QObject::tr("An arc from its start and end through a third point")});
  add({"sketch.arc_center", QObject::tr("Center Point Arc"), "arc-center",
       draw(sketch::arcTool, "center"), none, QObject::tr("An arc around a center, from its start to its end")});
  add({"sketch.arc_tangent", QObject::tr("Tangent Arc"), "arc-tangent",
       draw(sketch::arcTool, "tangent"), none, QObject::tr("An arc that continues a line or an arc smoothly")});
  // No natural default: the submenu has no button of its own.
  add({"sketch.ellipse", QObject::tr("Ellipse"), "ellipse",
       [&c] { c.startTool(sketch::ellipseTool(c)); }, none,
       QObject::tr("An ellipse: the center, the end of the major axis, a point on it")});
  // Conic Curve (mitcad#2236).
  add({"sketch.conic", QObject::tr("Conic Curve"), "conic", [&c] { c.startTool(sketch::conicTool(c)); }, none,
       QObject::tr("A conic: its start, its end and the shoulder where its end tangents meet, shaped by Rho")});
  add({"sketch.polygon", QObject::tr("Circumscribed Polygon"), "polygon",
       draw(sketch::polygonTool, "circumscribed"), none,
       QObject::tr("A regular polygon around a circle: the center, then a side's middle")});
  add({"sketch.polygon_inscribed", QObject::tr("Inscribed Polygon"), "polygon-inscribed",
       draw(sketch::polygonTool, "inscribed"), none,
       QObject::tr("A regular polygon in a circle: the center, then a corner")});
  add({"sketch.polygon_edge", QObject::tr("Edge Polygon"), "polygon-edge",
       draw(sketch::polygonTool, "edge"), none, QObject::tr("A regular polygon from one of its sides")});
  add({"sketch.slot", QObject::tr("Center to Center Slot"), "slot",
       draw(sketch::slotTool, "center_to_center"), none, QObject::tr("A slot between the centers of its ends")});
  add({"sketch.slot_overall", QObject::tr("Overall Slot"), "slot",
       draw(sketch::slotTool, "overall"), none, QObject::tr("A slot from end to end")});
  add({"sketch.slot_center", QObject::tr("Center Point Slot"), "slot",
       draw(sketch::slotTool, "center_point"), none,
       QObject::tr("A slot from its center and the center of an end")});
  add({"sketch.slot_arc", QObject::tr("Three Point Arc Slot"), "slot-arc",
       draw(sketch::slotTool, "three_point_arc"), none, QObject::tr("A curved slot along a 3-point arc")});
  add({"sketch.slot_arc_center", QObject::tr("Center Point Arc Slot"), "slot-arc",
       draw(sketch::slotTool, "center_point_arc"), none,
       QObject::tr("A curved slot along an arc around a center")});
  add({"sketch.spline", QObject::tr("Fit Point Spline"), "spline-fit",
       [&c] { c.startTool(sketch::splineTool(c, false)); }, none,
       QObject::tr("A smooth curve through points; Enter or a double-click ends it, a click on its first "
                   "point closes it")});
  add({"sketch.spline_control", QObject::tr("Control Point Spline"), "spline-control",
       [&c] { c.startTool(sketch::splineTool(c, true)); }, none,
       QObject::tr("A smooth curve shaped by control points; Enter or a double-click ends it")});
  // Equation curves (mitcad#2237), after the splines.
  addEquationCurve(registry, c);
  addEditEquationCurve(registry, c);
  add({"sketch.text", QObject::tr("Text"), "text", [&c] { c.startTool(sketch::textTool(c)); }, none, QObject::tr("Text: click where it starts, type it and its height")});
  addRectangularPattern(registry, c);
  addCircularPattern(registry, c);
  addMirror(registry, c);
  addProject(registry, c);
  addIntersect(registry, c);
  addCleanUpProjections(registry, c);
  addCleanUpStaleProjections(registry, c);
  addProjectAgain(registry, c);
  add({"sketch.dimension", QObject::tr("Sketch Dimension"), "dimension",
       [&c] {
         // A viewport click's queued pick must reach the selection before
         // the tool captures it (mitcad#660).
         c.host().deliverPendingPicks();
         // Three or more objects selected: their equal spacing (mitcad#221).
         if (!dimensionEqualSpacing(c)) {
           c.startTool(sketch::dimensionTool(c));
         }
       },
       QKeySequence(Qt::Key_D),
       QObject::tr("Dimensions what is picked: a length, distance, angle, radius or diameter")});

  // --- MODIFY
  addFillet(registry, c);
  addBlendCurve(registry, c);
  addChamfer(registry, c);
  add({"sketch.trim", QObject::tr("Trim"), "trim", [&c] { c.startTool(sketch::trimTool(c)); },
       QKeySequence(Qt::Key_T), QObject::tr("Removes the piece of a curve between the curves crossing it")});
  add({"sketch.extend", QObject::tr("Extend"), "extend", [&c] { c.startTool(sketch::extendTool(c)); },
       none, QObject::tr("Extends a line or an arc to the next curve")});
  // Extend to Meet (mitcad#2178).
  add({"sketch.extend_to_meet", QObject::tr("Extend to Meet"), "extend-to-meet",
       [&c] { c.startTool(sketch::extendToMeetTool(c)); }, none,
       QObject::tr("Extends or shortens two lines or arcs to the point where they meet, ending in one corner")});
  // Break and Split at Point (mitcad#2108).
  add({"sketch.break", QObject::tr("Break"), "sketch-break", [&c] { c.startTool(sketch::breakTool(c)); }, none,
       QObject::tr("Splits a curve where the curves around the click cross it, keeping every piece")});
  add({"sketch.split", QObject::tr("Split at Point"), "sketch-split", [&c] { c.startTool(sketch::splitTool(c)); },
       none, QObject::tr("Splits a curve at a point, keeping both pieces")});
  addOffset(registry, c);
  addMove(registry, c);
  addScale(registry, c);
  addEditPattern(registry, c);
  addEditOffset(registry, c);
  addEditRho(registry, c);
  addEditText(registry, c);
  // QR codes (mitcad#473).
  addQrCode(registry, c);
  addEditCode(registry, c);
  add({"sketch.construction", QObject::tr("Construction"), "construction",
       [&c] { c.toggleConstruction(); }, QKeySequence(Qt::Key_X),
       QObject::tr("Turns the selected curves into construction geometry and back; with nothing selected, "
                   "new geometry is construction geometry")},
      CommandDef::Kind::Action);
  add({"sketch.centerline", QObject::tr("Centerline"), "centerline", [&c] { c.toggleCenterline(); }, none, QObject::tr("Turns the selected lines into center lines and back")},
      CommandDef::Kind::Action);
  addConvertProjected(registry, c);
  addProfileBoundary(registry, c);
  add({"sketch.delete", QObject::tr("Delete"), "delete", [&c] { c.deleteSelection(); },
       QKeySequence(Qt::Key_Delete),
       QObject::tr("Deletes the selected sketch geometry, constraints and dimensions")},
      CommandDef::Kind::Action,
#ifdef Q_OS_MACOS
      // Mac keyboards have Backspace (the delete key) where others have Delete. A focused
      // text field still gets it first: QLineEdit accepts the ShortcutOverride for it.
      {QKeySequence(Qt::Key_Backspace)}
#else
      {}
#endif
  );
  // Copy, Cut and Paste of sketch geometry (mitcad#788, sketch/SketchClipboard.cpp):
  // the reserved keys, which are a text field's outside sketches.
  add({"sketch.cut", QObject::tr("Cut"), "cut", [&c] { c.cutSelection(); },
       QKeySequence(QKeySequence::Cut),
       QObject::tr("Copies the selected sketch geometry and its constraints, then deletes it")},
      CommandDef::Kind::Action);
  add({"sketch.copy", QObject::tr("Copy"), "copy", [&c] { c.copySelection(); },
       QKeySequence(QKeySequence::Copy),
       QObject::tr("Copies the selected sketch geometry with the constraints among it")},
      CommandDef::Kind::Action);
  add({"sketch.paste", QObject::tr("Paste"), "paste", [&c] { c.paste(); },
       QKeySequence(QKeySequence::Paste),
       QObject::tr("Places copied sketch geometry where you click, in this or another sketch")},
      CommandDef::Kind::Action);
  c.setToolIcon(QObject::tr("Paste"), QStringLiteral("paste")); // the Paste tool's cursor
  // Sketch blocks (mitcad#2110, sketch/SketchBlocks.cpp).
  add({"sketch.create_block", QObject::tr("Create Block"), "create-block", [&c] { c.createBlock(); }, none,
       QObject::tr("Saves the selected geometry as a named block of the design; the selection becomes an "
                   "instance of it")});
  add({"sketch.insert_block", QObject::tr("Insert Block"), "insert-block", [&c] { c.insertBlock(); }, none,
       QObject::tr("Places an instance of one of the design's blocks, which follows every edit of the block")});
  add({"sketch.explode_block", QObject::tr("Explode Block"), "explode-block", [&c] { c.explodeBlock(); }, none,
       QObject::tr("Turns the selected block instance into plain sketch geometry with its block's constraints")},
      CommandDef::Kind::Action);
  add({"sketch.edit_block", QObject::tr("Edit Block"), "edit-block", [&c] { c.editBlock(); }, none,
       QObject::tr("Opens the selected instance's block here to change it; Finish Block updates every instance")},
      CommandDef::Kind::Action);
  add({"sketch.finish_block", QObject::tr("Finish Block"), "finish-block", [&c] { c.finishBlock(false); }, none,
       QObject::tr("Takes the edit of the block: every instance of it follows")},
      CommandDef::Kind::Action);
  add({"sketch.cancel_block", QObject::tr("Cancel Block"), "cancel", [&c] { c.finishBlock(true); }, none,
       QObject::tr("Ends the edit of the block and leaves it as it was")},
      CommandDef::Kind::Action);
  c.setToolIcon(QObject::tr("Create Block"), QStringLiteral("create-block"));
  c.setToolIcon(QObject::tr("Insert Block"), QStringLiteral("insert-block"));
  // Explode Text (mitcad#2163, SketchController::explodeText): in the context
  // menu of a text and of a region a text bounds.
  add({"sketch.explode_text", QObject::tr("Explode Text"), "explode-text", [&c] { c.explodeText(); }, none,
       QObject::tr("Turns the selected text or QR code into plain lines and splines, which Trim and the other "
                   "sketch tools take")},
      CommandDef::Kind::Action);

  // --- CONSTRAINTS
  for (const QString& id : sketchConstraintCommands()) {
    if (!id.startsWith(QStringLiteral("sketch.constraint."))) {
      continue; // Equal Spacing: registered below
    }
    const QString type = id.mid(static_cast<int>(QStringLiteral("sketch.constraint.").size()));
    CommandDef def = sketchCommand("", sketch::constraintName(type), "", CommandDef::Kind::Tool);
    def.id = id;
    def.icon = type == QStringLiteral("horizontal_vertical") ? QStringLiteral("horizontal-vertical")
               : type == QStringLiteral("symmetric")         ? QStringLiteral("symmetry")
               : type == QStringLiteral("smooth")            ? QStringLiteral("curvature")
                                                              : type;
    // Shift and a letter, in sketches only (mitcad#1229): the same command
    // as the palette's and the toolbar's button, its key in their tooltips,
    // Keyboard Shortcuts (Sketch) and the Keyboard and Mouse Overview.
    def.shortcut = sketchConstraintShortcut(type);
    def.keywords = {QStringLiteral("constraint")};
    c.setToolIcon(def.name, def.icon);
    def.run = [&c, type] {
      if (!sketch::constrainSelection(c, type)) {
        c.startTool(sketch::constraintTool(c, type));
      }
    };
    if (type == QStringLiteral("horizontal_vertical")) {
      // Nearer to, or the other way when only that can be added
      // (mitcad#2184): the user guide says so.
      def.tooltip = QObject::tr("A line horizontal or vertical, or two points level or plumb");
    } else if (type == QStringLiteral("coincident")) {
      def.tooltip = QObject::tr("A point on a curve or at another point; two curve ends become one point");
    } else if (type == QStringLiteral("tangent") || type == QStringLiteral("smooth")) {
      // The end it applies at (mitcad#783).
      def.tooltip = QObject::tr("Two curves joined smoothly; pick each near the end where it should be tangent");
    } else if (type == QStringLiteral("fix")) {
      def.tooltip = QObject::tr("Fixes geometry in place, or frees it");
    }
    registry.add(def);
  }
  // Equal spacing (mitcad#221).
  registerEqualSpacingCommands(registry, c);
  // Auto Constrain (mitcad#2019).
  registerAutoConstrainCommands(registry, c);
  // Show Unconstrained (mitcad#2026, GitHub mitcad/Mitcad#413): the curves
  // and points that are not fully constrained selected; also the Sketch
  // Palette's button, the sketch's context menu and a click on the degrees
  // of freedom badge.
  add({"sketch.show_unconstrained", QObject::tr("Show Unconstrained"), "select", [&c] { c.selectUnconstrained(); },
       QKeySequence(), QObject::tr("Selects the curves and points that are not fully constrained")},
      CommandDef::Kind::Action);
}

} // namespace mitcad
