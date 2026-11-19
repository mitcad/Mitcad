// SPDX-License-Identifier: MIT
// Translation patterns at referenced points, with a model-resolved anchor.
#include <algorithm>
#include <cmath>
#include <functional>

#include <QJsonArray>

#include "../framework/ModelShapes.hpp"
#include "CommandFactories.hpp"
#include "CommandSupport.hpp"

namespace mitcad::cmd {
namespace {

auto sourceMode(const char* type) {
  return [type](const CommandState& state) {
    return state.choice(QStringLiteral("object_type")) == QLatin1String(type);
  };
}

// The same first placement the viewport uses for a component's sketches.
QString displayedContext(const CommandContext& context, const QString& component) {
  if (component == QStringLiteral("C0")) {
    return QString();
  }
  const QJsonObject assembly = context.queryObject({{"query", "components"}});
  std::function<QString(const QJsonArray&, const QString&)> find = [&](const QJsonArray& level, const QString& parent) {
    for (const QJsonValue& value : level) {
      const QJsonObject occurrence = value.toObject();
      const QString uid = occurrence.value(QStringLiteral("uid")).toString();
      const QString path = parent.isEmpty() ? uid : parent + QLatin1Char('/') + uid;
      if (occurrence.value(QStringLiteral("component")).toString() == component) {
        return path;
      }
      const QString child = find(occurrence.value(QStringLiteral("children")).toArray(), path);
      if (!child.isEmpty()) {
        return child;
      }
    }
    return QString();
  };
  return find(assembly.value(QStringLiteral("occurrences")).toArray(), QString());
}

QJsonObject pointReference(const SelectionItem& item) {
  // Empty is an explicit root occurrence; omission would mean feature-local.
  return {{QStringLiteral("point"), item.reference()}, {QStringLiteral("occurrence"), item.occurrence}};
}

SelectionItem pointItem(const QJsonValue& value, const QString& local, const CommandContext& context) {
  const QJsonObject reference = value.toObject();
  SelectionItem item = itemOf(reference.value(QStringLiteral("point")), context);
  item.occurrence = reference.value(QStringLiteral("occurrence")).toString(local);
  return item;
}

} // namespace

CommandDef pointPatternCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.pattern_at_points");
  def.name = QObject::tr("Pattern at Points");
  def.icon = QStringLiteral("pattern-at-points");
  def.tooltip = QObject::tr("Copies features, bodies or components to selected points");
  def.keywords = {QStringLiteral("array"), QStringLiteral("irregular"), QStringLiteral("copy"), QStringLiteral("anchor")};
  def.featureType = QStringLiteral("pattern_at_points");
  def.inputs = {
      choiceInput(QStringLiteral("object_type"), QObject::tr("Pattern Type"),
                  {{QStringLiteral("bodies"), QObject::tr("Bodies")},
                   {QStringLiteral("features"), QObject::tr("Features")},
                   {QStringLiteral("components"), QObject::tr("Components")}}, QStringLiteral("bodies")),
      selectionInput(QStringLiteral("bodies"), QObject::tr("Objects"), SelectKind::Body, 1, 0)
          .withVisible(sourceMode("bodies")),
      selectionInput(QStringLiteral("features"), QObject::tr("Objects"), SelectKind::Feature | SelectKind::Face, 1, 0)
          .withTooltip(QObject::tr("Features in the timeline, or a face a feature made"))
          .withAccepts([](const SelectionItem& item) { return !item.creatingFeature().isEmpty(); })
          .withConvert([](const SelectionItem& item) {
            return SelectionItem{SelectKind::Feature, item.creatingFeature(), QString(), QString(), item.occurrence};
          })
          .withVisible(sourceMode("features")),
      selectionInput(QStringLiteral("components"), QObject::tr("Objects"), SelectKind::Component | SelectKind::Body, 1, 0)
          .withTooltip(QObject::tr("Component occurrences in the browser, or bodies they place"))
          .withAccepts([](const SelectionItem& item) { return !item.occurrence.isEmpty(); })
          .withConvert([](const SelectionItem& item) {
            return SelectionItem{SelectKind::Component, QString(), QString(), QString(), item.occurrence};
          })
          .withVisible(sourceMode("components")),
      selectionInput(QStringLiteral("targets"), QObject::tr("Target Points"), kPointKinds, 1, 0)
          .withTooltip(QObject::tr("Sketch points, construction points or vertices; one additional copy at each point")),
      choiceInput(QStringLiteral("anchor_mode"), QObject::tr("Anchor"),
                  {{QStringLiteral("automatic"), QObject::tr("Automatic")},
                   {QStringLiteral("selected"), QObject::tr("Selected point")}}, QStringLiteral("automatic"))
          .withHeading(QObject::tr("Additional options"))
          .withTooltip(QObject::tr("Automatic uses component origins, hole placements or source bounds. The preview marks the anchor."))
          .withOnChange([](CommandState& state, const CommandContext&) {
            if (state.choice(QStringLiteral("anchor_mode")) == QStringLiteral("automatic")) {
              state.setItems(QStringLiteral("anchor"), {});
            }
          }),
      selectionInput(QStringLiteral("anchor"), QObject::tr("Anchor Point"), kPointKinds, 1, 1)
          .withVisible([](const CommandState& state) { return state.choice(QStringLiteral("anchor_mode")) == QStringLiteral("selected"); }),
      choiceInput(QStringLiteral("compute"), QObject::tr("Compute Option"),
                  {{QStringLiteral("adjust"), QObject::tr("Adjust")},
                   {QStringLiteral("identical"), QObject::tr("Identical")},
                   {QStringLiteral("optimized"), QObject::tr("Optimized")}}, QStringLiteral("adjust"))
          .withVisible(sourceMode("features")),
      textInput(QStringLiteral("context"), QString()).withVisible([](const CommandState&) { return false; }),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.init = [](CommandState& state, const CommandContext& model) {
    if (state.items(QStringLiteral("bodies")).isEmpty()) {
      if (!state.items(QStringLiteral("features")).isEmpty()) {
        state.setChoice(QStringLiteral("object_type"), QStringLiteral("features"));
      } else if (!state.items(QStringLiteral("components")).isEmpty()) {
        state.setChoice(QStringLiteral("object_type"), QStringLiteral("components"));
      }
    }
    const QString component = model.queryObject({{"query", "components"}}).value(QStringLiteral("active")).toString(QStringLiteral("C0"));
    state.setText(QStringLiteral("context"), displayedContext(model, component));
  };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QString mode = state.choice(QStringLiteral("object_type"));
    QString local = state.text(QStringLiteral("context"));
    if (mode == QStringLiteral("bodies") && !state.items(mode).isEmpty()) {
      local = state.items(mode).first().occurrence;
    } else if (mode == QStringLiteral("features")) {
      for (const SelectionItem& item : state.items(mode)) {
        if (!item.occurrence.isEmpty()) {
          local = item.occurrence;
          break;
        }
      }
    }
    QJsonArray objects;
    for (const SelectionItem& item : state.items(mode)) {
      if ((mode == QStringLiteral("bodies") || (mode == QStringLiteral("features") && !item.occurrence.isEmpty())) &&
          item.occurrence != local) {
        return Built::failure(QObject::tr("Select sources in the active component's displayed occurrence, or use Components."), mode);
      }
      const QJsonValue value = mode == QStringLiteral("components") ? item.occurrence : item.owner;
      if (!objects.contains(value)) {
        objects.append(value);
      }
    }
    QJsonArray targets;
    for (const SelectionItem& item : state.items(QStringLiteral("targets"))) {
      const QJsonObject reference = pointReference(item);
      if (!targets.contains(reference)) {
        targets.append(reference);
      }
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("pattern_at_points")},
                  {QStringLiteral("objects"), QJsonObject{{QStringLiteral("type"), mode},
                      {mode == QStringLiteral("components") ? QStringLiteral("occurrences") : mode, objects}}},
                  {QStringLiteral("targets"), targets}, {QStringLiteral("context"), local}};
    if (state.choice(QStringLiteral("anchor_mode")) == QStringLiteral("selected")) {
      result.def.insert(QStringLiteral("anchor"), pointReference(state.items(QStringLiteral("anchor")).first()));
    }
    if (mode == QStringLiteral("features")) {
      result.def.insert(QStringLiteral("compute"), state.choice(QStringLiteral("compute")));
    }
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& model) {
    const QString component = model.queryObject({{"query", "feature"}, {"uid", feature.value(QStringLiteral("uid"))}})
                                  .value(QStringLiteral("component")).toString(QStringLiteral("C0"));
    const QString local = feature.value(QStringLiteral("context")).toString(displayedContext(model, component));
    state.setText(QStringLiteral("context"), local);
    const QJsonObject objects = feature.value(QStringLiteral("objects")).toObject();
    const QString mode = objects.value(QStringLiteral("type")).toString();
    state.setChoice(QStringLiteral("object_type"), mode);
    Selection sources;
    for (const QJsonValue& value : objects.value(mode == QStringLiteral("components") ? QStringLiteral("occurrences") : mode).toArray()) {
      sources.append(mode == QStringLiteral("components")
          ? SelectionItem{SelectKind::Component, QString(), QString(), QString(), value.toString()}
          : SelectionItem{mode == QStringLiteral("bodies") ? SelectKind::Body : SelectKind::Feature,
                          value.toString(), QString(), QString(), local});
    }
    state.setItems(mode, sources);
    Selection targets;
    for (const QJsonValue& target : feature.value(QStringLiteral("targets")).toArray()) {
      targets.append(pointItem(target, local, model));
    }
    state.setItems(QStringLiteral("targets"), targets);
    const QJsonValue anchor = feature.value(QStringLiteral("anchor"));
    state.setChoice(QStringLiteral("anchor_mode"), anchor.isObject() ? QStringLiteral("selected") : QStringLiteral("automatic"));
    if (anchor.isObject()) {
      state.setItems(QStringLiteral("anchor"), {pointItem(anchor, local, model)});
    }
    state.setChoice(QStringLiteral("compute"), feature.value(QStringLiteral("compute")).toString(QStringLiteral("adjust")));
  };
  def.canEdit = [&context](const QJsonObject& feature) {
    const auto showable = [&context](const QJsonValue& point) {
      return isShowable(point.toObject().value(QStringLiteral("point")), context);
    };
    const QJsonArray targets = feature.value(QStringLiteral("targets")).toArray();
    return std::all_of(targets.begin(), targets.end(), showable) &&
           (!feature.value(QStringLiteral("anchor")).isObject() || showable(feature.value(QStringLiteral("anchor"))));
  };
  def.previewMarkers = [](const QJsonObject& report) {
    std::vector<PreviewMarker> result;
    const QJsonObject anchor = report.value(QStringLiteral("anchor")).toObject();
    const QJsonArray world = anchor.value(QStringLiteral("world")).toArray();
    if (world.size() == 3 && std::all_of(world.begin(), world.end(), [](const QJsonValue& value) {
          return value.isDouble() && std::isfinite(value.toDouble());
        })) {
      result.push_back({anchor.value(QStringLiteral("mode")).toString() == QStringLiteral("automatic")
                            ? QObject::tr("Automatic anchor") : QObject::tr("Selected anchor"),
                        pointOf(world)});
    }
    return result;
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added pattern at %1 points of %2 %3")
        .arg(state.items(QStringLiteral("targets")).size())
        .arg(state.items(state.choice(QStringLiteral("object_type"))).size())
        .arg(state.choice(QStringLiteral("object_type")));
  };
  return def;
}

} // namespace mitcad::cmd
