// SPDX-License-Identifier: MIT
// Equal spacing in sketches (mitcad#221): the Equal Spacing constraint's
// panel (from the Constraints, or from Sketch Dimension with three or more
// objects selected, then with its one common spacing dimension), and Edit
// Equal Spacing. Their definition is `sketch.add_equal_spacing` or
// `sketch.edit_equal_spacing` (core/model-api/src/commands.md): the preview
// runs it, so a conflict or a redundancy is shown before OK, and OK keeps it
// as one undo step. The overlay draws the previewed chain numbered in the
// order the model keeps (SketchOverlay::paintChain).
#include "Commands.hpp"

#include <algorithm>
#include <limits>
#include <optional>
#include <vector>

#include <QDebug>
#include <QJsonArray>
#include <QJsonObject>
#include <QObject>
#include <QVector>

#include "../framework/Command.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../sketch/SketchController.hpp"

namespace mitcad {
namespace {

using sketch::SketchController;
using sketch::SketchModel;
using sketch::V2;

const QString kHorizontal = QStringLiteral("horizontal");
const QString kVertical = QStringLiteral("vertical");
const QString kAlong = QStringLiteral("along");
const QString kChain = QStringLiteral("point_to_point");

CommandDef spacingCommand(const char* id, const QString& name) {
  CommandDef def;
  def.id = QString::fromLatin1(id);
  def.name = name;
  def.icon = QStringLiteral("equal-spacing");
  def.kind = CommandDef::Kind::Feature;
  def.mode = CommandDef::Mode::Sketch;
  def.tab = QStringLiteral("SKETCH");
  def.group = QStringLiteral("CONSTRAINTS");
  def.sketchCommand = true;
  def.keywords = {QStringLiteral("constraint"), QStringLiteral("spacing"), QStringLiteral("pitch"),
                  QStringLiteral("distribute")};
  return def;
}

// What an equal spacing measures from: a point, a line (its midpoint), a
// circle or an arc (its centre).
bool isAnchor(const SketchModel& model, const QString& id) {
  if (model.point(id) != nullptr) {
    return true;
  }
  const sketch::CurveData* curve = model.curve(id);
  return curve != nullptr && (curve->isLine() || curve->isCircular());
}

std::function<bool(const SelectionItem&)> anchorsOf(SketchController& c) {
  return [&c](const SelectionItem& item) { return item.owner == c.uid() && isAnchor(c.model(), item.name); };
}

std::function<bool(const SelectionItem&)> directionLines(SketchController& c) {
  return [&c](const SelectionItem& item) {
    const sketch::CurveData* curve = c.model().curve(item.name);
    return item.owner == c.uid() && curve != nullptr && curve->isLine();
  };
}

QStringList namesOf(const Selection& items) {
  QStringList names;
  for (const SelectionItem& item : items) {
    names << item.name;
  }
  return names;
}

// The way a directional mode measures, or none for a chain.
std::optional<V2> directionOf(const SketchModel& model, const QString& mode, const Selection& line) {
  if (mode == kHorizontal) {
    return V2{1.0, 0.0};
  }
  if (mode == kVertical) {
    return V2{0.0, 1.0};
  }
  if (mode == kAlong && !line.isEmpty()) {
    const sketch::CurveData* l = model.curve(line.first().name);
    if (l != nullptr && l->isLine() && sketch::norm(l->curve.b - l->curve.a) > 0.0) {
      return sketch::unit(l->curve.b - l->curve.a);
    }
  }
  return std::nullopt;
}

// The mean interval of the objects as the mode measures it (sorted along a
// direction, link by link for a chain); 0 when it cannot be measured.
double meanInterval(const SketchModel& model, const QStringList& anchors, const QString& mode,
                    const Selection& line) {
  std::vector<V2> at;
  for (const QString& id : anchors) {
    const auto p = model.anchorAt(id);
    if (!p) {
      return 0.0;
    }
    at.push_back(*p);
  }
  if (at.size() < 2) {
    return 0.0;
  }
  const double links = static_cast<double>(at.size() - 1);
  if (mode == kChain) {
    double sum = 0.0;
    for (std::size_t i = 1; i < at.size(); ++i) {
      sum += sketch::dist(at[i - 1], at[i]);
    }
    return sum / links;
  }
  const auto u = directionOf(model, mode, line);
  if (!u) {
    return 0.0;
  }
  double low = std::numeric_limits<double>::infinity();
  double high = -std::numeric_limits<double>::infinity();
  for (const V2& p : at) {
    low = std::min(low, sketch::dot(p, *u));
    high = std::max(high, sketch::dot(p, *u));
  }
  return (high - low) / links;
}

// The common spacing's field shows the mean interval until it is typed
// over: set when the panel starts and when the measurement changes.
void suggestSpacing(SketchController& c, CommandState& s, const CommandContext& context) {
  if (s.object(QStringLiteral("suggested")).value(QStringLiteral("kept")).toBool()) {
    return; // an existing dimension's value
  }
  const double mean = meanInterval(c.model(), namesOf(s.items(QStringLiteral("members"))),
                                   s.choice(QStringLiteral("mode")), s.items(QStringLiteral("direction")));
  if (!(mean > 0.0)) {
    return;
  }
  const QString suggested = s.object(QStringLiteral("suggested")).value(QStringLiteral("spacing")).toString();
  if (s.hasText(QStringLiteral("spacing")) && !suggested.isEmpty() && s.text(QStringLiteral("spacing")) != suggested) {
    return; // typed over
  }
  const QString text = QString::number(mean / context.unitMillimetres(), 'g', 6);
  s.setText(QStringLiteral("spacing"), text);
  // In the document's unit, also while the field is hidden; a shown field is
  // evaluated again (a NaN value is).
  s.setExpression(QStringLiteral("spacing"), QStringLiteral("%1 %2").arg(text, context.lengthUnit()));
  if (s.checked(QStringLiteral("dimension"))) {
    s.setValue(QStringLiteral("spacing"), std::numeric_limits<double>::quiet_NaN());
  }
  s.setObject(QStringLiteral("suggested"), QJsonObject{{QStringLiteral("spacing"), text}});
}

// The measurement the objects' spread suggests: across the wider extent.
QString suggestedMode(const SketchModel& model, const QStringList& anchors) {
  double xs[2] = {std::numeric_limits<double>::infinity(), -std::numeric_limits<double>::infinity()};
  double ys[2] = {xs[0], xs[1]};
  for (const QString& id : anchors) {
    if (const auto p = model.anchorAt(id)) {
      xs[0] = std::min(xs[0], p->x);
      xs[1] = std::max(xs[1], p->x);
      ys[0] = std::min(ys[0], p->y);
      ys[1] = std::max(ys[1], p->y);
    }
  }
  return ys[1] - ys[0] > xs[1] - xs[0] ? kVertical : kHorizontal;
}

QVector<QPair<QString, QString>> modeChoices() {
  return {{kHorizontal, QObject::tr("Horizontal")},
          {kVertical, QObject::tr("Vertical")},
          {kAlong, QObject::tr("Along a Line")},
          {kChain, QObject::tr("Point to Point")}};
}

// The inputs both panels share after their first: the objects (in order),
// the measurement, the direction, the order and the common spacing.
QVector<InputDef> spacingInputs(SketchController& c, int minimum, bool dimension) {
  const auto along = [](const CommandState& s) { return s.choice(QStringLiteral("mode")) == kAlong; };
  const auto directional = [](const CommandState& s) { return s.choice(QStringLiteral("mode")) != kChain; };
  const auto chained = [](const CommandState& s) { return s.choice(QStringLiteral("mode")) == kChain; };
  const auto dimensioned = [](const CommandState& s) { return s.checked(QStringLiteral("dimension")); };
  return {
      selectionInput(QStringLiteral("members"), QObject::tr("Objects"), SelectKind::SketchCurve | SelectKind::SketchPoint,
                     minimum, 0)
          .withAccepts(anchorsOf(c))
          .withOrdered()
          .withOnPick(
              [&c](const SelectionItem& item, CommandState& s, const CommandContext& context) {
                // Clicked one by one: the list's order is the clicks'.
                QJsonArray clicked = s.object(QStringLiteral("order")).value(QStringLiteral("clicked")).toArray();
                if (!clicked.contains(item.name)) {
                  clicked.append(item.name);
                }
                s.setObject(QStringLiteral("order"), QJsonObject{{QStringLiteral("clicked"), clicked}});
                suggestSpacing(c, s, context);
              },
              false)
          .withTooltip(QObject::tr("Points, circles and arcs (their centers) and lines (their midpoints): three or "
                                   "more, in order")),
      choiceInput(QStringLiteral("mode"), QObject::tr("Measure"), modeChoices(), kHorizontal)
          .withOnChange([&c](CommandState& s, const CommandContext& context) { suggestSpacing(c, s, context); })
          .withTooltip(QObject::tr("Equal x or y intervals, intervals along a line, or equal distances "
                                   "from object to object")),
      selectionInput(QStringLiteral("direction"), QObject::tr("Direction"), SelectKind::SketchCurve, 0, 1)
          .withAccepts(directionLines(c))
          .withVisible(along)
          .withTooltip(QObject::tr("The line the intervals are measured along")),
      checkInput(QStringLiteral("keep_order"), QObject::tr("Keep Listed Order"), false)
          .withVisible(directional)
          .withTooltip(QObject::tr("Orders the objects as listed, not along the direction")),
      checkInput(QStringLiteral("order_confirmed"), QObject::tr("Chain in Listed Order"), false)
          .withVisible(chained)
          .withTooltip(QObject::tr("The objects are chained in the list's order; move them in the list to "
                                   "change it")),
      checkInput(QStringLiteral("dimension"), QObject::tr("Common Spacing Dimension"), dimension)
          .withTooltip(QObject::tr("One dimension for every interval; without it their size is free")),
      valueInput(QStringLiteral("spacing"), QObject::tr("Spacing"), ValueKind::Length, QStringLiteral("10"))
          .withVisible(dimensioned),
  };
}

// The checks and fields of the objects, mode and direction both panels
// give the model; a failure when they are incomplete.
std::optional<Built> spacingFields(const CommandState& s, QJsonObject& cmd) {
  const QStringList members = namesOf(s.items(QStringLiteral("members")));
  if (members.size() < 3) {
    return Built::failure(QObject::tr("Pick at least three points, circles, arcs or lines."),
                          QStringLiteral("members"));
  }
  const QString mode = s.choice(QStringLiteral("mode"));
  cmd.insert(QStringLiteral("anchors"), QJsonArray::fromStringList(members));
  cmd.insert(QStringLiteral("mode"), mode);
  if (mode == kAlong) {
    const Selection& line = s.items(QStringLiteral("direction"));
    if (line.isEmpty()) {
      return Built::failure(QObject::tr("Pick the line to measure along."), QStringLiteral("direction"));
    }
    cmd.insert(QStringLiteral("direction"), line.first().name);
  }
  // A chain's order is explicit when its objects were clicked one by one in
  // the panel; a selection made before the command or a window has no
  // order of its own, so it is confirmed.
  const QJsonArray clicked = s.object(QStringLiteral("order")).value(QStringLiteral("clicked")).toArray();
  const bool clickedInOrder = std::all_of(members.begin(), members.end(),
                                          [&clicked](const QString& id) { return clicked.contains(id); });
  if (mode == kChain && !clickedInOrder && !s.checked(QStringLiteral("order_confirmed"))) {
    return Built::failure(QObject::tr("The objects are chained in the listed order: move them in the list if "
                                      "needed, then check Chain in Listed Order."),
                          QStringLiteral("order_confirmed"));
  }
  cmd.insert(QStringLiteral("keep_order"), mode == kChain || s.checked(QStringLiteral("keep_order")));
  return std::nullopt;
}

QJsonObject sketchCommand(SketchController& c, const char* name) {
  return {{QStringLiteral("cmd"), QString::fromLatin1(name)}, {QStringLiteral("sketch"), c.uid()}};
}

QString describeSpacing(const CommandState& s, const QString& what) {
  return QStringLiteral("%1 of %2 object(s), %3%4")
      .arg(what)
      .arg(s.items(QStringLiteral("members")).size())
      .arg(s.choice(QStringLiteral("mode")),
           s.checked(QStringLiteral("dimension"))
               ? QStringLiteral(", spacing %1").arg(s.expression(QStringLiteral("spacing")))
               : QString());
}

void addEqualSpacing(CommandRegistry& registry, SketchController& c, bool dimension) {
  CommandDef def = spacingCommand(dimension ? "sketch.equal_spacing_dimension" : "sketch.equal_spacing",
                                  dimension ? QObject::tr("Equal Spacing Dimension") : QObject::tr("Equal Spacing"));
  def.tooltip = dimension ? QObject::tr("One dimension for the equal intervals between three or more objects")
                          : QObject::tr("Equal intervals between three or more points, circles, arcs or lines");
  def.inputs = spacingInputs(c, 3, dimension);
  def.init = [&c](CommandState& s, const CommandContext& context) {
    const QStringList members = namesOf(s.items(QStringLiteral("members")));
    if (!members.isEmpty()) {
      s.setChoice(QStringLiteral("mode"), suggestedMode(c.model(), members));
    }
    suggestSpacing(c, s, context);
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    c.setSpacingPreview(namesOf(s.items(QStringLiteral("members"))));
    QJsonObject cmd = sketchCommand(c, "sketch.add_equal_spacing");
    if (const auto failed = spacingFields(s, cmd)) {
      return *failed;
    }
    if (s.checked(QStringLiteral("dimension"))) {
      cmd.insert(QStringLiteral("spacing"), s.expression(QStringLiteral("spacing")));
    }
    return Built::of(cmd);
  };
  def.describe = [dimension](const CommandState& s, const CommandContext&) {
    return describeSpacing(s, dimension ? QStringLiteral("Equal spacing dimension") : QStringLiteral("Equal spacing"));
  };
  registry.add(def);
}

// The equal spacing an item stands for: its symbol, or one of its objects.
const sketch::ConstraintData* spacingOf(const SketchModel& model, const QString& id) {
  const sketch::ConstraintData* k = model.constraint(id);
  if (k != nullptr) {
    return k->type == QStringLiteral("equal_spacing") ? k : nullptr;
  }
  return model.equalSpacingOf(id);
}

// Fills the edit panel from the equal spacing as it is; what it had is kept
// for the build (a preview changes the model under the panel).
void loadSpacing(SketchController& c, CommandState& s, const QString& picked, const CommandContext& context) {
  const sketch::ConstraintData* k = spacingOf(c.model(), picked);
  if (k == nullptr) {
    return;
  }
  Selection members;
  for (const QString& id : k->refs) {
    const sketch::CurveData* curve = c.model().curve(id);
    SelectionItem item;
    item.kind = curve != nullptr ? SelectKind::SketchCurve : SelectKind::SketchPoint;
    item.owner = c.uid();
    item.name = id;
    item.geometry = curve != nullptr ? curve->type : QString();
    members.append(item);
  }
  s.setItems(QStringLiteral("members"), members);
  s.setChoice(QStringLiteral("mode"), k->json.value(QStringLiteral("mode")).toString(kHorizontal));
  Selection line;
  const QString direction = k->json.value(QStringLiteral("direction")).toString();
  if (!direction.isEmpty()) {
    SelectionItem item;
    item.kind = SelectKind::SketchCurve;
    item.owner = c.uid();
    item.name = direction;
    item.geometry = QStringLiteral("line");
    line.append(item);
  }
  s.setItems(QStringLiteral("direction"), line);
  // The order kept is the stored one, until the user lets it be sorted.
  s.setChecked(QStringLiteral("keep_order"), true);
  s.setChecked(QStringLiteral("order_confirmed"), true);
  const sketch::DimensionData* d = c.model().spacingDimension(k->id);
  s.setChecked(QStringLiteral("dimension"), d != nullptr);
  if (d != nullptr) {
    const QString text = d->expression.isEmpty() ? QStringLiteral("%1 mm").arg(d->measured, 0, 'g', 10)
                                                 : d->expression;
    s.setText(QStringLiteral("spacing"), text);
    s.setExpression(QStringLiteral("spacing"), text);
    s.setValue(QStringLiteral("spacing"), std::numeric_limits<double>::quiet_NaN()); // evaluated again
    s.setObject(QStringLiteral("suggested"), QJsonObject{{QStringLiteral("kept"), true}});
  } else {
    s.setObject(QStringLiteral("suggested"), QJsonObject());
    suggestSpacing(c, s, context); // the mean interval, should a dimension be added
  }
  s.setObject(QStringLiteral("original"), QJsonObject{{QStringLiteral("constraint"), k->id},
                                                      {QStringLiteral("dimension"), d != nullptr ? d->id : QString()},
                                                      {QStringLiteral("driven"), d != nullptr && d->driven}});
}

void addEditEqualSpacing(CommandRegistry& registry, SketchController& c) {
  CommandDef def = spacingCommand("sketch.edit_equal_spacing", QObject::tr("Edit Equal Spacing"));
  def.tooltip = QObject::tr("Changes an equal spacing's objects, order, measurement and spacing");
  def.inputs = {
      selectionInput(QStringLiteral("constraint"), QObject::tr("Equal Spacing"),
                     SelectKind::SketchConstraint | SelectKind::SketchCurve | SelectKind::SketchPoint, 1, 1)
          .withAccepts([&c](const SelectionItem& item) {
            return item.owner == c.uid() && spacingOf(c.model(), item.name) != nullptr;
          })
          .withOnPick([&c](const SelectionItem& item, CommandState& s, const CommandContext& context) {
            loadSpacing(c, s, item.name, context);
          }, false),
  };
  def.inputs.append(spacingInputs(c, 0, false));
  def.init = [&c](CommandState& s, const CommandContext& context) {
    const Selection& picked = s.items(QStringLiteral("constraint"));
    if (!picked.isEmpty()) {
      loadSpacing(c, s, picked.first().name, context);
    }
  };
  def.build = [&c](const CommandState& s, const CommandContext&) -> Built {
    const QJsonObject original = s.object(QStringLiteral("original"));
    const QString id = original.value(QStringLiteral("constraint")).toString();
    c.setSpacingPreview(namesOf(s.items(QStringLiteral("members"))));
    if (id.isEmpty()) {
      return Built::failure(QObject::tr("Pick an equal spacing's symbol or one of its objects."),
                            QStringLiteral("constraint"));
    }
    QJsonObject edit = sketchCommand(c, "sketch.edit_equal_spacing");
    edit.insert(QStringLiteral("constraint"), id);
    if (const auto failed = spacingFields(s, edit)) {
      return *failed;
    }
    QJsonArray commands{edit};
    // The common spacing: added, changed or removed with the edit.
    const QString dimension = original.value(QStringLiteral("dimension")).toString();
    const QString value = s.expression(QStringLiteral("spacing"));
    if (s.checked(QStringLiteral("dimension"))) {
      if (dimension.isEmpty()) {
        QJsonObject add = sketchCommand(c, "sketch.add_dimension");
        add.insert(QStringLiteral("dimension"), QJsonObject{{QStringLiteral("type"), QStringLiteral("spacing")},
                                                            {QStringLiteral("constraint"), id}});
        add.insert(QStringLiteral("value"), value);
        commands.append(add);
      } else {
        QJsonObject set = sketchCommand(c, "sketch.set_dimension");
        set.insert(QStringLiteral("dimension"), dimension);
        set.insert(QStringLiteral("value"), value);
        commands.append(set);
      }
    } else if (!dimension.isEmpty()) {
      QJsonObject remove = sketchCommand(c, "sketch.remove");
      remove.insert(QStringLiteral("items"), QJsonArray{dimension});
      commands.append(remove);
    }
    return Built::of(QJsonObject{{QStringLiteral("commands"), commands}});
  };
  def.describe = [](const CommandState& s, const CommandContext&) {
    return describeSpacing(s, QStringLiteral("Edited equal spacing %1")
                                  .arg(s.object(QStringLiteral("original")).value(QStringLiteral("constraint")).toString()));
  };
  registry.add(def);
}

} // namespace

void registerEqualSpacingCommands(CommandRegistry& registry, sketch::SketchController& controller) {
  addEqualSpacing(registry, controller, false);
  addEqualSpacing(registry, controller, true);
  addEditEqualSpacing(registry, controller);
}

bool dimensionEqualSpacing(sketch::SketchController& controller) {
  int anchors = 0;
  for (const SelectionItem& item : controller.host().selection()) {
    if (item.owner != controller.uid() ||
        (item.kind != SelectKind::SketchCurve && item.kind != SelectKind::SketchPoint)) {
      continue;
    }
    if (!isAnchor(controller.model(), item.name)) {
      return false; // something else to dimension
    }
    ++anchors;
  }
  if (anchors < 3) {
    return false;
  }
  qDebug().noquote() << QStringLiteral("Sketch Dimension on %1 objects: Equal Spacing Dimension").arg(anchors);
  controller.host().trigger(QStringLiteral("sketch.equal_spacing_dimension"));
  return true;
}

} // namespace mitcad
