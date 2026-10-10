// SPDX-License-Identifier: MIT
// Commands of the SKETCH tab (U2): the drawing tools, the sketch dimension,
// the constraints, and the modify commands. Drawing and picking tools are
// `Tool` commands run by sketch mode (app/sketch); Offset, Mirror, the
// patterns, Project, Fillet, Chamfer and Move/Copy are panels of the
// command framework whose definition is a `sketch.*` command
// (CommandDef::sketchCommand).
#include "Commands.hpp"

#include <cmath>

#include <QFontDatabase>
#include <QJsonArray>
#include <QKeySequence>
#include <QRegularExpression>
#include <QSet>

#include "../framework/CommandRegistry.hpp"
#include "../sketch/SketchController.hpp"
#include "../sketch/SketchTool.hpp"

namespace mitcad {
namespace {

using sketch::SketchController;

const QString kSketchTab = QStringLiteral("SKETCH");

CommandDef sketchCommand(const char* id, const QString& name, const char* icon, const char* group,
                         CommandDef::Kind kind) {
  CommandDef def;
  def.id = QString::fromLatin1(id);
  def.name = name;
  def.icon = QString::fromLatin1(icon);
  def.kind = kind;
  def.mode = CommandDef::Mode::Sketch;
  def.tab = kSketchTab;
  def.group = QString::fromLatin1(group);
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

// "5 mm" gives "-5 mm", an expression "-(d1 / 2)".
QString negated(const QString& expression) {
  static const QRegularExpression number(QStringLiteral(R"(^\s*([-+]?)\s*(\d+\.?\d*|\.\d+)(.*)$)"));
  const auto match = number.match(expression);
  if (match.hasMatch() && !match.captured(3).contains(QRegularExpression(QStringLiteral("[-+*/^()]")))) {
    return (match.captured(1) == QStringLiteral("-") ? QString() : QStringLiteral("-")) + match.captured(2) +
           match.captured(3);
  }
  return QStringLiteral("-(%1)").arg(expression);
}

// The curves joined to the picked ones end to end, as a chain
// selection takes them: through points where exactly two such curves meet.
QStringList chainOf(const sketch::SketchModel& model, const QStringList& picked) {
  QStringList chain = picked;
  QSet<QString> in(picked.begin(), picked.end());
  // Lines, arcs, elliptical arcs and open splines chain; a closed curve is
  // offset on its own.
  const auto joinable = [](const sketch::CurveData& c) {
    return c.isLine() || c.isArc() || c.type == QStringLiteral("elliptical_arc") ||
           (c.isSpline() && c.start != c.end);
  };
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
  CommandDef def = sketchCommand("sketch.mirror", QObject::tr("Mirror"), "mirror", "CREATE",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("Mirrored copies about a line, kept symmetric");
  def.inputs = {
      selectionInput(QStringLiteral("entities"), QObject::tr("Objects"),
                     SelectKind::SketchCurve | SelectKind::SketchPoint, 1, 0)
          .withAccepts(ofSketch(c)),
      selectionInput(QStringLiteral("axis"), QObject::tr("Mirror Line"), SelectKind::SketchCurve, 1, 1)
          .withAccepts(linesOf(c)),
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
    cmd.insert(QStringLiteral("axis"), axis);
    return Built::of(cmd);
  };
  def.describe = [](const CommandState& s, const CommandContext&) {
    return QStringLiteral("Mirrored %1 object(s) about %2")
        .arg(s.items(QStringLiteral("entities")).size())
        .arg(s.items(QStringLiteral("axis")).first().name);
  };
  registry.add(def);
}

void addCircularPattern(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.circular_pattern", QObject::tr("Circular Pattern"),
                                 "circular-pattern", "CREATE", CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("Copies around a centre point (the origin when none is picked)");
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

void addRectangularPattern(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.rectangular_pattern", QObject::tr("Rectangular Pattern"),
                                 "rectangular-pattern", "CREATE", CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("Copies in rows and columns along a line (the sketch x axis when none is picked)");
  def.inputs = {
      selectionInput(QStringLiteral("entities"), QObject::tr("Objects"),
                     SelectKind::SketchCurve | SelectKind::SketchPoint, 1, 0)
          .withAccepts(ofSketch(c)),
      selectionInput(QStringLiteral("direction"), QObject::tr("Direction"), SelectKind::SketchCurve, 0, 1)
          .withAccepts(linesOf(c)),
      valueInput(QStringLiteral("quantity1"), QObject::tr("Quantity"), ValueKind::Unitless, QStringLiteral("3")),
      valueInput(QStringLiteral("distance1"), QObject::tr("Spacing"), ValueKind::Length, QStringLiteral("10")),
      valueInput(QStringLiteral("quantity2"), QObject::tr("Quantity 2"), ValueKind::Unitless, QStringLiteral("1")),
      valueInput(QStringLiteral("distance2"), QObject::tr("Spacing 2"), ValueKind::Length, QStringLiteral("10")),
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    const int n1 = static_cast<int>(std::lround(s.value(QStringLiteral("quantity1"))));
    const int n2 = static_cast<int>(std::lround(s.value(QStringLiteral("quantity2"))));
    if (n1 < 1 || n2 < 1 || n1 * n2 < 2) {
      return Built::failure(QObject::tr("The quantities make no copies."), QStringLiteral("quantity1"));
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
    cmd.insert(QStringLiteral("spacing"),
               QJsonArray{s.expression(QStringLiteral("distance1")), s.expression(QStringLiteral("distance2"))});
    return Built::of(cmd);
  };
  def.describe = [](const CommandState& s, const CommandContext&) {
    return QStringLiteral("Rectangular pattern of %1 object(s), %2 x %3")
        .arg(s.items(QStringLiteral("entities")).size())
        .arg(std::lround(s.value(QStringLiteral("quantity1"))))
        .arg(std::lround(s.value(QStringLiteral("quantity2"))));
  };
  registry.add(def);
}

void addProject(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.project", QObject::tr("Project"), "project", "CREATE",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.shortcut = QKeySequence(Qt::Key_P);
  def.tooltip = QObject::tr("Edges, faces (their edges) and vertices of the bodies onto the sketch plane");
  def.keywords = {QStringLiteral("include"), QStringLiteral("reference")};
  def.inputs = {
      selectionInput(QStringLiteral("geometry"), QObject::tr("Geometry"),
                     SelectKind::Edge | SelectKind::Face | SelectKind::Vertex, 1, 0),
      checkInput(QStringLiteral("linked"), QObject::tr("Projection Link"), true)
          .withTooltip(QObject::tr("The projection follows its source when the model changes")),
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    QJsonArray commands;
    for (const SelectionItem& item : s.items(QStringLiteral("geometry"))) {
      QJsonObject cmd = base(c, "sketch.project");
      cmd.insert(QStringLiteral("source"), item.name);
      cmd.insert(QStringLiteral("body"), item.owner);
      cmd.insert(QStringLiteral("linked"), s.checked(QStringLiteral("linked")));
      // Where it was picked: another component's geometry comes through
      // a link (mitcad#100).
      cmd.insert(QStringLiteral("occurrence"), item.occurrence);
      commands.append(cmd);
    }
    return Built::of(QJsonObject{{QStringLiteral("commands"), commands}});
  };
  def.describe = [](const CommandState& s, const CommandContext&) {
    return QStringLiteral("Projected %1 item(s)").arg(s.items(QStringLiteral("geometry")).size());
  };
  registry.add(def);
}

void addFillet(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.fillet", QObject::tr("Sketch Fillet"), "sketch-fillet", "MODIFY",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.pinned = true;
  def.tooltip = QObject::tr("Rounds the corner of two lines with a tangent arc and a radius dimension");
  def.inputs = {
      selectionInput(QStringLiteral("a"), QObject::tr("First Line"), SelectKind::SketchCurve, 1, 1)
          .withAccepts(linesOf(c)),
      selectionInput(QStringLiteral("b"), QObject::tr("Second Line"), SelectKind::SketchCurve, 1, 1)
          .withAccepts(linesOf(c)),
      valueInput(QStringLiteral("radius"), QObject::tr("Radius"), ValueKind::Length, QStringLiteral("2")),
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    QJsonObject cmd = base(c, "sketch.fillet");
    cmd.insert(QStringLiteral("a"), s.items(QStringLiteral("a")).first().name);
    cmd.insert(QStringLiteral("b"), s.items(QStringLiteral("b")).first().name);
    cmd.insert(QStringLiteral("radius"), s.expression(QStringLiteral("radius")));
    return Built::of(cmd);
  };
  def.describe = [](const CommandState& s, const CommandContext&) {
    return QStringLiteral("Sketch fillet of %1 and %2, radius %3")
        .arg(s.items(QStringLiteral("a")).first().name, s.items(QStringLiteral("b")).first().name,
             s.expression(QStringLiteral("radius")));
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
      valueInput(QStringLiteral("dx"), QObject::tr("X Distance"), ValueKind::Length, QStringLiteral("10"))
          .withVisible(translating),
      valueInput(QStringLiteral("dy"), QObject::tr("Y Distance"), ValueKind::Length, QStringLiteral("0"))
          .withVisible(translating),
      selectionInput(QStringLiteral("center"), QObject::tr("Pivot Point"), SelectKind::SketchPoint, 0, 1)
          .withAccepts(ofSketch(c))
          .withVisible(rotating),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("90 deg"))
          .withVisible(rotating),
      checkInput(QStringLiteral("copy"), QObject::tr("Create Copy"), false),
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    QJsonObject cmd = base(c, "sketch.move");
    cmd.insert(QStringLiteral("entities"), names(s.items(QStringLiteral("entities"))));
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
      valueInput(QStringLiteral("quantity2"), QObject::tr("Quantity 2"), ValueKind::Unitless, QStringLiteral("1"))
          .withVisible(rectangular),
      valueInput(QStringLiteral("distance2"), QObject::tr("Spacing 2"), ValueKind::Length, QStringLiteral("10"))
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
      cmd.insert(QStringLiteral("spacing"),
                 QJsonArray{s.expression(QStringLiteral("distance1")), s.expression(QStringLiteral("distance2"))});
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

// The font families: the bundled one first, then the system's.
QVector<QPair<QString, QString>> fontChoices() {
  QVector<QPair<QString, QString>> choices = {{QStringLiteral("Droid Sans"), QStringLiteral("Droid Sans")}};
  for (const QString& family : QFontDatabase::families()) {
    if (family != QStringLiteral("Droid Sans")) {
      choices.append({family, family});
    }
  }
  return choices;
}

void addEditText(CommandRegistry& registry, SketchController& c) {
  CommandDef def = sketchCommand("sketch.edit_text", QObject::tr("Edit Text"), "text", "CREATE",
                                 CommandDef::Kind::Feature);
  def.sketchCommand = true;
  def.tooltip = QObject::tr("A sketch text's content, size, font, style, alignment and path");
  const auto lookup = [](const sketch::SketchModel& m, const QString& id) { return m.text(id); };
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
      valueInput(QStringLiteral("height"), QObject::tr("Height"), ValueKind::Length, QStringLiteral("5")),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withVisible(free),
      choiceInput(QStringLiteral("font"), QObject::tr("Font"), fontChoices(), QStringLiteral("Droid Sans")),
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
    const Selection& picked = s.items(QStringLiteral("text"));
    const sketch::TextData* t = picked.isEmpty() ? nullptr : c.model().text(picked.first().name);
    if (t == nullptr) {
      return;
    }
    s.setText(QStringLiteral("content"), t->text);
    s.setText(QStringLiteral("height"), QString::number(t->height / context.unitMillimetres(), 'g', 12));
    s.setText(QStringLiteral("angle"), QStringLiteral("%1 deg").arg(t->angle * 180.0 / 3.14159265358979323846, 0, 'g', 12));
    s.setChoice(QStringLiteral("font"), t->font.isEmpty() ? QStringLiteral("Droid Sans") : t->font);
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
    if (content.trimmed().isEmpty()) {
      return Built::failure(QObject::tr("The text is empty."), QStringLiteral("content"));
    }
    QJsonObject cmd = base(c, "sketch.edit_text");
    cmd.insert(QStringLiteral("id"), picked.first().name);
    cmd.insert(QStringLiteral("text"), content);
    cmd.insert(QStringLiteral("height"), s.value(QStringLiteral("height")));
    if (!std::isnan(s.value(QStringLiteral("angle")))) {
      cmd.insert(QStringLiteral("angle"), s.value(QStringLiteral("angle")));
    }
    cmd.insert(QStringLiteral("font"), s.choice(QStringLiteral("font")));
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
    return QStringLiteral("Edited text %1: \"%2\", %3, %4")
        .arg(picked.isEmpty() ? QString() : picked.first().name, s.text(QStringLiteral("content")),
             s.choice(QStringLiteral("font")), s.choice(QStringLiteral("align")));
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
          QStringLiteral("sketch.constraint.symmetric"),          QStringLiteral("sketch.constraint.smooth")};
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

  // --- CREATE
  add({"sketch.line", QObject::tr("Line"), "line", "CREATE", [&c] { c.startTool(sketch::lineTool(c)); },
       QKeySequence(Qt::Key_L), true, QObject::tr("Lines end to end; drag from the end for a tangent arc")});
  add({"sketch.rectangle", QObject::tr("2-Point Rectangle"), "rectangle", "CREATE",
       draw(sketch::rectangleTool, "two_point"), QKeySequence(Qt::Key_R), true,
       QObject::tr("A rectangle from two opposite corners")});
  add({"sketch.rectangle_3pt", QObject::tr("3-Point Rectangle"), "rectangle-3pt", "CREATE",
       draw(sketch::rectangleTool, "three_point"), none, false,
       QObject::tr("A rectangle at any angle: a side, then the opposite side")});
  add({"sketch.rectangle_center", QObject::tr("Center Rectangle"), "rectangle-center", "CREATE",
       draw(sketch::rectangleTool, "center"), none, false, QObject::tr("A rectangle from its centre and a corner")});
  add({"sketch.circle", QObject::tr("Center Diameter Circle"), "circle", "CREATE",
       draw(sketch::circleTool, "center"), QKeySequence(Qt::Key_C), true,
       QObject::tr("A circle from its centre and a point on it")});
  add({"sketch.circle_2pt", QObject::tr("2-Point Circle"), "circle-2pt", "CREATE", draw(sketch::circleTool, "two_point"),
       none, false, QObject::tr("A circle from the ends of a diameter")});
  add({"sketch.circle_3pt", QObject::tr("3-Point Circle"), "circle-3pt", "CREATE",
       draw(sketch::circleTool, "three_point"), none, false, QObject::tr("A circle through three points")});
  add({"sketch.circle_2tangent", QObject::tr("2-Tangent Circle"), "circle-tangent", "CREATE",
       draw(sketch::circleTool, "two_tangent"), none, false, QObject::tr("A circle tangent to two lines")});
  add({"sketch.circle_3tangent", QObject::tr("3-Tangent Circle"), "circle-tangent", "CREATE",
       draw(sketch::circleTool, "three_tangent"), none, false, QObject::tr("A circle tangent to three lines")});
  add({"sketch.arc", QObject::tr("3-Point Arc"), "arc-3pt", "CREATE", draw(sketch::arcTool, "three_point"), none,
       true, QObject::tr("An arc from its start and end through a third point")});
  add({"sketch.arc_center", QObject::tr("Center Point Arc"), "arc-center", "CREATE", draw(sketch::arcTool, "center"),
       none, false, QObject::tr("An arc around a centre, from its start to its end")});
  add({"sketch.arc_tangent", QObject::tr("Tangent Arc"), "arc-tangent", "CREATE", draw(sketch::arcTool, "tangent"),
       none, false, QObject::tr("An arc that continues a line or an arc smoothly")});
  add({"sketch.polygon", QObject::tr("Circumscribed Polygon"), "polygon", "CREATE",
       draw(sketch::polygonTool, "circumscribed"), none, true,
       QObject::tr("A regular polygon around a circle: the centre, then a side's middle")});
  add({"sketch.polygon_inscribed", QObject::tr("Inscribed Polygon"), "polygon-inscribed", "CREATE",
       draw(sketch::polygonTool, "inscribed"), none, false,
       QObject::tr("A regular polygon in a circle: the centre, then a corner")});
  add({"sketch.polygon_edge", QObject::tr("Edge Polygon"), "polygon-edge", "CREATE", draw(sketch::polygonTool, "edge"),
       none, false, QObject::tr("A regular polygon from one of its sides")});
  add({"sketch.ellipse", QObject::tr("Ellipse"), "ellipse", "CREATE", [&c] { c.startTool(sketch::ellipseTool(c)); },
       none, false, QObject::tr("An ellipse: the centre, the end of the major axis, a point on it")});
  add({"sketch.slot", QObject::tr("Center to Center Slot"), "slot", "CREATE",
       draw(sketch::slotTool, "center_to_center"), none, false, QObject::tr("A slot between the centres of its ends")});
  add({"sketch.slot_overall", QObject::tr("Overall Slot"), "slot", "CREATE", draw(sketch::slotTool, "overall"), none,
       false, QObject::tr("A slot from end to end")});
  add({"sketch.slot_center", QObject::tr("Center Point Slot"), "slot", "CREATE", draw(sketch::slotTool, "center_point"),
       none, false, QObject::tr("A slot from its centre and the centre of an end")});
  add({"sketch.slot_arc", QObject::tr("Three Point Arc Slot"), "slot-arc", "CREATE",
       draw(sketch::slotTool, "three_point_arc"), none, false, QObject::tr("A curved slot along a 3-point arc")});
  add({"sketch.slot_arc_center", QObject::tr("Center Point Arc Slot"), "slot-arc", "CREATE",
       draw(sketch::slotTool, "center_point_arc"), none, false,
       QObject::tr("A curved slot along an arc around a centre")});
  add({"sketch.spline", QObject::tr("Fit Point Spline"), "spline-fit", "CREATE",
       [&c] { c.startTool(sketch::splineTool(c, false)); }, none, false,
       QObject::tr("A smooth curve through points; Enter or a double-click ends it")});
  add({"sketch.spline_control", QObject::tr("Control Point Spline"), "spline-control", "CREATE",
       [&c] { c.startTool(sketch::splineTool(c, true)); }, none, false,
       QObject::tr("A smooth curve shaped by control points; Enter or a double-click ends it")});
  add({"sketch.point", QObject::tr("Point"), "point", "CREATE", [&c] { c.startTool(sketch::pointTool(c)); }, none,
       false, QObject::tr("A sketch point")});
  add({"sketch.text", QObject::tr("Text"), "text", "CREATE", [&c] { c.startTool(sketch::textTool(c)); }, none, false,
       QObject::tr("Text: click where it starts, type it and its height")});
  addMirror(registry, c);
  addCircularPattern(registry, c);
  addRectangularPattern(registry, c);
  addProject(registry, c);
  add({"sketch.dimension", QObject::tr("Sketch Dimension"), "dimension", "CREATE",
       [&c] { c.startTool(sketch::dimensionTool(c)); }, QKeySequence(Qt::Key_D), true,
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
  add({"sketch.construction", QObject::tr("Construction"), "construction", "MODIFY",
       [&c] { c.toggleConstruction(); }, QKeySequence(Qt::Key_X), false,
       QObject::tr("Turns the selected curves into construction geometry and back; with nothing selected, "
                   "new geometry is construction geometry")},
      CommandDef::Kind::Action);
  add({"sketch.centerline", QObject::tr("Centerline"), "centerline", "MODIFY", [&c] { c.toggleCenterline(); }, none,
       false, QObject::tr("Turns the selected lines into centre lines and back")},
      CommandDef::Kind::Action);
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

  // --- CONSTRAINTS
  const QStringList pinned = {QStringLiteral("horizontal_vertical"), QStringLiteral("coincident"),
                              QStringLiteral("tangent"),             QStringLiteral("equal"),
                              QStringLiteral("parallel"),            QStringLiteral("perpendicular"),
                              QStringLiteral("fix")};
  for (const QString& id : sketchConstraintCommands()) {
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
    } else if (type == QStringLiteral("fix")) {
      def.tooltip = QObject::tr("Fixes geometry in place, or frees it");
    }
    registry.add(def);
  }
}

} // namespace mitcad
