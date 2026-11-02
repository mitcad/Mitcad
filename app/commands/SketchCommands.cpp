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

#include <QDir>
#include <QFileInfo>
#include <QFontDatabase>
#include <QJsonArray>
#include <QKeySequence>
#include <QSet>

#include "../browser/DocumentHost.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/Dialogs.hpp"
#include "../sketch/FontFiles.hpp"
#include "../sketch/SketchController.hpp"
#include "../sketch/SketchTool.hpp"
#include "CommandSupport.hpp"
#include "PatternOptions.hpp"

namespace mitcad {
namespace {

using cmd::flipApplies;
using cmd::negated;
using cmd::number;
using cmd::oneDirection;
using cmd::symmetricDirection;
using sketch::SketchController;

const QString kSketchTab = QStringLiteral("SKETCH");

// `group` is the toolbar group, "CREATE/Circles" with its subgroup
// (CommandDef::subgroup, mitcad#222).
CommandDef sketchCommand(const char* id, const QString& name, const char* icon, const char* group,
                         CommandDef::Kind kind) {
  CommandDef def;
  def.id = QString::fromLatin1(id);
  def.name = name;
  def.icon = QString::fromLatin1(icon);
  def.kind = kind;
  def.mode = CommandDef::Mode::Sketch;
  def.tab = kSketchTab;
  const QString path = QString::fromLatin1(group);
  const qsizetype slash = path.indexOf(QLatin1Char('/'));
  def.group = slash < 0 ? path : path.left(slash);
  def.subgroup = slash < 0 ? QString() : path.mid(slash + 1);
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

// Lines, arcs, elliptical arcs and open splines chain; a closed curve is
// offset on its own.
bool joinable(const sketch::CurveData& c) {
  return c.isLine() || c.isArc() || c.type == QStringLiteral("elliptical_arc") ||
         (c.isSpline() && c.start != c.end);
}

// Whether two open curves lie on each other (an edge projected twice,
// mitcad#741): of one kind, through the same places, a line or a spline
// either way round.
bool sameCurve(const sketch::CurveData& a, const sketch::CurveData& b) {
  if (a.id == b.id || a.type != b.type || a.curve.isClosed() || b.curve.isClosed()) {
    return false;
  }
  const sketch::Curve2& p = a.curve;
  const sketch::Curve2& q = b.curve;
  const double size = std::max({1.0, sketch::norm(p.startPoint()), sketch::norm(p.endPoint())});
  const double tolerance = 1e-7 * size;
  const auto along = [](const sketch::Curve2& c, double f) { return c.at(c.t0() + f * (c.t1() - c.t0())); };
  const std::array<double, 5> fractions = {0.0, 0.25, 0.5, 0.75, 1.0};
  bool ahead = true;
  bool back = a.isLine() || a.isSpline();
  for (const double f : fractions) {
    ahead = ahead && sketch::dist(along(p, f), along(q, f)) <= tolerance;
    back = back && sketch::dist(along(p, f), along(q, 1.0 - f)) <= tolerance;
  }
  return ahead || back;
}

// Whether a curve shares an end point with another curve a chain selection
// would join to it.
bool joinsOthers(const sketch::SketchModel& model, const sketch::CurveData& c) {
  for (const sketch::CurveData& other : model.curves) {
    if (other.id != c.id && joinable(other) && other.construction == c.construction &&
        (other.start == c.start || other.end == c.start || other.start == c.end || other.end == c.end)) {
      return true;
    }
  }
  return false;
}

// A picked curve, or the curve it lies on when it joins nothing and that
// one does (mitcad#741: a face projected and one of its edges again, the
// edge with points of its own): the chain grows through the face's curve.
QString chainStart(const sketch::SketchModel& model, const QString& picked) {
  const sketch::CurveData* c = model.curve(picked);
  if (c == nullptr || !joinable(*c) || joinsOthers(model, *c)) {
    return picked;
  }
  for (const sketch::CurveData& other : model.curves) {
    if (joinable(other) && other.construction == c->construction && sameCurve(*c, other) &&
        joinsOthers(model, other)) {
      return other.id;
    }
  }
  return picked;
}

// The curves joined to the picked ones end to end, as a chain
// selection takes them: through points where exactly two such curves meet.
QStringList chainOf(const sketch::SketchModel& model, const QStringList& picked) {
  QStringList chain;
  for (const QString& id : picked) {
    const QString start = chainStart(model, id);
    if (!chain.contains(start)) {
      chain << start;
    }
  }
  QSet<QString> in(chain.begin(), chain.end());
  bool grew = true;
  while (grew) {
    grew = false;
    for (const QString& id : QStringList(chain)) {
      const sketch::CurveData* c = model.curve(id);
      if (c == nullptr || !joinable(*c)) {
        continue;
      }
      for (const QString& end : {c->start, c->end}) {
        QStringList at;
        for (const sketch::CurveData& other : model.curves) {
          if (joinable(other) && other.construction == c->construction &&
              (other.start == end || other.end == end)) {
            at << other.id;
          }
        }
        if (at.size() != 2) {
          continue;
        }
        for (const QString& next : at) {
          if (!in.contains(next)) {
            in.insert(next);
            chain << next;
            grew = true;
          }
        }
      }
    }
  }
  return chain;
}

void addOffset(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.offset", QObject::tr("Offset"), "offset", "MODIFY",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.shortcut = QKeySequence(Qt::Key_O);
  def.pinned = true;
  def.tooltip = QObject::tr("A copy of a chain of curves at a distance, which stays at it");
  def.keywords = {QStringLiteral("parallel copy")};
  def.inputs = {
      selectionInput(QStringLiteral("curves"), QObject::tr("Curves"), SelectKind::SketchCurve, 1, 0)
          .withAccepts(ofSketch(c)),
      checkInput(QStringLiteral("chain"), QObject::tr("Chain Selection"), true),
      valueInput(QStringLiteral("distance"), QObject::tr("Offset Position"), ValueKind::Length,
                 QStringLiteral("5")),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip")),
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    QStringList curves;
    for (const SelectionItem& item : s.items(QStringLiteral("curves"))) {
      curves << item.name;
    }
    if (s.checked(QStringLiteral("chain"))) {
      curves = chainOf(c.model(), curves);
    }
    QString distance = s.expression(QStringLiteral("distance"));
    if (s.checked(QStringLiteral("flip"))) {
      distance = negated(distance);
    }
    QJsonObject cmd = base(c, "sketch.offset");
    cmd.insert(QStringLiteral("curves"), QJsonArray::fromStringList(curves));
    cmd.insert(QStringLiteral("distance"), distance);
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
  CommandDef def = sketchCommand("sketch.mirror", QObject::tr("Mirror"), "mirror", "CREATE/Patterns",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("Mirrored copies about a line or a sketch axis, kept symmetric");
  // The line, or the sketch's x or y axis (mitcad#876), which sketch mode
  // picks for this input only, as its pick id.
  InputDef mirrorLine =
      selectionInput(QStringLiteral("axis"), QObject::tr("Mirror Line"), SelectKind::SketchCurve, 1, 1)
          .withAccepts(linesOf(c));
  mirrorLine.sketchAxes = true;
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
                                 "circular-pattern", "CREATE/Patterns", CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("Copies around a center point (the origin when none is picked)");
  def.inputs = {
      selectionInput(QStringLiteral("entities"), QObject::tr("Objects"),
                     SelectKind::SketchCurve | SelectKind::SketchPoint, 1, 0)
          .withAccepts(ofSketch(c)),
      selectionInput(QStringLiteral("center"), QObject::tr("Center Point"), SelectKind::SketchPoint, 0, 1)
          .withAccepts(ofSketch(c)),
      valueInput(QStringLiteral("quantity"), QObject::tr("Quantity"), ValueKind::Unitless, QStringLiteral("3")),
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

void addRectangularPattern(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.rectangular_pattern", QObject::tr("Rectangular Pattern"),
                                 "rectangular-pattern", "CREATE/Patterns", CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("Copies in rows and columns along a line (the sketch x axis when none is picked)");
  def.inputs = {
      selectionInput(QStringLiteral("entities"), QObject::tr("Objects"),
                     SelectKind::SketchCurve | SelectKind::SketchPoint, 1, 0)
          .withAccepts(ofSketch(c)),
      selectionInput(QStringLiteral("direction"), QObject::tr("Direction"), SelectKind::SketchCurve, 0, 1)
          .withAccepts(linesOf(c)),
      sketchDirectionType(QStringLiteral("direction_type1"), QObject::tr("Direction Type")),
      valueInput(QStringLiteral("quantity1"), QObject::tr("Quantity"), ValueKind::Unitless, QStringLiteral("3")),
      valueInput(QStringLiteral("distance1"), QObject::tr("Spacing"), ValueKind::Length, QStringLiteral("10")),
      flipInput(QStringLiteral("flip1"), QObject::tr("Flip"))
          .withTooltip(QObject::tr("Copies against the direction"))
          .withVisible(notSymmetric(QStringLiteral("direction_type1"))),
      sketchDirectionType(QStringLiteral("direction_type2"), QObject::tr("Direction Type 2")),
      valueInput(QStringLiteral("quantity2"), QObject::tr("Quantity 2"), ValueKind::Unitless, QStringLiteral("1")),
      valueInput(QStringLiteral("distance2"), QObject::tr("Spacing 2"), ValueKind::Length, QStringLiteral("10")),
      flipInput(QStringLiteral("flip2"), QObject::tr("Flip 2"))
          .withTooltip(QObject::tr("Copies against the second direction"))
          .withVisible(notSymmetric(QStringLiteral("direction_type2"))),
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
    QJsonArray direction{1.0, 0.0};
    const Selection& line = s.items(QStringLiteral("direction"));
    if (!line.isEmpty()) {
      if (const sketch::CurveData* d = c.model().curve(line.first().name)) {
        const sketch::V2 u = sketch::unit(d->curve.b - d->curve.a);
        direction = QJsonArray{u.x, u.y};
      }
    }
    QJsonObject cmd = base(c, "sketch.rectangular_pattern");
    cmd.insert(QStringLiteral("entities"), names(s.items(QStringLiteral("entities"))));
    cmd.insert(QStringLiteral("direction"), direction);
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

void addProject(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.project", QObject::tr("Project"), "project", "CREATE/References",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.shortcut = QKeySequence(Qt::Key_P);
  def.tooltip = QObject::tr("Body edges, faces and vertices, and other sketches' curves, onto the plane");
  def.keywords = {QStringLiteral("include"), QStringLiteral("reference")};
  def.inputs = {
      selectionInput(QStringLiteral("geometry"), QObject::tr("Geometry"),
                     SelectKind::Edge | SelectKind::Face | SelectKind::Vertex | SelectKind::SketchCurve |
                         SelectKind::SketchPoint | SelectKind::Sketch,
                     1, 0)
          .withAccepts([&c](const SelectionItem& item) {
            // Not the edited sketch's own geometry, nor the sketch itself.
            return !projectsSketchGeometry(item) || item.owner != c.uid();
          }),
      checkInput(QStringLiteral("linked"), QObject::tr("Projection Link"), true)
          .withTooltip(QObject::tr("The projection follows its source when the design changes")),
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    QJsonArray commands;
    for (const SelectionItem& item : s.items(QStringLiteral("geometry"))) {
      QJsonObject cmd = base(c, "sketch.project");
      if (projectsSketchGeometry(item)) {
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
    // Other sketches' geometry by name (mitcad#904).
    QStringList sketched;
    for (const SelectionItem& item : items) {
      if (projectsSketchGeometry(item)) {
        sketched << item.describe();
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
  CommandDef def = sketchCommand("sketch.intersect", QObject::tr("Intersect"), "sketch-intersect",
                                 "CREATE/References", CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("Where the sketch plane cuts bodies, faces, edges and other sketches' curves");
  def.keywords = {QStringLiteral("section"), QStringLiteral("cut"), QStringLiteral("reference"),
                  QStringLiteral("project")};
  def.inputs = {
      selectionInput(QStringLiteral("geometry"), QObject::tr("Geometry"),
                     SelectKind::Body | SelectKind::Face | SelectKind::Edge | SelectKind::SketchCurve |
                         SelectKind::SketchPoint | SelectKind::Sketch,
                     1, 0)
          .withAccepts([&c](const SelectionItem& item) {
            // Not the edited sketch's own geometry, nor the sketch itself.
            return !projectsSketchGeometry(item) || item.owner != c.uid();
          }),
      checkInput(QStringLiteral("linked"), QObject::tr("Projection Link"), true)
          .withTooltip(QObject::tr("The curves follow the design when it changes")),
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    QJsonArray commands;
    for (const SelectionItem& item : s.items(QStringLiteral("geometry"))) {
      QJsonObject cmd = base(c, "sketch.project");
      if (projectsSketchGeometry(item)) {
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
    // Other sketches' geometry by name, as Project's log (mitcad#1031).
    QStringList sketched;
    for (const SelectionItem& item : items) {
      if (projectsSketchGeometry(item)) {
        sketched << item.describe();
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
  CommandDef def = sketchCommand("sketch.clean_up_projections", QObject::tr("Clean Up Projections"), "project",
                                 "CREATE/References", CommandDef::Kind::Feature);
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
                                 "project", "CREATE/References", CommandDef::Kind::Feature);
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

// Convert to Sketch Geometry (mitcad#257): the selected projected curves and
// points become ordinary free sketch geometry that no longer follows its
// source, at once, as one undo step. Enabled while the selection holds
// projected geometry (SketchController::selectionHasProjected); also in the
// context menu of projected geometry.
void addConvertProjected(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.convert_projected", QObject::tr("Convert to Sketch Geometry"),
                                 "convert-projected", "MODIFY", CommandDef::Kind::Action);
  def.run = [&c] { c.convertProjected(); };
  def.enabled = [&c] { return c.selectionHasProjected(); };
  def.tooltip = QObject::tr("Turns the selected projected geometry into ordinary sketch geometry");
  def.keywords = {QStringLiteral("projected"), QStringLiteral("unlink"), QStringLiteral("reference"),
                  QStringLiteral("convert")};
  registry.add(def);
}

// A point of the sketch where exactly two lines of one kind end (the drawn
// ones, else the construction ones): a corner Sketch Fillet rounds
// (mitcad#867; the model's `sketch.fillet` with `points`).
bool cornerOfTwoLines(const sketch::SketchModel& model, const QString& point) {
  for (const bool construction : {false, true}) {
    int lines = 0;
    int others = 0;
    for (const sketch::CurveData& curve : model.curves) {
      if (curve.construction == construction && (curve.start == point || curve.end == point)) {
        ++(curve.isLine() ? lines : others);
      }
    }
    if (lines + others > 0) {
      return lines == 2 && others == 0;
    }
  }
  return false;
}

// Sketch Fillet (mitcad#867): two lines, a closed chain of lines (a whole
// rectangle), lines and corner points, several corners at once with one
// radius dimension and the arcs Equal; one undo step.
void addFillet(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.fillet", QObject::tr("Sketch Fillet"), "sketch-fillet", "MODIFY",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.pinned = true;
  def.tooltip = QObject::tr("Rounds line corners with tangent arcs: two lines, a chain or corner points");
  def.keywords = {QStringLiteral("round corners"), QStringLiteral("rounded rectangle")};
  def.inputs = {
      selectionInput(QStringLiteral("corners"), QObject::tr("Lines or Corners"),
                     SelectKind::SketchCurve | SelectKind::SketchPoint, 1, 0)
          .withAccepts([&c](const SelectionItem& item) {
            if (item.owner != c.uid()) {
              return false;
            }
            if (item.geometry == QStringLiteral("line")) {
              return true;
            }
            return c.model().pointAt(item.name).has_value() && cornerOfTwoLines(c.model(), item.name);
          })
          .withTooltip(QObject::tr("Two lines: their corner; more lines: every corner where two end together; "
                                   "a point: its corner")),
      valueInput(QStringLiteral("radius"), QObject::tr("Radius"), ValueKind::Length, QStringLiteral("2")),
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    QJsonArray curves;
    QJsonArray points;
    for (const SelectionItem& item : s.items(QStringLiteral("corners"))) {
      (c.model().pointAt(item.name) ? points : curves).append(item.name);
    }
    if (points.isEmpty() && curves.size() < 2) {
      return Built::failure(QObject::tr("Pick two lines, a chain of lines or corner points."),
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
  CommandDef def = sketchCommand("sketch.chamfer", QObject::tr("Sketch Chamfer"), "sketch-chamfer", "MODIFY",
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
      valueInput(QStringLiteral("distance"), QObject::tr("Distance"), ValueKind::Length, QStringLiteral("2")),
      valueInput(QStringLiteral("distance2"), QObject::tr("Distance 2"), ValueKind::Length, QStringLiteral("2"))
          .withVisible([](const CommandState& s) { return s.choice(QStringLiteral("type")) == QStringLiteral("two_distances"); }),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("45 deg"))
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

// Where the points of sketch entities are, each point once, in the order
// the entities name them: "p2 (-20.000, 0.000), p3 (20.000, 10.000)";
// "none" without points.
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

// Move/Copy starts at 0 (mitcad#1163): X and Y Distance 0 mm, so the
// preview that runs as the panel opens leaves the objects where they are
// until a distance is typed. Rotate's Angle keeps its 90 deg (a separate
// type the user chooses).
void addMove(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.move", QObject::tr("Move/Copy"), "move", "MODIFY",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.shortcut = QKeySequence(Qt::Key_M);
  def.tooltip = QObject::tr("Moves or copies sketch geometry; a move keeps its constraints");
  const QVector<QPair<QString, QString>> types = {{QStringLiteral("translate"), QObject::tr("Translate")},
                                                   {QStringLiteral("rotate"), QObject::tr("Rotate")}};
  const auto rotating = [](const CommandState& s) { return s.choice(QStringLiteral("type")) == QStringLiteral("rotate"); };
  const auto translating = [](const CommandState& s) {
    return s.choice(QStringLiteral("type")) != QStringLiteral("rotate");
  };
  def.inputs = {
      selectionInput(QStringLiteral("entities"), QObject::tr("Objects"),
                     SelectKind::SketchCurve | SelectKind::SketchPoint, 1, 0)
          .withAccepts(ofSketch(c)),
      choiceInput(QStringLiteral("type"), QObject::tr("Move Type"), types, QStringLiteral("translate")),
      valueInput(QStringLiteral("dx"), QObject::tr("X Distance"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(translating),
      valueInput(QStringLiteral("dy"), QObject::tr("Y Distance"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(translating),
      selectionInput(QStringLiteral("center"), QObject::tr("Pivot Point"), SelectKind::SketchPoint, 0, 1)
          .withAccepts(ofSketch(c))
          .withVisible(rotating),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("90 deg"))
          .withVisible(rotating),
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
    cmd.insert(QStringLiteral("entities"), names(entities));
    if (s.choice(QStringLiteral("type")) == QStringLiteral("rotate")) {
      const Selection& center = s.items(QStringLiteral("center"));
      cmd.insert(QStringLiteral("rotate"),
                 QJsonObject{{QStringLiteral("center"), center.isEmpty() ? QJsonValue(QJsonArray{0.0, 0.0})
                                                                         : QJsonValue(center.first().name)},
                             {QStringLiteral("angle"), s.value(QStringLiteral("angle"))}});
    } else {
      cmd.insert(QStringLiteral("by"), QJsonArray{s.value(QStringLiteral("dx")), s.value(QStringLiteral("dy"))});
    }
    cmd.insert(QStringLiteral("copy"), s.checked(QStringLiteral("copy")));
    return Built::of(cmd);
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

// --- Patterns, offsets and texts edited afterwards (P3, P4)

// The item of the selection an edit panel works on, by the model's lookup.
template <class Lookup>
std::function<bool(const SelectionItem&)> editable(SketchController& c, Lookup lookup) {
  return [&c, lookup](const SelectionItem& item) {
    return item.owner == c.uid() && lookup(c.model(), item.name) != nullptr;
  };
}

void addEditPattern(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.edit_pattern", QObject::tr("Edit Pattern"), "circular-pattern", "MODIFY",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("The quantity, angle or spacing of a sketch pattern, picked by one of its curves");
  def.keywords = {QStringLiteral("pattern quantity")};
  const auto lookup = [](const sketch::SketchModel& m, const QString& id) { return m.patternOf(id); };
  const auto circular = [](const CommandState& s) { return s.choice(QStringLiteral("kind")) == QStringLiteral("circular"); };
  const auto rectangular = [](const CommandState& s) {
    return s.choice(QStringLiteral("kind")) == QStringLiteral("rectangular");
  };
  def.inputs = {
      selectionInput(QStringLiteral("entity"), QObject::tr("Pattern"), SelectKind::SketchCurve | SelectKind::SketchPoint,
                     1, 1)
          .withAccepts(editable(c, lookup)),
      valueInput(QStringLiteral("quantity"), QObject::tr("Quantity"), ValueKind::Unitless, QStringLiteral("3")),
      valueInput(QStringLiteral("angle"), QObject::tr("Total Angle"), ValueKind::Angle, QStringLiteral("360 deg"))
          .withVisible(circular),
      valueInput(QStringLiteral("distance1"), QObject::tr("Spacing"), ValueKind::Length, QStringLiteral("10"))
          .withVisible(rectangular),
      flipInput(QStringLiteral("flip1"), QObject::tr("Flip"))
          .withTooltip(QObject::tr("Copies against the direction"))
          .withVisible(rectangular),
      valueInput(QStringLiteral("quantity2"), QObject::tr("Quantity 2"), ValueKind::Unitless, QStringLiteral("1"))
          .withVisible(rectangular),
      valueInput(QStringLiteral("distance2"), QObject::tr("Spacing 2"), ValueKind::Length, QStringLiteral("10"))
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
      s.setText(QStringLiteral("distance1"), p->values.value(0));
      s.setText(QStringLiteral("distance2"), p->values.value(1));
      s.setChecked(QStringLiteral("flip1"), false);
      s.setChecked(QStringLiteral("flip2"), false);
    }
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    const Selection& picked = s.items(QStringLiteral("entity"));
    const sketch::PatternData* p = picked.isEmpty() ? nullptr : c.model().patternOf(picked.first().name);
    if (p == nullptr) {
      return Built::failure(QObject::tr("Pick a curve of a pattern."), QStringLiteral("entity"));
    }
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
  registry.add(def);
}

void addEditOffset(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.edit_offset", QObject::tr("Edit Offset"), "offset", "MODIFY",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("The distance or side of an offset, picked by one of the curves it made");
  const auto lookup = [](const sketch::SketchModel& m, const QString& id) { return m.offsetOf(id); };
  def.inputs = {
      selectionInput(QStringLiteral("curve"), QObject::tr("Offset"), SelectKind::SketchCurve, 1, 1)
          .withAccepts(editable(c, lookup)),
      valueInput(QStringLiteral("distance"), QObject::tr("Offset Position"), ValueKind::Length, QStringLiteral("5")),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip")),
  };
  def.init = [&c](CommandState& s, const CommandContext&) {
    const Selection& picked = s.items(QStringLiteral("curve"));
    const sketch::OffsetData* o = picked.isEmpty() ? nullptr : c.model().offsetOf(picked.first().name);
    if (o != nullptr) {
      s.setText(QStringLiteral("distance"), o->expression);
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
    return Built::of(cmd);
  };
  def.describe = [&c](const CommandState& s, const CommandContext&) {
    const Selection& picked = s.items(QStringLiteral("curve"));
    const sketch::OffsetData* o = picked.isEmpty() ? nullptr : c.model().offsetOf(picked.first().name);
    return QStringLiteral("Edited offset %1: %2%3")
        .arg(o != nullptr ? o->id : QString(), s.expression(QStringLiteral("distance")),
             s.checked(QStringLiteral("flip")) ? QStringLiteral(", flipped") : QString());
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
      sketchCommand("sketch.add_code", QObject::tr("QR Code"), "text", "CREATE/Text", CommandDef::Kind::Feature);
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

// Edit QR Code (mitcad#473): a code's content, module size and error
// correction (`sketch.edit_code`); a double-click on the code opens it.
void addEditCode(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.edit_code", QObject::tr("Edit QR Code"), "text", "MODIFY",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("A QR code's content, module size and error correction");
  const auto lookup = [](const sketch::SketchModel& m, const QString& id) -> const sketch::TextData* {
    const sketch::TextData* t = m.text(id);
    return t != nullptr && t->code ? t : nullptr;
  };
  def.inputs = {
      selectionInput(QStringLiteral("code"), QObject::tr("QR Code"), SelectKind::SketchCurve, 1, 1)
          .withAccepts(editable(c, lookup)),
      textInput(QStringLiteral("content"), QObject::tr("Content")),
      valueInput(QStringLiteral("module"), QObject::tr("Module Size"), ValueKind::Length, QStringLiteral("1 mm")),
      choiceInput(QStringLiteral("ecc"), QObject::tr("Error Correction"), kCodeLevels, QStringLiteral("M")),
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
  };
  def.build = [&c, lookup](const CommandState& s, const CommandContext&) -> Built {
    const Selection& picked = s.items(QStringLiteral("code"));
    if (picked.isEmpty() || lookup(c.model(), picked.first().name) == nullptr) {
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

void addEditText(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.edit_text", QObject::tr("Edit Text"), "text", "MODIFY",
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
  def.inputs = {
      selectionInput(QStringLiteral("text"), QObject::tr("Text"), SelectKind::SketchCurve, 1, 1)
          .withAccepts(editable(c, lookup)),
      textInput(QStringLiteral("content"), QObject::tr("Text")),
      // The text from a text parameter or expression (mitcad#664).
      textInput(QStringLiteral("value"), QObject::tr("Text Parameter"))
          .withTooltip(QObject::tr(
              "A text parameter or text expression, such as 'Box ' & Width, for the text; empty for the text above")),
      valueInput(QStringLiteral("height"), QObject::tr("Height"), ValueKind::Length, QStringLiteral("5")),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withVisible(free),
      fontInput(c),
      checkInput(QStringLiteral("bold"), QObject::tr("Bold")),
      checkInput(QStringLiteral("italic"), QObject::tr("Italic")),
      choiceInput(QStringLiteral("align"), QObject::tr("Horizontal Alignment"),
                  {{QStringLiteral("left"), QObject::tr("Left")},
                   {QStringLiteral("center"), QObject::tr("Center")},
                   {QStringLiteral("right"), QObject::tr("Right")}},
                  QStringLiteral("left")),
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
    const QString content = s.text(QStringLiteral("content"));
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
    const QString fontFile = sketch::fontFileOfChoice(s.choice(QStringLiteral("font")));
    if (fontFile.isEmpty()) {
      cmd.insert(QStringLiteral("font"), s.choice(QStringLiteral("font")));
    } else {
      cmd.insert(QStringLiteral("font_file"), fontFile);
    }
    cmd.insert(QStringLiteral("bold"), s.checked(QStringLiteral("bold")));
    cmd.insert(QStringLiteral("italic"), s.checked(QStringLiteral("italic")));
    cmd.insert(QStringLiteral("align"), s.choice(QStringLiteral("align")));
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
    return QStringLiteral("Edited text %1: \"%2\", %3, %4")
        .arg(picked.isEmpty() ? QString() : picked.first().name, s.text(QStringLiteral("content")),
             file.isEmpty() ? font : sketch::fontFileLabel(file), s.choice(QStringLiteral("align")));
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
    const char* group;
    std::function<void()> run;
    QKeySequence shortcut;
    bool pinned;
    QString tooltip;
  };
  const auto add = [&registry, &c](const Spec& spec, CommandDef::Kind kind = CommandDef::Kind::Tool,
                                   const QList<QKeySequence>& alternates = {}) {
    CommandDef def = sketchCommand(spec.id, spec.name, spec.icon, spec.group, kind);
    def.run = spec.run;
    def.shortcut = spec.shortcut;
    def.alternates = alternates;
    def.pinned = spec.pinned;
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

  // --- CREATE, in submenus (mitcad#222): Lines, Rectangles, Circles,
  // Arcs, Other Geometry, Text, Patterns, References; a pinned command is
  // its submenu's split button default. Offset is in MODIFY.
  add({"sketch.line", QObject::tr("Line"), "line", "CREATE/Lines", [&c] { c.startTool(sketch::lineTool(c)); },
       QKeySequence(Qt::Key_L), true, QObject::tr("Lines end to end; drag from the end for a tangent arc")});
  add({"sketch.rectangle", QObject::tr("2-Point Rectangle"), "rectangle", "CREATE/Rectangles",
       draw(sketch::rectangleTool, "two_point"), QKeySequence(Qt::Key_R), true,
       QObject::tr("A rectangle from two opposite corners")});
  add({"sketch.rectangle_3pt", QObject::tr("3-Point Rectangle"), "rectangle-3pt", "CREATE/Rectangles",
       draw(sketch::rectangleTool, "three_point"), none, false,
       QObject::tr("A rectangle at any angle: a side, then the opposite side")});
  add({"sketch.rectangle_center", QObject::tr("Center Rectangle"), "rectangle-center", "CREATE/Rectangles",
       draw(sketch::rectangleTool, "center"), none, false, QObject::tr("A rectangle from its center and a corner")});
  // Rounded corners (mitcad#867): four tangent arcs, Equal, one radius.
  add({"sketch.rectangle_rounded", QObject::tr("2-Point Rounded Rectangle"), "rectangle-rounded",
       "CREATE/Rectangles", draw(sketch::roundedRectangleTool, "two_point"), none, false,
       QObject::tr("A rectangle with rounded corners from two opposite corners and a corner radius")});
  add({"sketch.rectangle_rounded_center", QObject::tr("Center Rounded Rectangle"), "rectangle-rounded-center",
       "CREATE/Rectangles", draw(sketch::roundedRectangleTool, "center"), none, false,
       QObject::tr("A rectangle with rounded corners from its center, a corner and a corner radius")});
  add({"sketch.circle", QObject::tr("Center Diameter Circle"), "circle", "CREATE/Circles",
       draw(sketch::circleTool, "center"), QKeySequence(Qt::Key_C), true,
       QObject::tr("A circle from its center and a point on it")});
  add({"sketch.circle_2pt", QObject::tr("2-Point Circle"), "circle-2pt", "CREATE/Circles",
       draw(sketch::circleTool, "two_point"), none, false, QObject::tr("A circle from the ends of a diameter")});
  add({"sketch.circle_3pt", QObject::tr("3-Point Circle"), "circle-3pt", "CREATE/Circles",
       draw(sketch::circleTool, "three_point"), none, false, QObject::tr("A circle through three points")});
  add({"sketch.circle_2tangent", QObject::tr("2-Tangent Circle"), "circle-tangent", "CREATE/Circles",
       draw(sketch::circleTool, "two_tangent"), none, false, QObject::tr("A circle tangent to two lines")});
  add({"sketch.circle_3tangent", QObject::tr("3-Tangent Circle"), "circle-tangent", "CREATE/Circles",
       draw(sketch::circleTool, "three_tangent"), none, false, QObject::tr("A circle tangent to three lines")});
  add({"sketch.arc", QObject::tr("3-Point Arc"), "arc-3pt", "CREATE/Arcs", draw(sketch::arcTool, "three_point"),
       none, true, QObject::tr("An arc from its start and end through a third point")});
  add({"sketch.arc_center", QObject::tr("Center Point Arc"), "arc-center", "CREATE/Arcs",
       draw(sketch::arcTool, "center"), none, false, QObject::tr("An arc around a center, from its start to its end")});
  add({"sketch.arc_tangent", QObject::tr("Tangent Arc"), "arc-tangent", "CREATE/Arcs",
       draw(sketch::arcTool, "tangent"), none, false, QObject::tr("An arc that continues a line or an arc smoothly")});
  // No natural default: the submenu has no button of its own.
  add({"sketch.ellipse", QObject::tr("Ellipse"), "ellipse", "CREATE/Other Geometry",
       [&c] { c.startTool(sketch::ellipseTool(c)); }, none, false,
       QObject::tr("An ellipse: the center, the end of the major axis, a point on it")});
  add({"sketch.polygon", QObject::tr("Circumscribed Polygon"), "polygon", "CREATE/Other Geometry",
       draw(sketch::polygonTool, "circumscribed"), none, false,
       QObject::tr("A regular polygon around a circle: the center, then a side's middle")});
  add({"sketch.polygon_inscribed", QObject::tr("Inscribed Polygon"), "polygon-inscribed", "CREATE/Other Geometry",
       draw(sketch::polygonTool, "inscribed"), none, false,
       QObject::tr("A regular polygon in a circle: the center, then a corner")});
  add({"sketch.polygon_edge", QObject::tr("Edge Polygon"), "polygon-edge", "CREATE/Other Geometry",
       draw(sketch::polygonTool, "edge"), none, false, QObject::tr("A regular polygon from one of its sides")});
  add({"sketch.slot", QObject::tr("Center to Center Slot"), "slot", "CREATE/Other Geometry",
       draw(sketch::slotTool, "center_to_center"), none, false, QObject::tr("A slot between the centers of its ends")});
  add({"sketch.slot_overall", QObject::tr("Overall Slot"), "slot", "CREATE/Other Geometry",
       draw(sketch::slotTool, "overall"), none, false, QObject::tr("A slot from end to end")});
  add({"sketch.slot_center", QObject::tr("Center Point Slot"), "slot", "CREATE/Other Geometry",
       draw(sketch::slotTool, "center_point"), none, false,
       QObject::tr("A slot from its center and the center of an end")});
  add({"sketch.slot_arc", QObject::tr("Three Point Arc Slot"), "slot-arc", "CREATE/Other Geometry",
       draw(sketch::slotTool, "three_point_arc"), none, false, QObject::tr("A curved slot along a 3-point arc")});
  add({"sketch.slot_arc_center", QObject::tr("Center Point Arc Slot"), "slot-arc", "CREATE/Other Geometry",
       draw(sketch::slotTool, "center_point_arc"), none, false,
       QObject::tr("A curved slot along an arc around a center")});
  add({"sketch.spline", QObject::tr("Fit Point Spline"), "spline-fit", "CREATE/Other Geometry",
       [&c] { c.startTool(sketch::splineTool(c, false)); }, none, false,
       QObject::tr("A smooth curve through points; Enter or a double-click ends it, a click on its first "
                   "point closes it")});
  add({"sketch.spline_control", QObject::tr("Control Point Spline"), "spline-control", "CREATE/Other Geometry",
       [&c] { c.startTool(sketch::splineTool(c, true)); }, none, false,
       QObject::tr("A smooth curve shaped by control points; Enter or a double-click ends it")});
  add({"sketch.point", QObject::tr("Point"), "point", "CREATE/Other Geometry",
       [&c] { c.startTool(sketch::pointTool(c)); }, none, false, QObject::tr("A sketch point")});
  add({"sketch.text", QObject::tr("Text"), "text", "CREATE/Text", [&c] { c.startTool(sketch::textTool(c)); }, none,
       false, QObject::tr("Text: click where it starts, type it and its height")});
  addRectangularPattern(registry, c);
  addCircularPattern(registry, c);
  addMirror(registry, c);
  addProject(registry, c);
  addIntersect(registry, c);
  addCleanUpProjections(registry, c);
  addCleanUpStaleProjections(registry, c);
  add({"sketch.dimension", QObject::tr("Sketch Dimension"), "dimension", "CREATE",
       [&c] {
         // A viewport click's queued pick must reach the selection before
         // the tool captures it (mitcad#660).
         c.host().deliverPendingPicks();
         // Three or more objects selected: their equal spacing (mitcad#221).
         if (!dimensionEqualSpacing(c)) {
           c.startTool(sketch::dimensionTool(c));
         }
       },
       QKeySequence(Qt::Key_D), true,
       QObject::tr("Dimensions what is picked: a length, distance, angle, radius or diameter")});

  // --- MODIFY
  addFillet(registry, c);
  addChamfer(registry, c);
  add({"sketch.trim", QObject::tr("Trim"), "trim", "MODIFY", [&c] { c.startTool(sketch::trimTool(c)); },
       QKeySequence(Qt::Key_T), true, QObject::tr("Removes the piece of a curve between the curves crossing it")});
  add({"sketch.extend", QObject::tr("Extend"), "extend", "MODIFY", [&c] { c.startTool(sketch::extendTool(c)); },
       none, false, QObject::tr("Extends a line or an arc to the next curve")});
  addOffset(registry, c);
  addMove(registry, c);
  addEditPattern(registry, c);
  addEditOffset(registry, c);
  addEditText(registry, c);
  // QR codes (mitcad#473).
  addQrCode(registry, c);
  addEditCode(registry, c);
  add({"sketch.construction", QObject::tr("Construction"), "construction", "MODIFY",
       [&c] { c.toggleConstruction(); }, QKeySequence(Qt::Key_X), false,
       QObject::tr("Turns the selected curves into construction geometry and back; with nothing selected, "
                   "new geometry is construction geometry")},
      CommandDef::Kind::Action);
  add({"sketch.centerline", QObject::tr("Centerline"), "centerline", "MODIFY", [&c] { c.toggleCenterline(); }, none,
       false, QObject::tr("Turns the selected lines into center lines and back")},
      CommandDef::Kind::Action);
  addConvertProjected(registry, c);
  add({"sketch.delete", QObject::tr("Delete"), "delete", "MODIFY", [&c] { c.deleteSelection(); },
       QKeySequence(Qt::Key_Delete), false,
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
  add({"sketch.cut", QObject::tr("Cut"), "cut", "MODIFY", [&c] { c.cutSelection(); },
       QKeySequence(QKeySequence::Cut), false,
       QObject::tr("Copies the selected sketch geometry and its constraints, then deletes it")},
      CommandDef::Kind::Action);
  add({"sketch.copy", QObject::tr("Copy"), "copy", "MODIFY", [&c] { c.copySelection(); },
       QKeySequence(QKeySequence::Copy), false,
       QObject::tr("Copies the selected sketch geometry with the constraints among it")},
      CommandDef::Kind::Action);
  add({"sketch.paste", QObject::tr("Paste"), "paste", "MODIFY", [&c] { c.paste(); },
       QKeySequence(QKeySequence::Paste), false,
       QObject::tr("Places copied sketch geometry where you click, in this or another sketch")},
      CommandDef::Kind::Action);
  c.setToolIcon(QObject::tr("Paste"), QStringLiteral("paste")); // the Paste tool's cursor

  // --- CONSTRAINTS
  const QStringList pinned = {QStringLiteral("horizontal_vertical"), QStringLiteral("coincident"),
                              QStringLiteral("tangent"),             QStringLiteral("equal"),
                              QStringLiteral("parallel"),            QStringLiteral("perpendicular"),
                              QStringLiteral("fix")};
  for (const QString& id : sketchConstraintCommands()) {
    if (!id.startsWith(QStringLiteral("sketch.constraint."))) {
      continue; // Equal Spacing: registered below
    }
    const QString type = id.mid(static_cast<int>(QStringLiteral("sketch.constraint.").size()));
    CommandDef def = sketchCommand("", sketch::constraintName(type), "", "CONSTRAINTS", CommandDef::Kind::Tool);
    def.id = id;
    def.icon = type == QStringLiteral("horizontal_vertical") ? QStringLiteral("horizontal-vertical")
               : type == QStringLiteral("symmetric")         ? QStringLiteral("symmetry")
               : type == QStringLiteral("smooth")            ? QStringLiteral("curvature")
                                                              : type;
    def.pinned = pinned.contains(type);
    def.keywords = {QStringLiteral("constraint")};
    c.setToolIcon(def.name, def.icon);
    def.run = [&c, type] {
      if (!sketch::constrainSelection(c, type)) {
        c.startTool(sketch::constraintTool(c, type));
      }
    };
    if (type == QStringLiteral("horizontal_vertical")) {
      def.tooltip = QObject::tr("A line horizontal or vertical (as it is nearer to), or two points level");
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
}

} // namespace mitcad
