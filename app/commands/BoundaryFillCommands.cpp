// SPDX-License-Identifier: MIT
// Boundary Fill (mitcad#457, its panel mitcad#2149; app/COMMANDS.md
// "Boundary Fill"): solids of the space that solids, surface bodies and
// planes close off together, new bodies or joined to, cut from or
// intersected with bodies. Home Solid › Create, a second place Surface ›
// Modify after Thicken.
//
// The panel lists the closed regions of the tools picked through the
// model's `boundary_regions` query (once per set of tools) and shows a dot
// at each region's point in the view; a click on a dot adds the region to
// Cells or takes it out (the Cells field holds their numbers, "1, 3"). When
// the tools close off exactly one region it is taken at once. The
// definition's `cells` are the regions' points; editing places a stored
// point in the region that holds it, once the timeline is rolled back, and
// keeps a point in no closed region as it is. The preview shows every
// closed region see-through (the feature's preview tool) beside the filled
// cells.
#include <algorithm>
#include <exception>
#include <memory>
#include <optional>
#include <vector>

#include <QDebug>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QList>
#include <QStringList>

#include <gp_Pnt.hxx>

#include "../framework/Json.hpp"
#include "CommandFactories.hpp"
#include "CommandSupport.hpp"

namespace mitcad::cmd {
namespace {

const QString kTools = QStringLiteral("tools");
const QString kCells = QStringLiteral("cells");
const QString kRemoveTools = QStringLiteral("remove_tools");
// The state beside the inputs: the points of the definition being edited
// (`points`), and those of them that lie in no closed region now, kept as
// they are (`kept`).
const QString kLoaded = QStringLiteral("loaded_cells");
const QString kKept = QStringLiteral("kept_cells");

// The closed regions of a set of tools as the `boundary_regions` query
// lists them: [{"point": [x, y, z], "min", "max"}], for the tools of
// `key`; `error` when the query failed.
struct Regions {
  QString key;
  QJsonArray list;
  QString error;
};

// The definition's `tools` (the bodies) and `planes` (plane references) of
// the picks.
QJsonObject toolFields(const Selection& tools) {
  QJsonArray bodies;
  QJsonArray planes;
  for (const SelectionItem& item : tools) {
    if (item.kind == SelectKind::Body) {
      bodies.append(item.owner);
    } else {
      planes.append(item.reference());
    }
  }
  QJsonObject fields{{QStringLiteral("tools"), bodies}};
  if (!planes.isEmpty()) {
    fields.insert(QStringLiteral("planes"), planes);
  }
  return fields;
}

QString keyOf(const QJsonObject& fields) {
  return QString::fromUtf8(QJsonDocument(fields).toJson(QJsonDocument::Compact));
}

// The query's answer for the tools, with `points` placed among the regions
// (`places`), or the error.
QJsonObject queryRegions(const QJsonObject& fields, const QJsonArray& points, const CommandContext& context,
                         QString& error) {
  QJsonObject query = fields;
  query.insert(QStringLiteral("query"), QStringLiteral("boundary_regions"));
  if (!points.isEmpty()) {
    query.insert(QStringLiteral("points"), points);
  }
  try {
    return context.queryObject(query);
  } catch (const std::exception& e) {
    error = errorText(e);
    return QJsonObject();
  }
}

// The closed regions of the tools picked, asked of the model once for each
// set of tools.
const Regions& regionsOf(const Selection& tools, const CommandContext& context, Regions& cache) {
  const QJsonObject fields = toolFields(tools);
  const QString key = keyOf(fields);
  if (cache.key == key) {
    return cache;
  }
  cache.key = key;
  cache.list = QJsonArray();
  cache.error.clear();
  if (!tools.isEmpty()) {
    cache.list = queryRegions(fields, QJsonArray(), context, cache.error)
                     .value(QStringLiteral("regions"))
                     .toArray();
  }
  return cache;
}

// The region numbers in Cells ("1, 3"), each once, in their order.
QList<int> cellNumbers(const CommandState& state) {
  QList<int> numbers;
  for (const QString& part : state.text(kCells).split(QLatin1Char(','), Qt::SkipEmptyParts)) {
    bool ok = false;
    const int number = part.trimmed().toInt(&ok);
    if (ok && number > 0 && !numbers.contains(number)) {
      numbers.append(number);
    }
  }
  return numbers;
}

QString numbersText(const QList<int>& numbers) {
  QStringList texts;
  for (const int number : numbers) {
    texts << QString::number(number);
  }
  return texts.join(QStringLiteral(", "));
}

std::optional<gp_Pnt> pointOf(const QJsonValue& value) {
  const QJsonArray xyz = value.toArray();
  if (xyz.size() != 3) {
    return std::nullopt;
  }
  return gp_Pnt(xyz[0].toDouble(), xyz[1].toDouble(), xyz[2].toDouble());
}

// The points of the definition being edited placed in the regions that
// hold them: their numbers into Cells, the others kept as they are. Asked
// with the tools as they are where the model is now; again once the
// timeline is rolled back to the feature.
void placeLoaded(CommandState& state, const CommandContext& context, Regions& cache) {
  const QJsonArray points = state.object(kLoaded).value(QStringLiteral("points")).toArray();
  if (points.isEmpty()) {
    return;
  }
  const Selection& tools = state.items(kTools);
  const QJsonObject fields = toolFields(tools);
  QString error;
  const QJsonObject answer = tools.isEmpty() ? QJsonObject() : queryRegions(fields, points, context, error);
  cache.key = keyOf(fields);
  cache.list = answer.value(QStringLiteral("regions")).toArray();
  cache.error = error;
  const QJsonArray places = answer.value(QStringLiteral("places")).toArray();
  QList<int> numbers;
  QJsonArray kept;
  for (int i = 0; i < points.size(); ++i) {
    const QJsonObject place = places.at(i).toObject();
    if (place.contains(QStringLiteral("region"))) {
      const int number = place.value(QStringLiteral("region")).toInt() + 1;
      if (!numbers.contains(number)) {
        numbers.append(number);
      }
    } else {
      kept.append(points.at(i));
    }
  }
  state.setText(kCells, numbersText(numbers));
  state.setObject(kKept, {{QStringLiteral("points"), kept}});
  qDebug().noquote() << QStringLiteral("Boundary Fill cells loaded: %1 of %2 closed regions, %3 points kept")
                            .arg(numbersText(numbers).isEmpty() ? QStringLiteral("none") : numbersText(numbers))
                            .arg(cache.list.size())
                            .arg(kept.size());
}

} // namespace

CommandDef boundaryFillCommand(const CommandContext& context) {
  const auto cache = std::make_shared<Regions>();
  CommandDef def;
  def.id = QStringLiteral("solid.boundary_fill");
  def.name = QObject::tr("Boundary Fill");
  def.icon = QStringLiteral("boundary-fill");
  def.tooltip = QObject::tr("Fills the space that solids, surfaces and planes close off together with a solid");
  def.keywords = {QStringLiteral("fill"),     QStringLiteral("boundary"), QStringLiteral("cell"),
                  QStringLiteral("enclosed"), QStringLiteral("volume"),   QStringLiteral("surfaces"),
                  QStringLiteral("solid from surfaces")};
  def.featureType = QStringLiteral("boundary_fill");
  // Every closed region see-through beside the filled cells (the
  // feature's preview tool).
  def.extendedToolPreview = true;
  // Its load asks the model for the regions.
  def.loadForHover = false;

  InputDef cells =
      textInput(kCells, QObject::tr("Cells"), QString())
          .withTooltip(QObject::tr("The closed regions to fill, by number (1, 3): click their dots in the view"))
          .withToggles([cache](const CommandState& state, const CommandContext& model, const QJsonObject&) {
            std::vector<Manipulator> toggles;
            const Regions& regions = regionsOf(state.items(kTools), model, *cache);
            const QList<int> on = cellNumbers(state);
            for (int i = 0; i < regions.list.size(); ++i) {
              const std::optional<gp_Pnt> at =
                  pointOf(regions.list.at(i).toObject().value(QStringLiteral("point")));
              if (!at) {
                continue;
              }
              Manipulator toggle;
              toggle.kind = Manipulator::Kind::Toggle;
              toggle.origin = *at;
              toggle.element = i + 1;
              toggle.on = on.contains(i + 1);
              toggles.push_back(toggle);
            }
            return toggles;
          });
  cells.togglesUnpreviewed = true; // the dots before any region is picked

  def.inputs = {
      selectionInput(kTools, QObject::tr("Tools"), SelectKind::Body | SelectKind::Plane, 1, 0)
          .withTooltip(QObject::tr("Solids, surfaces and planes that close off the space; surfaces may overlap"))
          .withoutMeshes()
          .withOnChange([cache](CommandState& state, const CommandContext& model) {
            // Other tools number the regions anew: the one region they
            // close off is taken at once, else none.
            state.setObject(kLoaded, QJsonObject());
            state.setObject(kKept, QJsonObject());
            const Regions& regions = regionsOf(state.items(kTools), model, *cache);
            state.setText(kCells, regions.list.size() == 1 ? QStringLiteral("1") : QString());
            qDebug().noquote() << QStringLiteral("Boundary Fill closed regions: %1").arg(regions.list.size());
          }),
      cells,
  };
  def.inputs += operationInputs();
  def.inputs += {
      checkInput(kRemoveTools, QObject::tr("Remove Tools"), false)
          .withTooltip(QObject::tr("Takes the tool bodies out of the design")),
  };
  def.enabled = [&context] { return hasBodies(context); };

  def.build = [cache](const CommandState& state, const CommandContext& model) {
    const Selection& tools = state.items(kTools);
    if (tools.isEmpty()) {
      return Built::failure(QObject::tr("Select the solids, surfaces and planes that close off the space to fill."),
                            kTools);
    }
    QJsonObject definition = toolFields(tools);
    definition.insert(QStringLiteral("type"), QStringLiteral("boundary_fill"));
    const Regions& regions = regionsOf(tools, model, *cache);
    if (!regions.error.isEmpty()) {
      return Built::failure(regions.error, kTools);
    }
    QJsonArray points = state.object(kKept).value(QStringLiteral("points")).toArray();
    for (const int number : cellNumbers(state)) {
      if (number <= regions.list.size()) {
        points.append(regions.list.at(number - 1).toObject().value(QStringLiteral("point")));
      }
    }
    if (points.isEmpty()) {
      return Built::failure(regions.list.isEmpty()
                                ? QObject::tr("The tools close off no region: add surfaces or planes until "
                                              "they close one off all round.")
                                : QObject::tr("Click the dots of the closed regions to fill in the view."),
                            kCells);
    }
    definition.insert(QStringLiteral("cells"), points);
    addOperation(definition, state);
    if (state.checked(kRemoveTools)) {
      definition.insert(QStringLiteral("remove_tools"), true);
    }
    return Built::of(definition);
  };

  def.load = [cache](const QJsonObject& feature, CommandState& state, const CommandContext& model) {
    Selection tools = bodiesOf(feature.value(QStringLiteral("tools")).toArray());
    for (const QJsonValue& plane : feature.value(QStringLiteral("planes")).toArray()) {
      const SelectionItem item = itemOf(plane, model);
      if (item.isValid()) {
        tools.append(item);
      }
    }
    state.setItems(kTools, tools);
    state.setText(kCells, QString());
    state.setObject(kLoaded, {{QStringLiteral("points"), feature.value(QStringLiteral("cells"))}});
    state.setObject(kKept, {{QStringLiteral("points"), feature.value(QStringLiteral("cells"))}});
    loadOperation(feature, state);
    state.setChecked(kRemoveTools, feature.value(QStringLiteral("remove_tools")).toBool());
    placeLoaded(state, model, *cache);
  };
  // The regions of the tools as they are before the feature.
  def.rolledBack = [cache](CommandState& state, const CommandContext& model) {
    cache->key.clear();
    placeLoaded(state, model, *cache);
  };
  // Planes the panel can show (an import's fixed plane cannot be picked).
  def.canEdit = [&context](const QJsonObject& feature) {
    const QJsonArray planes = feature.value(QStringLiteral("planes")).toArray();
    return std::all_of(planes.begin(), planes.end(),
                       [&context](const QJsonValue& plane) { return isShowable(plane, context); });
  };
  def.ended = [cache](bool) { *cache = Regions(); };
  def.describe = [](const CommandState& state, const CommandContext&) {
    const qsizetype kept = state.object(kKept).value(QStringLiteral("points")).toArray().size();
    return QStringLiteral("Added boundary fill (%1) of %2 cells from %3 tools")
        .arg(label(kOperations, state.choice(QStringLiteral("operation"))))
        .arg(cellNumbers(state).size() + kept)
        .arg(state.items(kTools).size());
  };
  return def;
}

} // namespace mitcad::cmd
