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
          .withVisible(choiceIs(QStringLiteral("object_type"), QStringLiteral("faces"))),
      choiceInput(QStringLiteral("compute"), QObject::tr("Compute Option"), kComputes, QStringLiteral("adjust"))
          .withTooltip(QObject::tr("Adjust computes each copy where it is; Identical moves the "
                                   "original's result"))
          .withVisible(notBodies),
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
      .withTooltip(QObject::tr("Instances left out, by number (2, 5): click the dots in the view"))
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

// An arrow from the objects' middle along an axis input.
std::function<std::optional<Manipulator>(const CommandState&, const CommandContext&)>
alongAxis(const QString& axis) {
  return [axis](const CommandState& state, const CommandContext& model) -> std::optional<Manipulator> {
    const Selection& axisItems = state.items(axis);
    const auto center = centerOf(state.items(objectsKey(state)), model);
    if (axisItems.isEmpty() || !center) {
      return std::nullopt;
    }
    const auto line = axisOf(axisItems.first(), model);
    return line ? std::optional<Manipulator>(arrow(*center, line->second)) : std::nullopt;
  };
}

InputDef lineInput(const QString& id, const QString& name, int min) {
  return selectionInput(id, name, SelectKind::Axis | SelectKind::Edge | SelectKind::SketchCurve, min, 1)
      .withTooltip(QObject::tr("An axis, a straight edge or a sketch line"))
      .withAccepts([](const SelectionItem& item) { return item.kind == SelectKind::Axis || isLine(item); });
}

QString describePattern(const char* kind, const CommandState& state) {
  return QStringLiteral("Added %1 pattern of %2 %3")
      .arg(QString::fromLatin1(kind))
      .arg(state.items(objectsKey(state)).size())
      .arg(state.choice(QStringLiteral("object_type")));
}

} // namespace

// ---------------------------------------------------------------------------
// Rectangular Pattern

CommandDef rectangularPatternCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.rectangular_pattern");
  def.name = QObject::tr("Rectangular Pattern");
  def.icon = QStringLiteral("rectangular-pattern");
  def.tooltip = QObject::tr("Copies in rows and columns along one or two directions");
  def.group = QStringLiteral("CREATE");
  def.keywords = {QStringLiteral("array"), QStringLiteral("linear pattern"), QStringLiteral("copy")};
  def.featureType = QStringLiteral("rectangular_pattern");
  def.inputs = patternObjects();
  def.inputs += {
      lineInput(QStringLiteral("direction1"), QObject::tr("Direction 1"), 1),
      valueInput(QStringLiteral("quantity1"), QObject::tr("Quantity"), ValueKind::Unitless, QStringLiteral("3")),
      valueInput(QStringLiteral("distance1"), QObject::tr("Distance"), ValueKind::Length, QStringLiteral("20 mm"))
          .withManipulator(alongAxis(QStringLiteral("direction1"))),
      checkInput(QStringLiteral("symmetric1"), QObject::tr("Symmetric"), false),
      lineInput(QStringLiteral("direction2"), QObject::tr("Direction 2"), 0)
          .withTooltip(QObject::tr("A second direction; none: one row")),
      valueInput(QStringLiteral("quantity2"), QObject::tr("Quantity 2"), ValueKind::Unitless, QStringLiteral("2"))
          .withVisible([](const CommandState& s) { return !s.items(QStringLiteral("direction2")).isEmpty(); }),
      valueInput(QStringLiteral("distance2"), QObject::tr("Distance 2"), ValueKind::Length, QStringLiteral("20 mm"))
          .withVisible([](const CommandState& s) { return !s.items(QStringLiteral("direction2")).isEmpty(); })
          .withManipulator(alongAxis(QStringLiteral("direction2"))),
      checkInput(QStringLiteral("symmetric2"), QObject::tr("Symmetric 2"), false)
          .withVisible([](const CommandState& s) { return !s.items(QStringLiteral("direction2")).isEmpty(); }),
      choiceInput(QStringLiteral("distance_type"), QObject::tr("Distance Type"), kSpacings, QStringLiteral("spacing")),
      suppressedInput(),
  };
  def.enabled = [&context] { return hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QJsonObject objects = objectsOf(state, result);
    if (!result.error.isEmpty()) {
      return result;
    }
    const auto direction = [&state](const char* suffix) {
      const QString n = QString::fromLatin1(suffix);
      QJsonObject d{{QStringLiteral("axis"), state.items(QStringLiteral("direction") + n).first().reference()},
                    {QStringLiteral("quantity"), countValue(state, QStringLiteral("quantity") + n)},
                    {QStringLiteral("distance"), state.expression(QStringLiteral("distance") + n)}};
      if (state.checked(QStringLiteral("symmetric") + n)) {
        d.insert(QStringLiteral("symmetric"), true);
      }
      return d;
    };
    result.def = {{QStringLiteral("type"), QStringLiteral("rectangular_pattern")},
                  {QStringLiteral("objects"), objects},
                  {QStringLiteral("direction1"), direction("1")},
                  {QStringLiteral("distance_type"), state.choice(QStringLiteral("distance_type"))}};
    if (!state.items(QStringLiteral("direction2")).isEmpty()) {
      result.def.insert(QStringLiteral("direction2"), direction("2"));
    }
    addCompute(result.def, state);
    addSuppressed(result.def, state);
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadObjects(feature, state, context);
    for (const char* n : {"1", "2"}) {
      const QString suffix = QString::fromLatin1(n);
      const QJsonObject d = feature.value(QStringLiteral("direction") + suffix).toObject();
      if (d.isEmpty()) {
        continue;
      }
      state.setItems(QStringLiteral("direction") + suffix, {itemOf(d.value(QStringLiteral("axis")), context)});
      loadCount(state, QStringLiteral("quantity") + suffix, d.value(QStringLiteral("quantity")), feature, context);
      loadValue(state, QStringLiteral("distance") + suffix, d.value(QStringLiteral("distance")), feature, context);
      state.setChecked(QStringLiteral("symmetric") + suffix, d.value(QStringLiteral("symmetric")).toBool());
    }
    state.setChoice(QStringLiteral("distance_type"), str(feature, "distance_type"));
    loadSuppressed(feature, state);
  };

  def.canEdit = [&context](const QJsonObject& feature) {
    for (const char* key : {"direction1", "direction2"}) {
      const QJsonObject d = feature.value(QLatin1String(key)).toObject();
      if (!d.isEmpty() && !isShowable(d.value(QStringLiteral("axis")), context)) {
        return false;
      }
    }
    return true;
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    return describePattern("rectangular", state);
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
  def.keywords = {QStringLiteral("array"), QStringLiteral("polar pattern"), QStringLiteral("radial")};
  def.featureType = QStringLiteral("circular_pattern");
  def.inputs = patternObjects();
  def.inputs += {
      selectionInput(QStringLiteral("axis"), QObject::tr("Axis"), kAxisKinds, 1, 1)
          .withTooltip(QObject::tr("An axis, a straight or circular edge, a sketch line or a "
                                   "cylindrical face"))
          .withAccepts(isAxial),
      valueInput(QStringLiteral("quantity"), QObject::tr("Quantity"), ValueKind::Unitless, QStringLiteral("4")),
      valueInput(QStringLiteral("angle"), QObject::tr("Total Angle"), ValueKind::Angle, QStringLiteral("360 deg"))
          .withManipulator([](const CommandState& state, const CommandContext& model) -> std::optional<Manipulator> {
            const Selection& axis = state.items(QStringLiteral("axis"));
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
          }),
      checkInput(QStringLiteral("symmetric"), QObject::tr("Symmetric"), false),
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
                  {QStringLiteral("quantity"), countValue(state, QStringLiteral("quantity"))},
                  {QStringLiteral("angle"), state.expression(QStringLiteral("angle"))}};
    if (state.checked(QStringLiteral("symmetric"))) {
      result.def.insert(QStringLiteral("symmetric"), true);
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
    loadSuppressed(feature, state);
  };

  def.canEdit = [&context](const QJsonObject& feature) {
    return isShowable(feature.value(QStringLiteral("axis")), context);
  };

  def.describe = [](const CommandState& state, const CommandContext&) { return describePattern("circular", state); };
  return def;
}

// ---------------------------------------------------------------------------
// Pattern on Path

CommandDef pathPatternCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.path_pattern");
  def.name = QObject::tr("Pattern on Path");
  def.icon = QStringLiteral("path-pattern");
  def.tooltip = QObject::tr("Copies along a sketch line, arc or circle");
  def.group = QStringLiteral("CREATE");
  def.keywords = {QStringLiteral("array"), QStringLiteral("path pattern"), QStringLiteral("curve")};
  def.featureType = QStringLiteral("path_pattern");
  def.inputs = patternObjects();
  def.inputs += {
      selectionInput(QStringLiteral("path"), QObject::tr("Path"), SelectKind::SketchCurve, 1, 1)
          .withTooltip(QObject::tr("A sketch line, arc or circle"))
          .withAccepts([](const SelectionItem& item) { return isLine(item) || isCircular(item); }),
      valueInput(QStringLiteral("quantity"), QObject::tr("Quantity"), ValueKind::Unitless, QStringLiteral("3")),
      valueInput(QStringLiteral("distance"), QObject::tr("Distance"), ValueKind::Length, QStringLiteral("20 mm")),
      choiceInput(QStringLiteral("distance_type"), QObject::tr("Distance Type"), kSpacings, QStringLiteral("spacing")),
      valueInput(QStringLiteral("start"), QObject::tr("Start Point"), ValueKind::Unitless, QStringLiteral("0"))
          .withTooltip(QObject::tr("Where the original is on the path, 0 at its start and 1 at its end")),
      choiceInput(QStringLiteral("orientation"), QObject::tr("Orientation"),
                  {{QStringLiteral("identical"), QStringLiteral("Identical")},
                   {QStringLiteral("path"), QStringLiteral("Path Direction")}},
                  QStringLiteral("identical")),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip")),
      checkInput(QStringLiteral("symmetric"), QObject::tr("Symmetric"), false),
      suppressedInput(),
  };
  def.enabled = [&context] { return hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QJsonObject objects = objectsOf(state, result);
    if (!result.error.isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("path_pattern")},
                  {QStringLiteral("objects"), objects},
                  {QStringLiteral("path"), state.items(QStringLiteral("path")).first().reference()},
                  {QStringLiteral("quantity"), countValue(state, QStringLiteral("quantity"))},
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
    if (state.checked(QStringLiteral("symmetric"))) {
      result.def.insert(QStringLiteral("symmetric"), true);
    }
    addCompute(result.def, state);
    addSuppressed(result.def, state);
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadObjects(feature, state, context);
    loadSuppressed(feature, state);
    state.setItems(QStringLiteral("path"), {itemOf(feature.value(QStringLiteral("path")), context)});
    loadCount(state, QStringLiteral("quantity"), feature.value(QStringLiteral("quantity")), feature, context);
    loadValue(state, QStringLiteral("distance"), feature.value(QStringLiteral("distance")), feature, context);
    state.setChoice(QStringLiteral("distance_type"), str(feature, "distance_type"));
    state.setText(QStringLiteral("start"), number(feature.value(QStringLiteral("start")).toDouble()));
    state.setChecked(QStringLiteral("flip"), feature.value(QStringLiteral("flip")).toBool());
    state.setChoice(QStringLiteral("orientation"), feature.value(QStringLiteral("along_path")).toBool()
                                                       ? QStringLiteral("path")
                                                       : QStringLiteral("identical"));
    state.setChecked(QStringLiteral("symmetric"), feature.value(QStringLiteral("symmetric")).toBool());
  };

  // A fixed line as the path has no input.
  def.canEdit = [](const QJsonObject& feature) {
    return feature.value(QStringLiteral("path")).toObject().contains(QStringLiteral("sketch"));
  };

  def.describe = [](const CommandState& state, const CommandContext&) { return describePattern("path", state); };
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
  def.keywords = {QStringLiteral("reflect"), QStringLiteral("symmetry")};
  def.featureType = QStringLiteral("mirror");
  def.inputs = patternObjects();
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
