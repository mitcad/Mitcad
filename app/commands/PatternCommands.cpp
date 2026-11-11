// SPDX-License-Identifier: MIT
// Rectangular, Circular and Path Pattern and Mirror (U4; commands.md,
// "Patterns", "mirror"): of bodies, of features (picked in the timeline or
// through a face they made) or of faces of one body. A pattern's instances
// can be suppressed one by one (P9).
#include <cmath>

#include <QJsonArray>

#include <gp_Trsf.hxx>

#include "CommandFactories.hpp"
#include "CommandSupport.hpp"
#include "PatternOptions.hpp"

namespace mitcad::cmd {
namespace {

std::function<bool(const CommandState&)> choiceIs(const QString& id, const QString& value) {
  return [id, value](const CommandState& state) { return state.choice(id) == value; };
}

const Choices kComputes = {{QStringLiteral("adjust"), QStringLiteral("Adjust")},
                           {QStringLiteral("identical"), QStringLiteral("Identical")},
                           {QStringLiteral("optimized"), QStringLiteral("Optimized")}};
const Choices kSpacings = {{QStringLiteral("spacing"), QStringLiteral("Spacing")},
                           {QStringLiteral("extent"), QStringLiteral("Extent")}};
const Choices kDirectionTypes = {{oneDirection(), QStringLiteral("One Direction")},
                                 {symmetricDirection(), QStringLiteral("Symmetric")}};

// The Direction Type and Distance Type rows (mitcad#545; commands.md,
// "Patterns": `symmetric`, `distance_type`). The defaults are the
// definition's: one direction, spacing.
InputDef directionTypeInput(const QString& id, const QString& name) {
  return choiceInput(id, name, kDirectionTypes, oneDirection())
      .withTooltip(QObject::tr("One Direction: copies along the direction; Symmetric: on both sides of the "
                               "original"));
}

InputDef distanceTypeInput() {
  return choiceInput(QStringLiteral("distance_type"), QObject::tr("Distance Type"), kSpacings,
                     QStringLiteral("spacing"))
      .withTooltip(QObject::tr("Spacing: from one copy to the next; Extent: from the first copy to the last"));
}

// A length row whose label follows the Distance Type: "Spacing" or
// "Extent" (with the direction's suffix).
InputDef lengthInput(const QString& id, const QString& suffix) {
  return valueInput(id, lengthLabel(QStringLiteral("spacing"), suffix), ValueKind::Length, QStringLiteral("20 mm"))
      .withLabel([suffix](const CommandState& state) {
        return lengthLabel(state.choice(QStringLiteral("distance_type")), suffix);
      });
}

// The objects of a pattern or mirror: the Pattern Type and its pick.
QVector<InputDef> patternObjects() {
  const auto notBodies = [](const CommandState& s) {
    return s.choice(QStringLiteral("object_type")) != QStringLiteral("bodies");
  };
  return {
      choiceInput(QStringLiteral("object_type"), QObject::tr("Pattern Type"),
                  {{QStringLiteral("bodies"), QStringLiteral("Bodies")},
                   {QStringLiteral("features"), QStringLiteral("Features")},
                   {QStringLiteral("faces"), QStringLiteral("Faces")}},
                  QStringLiteral("bodies")),
      selectionInput(QStringLiteral("bodies"), QObject::tr("Objects"), SelectKind::Body, 1, 0)
          .withVisible(choiceIs(QStringLiteral("object_type"), QStringLiteral("bodies"))),
      selectionInput(QStringLiteral("features"), QObject::tr("Objects"), SelectKind::Feature | SelectKind::Face, 1, 0)
          .withTooltip(QObject::tr("Features in the timeline, or a face a feature made"))
          .withAccepts([](const SelectionItem& item) { return !item.creatingFeature().isEmpty(); })
          .withConvert([](const SelectionItem& item) {
            return SelectionItem{SelectKind::Feature, item.creatingFeature(), QString(), QString()};
          })
          .withVisible(choiceIs(QStringLiteral("object_type"), QStringLiteral("features"))),
      selectionInput(QStringLiteral("faces"), QObject::tr("Objects"), SelectKind::Face, 1, 0)
          .withoutMeshes()
          .withVisible(choiceIs(QStringLiteral("object_type"), QStringLiteral("faces"))),
      choiceInput(QStringLiteral("compute"), QObject::tr("Compute Option"), kComputes, QStringLiteral("adjust"))
          .withTooltip(QObject::tr("Adjust computes each copy where it is; Identical moves the "
                                   "original's result"))
          .withVisible(notBodies),
  };
}

// The Pattern Type from the selection made before the command
// (mitcad#1290), shared by the four commands of `patternObjects()`: a
// preselected timeline feature, or a face a feature made, went to the
// hidden Features input, so the panel starts with Pattern Type Features
// and the feature in Objects; a body keeps Bodies.
void choosePatternTypeOnPreselection(CommandDef& def) {
  def.preselected = [](const Selection&, CommandState& state, const CommandContext&) {
    choosePreselectedPatternType(state);
  };
}

// The selection input in use, for the objects' middle.
QString objectsKey(const CommandState& state) {
  return state.choice(QStringLiteral("object_type"));
}

QJsonObject objectsOf(const CommandState& state, Built& error) {
  const QString type = state.choice(QStringLiteral("object_type"));
  const Selection& items = state.items(type);
  if (type == QStringLiteral("bodies")) {
    return {{QStringLiteral("type"), type}, {QStringLiteral("bodies"), bodyUids(items)}};
  }
  if (type == QStringLiteral("features")) {
    QJsonArray features;
    for (const SelectionItem& item : items) {
      features.append(item.owner);
    }
    return {{QStringLiteral("type"), type}, {QStringLiteral("features"), features}};
  }
  const QString body = oneBody(items, error, QStringLiteral("faces"));
  return {{QStringLiteral("type"), type},
          {QStringLiteral("body"), body},
          {QStringLiteral("faces"), names(items, SelectKind::Face)}};
}

void addCompute(QJsonObject& def, const CommandState& state) {
  const QString compute = state.choice(QStringLiteral("compute"));
  if (state.choice(QStringLiteral("object_type")) != QStringLiteral("bodies") &&
      compute != QStringLiteral("adjust")) {
    def.insert(QStringLiteral("compute"), compute);
  }
}

void loadObjects(const QJsonObject& feature, CommandState& state, const CommandContext& context) {
  const QJsonObject objects = feature.value(QStringLiteral("objects")).toObject();
  const QString type = str(objects, "type");
  state.setChoice(QStringLiteral("object_type"), type);
  if (type == QStringLiteral("bodies")) {
    state.setItems(type, bodiesOf(objects.value(QStringLiteral("bodies")).toArray()));
  } else if (type == QStringLiteral("features")) {
    Selection items;
    for (const QJsonValue& uid : objects.value(QStringLiteral("features")).toArray()) {
      items.append({SelectKind::Feature, uid.toString(), QString(), QString()});
    }
    state.setItems(type, items);
  } else {
    Selection items;
    for (const QJsonValue& face : objects.value(QStringLiteral("faces")).toArray()) {
      items.append(itemOf(QJsonObject{{QStringLiteral("body"), objects.value(QStringLiteral("body"))},
                                      {QStringLiteral("face"), face}},
                          context));
    }
    state.setItems(type, items);
  }
  state.setChoice(QStringLiteral("compute"), feature.value(QStringLiteral("compute")).toString(QStringLiteral("adjust")));
}

// The suppressed instances (P9): element numbers in a text ("2, 5"), and a
// dot at each instance in the view that a click turns off and on.
// Element 0 is the original.
QList<int> suppressedOf(const CommandState& state) {
  QList<int> numbers;
  for (const QString& part : state.text(QStringLiteral("suppressed")).split(QLatin1Char(','), Qt::SkipEmptyParts)) {
    bool ok = false;
    const int number = part.trimmed().toInt(&ok);
    if (ok && !numbers.contains(number)) {
      numbers.append(number);
    }
  }
  return numbers;
}

InputDef suppressedInput() {
  return textInput(QStringLiteral("suppressed"), QObject::tr("Suppressed"), QString())
      .withTooltip(QObject::tr("Copies left out, by number (2, 5): click the dots in the view"))
      .withToggles([](const CommandState& state, const CommandContext& model, const QJsonObject& preview) {
        std::vector<Manipulator> toggles;
        const auto center = centerOf(state.items(objectsKey(state)), model);
        const QJsonArray elements = preview.value(QStringLiteral("elements")).toArray();
        if (!center) {
          return toggles;
        }
        const QList<int> off = suppressedOf(state);
        for (int i = 1; i < elements.size(); ++i) {
          const QJsonArray rows = elements[i].toArray();
          const auto at = [&rows](int r, int c) { return rows.at(r).toArray().at(c).toDouble(); };
          gp_Trsf place;
          place.SetValues(at(0, 0), at(0, 1), at(0, 2), at(0, 3), at(1, 0), at(1, 1), at(1, 2), at(1, 3), at(2, 0),
                          at(2, 1), at(2, 2), at(2, 3));
          Manipulator toggle;
          toggle.kind = Manipulator::Kind::Toggle;
          toggle.origin = center->Transformed(place);
          toggle.element = i;
          toggle.on = !off.contains(i);
          toggles.push_back(toggle);
        }
        return toggles;
      });
}

void addSuppressed(QJsonObject& def, const CommandState& state) {
  QJsonArray numbers;
  for (const int number : suppressedOf(state)) {
    numbers.append(number);
  }
  if (!numbers.isEmpty()) {
    def.insert(QStringLiteral("suppressed_elements"), numbers);
  }
}

void loadSuppressed(const QJsonObject& feature, CommandState& state) {
  QStringList numbers;
  for (const QJsonValue& number : feature.value(QStringLiteral("suppressed_elements")).toArray()) {
    numbers << QString::number(number.toInt());
  }
  state.setText(QStringLiteral("suppressed"), numbers.join(QStringLiteral(", ")));
}

// An arrow from the objects' middle along an axis input, against it while
// the check input `flip` is on and the direction is not symmetric (choice
// input `type`). The value is signed: a handle dragged past the middle
// gives a negative distance.
std::function<std::optional<Manipulator>(const CommandState&, const CommandContext&)>
alongAxis(const QString& axis, const QString& flip, const QString& type) {
  return [axis, flip, type](const CommandState& state, const CommandContext& model) -> std::optional<Manipulator> {
    const Selection& axisItems = state.items(axis);
    const auto center = centerOf(state.items(objectsKey(state)), model);
    if (axisItems.isEmpty() || !center) {
      return std::nullopt;
    }
    const auto line = axisOf(axisItems.first(), model);
    if (!line) {
      return std::nullopt;
    }
    return arrow(*center,
                 flipApplies(state.choice(type), state.checked(flip)) ? line->second.Reversed() : line->second);
  };
}

// A ring about an axis input's line through the foot of the objects'
// middle, its reference toward the middle: a circular pattern's Total
// Angle about that axis.
std::function<std::optional<Manipulator>(const CommandState&, const CommandContext&)> ringAbout(const QString& id) {
  return [id](const CommandState& state, const CommandContext& model) -> std::optional<Manipulator> {
    const Selection& axis = state.items(id);
    const auto center = centerOf(state.items(objectsKey(state)), model);
    if (axis.isEmpty() || !center) {
      return std::nullopt;
    }
    const auto line = axisOf(axis.first(), model);
    if (!line) {
      return std::nullopt;
    }
    const gp_Vec along(line->second);
    const gp_Vec out(line->first, *center);
    const gp_Pnt foot = line->first.Translated(along * out.Dot(along));
    Manipulator m = ring(foot, line->second);
    if (foot.Distance(*center) > 1e-9) {
      m.reference = gp_Dir(gp_Vec(foot, *center));
    }
    return m;
  };
}

InputDef lineInput(const QString& id, const QString& name, int min) {
  return selectionInput(id, name, SelectKind::Axis | SelectKind::Edge | SelectKind::SketchCurve, min, 1)
      .withTooltip(QObject::tr("An axis, a straight edge or a sketch line"))
      .withAccepts([](const SelectionItem& item) { return item.kind == SelectKind::Axis || isLine(item); });
}

// "Added rectangular pattern of 1 bodies: <its options>" (mitcad#545: the
// definition's Direction Types and Distance Type, for the log).
QString describePattern(const char* kind, const CommandState& state, const Built& built) {
  const QString options = built.error.isEmpty() ? patternOptionsText(built.def) : QString();
  return QStringLiteral("Added %1 pattern of %2 %3%4")
      .arg(QString::fromLatin1(kind))
      .arg(state.items(objectsKey(state)).size())
      .arg(state.choice(QStringLiteral("object_type")),
           options.isEmpty() ? QString() : QStringLiteral(": ") + options);
}

} // namespace

// ---------------------------------------------------------------------------
// Rectangular Pattern

CommandDef rectangularPatternCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.rectangular_pattern");
  def.name = QObject::tr("Rectangular Pattern");
  def.icon = QStringLiteral("rectangular-pattern");
  def.tooltip = QObject::tr("Copies in rows, columns and layers along up to three directions");
  def.group = QStringLiteral("CREATE");
  def.section = QStringLiteral("patterns");
  def.subgroup = QStringLiteral("Pattern");
  def.keywords = {QStringLiteral("array"), QStringLiteral("linear pattern"), QStringLiteral("copy")};
  def.featureType = QStringLiteral("rectangular_pattern");
  const auto second = [](const CommandState& s) { return !s.items(QStringLiteral("direction2")).isEmpty(); };
  // Direction 3 (mitcad#1446): a 3D grid; its rows as Direction 2's.
  const auto third = [](const CommandState& s) { return !s.items(QStringLiteral("direction3")).isEmpty(); };
  // Flip only for a direction of one way (mitcad#545): a symmetric one has
  // copies on both sides.
  const auto flipShown = [](const char* suffix) {
    const QString type = QStringLiteral("direction_type") + QString::fromLatin1(suffix);
    return [type](const CommandState& s) { return s.choice(type) != symmetricDirection(); };
  };
  def.inputs = patternObjects();
  choosePatternTypeOnPreselection(def);
  // The hover text names the Pattern Type first (mitcad#1235).
  def.keyInputs = {QStringLiteral("object_type")};
  def.inputs += {
      distanceTypeInput(),
      lineInput(QStringLiteral("direction1"), QObject::tr("Direction 1"), 1),
      directionTypeInput(QStringLiteral("direction_type1"), QObject::tr("Direction Type")),
      valueInput(QStringLiteral("quantity1"), QObject::tr("Quantity"), ValueKind::Unitless, QStringLiteral("3")).withStepMinimum(1),
      lengthInput(QStringLiteral("distance1"), QString())
          .withManipulator(alongAxis(QStringLiteral("direction1"), QStringLiteral("flip1"),
                                     QStringLiteral("direction_type1"))),
      flipInput(QStringLiteral("flip1"), QObject::tr("Flip"))
          .withTooltip(QObject::tr("Copies against the direction"))
          .withVisible(flipShown("1")),
      lineInput(QStringLiteral("direction2"), QObject::tr("Direction 2"), 0)
          .withTooltip(QObject::tr("A second direction; none: one row")),
      directionTypeInput(QStringLiteral("direction_type2"), QObject::tr("Direction Type 2")).withVisible(second),
      valueInput(QStringLiteral("quantity2"), QObject::tr("Quantity 2"), ValueKind::Unitless, QStringLiteral("2")).withStepMinimum(1)
          .withVisible(second),
      lengthInput(QStringLiteral("distance2"), QStringLiteral("2"))
          .withVisible(second)
          .withManipulator(alongAxis(QStringLiteral("direction2"), QStringLiteral("flip2"),
                                     QStringLiteral("direction_type2"))),
      flipInput(QStringLiteral("flip2"), QObject::tr("Flip 2"))
          .withTooltip(QObject::tr("Copies against the second direction"))
          .withVisible([second, shown = flipShown("2")](const CommandState& s) { return second(s) && shown(s); }),
      lineInput(QStringLiteral("direction3"), QObject::tr("Direction 3"), 0)
          .withTooltip(QObject::tr("A third direction; none: one layer")),
      directionTypeInput(QStringLiteral("direction_type3"), QObject::tr("Direction Type 3")).withVisible(third),
      valueInput(QStringLiteral("quantity3"), QObject::tr("Quantity 3"), ValueKind::Unitless, QStringLiteral("2")).withStepMinimum(1)
          .withVisible(third),
      lengthInput(QStringLiteral("distance3"), QStringLiteral("3"))
          .withVisible(third)
          .withManipulator(alongAxis(QStringLiteral("direction3"), QStringLiteral("flip3"),
                                     QStringLiteral("direction_type3"))),
      flipInput(QStringLiteral("flip3"), QObject::tr("Flip 3"))
          .withTooltip(QObject::tr("Copies against the third direction"))
          .withVisible([third, shown = flipShown("3")](const CommandState& s) { return third(s) && shown(s); }),
      suppressedInput(),
  };
  def.enabled = [&context] { return hasBodies(context); };

  // The model keeps one signed distance per direction (mitcad#447): Flip
  // negates the distance that is built, and a loaded pattern shows its
  // stored signed distance with Flip off. A symmetric direction hides Flip
  // and builds the distance as typed (mitcad#545): its copies lie on both
  // sides, and only an even quantity's extra copy follows the sign.
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QJsonObject objects = objectsOf(state, result);
    if (!result.error.isEmpty()) {
      return result;
    }
    const auto direction = [&state](const char* suffix) {
      const QString n = QString::fromLatin1(suffix);
      const QString distance = state.expression(QStringLiteral("distance") + n);
      const QString type = state.choice(QStringLiteral("direction_type") + n);
      QJsonObject d{{QStringLiteral("axis"), state.items(QStringLiteral("direction") + n).first().reference()},
                    {QStringLiteral("quantity"), state.expression(QStringLiteral("quantity") + n)},
                    {QStringLiteral("distance"),
                     flipApplies(type, state.checked(QStringLiteral("flip") + n)) ? negated(distance) : distance}};
      setDirectionType(d, type);
      return d;
    };
    result.def = {{QStringLiteral("type"), QStringLiteral("rectangular_pattern")},
                  {QStringLiteral("objects"), objects},
                  {QStringLiteral("direction1"), direction("1")},
                  {QStringLiteral("distance_type"), state.choice(QStringLiteral("distance_type"))}};
    if (!state.items(QStringLiteral("direction2")).isEmpty()) {
      result.def.insert(QStringLiteral("direction2"), direction("2"));
    }
    if (!state.items(QStringLiteral("direction3")).isEmpty()) {
      result.def.insert(QStringLiteral("direction3"), direction("3"));
    }
    addCompute(result.def, state);
    addSuppressed(result.def, state);
    return result;
  };

  // An imported or saved pattern shows its stored Direction Types and
  // Distance Type (mitcad#545).
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadObjects(feature, state, context);
    state.setChoice(QStringLiteral("distance_type"),
                    feature.value(QStringLiteral("distance_type")).toString(QStringLiteral("spacing")));
    for (const char* n : {"1", "2", "3"}) {
      const QString suffix = QString::fromLatin1(n);
      const QJsonObject d = feature.value(QStringLiteral("direction") + suffix).toObject();
      if (d.isEmpty()) {
        continue;
      }
      state.setItems(QStringLiteral("direction") + suffix, {itemOf(d.value(QStringLiteral("axis")), context)});
      state.setChoice(QStringLiteral("direction_type") + suffix, directionTypeOf(d));
      loadCount(state, QStringLiteral("quantity") + suffix, d.value(QStringLiteral("quantity")), feature, context);
      loadValue(state, QStringLiteral("distance") + suffix, d.value(QStringLiteral("distance")), feature, context);
      state.setChecked(QStringLiteral("flip") + suffix, false);
    }
    loadSuppressed(feature, state);
  };

  def.canEdit = [&context](const QJsonObject& feature) {
    for (const char* key : {"direction1", "direction2", "direction3"}) {
      const QJsonObject d = feature.value(QLatin1String(key)).toObject();
      if (!d.isEmpty() && !isShowable(d.value(QStringLiteral("axis")), context)) {
        return false;
      }
    }
    return true;
  };

  def.describe = [build = def.build](const CommandState& state, const CommandContext& model) {
    return describePattern("rectangular", state, build(state, model));
  };
  return def;
}

// ---------------------------------------------------------------------------
// Circular Pattern

CommandDef circularPatternCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.circular_pattern");
  def.name = QObject::tr("Circular Pattern");
  def.icon = QStringLiteral("circular-pattern");
  def.tooltip = QObject::tr("Copies about an axis");
  def.group = QStringLiteral("CREATE");
  def.section = QStringLiteral("patterns");
  def.subgroup = QStringLiteral("Pattern");
  def.keywords = {QStringLiteral("array"), QStringLiteral("polar pattern"), QStringLiteral("radial")};
  def.featureType = QStringLiteral("circular_pattern");
  // Axis 2 (mitcad#1588): the copies turned again about a second axis; its
  // rows stay hidden until it is picked, as Rectangular Pattern's
  // Direction 2 rows do.
  const auto second = [](const CommandState& s) { return !s.items(QStringLiteral("axis2")).isEmpty(); };
  def.inputs = patternObjects();
  choosePatternTypeOnPreselection(def);
  // The hover text names the Pattern Type first (mitcad#1235).
  def.keyInputs = {QStringLiteral("object_type")};
  def.inputs += {
      selectionInput(QStringLiteral("axis"), QObject::tr("Axis"), kAxisKinds, 1, 1)
          .withTooltip(QObject::tr("An axis, a straight or circular edge, a sketch line or a "
                                   "cylindrical face"))
          .withAccepts(isAxial),
      valueInput(QStringLiteral("quantity"), QObject::tr("Quantity"), ValueKind::Unitless, QStringLiteral("4")).withStepMinimum(1),
      valueInput(QStringLiteral("angle"), QObject::tr("Total Angle"), ValueKind::Angle, QStringLiteral("360 deg"))
          .withManipulator(ringAbout(QStringLiteral("axis"))),
      // The angle centred on the originals (mitcad#545); a full turn is the
      // same either way.
      checkInput(QStringLiteral("symmetric"), QObject::tr("Symmetric"), false)
          .withTooltip(QObject::tr("Copies on both sides of the original, the angle centered on it")),
      selectionInput(QStringLiteral("axis2"), QObject::tr("Axis 2"), kAxisKinds, 0, 1)
          .withTooltip(QObject::tr("A second axis the copies turn about again; none: one axis"))
          .withAccepts(isAxial),
      valueInput(QStringLiteral("quantity2"), QObject::tr("Quantity 2"), ValueKind::Unitless, QStringLiteral("2"))
          .withStepMinimum(1)
          .withVisible(second),
      valueInput(QStringLiteral("angle2"), QObject::tr("Total Angle 2"), ValueKind::Angle, QStringLiteral("360 deg"))
          .withVisible(second)
          .withManipulator(ringAbout(QStringLiteral("axis2"))),
      checkInput(QStringLiteral("symmetric2"), QObject::tr("Symmetric 2"), false)
          .withTooltip(QObject::tr("Copies on both sides about the second axis, the angle centered on them"))
          .withVisible(second),
      suppressedInput(),
  };
  def.enabled = [&context] { return hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QJsonObject objects = objectsOf(state, result);
    if (!result.error.isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("circular_pattern")},
                  {QStringLiteral("objects"), objects},
                  {QStringLiteral("axis"), state.items(QStringLiteral("axis")).first().reference()},
                  {QStringLiteral("quantity"), state.expression(QStringLiteral("quantity"))},
                  {QStringLiteral("angle"), state.expression(QStringLiteral("angle"))}};
    if (state.checked(QStringLiteral("symmetric"))) {
      result.def.insert(QStringLiteral("symmetric"), true);
    }
    // The definition gets `axis2` only when a second axis is picked, so a
    // pattern without one saves as before.
    if (!state.items(QStringLiteral("axis2")).isEmpty()) {
      QJsonObject axis2{{QStringLiteral("axis"), state.items(QStringLiteral("axis2")).first().reference()},
                        {QStringLiteral("quantity"), state.expression(QStringLiteral("quantity2"))},
                        {QStringLiteral("angle"), state.expression(QStringLiteral("angle2"))}};
      if (state.checked(QStringLiteral("symmetric2"))) {
        axis2.insert(QStringLiteral("symmetric"), true);
      }
      result.def.insert(QStringLiteral("axis2"), axis2);
    }
    addCompute(result.def, state);
    addSuppressed(result.def, state);
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadObjects(feature, state, context);
    state.setItems(QStringLiteral("axis"), {itemOf(feature.value(QStringLiteral("axis")), context)});
    loadCount(state, QStringLiteral("quantity"), feature.value(QStringLiteral("quantity")), feature, context);
    loadValue(state, QStringLiteral("angle"), feature.value(QStringLiteral("angle")), feature, context);
    state.setChecked(QStringLiteral("symmetric"), feature.value(QStringLiteral("symmetric")).toBool());
    const QJsonObject axis2 = feature.value(QStringLiteral("axis2")).toObject();
    if (!axis2.isEmpty()) {
      state.setItems(QStringLiteral("axis2"), {itemOf(axis2.value(QStringLiteral("axis")), context)});
      loadCount(state, QStringLiteral("quantity2"), axis2.value(QStringLiteral("quantity")), feature, context);
      loadValue(state, QStringLiteral("angle2"), axis2.value(QStringLiteral("angle")), feature, context);
      state.setChecked(QStringLiteral("symmetric2"), axis2.value(QStringLiteral("symmetric")).toBool());
    }
    loadSuppressed(feature, state);
  };

  def.canEdit = [&context](const QJsonObject& feature) {
    const QJsonObject axis2 = feature.value(QStringLiteral("axis2")).toObject();
    return isShowable(feature.value(QStringLiteral("axis")), context) &&
           (axis2.isEmpty() || isShowable(axis2.value(QStringLiteral("axis")), context));
  };

  def.describe = [build = def.build](const CommandState& state, const CommandContext& model) {
    return describePattern("circular", state, build(state, model));
  };
  return def;
}

// ---------------------------------------------------------------------------
// Pattern on Path

CommandDef pathPatternCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.path_pattern");
  def.name = QObject::tr("Pattern on Path");
  def.icon = QStringLiteral("path-pattern");
  def.tooltip = QObject::tr("Copies along a sketch line, arc or circle, or a chain of lines and arcs");
  def.group = QStringLiteral("CREATE");
  def.section = QStringLiteral("patterns");
  def.subgroup = QStringLiteral("Pattern");
  def.keywords = {QStringLiteral("array"), QStringLiteral("path pattern"), QStringLiteral("curve")};
  def.featureType = QStringLiteral("path_pattern");
  def.inputs = patternObjects();
  choosePatternTypeOnPreselection(def);
  // The hover text names the Pattern Type first (mitcad#1235).
  def.keyInputs = {QStringLiteral("object_type")};
  def.inputs += {
      // Several curves are a chain, joined end to end (mitcad#464).
      // Or an Intersection Curve (mitcad#1405), picked in the view or on the
      // timeline.
      // Or curves of a 3D sketch, or its route whole from the Browser or
      // the timeline (mitcad#1550).
      selectionInput(QStringLiteral("path"), QObject::tr("Path"),
                     SelectKind::SketchCurve | SelectKind::Feature | SelectKind::Sketch, 1, 0)
          .withTooltip(QObject::tr("A sketch line, arc or circle, a chain of them, an intersection curve, or 3D "
                                   "sketch curves"))
          .withAccepts([&context](const SelectionItem& item) {
            // A planar sketch gives lines, arcs and circles; a 3D sketch
            // any curve that is a path piece.
            if (item.kind == SelectKind::SketchCurve && !isLine(item) && !isCircular(item)) {
              return isSpatialSketch(item.owner, context) && spatialPathTakes(item, context);
            }
            return item.kind == SelectKind::SketchCurve || spatialPathTakes(item, context);
          })
          .withRefusedHint([&context](const SelectionItem& item, const CommandState&) {
            return spatialPathRefusal(item, context);
          })
          .withOnPick([](const SelectionItem& item, CommandState& state,
                         const CommandContext& model) { takeWholeRoute(item, state, model, QStringLiteral("path")); },
                      false)
          .withItemsText([](const Selection& items, const CommandState& state, const CommandContext& model) {
            return spatialRouteListing(items, false, keptPathReversed(state, QStringLiteral("path")), model,
                                       QStringLiteral("path"));
          }),
      distanceTypeInput(),
      directionTypeInput(QStringLiteral("direction_type"), QObject::tr("Direction Type")),
      valueInput(QStringLiteral("quantity"), QObject::tr("Quantity"), ValueKind::Unitless, QStringLiteral("3")).withStepMinimum(1),
      lengthInput(QStringLiteral("distance"), QString()),
      valueInput(QStringLiteral("start"), QObject::tr("Start Point"), ValueKind::Unitless, QStringLiteral("0"))
          .withTooltip(QObject::tr("Where the original is on the path, 0 at its start and 1 at its end")),
      choiceInput(QStringLiteral("orientation"), QObject::tr("Orientation"),
                  {{QStringLiteral("identical"), QStringLiteral("Identical")},
                   {QStringLiteral("path"), QStringLiteral("Path Direction")}},
                  QStringLiteral("identical")),
      // Flip stays while symmetric (mitcad#545): the definition keeps it, and
      // it puts an even quantity's extra copy at the other side.
      flipInput(QStringLiteral("flip"), QObject::tr("Flip")).withTooltip(QObject::tr("Copies the other way along the path")),
      suppressedInput(),
  };
  def.enabled = [&context] { return hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext& context) {
    Built result;
    const QJsonObject objects = objectsOf(state, result);
    if (!result.error.isEmpty()) {
      return result;
    }
    const QString id = QStringLiteral("path");
    const Selection& curves = state.items(id);
    // A 3D sketch's curves or the sketch whole: its typed path, running as
    // an edited one ran (mitcad#1550).
    const bool spatial = !spatialOwner(curves, context).isEmpty() ||
                         (curves.size() == 1 && wholeSketchPick(curves.first()));
    const QJsonValue path = spatial ? pathOf(curves, false, result, id, context, keptPathReversed(state, id))
                            : curves.size() > 1 || curves.first().kind == SelectKind::Feature
                                ? pathOf(curves, false, result, id)
                                : curves.first().reference();
    if (!result.error.isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("path_pattern")},
                  {QStringLiteral("objects"), objects},
                  {QStringLiteral("path"), path},
                  {QStringLiteral("quantity"), state.expression(QStringLiteral("quantity"))},
                  {QStringLiteral("distance"), state.expression(QStringLiteral("distance"))},
                  {QStringLiteral("distance_type"), state.choice(QStringLiteral("distance_type"))}};
    if (nonZero(state, QStringLiteral("start"))) {
      result.def.insert(QStringLiteral("start"), state.value(QStringLiteral("start")));
    }
    if (state.checked(QStringLiteral("flip"))) {
      result.def.insert(QStringLiteral("flip"), true);
    }
    if (state.choice(QStringLiteral("orientation")) == QStringLiteral("path")) {
      result.def.insert(QStringLiteral("along_path"), true);
    }
    setDirectionType(result.def, state.choice(QStringLiteral("direction_type")));
    addCompute(result.def, state);
    addSuppressed(result.def, state);
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadObjects(feature, state, context);
    loadSuppressed(feature, state);
    const QJsonValue path = feature.value(QStringLiteral("path"));
    if (path.toObject().contains(QStringLiteral("curves")) || path.toObject().contains(QStringLiteral("feature"))) {
      state.setItems(QStringLiteral("path"), pathItems(path, context).value_or(Selection{}));
    } else {
      state.setItems(QStringLiteral("path"), {itemOf(path, context)});
    }
    // A 3D sketch's path keeps its direction (mitcad#1550).
    keepPathDirection(path, state, QStringLiteral("path"));
    state.setChoice(QStringLiteral("distance_type"),
                    feature.value(QStringLiteral("distance_type")).toString(QStringLiteral("spacing")));
    state.setChoice(QStringLiteral("direction_type"), directionTypeOf(feature));
    loadCount(state, QStringLiteral("quantity"), feature.value(QStringLiteral("quantity")), feature, context);
    loadValue(state, QStringLiteral("distance"), feature.value(QStringLiteral("distance")), feature, context);
    state.setText(QStringLiteral("start"), number(feature.value(QStringLiteral("start")).toDouble()));
    state.setChecked(QStringLiteral("flip"), feature.value(QStringLiteral("flip")).toBool());
    state.setChoice(QStringLiteral("orientation"), feature.value(QStringLiteral("along_path")).toBool()
                                                       ? QStringLiteral("path")
                                                       : QStringLiteral("identical"));
  };

  // A fixed line as the path has no input; a 3D sketch's path is its
  // curves (mitcad#1550).
  def.canEdit = [](const QJsonObject& feature) {
    const QJsonObject path = feature.value(QStringLiteral("path")).toObject();
    return path.contains(QStringLiteral("sketch")) || path.contains(QStringLiteral("spatial_sketch"));
  };

  def.describe = [build = def.build](const CommandState& state, const CommandContext& model) {
    return describePattern("path", state, build(state, model));
  };
  return def;
}

// ---------------------------------------------------------------------------
// Mirror

CommandDef mirrorCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.mirror");
  def.name = QObject::tr("Mirror");
  def.icon = QStringLiteral("mirror");
  def.tooltip = QObject::tr("A mirror image about a plane");
  def.group = QStringLiteral("CREATE");
  // A plain entry after the Pattern submenu, in its section (mitcad#579).
  def.section = QStringLiteral("patterns");
  def.keywords = {QStringLiteral("reflect"), QStringLiteral("symmetry")};
  def.featureType = QStringLiteral("mirror");
  def.inputs = patternObjects();
  choosePatternTypeOnPreselection(def);
  def.inputs.first().label = QObject::tr("Type");
  def.inputs += {
      selectionInput(QStringLiteral("plane"), QObject::tr("Mirror Plane"), kPlaneKinds, 1, 1)
          .withAccepts(isPlanar),
      choiceInput(QStringLiteral("operation"), QObject::tr("Operation"),
                  {{QStringLiteral("new_body"), QStringLiteral("New Body")}, {QStringLiteral("join"), QStringLiteral("Join")}},
                  QStringLiteral("new_body"))
          .withTooltip(QObject::tr("Join a mirrored body to its original where they touch"))
          .withVisible(choiceIs(QStringLiteral("object_type"), QStringLiteral("bodies"))),
  };
  def.enabled = [&context] { return hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QJsonObject objects = objectsOf(state, result);
    if (!result.error.isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("mirror")},
                  {QStringLiteral("objects"), objects},
                  {QStringLiteral("plane"), state.items(QStringLiteral("plane")).first().reference()}};
    if (state.choice(QStringLiteral("object_type")) == QStringLiteral("bodies") &&
        state.choice(QStringLiteral("operation")) == QStringLiteral("join")) {
      result.def.insert(QStringLiteral("combine"), true);
    }
    addCompute(result.def, state);
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadObjects(feature, state, context);
    state.setItems(QStringLiteral("plane"), {itemOf(feature.value(QStringLiteral("plane")), context)});
    state.setChoice(QStringLiteral("operation"), feature.value(QStringLiteral("combine")).toBool()
                                                     ? QStringLiteral("join")
                                                     : QStringLiteral("new_body"));
  };

  def.canEdit = [&context](const QJsonObject& feature) {
    return isShowable(feature.value(QStringLiteral("plane")), context);
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added mirror of %1 %2")
        .arg(state.items(objectsKey(state)).size())
        .arg(state.choice(QStringLiteral("object_type")));
  };
  return def;
}

} // namespace mitcad::cmd
